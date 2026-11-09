//! One bounded snapshot: revision watermark, report sequence and latest outbox item.
use crate::{DesiredState, RevisionGuard, Setting, enrollment::EnrollmentInfo};
use serde::{Deserialize, Serialize};
use std::io;
use url::Url;
use uuid::Uuid;

fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

pub const MAX_STATE_BYTES: usize = 16384;

pub trait StateStore {
    fn load(&mut self) -> io::Result<Vec<u8>>;
    /// Replace the complete snapshot durably, or return an error.
    fn save(&mut self, bytes: &[u8]) -> io::Result<()>;
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatusReport {
    schema_version: u32,
    agent_uuid: Uuid,
    #[serde(deserialize_with = "required_nullable")]
    rustdesk_id: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    rustdesk_version: Option<String>,
    desired_revision: u64,
    observation_sequence: u64,
    #[serde(deserialize_with = "required_nullable")]
    applied_revision: Option<u64>,
    results: Results,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Results {
    #[serde(skip_serializing_if = "Option::is_none")]
    approve_mode: Option<SettingResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verification_method: Option<SettingResult>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SettingResult {
    Unsupported,
    /// Matching CLI output is not independent live verification.
    UnverifiedCliMatch,
    ReadFailed,
    WriteFailed,
    Mismatch,
    /// Persisted before I/O; interruption can leave the actual outcome unknown.
    Indeterminate,
}

impl StatusReport {
    pub(crate) fn set_result(&mut self, key: Setting, result: SettingResult) {
        match key {
            Setting::ApproveMode => self.results.approve_mode = Some(result),
            Setting::VerificationMethod => self.results.verification_method = Some(result),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurableState {
    schema_version: u32,
    agent_uuid: Uuid,
    management_origin: Url,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) accepted: Option<DesiredState>,
    sequence: u64,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) pending: Option<StatusReport>,
}
impl DurableState {
    pub fn initial(info: &EnrollmentInfo) -> Self {
        Self {
            schema_version: 1,
            agent_uuid: info.agent_uuid,
            management_origin: info.management_origin.clone(),
            accepted: None,
            sequence: 0,
            pending: None,
        }
    }
    pub fn decode(bytes: &[u8], info: &EnrollmentInfo) -> Result<Self, &'static str> {
        if bytes.len() > MAX_STATE_BYTES {
            return Err("invalid_state_store");
        }
        let state: Self = serde_json::from_slice(bytes).map_err(|_| "invalid_state_store")?;
        if state.schema_version != 1
            || state.agent_uuid != info.agent_uuid
            || state.management_origin != info.management_origin
        {
            return Err("state_enrollment_mismatch");
        }
        if let Some(accepted) = &state.accepted {
            RevisionGuard::default().accept(accepted)?;
            if state.sequence == 0 {
                return Err("invalid_state_store");
            }
        } else if state.sequence != 0 || state.pending.is_some() {
            return Err("invalid_state_store");
        }
        if let Some(report) = &state.pending {
            let accepted = state.accepted.as_ref().ok_or("invalid_state_store")?;
            let mut expected = state.report(
                accepted,
                report.rustdesk_id.clone(),
                report.rustdesk_version.clone(),
            );
            for (key, _) in accepted.pairs() {
                let result = match key {
                    Setting::ApproveMode => report.results.approve_mode,
                    Setting::VerificationMethod => report.results.verification_method,
                }
                .ok_or("invalid_pending_report")?;
                expected.set_result(key, result);
            }
            if report != &expected {
                return Err("invalid_pending_report");
            }
            validate_identity(
                report.rustdesk_id.as_deref(),
                report.rustdesk_version.as_deref(),
            )?;
        }
        Ok(state)
    }
    pub fn encode(&self) -> Result<Vec<u8>, &'static str> {
        let bytes = serde_json::to_vec(self).map_err(|_| "invalid_state_store")?;
        if bytes.len() > MAX_STATE_BYTES {
            return Err("invalid_state_store");
        }
        Ok(bytes)
    }
    pub(crate) fn observe(
        &mut self,
        desired: &DesiredState,
        id: Option<String>,
        version: Option<String>,
    ) -> Result<(), &'static str> {
        validate_identity(id.as_deref(), version.as_deref())?;
        let mut guard = RevisionGuard::default();
        if let Some(accepted) = &self.accepted {
            guard.accept(accepted)?;
        }
        guard.accept(desired)?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or("observation_sequence_exhausted")?;
        self.sequence = sequence;
        self.accepted = Some(desired.clone());
        self.pending = Some(self.report(desired, id, version));
        Ok(())
    }
    fn report(
        &self,
        desired: &DesiredState,
        id: Option<String>,
        version: Option<String>,
    ) -> StatusReport {
        StatusReport {
            schema_version: 1,
            agent_uuid: self.agent_uuid,
            rustdesk_id: id,
            rustdesk_version: version,
            desired_revision: desired.revision,
            observation_sequence: self.sequence,
            applied_revision: None,
            results: Results {
                approve_mode: desired
                    .settings
                    .approve_mode
                    .map(|_| SettingResult::Unsupported),
                verification_method: desired
                    .settings
                    .verification_method
                    .map(|_| SettingResult::Unsupported),
            },
        }
    }
}
fn validate_identity(id: Option<&str>, version: Option<&str>) -> Result<(), &'static str> {
    use crate::detection::{Observation, Query, parse_observation};
    for (query, value) in [(Query::Id, id), (Query::Version, version)] {
        if let Some(value) = value
            && (value.len() > 64
                || parse_observation(query, true, value.as_bytes(), &[])
                    != Observation::Observed(value.into()))
        {
            return Err("invalid_report_identity");
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    pub(crate) struct MemoryStore(pub Arc<Mutex<Vec<u8>>>);
    impl MemoryStore {
        pub fn new(info: &EnrollmentInfo) -> Self {
            Self(Arc::new(Mutex::new(
                DurableState::initial(info).encode().unwrap(),
            )))
        }
    }
    impl StateStore for MemoryStore {
        fn load(&mut self) -> io::Result<Vec<u8>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&mut self, bytes: &[u8]) -> io::Result<()> {
            *self.0.lock().unwrap() = bytes.to_vec();
            Ok(())
        }
    }
    fn info() -> EnrollmentInfo {
        EnrollmentInfo {
            agent_uuid: Uuid::new_v4(),
            management_origin: Url::parse("https://example.test").unwrap(),
        }
    }
    fn desired(revision: u64) -> DesiredState {
        crate::parse_desired(format!(r#"{{"schema_version":1,"revision":{revision},"settings":{{"approve_mode":"click"}}}}"#).as_bytes()).unwrap()
    }
    #[test]
    fn restart_preserves_revision_sequence_and_exact_pending_report() {
        let info = info();
        let mut state = DurableState::initial(&info);
        state
            .observe(&desired(7), Some("123".into()), Some("1.4.7".into()))
            .unwrap();
        let mut restored = DurableState::decode(&state.encode().unwrap(), &info).unwrap();
        assert_eq!(restored.pending, state.pending);
        assert_eq!(
            restored.observe(&desired(6), None, None),
            Err("stale_revision")
        );
        restored.observe(&desired(7), None, None).unwrap();
        assert_eq!(restored.sequence, 2);
        assert_eq!(restored.pending.unwrap().applied_revision, None);
    }
    #[test]
    fn invalid_or_mismatched_state_never_resets_history() {
        let info = info();
        let state = DurableState::initial(&info);
        assert!(DurableState::decode(b"", &info).is_err());
        assert!(DurableState::decode(&state.encode().unwrap(), &super::tests::info()).is_err());
        let mut state = state;
        state.observe(&desired(1), None, None).unwrap();
        state.sequence = u64::MAX;
        assert_eq!(
            state.observe(&desired(2), None, None),
            Err("observation_sequence_exhausted")
        );
        state.pending.as_mut().unwrap().applied_revision = Some(1);
        assert!(DurableState::decode(&state.encode().unwrap(), &info).is_err());
    }
}
