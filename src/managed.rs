//! Managed settings adapter. Reuses the locked, job-contained CLI worker.
use crate::{
    Setting,
    detection::{Observation, Query},
    state::SettingResult,
};
use std::{path::Path, time::Duration};

pub async fn apply(
    agent: &Path,
    rustdesk: &Path,
    key: Setting,
    wanted: &'static str,
) -> SettingResult {
    if crate::probe::query(rustdesk, Query::Version).await != Observation::Observed("1.4.7".into())
    {
        return SettingResult::Unsupported;
    }
    let query = match key {
        Setting::ApproveMode => Query::ApproveMode,
        Setting::VerificationMethod => Query::VerificationMethod,
    };
    match crate::probe::query(rustdesk, query).await {
        Observation::UnverifiedCliValue(value) if value == wanted => {
            return SettingResult::UnverifiedCliMatch;
        }
        Observation::UnverifiedCliValue(_) | Observation::AmbiguousDefault => (),
        _ => return SettingResult::ReadFailed,
    }
    let mut command = tokio::process::Command::new(agent);
    command
        .arg("--set-option")
        .arg(rustdesk)
        .arg(query.args()[1])
        .arg(wanted);
    // 3s lock wait plus three 3s commands, with bounded process startup overhead.
    match crate::probe::capture(&mut command, Duration::from_secs(15)).await {
        Ok((success, stdout, _)) => classify(success, &stdout, wanted),
        Err(_) => SettingResult::WriteFailed,
    }
}

fn classify(success: bool, bytes: &[u8], wanted: &str) -> SettingResult {
    let Ok(report) = serde_json::from_slice::<crate::setter::SetReport>(bytes) else {
        return SettingResult::WriteFailed;
    };
    if !report.command_accepted || report.live_state_verified {
        return SettingResult::WriteFailed;
    }
    match report.readback {
        Observation::UnverifiedCliValue(value) if value == wanted => {
            if success && report.readback_matches {
                SettingResult::UnverifiedCliMatch
            } else {
                SettingResult::WriteFailed
            }
        }
        Observation::UnverifiedCliValue(_) | Observation::AmbiguousDefault => {
            SettingResult::Mismatch
        }
        _ => SettingResult::ReadFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_results_never_claim_verification_or_hide_failure() {
        for (success, readback, expected) in [
            (
                true,
                Observation::UnverifiedCliValue("click".into()),
                SettingResult::UnverifiedCliMatch,
            ),
            (
                false,
                Observation::UnverifiedCliValue("password".into()),
                SettingResult::Mismatch,
            ),
            (false, Observation::TimedOut, SettingResult::ReadFailed),
        ] {
            let report = crate::setter::SetReport {
                command_accepted: true,
                readback,
                readback_matches: success,
                live_state_verified: false,
            };
            assert_eq!(
                classify(success, &serde_json::to_vec(&report).unwrap(), "click"),
                expected
            );
        }
        assert_eq!(classify(false, b"", "click"), SettingResult::WriteFailed);
    }
}
