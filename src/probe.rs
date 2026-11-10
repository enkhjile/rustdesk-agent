//! Fixed read commands only; deliberately does not implement SettingsIo.
use crate::detection::{Observation, Query, parse_observation};
use serde::Serialize;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

const LIMIT: u64 = 4096;

async fn read_output(stream: impl AsyncRead + Unpin) -> Result<Vec<u8>, Observation> {
    let mut bytes = Vec::new();
    stream
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| Observation::CommandFailed)?;
    if bytes.len() as u64 > LIMIT {
        Err(Observation::OutputTooLarge)
    } else {
        Ok(bytes)
    }
}

pub(crate) async fn capture(
    command: &mut Command,
    timeout: Duration,
) -> Result<(bool, Vec<u8>, Vec<u8>), Observation> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW; never request elevation.
    let mut child = command.spawn().map_err(|_| Observation::Unavailable)?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let result = tokio::time::timeout(timeout, async {
        let (stdout, stderr, status) =
            tokio::try_join!(read_output(stdout), read_output(stderr), async {
                child.wait().await.map_err(|_| Observation::CommandFailed)
            })?;
        Ok((status.success(), stdout, stderr))
    })
    .await
    .unwrap_or(Err(Observation::TimedOut));
    if result.is_err() {
        let _ = child.kill().await;
    }
    result
}

pub async fn query(executable: &Path, query: Query) -> Observation {
    if !executable.is_absolute() || !executable.is_file() {
        return Observation::Unavailable;
    }
    let mut command = Command::new(executable);
    command.args(query.args());
    match capture(&mut command, Duration::from_secs(3)).await {
        Ok((success, stdout, stderr)) => parse_observation(query, success, &stdout, &stderr),
        Err(error) => error,
    }
}

/// Official CLI acknowledgement only; no password or raw output is reported.
#[cfg(windows)]
pub async fn change_password(
    executable: &Path,
    password: zeroize::Zeroizing<String>,
) -> crate::password::Outcome {
    use crate::password::Outcome;
    if !executable.is_absolute()
        || !executable.is_file()
        || crate::credentials::require_password_service_identity().is_err()
        || query(executable, Query::Version).await != Observation::Observed("1.4.7".into())
    {
        return Outcome::Unsupported;
    }
    let mut command = Command::new(executable);
    command.arg("--password").arg(password.as_str());
    match capture(&mut command, Duration::from_secs(15)).await {
        Ok((success, stdout, stderr)) => {
            let stdout = zeroize::Zeroizing::new(stdout);
            let stderr = zeroize::Zeroizing::new(stderr);
            if success && stderr.is_empty() && stdout.as_slice().trim_ascii() == b"Done!" {
                Outcome::CommandAccepted
            } else {
                Outcome::Indeterminate
            }
        }
        Err(_) => Outcome::Indeterminate,
    }
}

#[derive(Serialize)]
pub struct Detection {
    pub executable_present: bool,
    pub live_state_verified: bool,
    pub version: Observation,
    pub rustdesk_id: Observation,
    pub approve_mode: Observation,
    pub verification_method: Observation,
}

pub async fn detect(executable: &Path) -> Detection {
    let version = query(executable, Query::Version).await;
    // Only this CLI version has been source-inspected. Runtime support is unverified.
    let inspected = version == Observation::Observed("1.4.7".into());
    Detection {
        executable_present: executable.is_absolute() && executable.is_file(),
        live_state_verified: false,
        version,
        rustdesk_id: if inspected {
            query(executable, Query::Id).await
        } else {
            Observation::UnsupportedVersion
        },
        approve_mode: if inspected {
            query(executable, Query::ApproveMode).await
        } else {
            Observation::UnsupportedVersion
        },
        verification_method: if inspected {
            query(executable, Query::VerificationMethod).await
        } else {
            Observation::UnsupportedVersion
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child() {
        match std::env::var("RUSTDESK_AGENT_TEST_CHILD").as_deref() {
            Ok("timeout") => std::thread::sleep(Duration::from_secs(10)),
            Ok("large") => println!("{}", "x".repeat(8192)),
            Ok("fail") => std::process::exit(1),
            _ => (),
        }
    }

    #[tokio::test]
    async fn process_limits_and_exit_status() {
        for mode in ["timeout", "large", "fail"] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "probe::tests::child", "--nocapture"])
                .env("RUSTDESK_AGENT_TEST_CHILD", mode);
            let timeout = if mode == "timeout" {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(3)
            };
            match (mode, capture(&mut command, timeout).await) {
                ("timeout", Err(Observation::TimedOut))
                | ("large", Err(Observation::OutputTooLarge))
                | ("fail", Ok((false, _, _))) => (),
                other => panic!("unexpected capture result: {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn missing_executable_is_reported_without_spawning() {
        let report = detect(Path::new("relative-rustdesk.exe")).await;
        assert!(!report.executable_present);
        assert_eq!(report.version, Observation::Unavailable);
        assert!(!report.live_state_verified);
    }
}
