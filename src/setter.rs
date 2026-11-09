//! One-shot settings commands, also used by the opt-in lab service's CLI worker.
use crate::detection::{Observation, Query, parse_observation};
use serde::{Deserialize, Serialize};
use std::{future::Future, path::Path};

#[derive(Clone, Copy, Debug)]
pub struct Change {
    query: Query,
    value: &'static str,
}

impl Change {
    pub fn parse(key: &str, value: &str) -> Result<Self, &'static str> {
        let query = match key {
            "approve-mode" => Query::ApproveMode,
            "verification-method" => Query::VerificationMethod,
            _ => return Err("unsupported setting or value"),
        };
        let value = match (query, value) {
            (Query::ApproveMode, "password") => "password",
            (Query::ApproveMode, "click") => "click",
            (Query::ApproveMode, "password-click") => "password-click",
            (Query::VerificationMethod, "use-temporary-password") => "use-temporary-password",
            (Query::VerificationMethod, "use-permanent-password") => "use-permanent-password",
            (Query::VerificationMethod, "use-both-passwords") => "use-both-passwords",
            _ => return Err("unsupported setting or value"),
        };
        Ok(Self { query, value })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetReport {
    pub command_accepted: bool,
    pub readback: Observation,
    pub readback_matches: bool,
    pub live_state_verified: bool,
}

type Output = Result<(bool, Vec<u8>, Vec<u8>), Observation>;

async fn execute<F, Fut>(change: Change, mut run: F) -> Result<SetReport, Observation>
where
    F: FnMut(Vec<&'static str>) -> Fut,
    Fut: Future<Output = Output>,
{
    let (success, stdout, stderr) = run(Query::Version.args().to_vec()).await?;
    match parse_observation(Query::Version, success, &stdout, &stderr) {
        Observation::Observed(version) if version == "1.4.7" => (),
        Observation::Observed(_) => return Err(Observation::UnsupportedVersion),
        error => return Err(error),
    }
    let mut args = change.query.args().to_vec();
    args.push(change.value);
    let (success, stdout, stderr) = run(args).await?;
    if !success || !stderr.is_empty() || !stdout.iter().all(u8::is_ascii_whitespace) {
        return Err(Observation::CommandFailed);
    }
    let readback = match run(change.query.args().to_vec()).await {
        Ok((success, stdout, stderr)) => parse_observation(change.query, success, &stdout, &stderr),
        Err(error) => error,
    };
    let readback_matches = readback == Observation::UnverifiedCliValue(change.value.into());
    Ok(SetReport {
        command_accepted: true,
        readback,
        readback_matches,
        live_state_verified: false,
    })
}

/// Run one allowlisted change. CLI readback can fall back to cached configuration.
/// A failed command may still have changed the setting; no automatic retry occurs.
pub async fn set_option(executable: &Path, change: Change) -> Result<SetReport, Observation> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err(Observation::Unavailable);
    }
    execute(change, |args| async move {
        let mut command = tokio::process::Command::new(executable);
        command.args(args);
        crate::probe::capture(&mut command, std::time::Duration::from_secs(3)).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, future::ready};

    fn ok(output: &str) -> Output {
        Ok((true, output.as_bytes().to_vec(), vec![]))
    }

    #[tokio::test]
    async fn relative_path_is_rejected_before_execution() {
        let result = set_option(
            Path::new("rustdesk.exe"),
            Change::parse("approve-mode", "click").unwrap(),
        )
        .await;
        assert!(matches!(result, Err(Observation::Unavailable)));
    }

    #[tokio::test]
    async fn all_values_use_only_fixed_commands_and_never_claim_live_verification() {
        for (key, values) in [
            ("approve-mode", ["password", "click", "password-click"]),
            (
                "verification-method",
                [
                    "use-temporary-password",
                    "use-permanent-password",
                    "use-both-passwords",
                ],
            ),
        ] {
            for value in values {
                let change = Change::parse(key, value).unwrap();
                let mut outputs = VecDeque::from([ok("1.4.7\n"), ok(""), ok(value)]);
                let mut calls = vec![];
                let report = execute(change, |args| {
                    calls.push(args);
                    ready(outputs.pop_front().unwrap())
                })
                .await
                .unwrap();
                assert_eq!(
                    calls,
                    vec![
                        vec!["--version"],
                        vec!["--option", key, value],
                        vec!["--option", key]
                    ]
                );
                assert!(report.command_accepted && report.readback_matches);
                assert!(!report.live_state_verified);
                assert_eq!(
                    report.readback,
                    Observation::UnverifiedCliValue(value.into())
                );
            }
        }
    }

    #[test]
    fn rejects_other_settings_empty_values_and_command_injection() {
        for (key, value) in [
            ("password", "secret"),
            ("key", "key"),
            ("approve-mode", ""),
            ("approve-mode", "click --password x"),
            ("approve-mode", "use-both-passwords"),
            ("verification-method", "click"),
            ("--config", "x"),
        ] {
            assert!(Change::parse(key, value).is_err());
        }
    }

    #[tokio::test]
    async fn unsupported_or_failed_version_never_runs_a_setter() {
        for version in [
            ok("1.4.8"),
            ok("garbage"),
            Err(Observation::Unavailable),
            Err(Observation::TimedOut),
        ] {
            let mut outputs = VecDeque::from([version]);
            let result = execute(Change::parse("approve-mode", "click").unwrap(), |args| {
                assert_eq!(args, vec!["--version"]);
                ready(outputs.pop_front().unwrap())
            })
            .await;
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn setter_errors_including_exit_zero_diagnostics_stop_before_readback() {
        for output in [
            ok("Settings are disabled!"),
            Ok((false, vec![], vec![])),
            Ok((true, vec![], b"error".to_vec())),
            Err(Observation::TimedOut),
            Err(Observation::OutputTooLarge),
        ] {
            let mut outputs = VecDeque::from([ok("1.4.7"), output]);
            let result = execute(Change::parse("approve-mode", "click").unwrap(), |_| {
                ready(outputs.pop_front().unwrap())
            })
            .await;
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn failed_blank_or_different_readback_is_not_a_match() {
        for output in [
            ok(""),
            ok("password"),
            ok("Settings are disabled!"),
            Err(Observation::TimedOut),
        ] {
            let mut outputs = VecDeque::from([ok("1.4.7"), ok(""), output]);
            let report = execute(Change::parse("approve-mode", "click").unwrap(), |_| {
                ready(outputs.pop_front().unwrap())
            })
            .await
            .unwrap();
            assert!(report.command_accepted);
            assert!(!report.readback_matches && !report.live_state_verified);
        }
    }
}
