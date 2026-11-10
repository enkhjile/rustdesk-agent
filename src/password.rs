//! Password operations are separate from settings policies and normal reports.
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

// Deliberately neither Debug, Clone nor Serialize: this is a short-lived secret
// delivery envelope, never a diagnostic or durable-state object.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    schema_version: u32,
    pub operation_id: Uuid,
    pub sequence: u64,
    pub expires_at: u64,
    #[serde(deserialize_with = "required_password")]
    pub password: Option<Zeroizing<String>>,
}

fn required_password<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Zeroizing<String>>, D::Error> {
    Option::deserialize(d)
}

impl Operation {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        const INVALID: &str = "invalid_password_operation";
        if bytes.len() > 4096 {
            return Err(INVALID);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
        if value.schema_version != 1
            || !valid_id(value.operation_id)
            || value.sequence == 0
            || value.expires_at == 0
            || value.password.as_ref().is_some_and(|secret| {
                !(8..=128).contains(&secret.len())
                    || !secret.bytes().all(|c| (0x20..=0x7e).contains(&c))
            })
        {
            return Err(INVALID);
        }
        Ok(value)
    }
}

fn valid_id(id: Uuid) -> bool {
    id.get_version_num() == 4 && id.get_variant() == uuid::Variant::RFC4122
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    CommandAccepted,
    Indeterminate,
    Expired,
    Unsupported,
}

/// Persist before executing; retry only this result after an interruption.
/// Acknowledgement allows a newer operation, never replay of this operation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub operation_id: Uuid,
    pub sequence: u64,
    pub outcome: Outcome,
    pub acknowledged: bool,
}

impl Attempt {
    pub fn begin(
        previous: Option<&Self>,
        command: &Operation,
        now: u64,
    ) -> Result<Self, &'static str> {
        if previous.is_some_and(|last| {
            !last.acknowledged
                || command.sequence <= last.sequence
                || command.operation_id == last.operation_id
        }) {
            return Err("password_operation_conflict");
        }
        Ok(Self {
            operation_id: command.operation_id,
            sequence: command.sequence,
            outcome: if command.password.is_none() || now >= command.expires_at {
                Outcome::Expired
            } else {
                Outcome::Indeterminate
            },
            acknowledged: false,
        })
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.sequence == 0 || !valid_id(self.operation_id) {
            return Err("invalid_password_attempt");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(sequence: u64) -> Operation {
        Operation::parse(
            serde_json::json!({
                "schema_version": 1,
                "operation_id": uuid::Uuid::new_v4(),
                "sequence": sequence,
                "expires_at": 1900,
                "password": "test-only-password"
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn interrupted_attempt_is_reported_without_replaying_secret() {
        let command = operation(1);
        let intent = Attempt::begin(None, &command, 1000).unwrap();
        let saved = serde_json::to_string(&intent).unwrap();
        assert!(!saved.contains("test-only-password"));
        let recovered: Attempt = serde_json::from_str(&saved).unwrap();
        assert_eq!(recovered.outcome, Outcome::Indeterminate);
        assert!(!recovered.acknowledged);
        assert!(Attempt::begin(Some(&recovered), &command, 1001).is_err());
        assert!(Attempt::begin(Some(&recovered), &operation(2), 1001).is_err());
    }

    #[test]
    fn acknowledgement_allows_only_a_newer_operation() {
        let first = operation(1);
        let mut attempt = Attempt::begin(None, &first, 1000).unwrap();
        attempt.outcome = Outcome::CommandAccepted;
        attempt.acknowledged = true;
        assert!(Attempt::begin(Some(&attempt), &first, 1001).is_err());
        let newer = operation(2);
        assert!(Attempt::begin(Some(&attempt), &newer, 1001).is_ok());
        let later = Attempt::begin(Some(&attempt), &newer, 1001).unwrap();
        assert!(Attempt::begin(Some(&later), &first, 1002).is_err());
    }

    #[test]
    fn expired_operation_records_a_terminal_non_execution_result() {
        let command = operation(1);
        let attempt = Attempt::begin(None, &command, 1900).unwrap();
        assert_eq!(attempt.outcome, Outcome::Expired);
        assert!(!attempt.acknowledged);
    }

    #[test]
    fn erased_secret_is_a_non_executing_tombstone_but_missing_field_is_invalid() {
        let mut value = serde_json::json!({"schema_version":1,"operation_id":Uuid::new_v4(),
            "sequence":1,"expires_at":1900,"password":null});
        let command = Operation::parse(value.to_string().as_bytes()).unwrap();
        assert_eq!(
            Attempt::begin(None, &command, 1000).unwrap().outcome,
            Outcome::Expired
        );
        value.as_object_mut().unwrap().remove("password");
        assert!(Operation::parse(value.to_string().as_bytes()).is_err());
    }

    #[test]
    fn rejects_invalid_password_envelopes_without_echoing_the_secret() {
        for value in [
            serde_json::json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4(),"sequence":0,"expires_at":1900,"password":"test-only-password"}),
            serde_json::json!({"schema_version":1,"operation_id":uuid::Uuid::nil(),"sequence":1,"expires_at":1900,"password":"test-only-password"}),
            serde_json::json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4(),"sequence":1,"expires_at":1900,"password":"short"}),
            serde_json::json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4(),"sequence":1,"expires_at":1900,"password":"test-only-password","command":"ignored"}),
        ] {
            let error = Operation::parse(value.to_string().as_bytes())
                .err()
                .unwrap();
            assert_eq!(error, "invalid_password_operation");
        }
    }
}
