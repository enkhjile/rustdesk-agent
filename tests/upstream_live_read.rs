// Compile the proposed upstream module itself, not a second implementation.
extern crate self as hbb_common;
pub use tokio;
#[path = "../proposals/rustdesk-live-read/settings_read.rs"]
mod settings_read;

use settings_read::{ReadError, read_settings};
use std::collections::HashMap;

fn args() -> Vec<String> {
    vec!["--get-settings-json".into()]
}

#[test]
fn upstream_patch_contains_the_tested_module() {
    let patch = include_str!("../proposals/rustdesk-live-read/rustdesk-1.4.7.patch");
    let (_, module_patch) = patch
        .split_once("diff --git a/src/settings_read.rs b/src/settings_read.rs")
        .unwrap();
    let source = module_patch
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .map(|line| &line[1..])
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        source.trim_end(),
        include_str!("../proposals/rustdesk-live-read/settings_read.rs")
            .replace("\r\n", "\n")
            .trim_end()
    );
}

#[tokio::test]
async fn live_response_is_allowlisted_and_preserves_unset_values() {
    let response = HashMap::from([
        ("approve-mode".into(), "".into()),
        ("permanent-password".into(), "must-not-leak".into()),
    ]);
    let output = read_settings(&args(), true, true, false, || async { Ok(response) })
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&output).unwrap(),
        serde_json::json!({
            "schema_version": 1, "source": "live_ipc",
            "settings": {"approve_mode": "", "verification_method": null}
        })
    );
    assert!(!output.contains("must-not-leak"));
    let output = read_settings(&args(), true, true, false, || async { Ok(HashMap::new()) })
        .await
        .unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&output).unwrap()["settings"]["approve_mode"]
            .is_null()
    );
}

#[tokio::test]
async fn all_allowed_values_are_returned_without_default_normalization() {
    for approve in ["password", "click", "password-click"] {
        for verification in [
            "use-temporary-password",
            "use-permanent-password",
            "use-both-passwords",
        ] {
            let response = HashMap::from([
                ("approve-mode".into(), approve.into()),
                ("verification-method".into(), verification.into()),
            ]);
            let output = read_settings(&args(), true, true, false, || async { Ok(response) })
                .await
                .unwrap();
            let json: serde_json::Value = serde_json::from_str(&output).unwrap();
            assert_eq!(json["settings"]["approve_mode"], approve);
            assert_eq!(json["settings"]["verification_method"], verification);
        }
    }
}

#[tokio::test]
async fn usage_and_policy_denials_do_not_contact_ipc() {
    for (arguments, installed, elevated, disabled, expected) in [
        (vec![], true, true, false, ReadError::Usage),
        (vec!["--option".into()], true, true, false, ReadError::Usage),
        (
            vec!["--get-settings-json".into(), "click".into()],
            true,
            true,
            false,
            ReadError::Usage,
        ),
        (args(), false, true, false, ReadError::Denied),
        (args(), true, false, false, ReadError::Denied),
        (args(), true, true, true, ReadError::Denied),
    ] {
        let result = read_settings(&arguments, installed, elevated, disabled, || {
            panic!("denied request contacted IPC");
            #[allow(unreachable_code)]
            async {
                Ok(HashMap::new())
            }
        })
        .await;
        assert_eq!(result, Err(expected));
    }
}

#[tokio::test]
async fn transport_failure_and_timeout_cannot_return_cached_success() {
    assert_eq!(
        read_settings(&args(), true, true, false, || async { Err(()) }).await,
        Err(ReadError::IpcUnavailable)
    );
    let start = std::time::Instant::now();
    assert_eq!(
        read_settings(&args(), true, true, false, std::future::pending).await,
        Err(ReadError::IpcUnavailable)
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    assert!(start.elapsed() >= std::time::Duration::from_millis(900));
}

#[tokio::test]
async fn unknown_values_are_rejected_without_leaking_them() {
    for key in ["approve-mode", "verification-method"] {
        let response = HashMap::from([(key.into(), "unexpected-sensitive-value".into())]);
        let error = read_settings(&args(), true, true, false, || async { Ok(response) })
            .await
            .unwrap_err();
        assert_eq!(error, ReadError::UnsupportedValue);
        assert!(!error.message().contains("unexpected-sensitive-value"));
    }
    for (error, code) in [
        (ReadError::Usage, 2),
        (ReadError::Denied, 3),
        (ReadError::IpcUnavailable, 4),
        (ReadError::UnsupportedValue, 5),
    ] {
        assert_eq!(error.exit_code(), code);
    }
}
