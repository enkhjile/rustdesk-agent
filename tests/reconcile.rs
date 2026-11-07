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
