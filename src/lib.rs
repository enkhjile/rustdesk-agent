use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Setting {
    ApproveMode,
    VerificationMethod,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesiredState {
    pub schema_version: u32,
    pub revision: u64,
    pub settings: Settings,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default, deserialize_with = "present_setting")]
    pub approve_mode: Option<ApproveMode>,
    #[serde(default, deserialize_with = "present_setting")]
    pub verification_method: Option<VerificationMethod>,
}

// Missing fields default to None; present fields must contain a valid enum, never null.
fn present_setting<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ApproveMode {
    Password,
    Click,
    PasswordClick,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationMethod {
    UseTemporaryPassword,
    UsePermanentPassword,
    UseBothPasswords,
}

impl DesiredState {
    fn is_valid(&self) -> bool {
        self.schema_version == 1 && self.revision > 0 && !self.pairs().is_empty()
    }

    fn pairs(&self) -> Vec<(Setting, &'static str)> {
        let mut pairs = Vec::new();
        if let Some(mode) = self.settings.approve_mode {
            pairs.push((
                Setting::ApproveMode,
                match mode {
                    ApproveMode::Password => "password",
                    ApproveMode::Click => "click",
                    ApproveMode::PasswordClick => "password-click",
                },
            ));
        }
        if let Some(method) = self.settings.verification_method {
            pairs.push((
                Setting::VerificationMethod,
                match method {
                    VerificationMethod::UseTemporaryPassword => "use-temporary-password",
                    VerificationMethod::UsePermanentPassword => "use-permanent-password",
                    VerificationMethod::UseBothPasswords => "use-both-passwords",
                },
            ));
        }
        pairs
    }
}

pub fn parse_desired(json: &[u8]) -> Result<DesiredState, &'static str> {
    let desired: DesiredState =
        serde_json::from_slice(json).map_err(|_| "invalid_json_or_schema")?;
    if !desired.is_valid() {
        return Err("invalid_state");
    }
    Ok(desired)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsError;

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("settings operation failed")
    }
}

impl std::error::Error for SettingsError {}

pub trait SettingsIo {
    fn read(&mut self, key: Setting) -> Result<String, SettingsError>;
    fn write(&mut self, key: Setting, value: &str) -> Result<(), SettingsError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Verified,
    ReadFailed,
    WriteFailed,
    Mismatch,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub desired_revision: u64,
    pub applied_revision: Option<u64>,
    pub results: BTreeMap<Setting, Outcome>,
}

/// In-memory accepted-state watermark. Reuse one guard per enrolled device.
/// This does not provide restart protection; durable storage is not implemented.
#[derive(Debug, Default)]
pub struct RevisionGuard {
    accepted: Option<DesiredState>,
}

impl RevisionGuard {
    pub fn accepted(&self) -> Option<&DesiredState> {
        self.accepted.as_ref()
    }

    /// Reject invalid or stale state before I/O, retaining acceptance even if
    /// application fails. Equal revisions must have identical parsed settings.
    pub fn reconcile(
        &mut self,
        io: &mut impl SettingsIo,
        desired: &DesiredState,
    ) -> Result<Report, &'static str> {
        if !desired.is_valid() {
            return Err("invalid_state");
        }
        if let Some(accepted) = &self.accepted {
            if desired.revision < accepted.revision {
                return Err("stale_revision");
            }
            if desired.revision == accepted.revision && desired.settings != accepted.settings {
                return Err("revision_conflict");
            }
        }
        self.accepted = Some(desired.clone());
        Ok(reconcile(io, desired))
    }
}

/// Reconcile one state without revision history. Use RevisionGuard for repeated polls.
pub fn reconcile(io: &mut impl SettingsIo, desired: &DesiredState) -> Report {
    let mut results = BTreeMap::new();
    let pairs = desired.pairs();
    let valid = desired.is_valid();
    if !valid {
        return Report {
            desired_revision: desired.revision,
            applied_revision: None,
            results,
        };
    }
    for (key, wanted) in &pairs {
        let outcome = match io.read(*key) {
            Err(SettingsError) => Outcome::ReadFailed,
            Ok(actual) if actual == *wanted => Outcome::Verified,
            Ok(_) => match io.write(*key, wanted) {
                Err(SettingsError) => Outcome::WriteFailed,
                Ok(()) => match io.read(*key) {
                    Err(SettingsError) => Outcome::ReadFailed,
                    Ok(actual) if actual == *wanted => Outcome::Verified,
                    Ok(_) => Outcome::Mismatch,
                },
            },
        };
        results.insert(*key, outcome);
    }
    let applied_revision =
        (valid && results.values().all(|r| *r == Outcome::Verified)).then_some(desired.revision);
    Report {
        desired_revision: desired.revision,
        applied_revision,
        results,
    }
}

pub fn backoff_seconds(failures: u32) -> u64 {
    5u64.saturating_mul(1u64 << failures.min(6)).min(300)
}
