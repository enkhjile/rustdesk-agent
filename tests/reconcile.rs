use rustdesk_agent::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct Fake {
    values: BTreeMap<Setting, String>,
    fail_write: bool,
    lie: bool,
}
impl SettingsIo for Fake {
    fn read(&mut self, key: Setting) -> Result<String, SettingsError> {
        self.values.get(&key).cloned().ok_or(SettingsError)
    }
    fn write(&mut self, key: Setting, value: &str) -> Result<(), SettingsError> {
        if self.fail_write {
            return Err(SettingsError);
        }
        if !self.lie {
            self.values.insert(key, value.to_owned());
        }
        Ok(())
    }
}
fn desired() -> DesiredState {
    serde_json::from_str(r#"{"schema_version":1,"revision":7,"settings":{"approve_mode":"password","verification_method":"use-permanent-password"}}"#).unwrap()
}
fn fake() -> Fake {
    Fake {
        values: [
            (Setting::ApproveMode, "click".into()),
            (Setting::VerificationMethod, "use-both-passwords".into()),
        ]
        .into(),
        ..Default::default()
    }
}
#[test]
fn rejects_unknown_fields_and_values() {
    assert!(
        serde_json::from_str::<DesiredState>(
            r#"{"schema_version":1,"revision":1,"settings":{"password":"secret"}}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<DesiredState>(
            r#"{"schema_version":1,"revision":1,"settings":{"approve_mode":"--help"}}"#
        )
        .is_err()
    );
}
#[test]
fn verifies_each_setting_before_marking_revision() {
    let report = reconcile(&mut fake(), &desired());
    assert_eq!(report.applied_revision, Some(7));
    assert!(report.results.values().all(|v| *v == Outcome::Verified));
}
#[test]
fn does_not_trust_successful_write_without_readback() {
    let mut f = Fake {
        lie: true,
        ..fake()
    };
    let report = reconcile(&mut f, &desired());
    assert_eq!(report.applied_revision, None);
    assert_eq!(report.results[&Setting::ApproveMode], Outcome::Mismatch);
}
#[test]
fn failed_reads_do_not_trigger_writes() {
    let mut f = Fake::default();
    let report = reconcile(&mut f, &desired());
    assert_eq!(report.applied_revision, None);
    assert!(f.values.is_empty());
    assert_eq!(report.results[&Setting::ApproveMode], Outcome::ReadFailed);
}
#[test]
fn failed_writes_are_not_applied() {
    let mut f = Fake {
        fail_write: true,
        ..fake()
    };
    let report = reconcile(&mut f, &desired());
    assert_eq!(report.results[&Setting::ApproveMode], Outcome::WriteFailed);
    assert_eq!(report.applied_revision, None);
}
#[test]
fn invalid_schema_and_empty_state_are_rejected() {
    assert!(
        parse_desired(
            br#"{"schema_version":2,"revision":1,"settings":{"approve_mode":"password"}}"#
        )
        .is_err()
    );
    assert!(parse_desired(br#"{"schema_version":1,"revision":0,"settings":{}}"#).is_err());
}
#[test]
fn backoff_is_bounded() {
    assert_eq!(backoff_seconds(0), 5);
    assert_eq!(backoff_seconds(100), 300);
}

#[test]
fn invalid_state_cannot_apply_even_if_constructed_directly() {
    let state: DesiredState = serde_json::from_str(
        r#"{"schema_version":2,"revision":1,"settings":{"approve_mode":"password"}}"#,
    )
    .unwrap();
    let mut f = fake();
    let report = reconcile(&mut f, &state);
    assert_eq!(report.applied_revision, None);
    assert_eq!(f.values[&Setting::ApproveMode], "click");
}

#[test]
fn rejects_explicit_null_even_with_another_valid_setting() {
    for settings in [
        r#"{"approve_mode":null,"verification_method":"use-both-passwords"}"#,
        r#"{"approve_mode":"click","verification_method":null}"#,
    ] {
        let json = format!(r#"{{"schema_version":1,"revision":1,"settings":{settings}}}"#);
        assert!(parse_desired(json.as_bytes()).is_err());
    }
}

#[test]
fn rejects_duplicate_fields_at_each_level() {
    for json in [
        r#"{"schema_version":1,"revision":1,"revision":2,"settings":{"approve_mode":"click"}}"#,
        r#"{"schema_version":1,"revision":1,"settings":{"approve_mode":"click","approve_mode":"password"}}"#,
    ] {
        assert!(parse_desired(json.as_bytes()).is_err());
    }
}

#[test]
fn same_revision_repairs_drift() {
    let mut io = fake();
    assert_eq!(reconcile(&mut io, &desired()).applied_revision, Some(7));
    io.values.insert(Setting::ApproveMode, "click".into());
    assert_eq!(reconcile(&mut io, &desired()).applied_revision, Some(7));
    assert_eq!(io.values[&Setting::ApproveMode], "password");
}

#[test]
fn partial_success_does_not_mark_revision_applied() {
    let mut io = fake();
    io.values.remove(&Setting::VerificationMethod);
    let report = reconcile(&mut io, &desired());
    assert_eq!(report.applied_revision, None);
    assert_eq!(report.results[&Setting::ApproveMode], Outcome::Verified);
    assert_eq!(
        report.results[&Setting::VerificationMethod],
        Outcome::ReadFailed
    );
}

#[test]
fn failed_post_write_read_is_not_verified() {
    struct DisconnectsAfterWrite(bool);
    impl SettingsIo for DisconnectsAfterWrite {
        fn read(&mut self, _: Setting) -> Result<String, SettingsError> {
            if self.0 {
                Err(SettingsError)
            } else {
                Ok("click".into())
            }
        }
        fn write(&mut self, _: Setting, _: &str) -> Result<(), SettingsError> {
            self.0 = true;
            Ok(())
        }
    }
    let state = parse_desired(
        br#"{"schema_version":1,"revision":1,"settings":{"approve_mode":"password"}}"#,
    )
    .unwrap();
    let report = reconcile(&mut DisconnectsAfterWrite(false), &state);
    assert_eq!(report.applied_revision, None);
    assert_eq!(report.results[&Setting::ApproveMode], Outcome::ReadFailed);
}

#[test]
fn revision_guard_rejects_stale_and_changed_state_without_io() {
    struct NoIo;
    impl SettingsIo for NoIo {
        fn read(&mut self, _: Setting) -> Result<String, SettingsError> {
            panic!("rejected state must not read settings")
        }
        fn write(&mut self, _: Setting, _: &str) -> Result<(), SettingsError> {
            panic!("rejected state must not write settings")
        }
    }
    let mut guard = RevisionGuard::default();
    guard.reconcile(&mut fake(), &desired()).unwrap();
    let mut state = desired();
    state.revision = 6;
    assert_eq!(
        guard.reconcile(&mut NoIo, &state).unwrap_err(),
        "stale_revision"
    );
    state.revision = 7;
    state.settings.approve_mode = Some(ApproveMode::Click);
    assert_eq!(
        guard.reconcile(&mut NoIo, &state).unwrap_err(),
        "revision_conflict"
    );
    state = desired();
    state.settings.verification_method = None;
    assert_eq!(
        guard.reconcile(&mut NoIo, &state).unwrap_err(),
        "revision_conflict"
    );
    state.schema_version = 2;
    state.revision = 100;
    assert_eq!(
        guard.reconcile(&mut NoIo, &state).unwrap_err(),
        "invalid_state"
    );
    assert_eq!(guard.accepted().unwrap().revision, 7);
}

#[test]
fn revision_guard_compares_parsed_settings_and_allows_drift_repair() {
    let mut guard = RevisionGuard::default();
    let mut io = fake();
    guard.reconcile(&mut io, &desired()).unwrap();
    let reordered = parse_desired(br#"{ "settings": {"verification_method":"use-permanent-password", "approve_mode":"password"}, "revision":7, "schema_version":1 }"#).unwrap();
    io.values.insert(Setting::ApproveMode, "click".into());
    assert_eq!(
        guard
            .reconcile(&mut io, &reordered)
            .unwrap()
            .applied_revision,
        Some(7)
    );
    assert_eq!(io.values[&Setting::ApproveMode], "password");
}

#[test]
fn revision_guard_keeps_newer_revision_after_failed_application() {
    let mut guard = RevisionGuard::default();
    guard.reconcile(&mut fake(), &desired()).unwrap();
    let mut newer = desired();
    newer.revision = u64::MAX;
    let report = guard.reconcile(&mut Fake::default(), &newer).unwrap();
    assert_eq!(report.applied_revision, None);
    assert_eq!(guard.accepted().unwrap().revision, u64::MAX);
    assert_eq!(
        guard.reconcile(&mut fake(), &desired()).unwrap_err(),
        "stale_revision"
    );
    assert_eq!(
        guard
            .reconcile(&mut fake(), &newer)
            .unwrap()
            .applied_revision,
        Some(u64::MAX)
    );
}

#[test]
fn invalid_first_revision_does_not_initialize_guard() {
    let mut guard = RevisionGuard::default();
    let mut state = desired();
    state.revision = 0;
    assert!(guard.reconcile(&mut fake(), &state).is_err());
    assert!(guard.accepted().is_none());
    assert!(guard.reconcile(&mut fake(), &desired()).is_ok());
}
