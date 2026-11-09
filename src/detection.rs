//! Read-only CLI observations, never evidence that settings are live or applied.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug)]
pub enum Query {
    Version,
    Id,
    ApproveMode,
    VerificationMethod,
}
impl Query {
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Version => &["--version"],
            Self::Id => &["--get-id"],
            Self::ApproveMode => &["--option", "approve-mode"],
            Self::VerificationMethod => &["--option", "verification-method"],
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Observation {
    Observed(String),
    /// A valid setting value printed by the CLI, which may come from its cache.
    UnverifiedCliValue(String),
    /// Empty option output cannot distinguish an unset default from IPC failure.
    AmbiguousDefault,
    Unavailable,
    CommandFailed,
    InvalidOutput,
    TimedOut,
    OutputTooLarge,
    UnsupportedVersion,
}

pub fn parse_observation(query: Query, success: bool, stdout: &[u8], stderr: &[u8]) -> Observation {
    if !success || !stderr.is_empty() {
        return Observation::CommandFailed;
    }
    let Ok(value) = std::str::from_utf8(stdout) else {
        return Observation::InvalidOutput;
    };
    let value = value.trim();
    let setting = matches!(query, Query::ApproveMode | Query::VerificationMethod);
    if setting && value.is_empty() {
        return Observation::AmbiguousDefault;
    }
    let valid = match query {
        Query::Version => {
            let parts: Vec<_> = value.split('.').collect();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        }
        Query::Id => {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }
        Query::ApproveMode => matches!(value, "password" | "click" | "password-click"),
        Query::VerificationMethod => matches!(
            value,
            "use-temporary-password" | "use-permanent-password" | "use-both-passwords"
        ),
    };
    if valid && setting {
        Observation::UnverifiedCliValue(value.to_owned())
    } else if valid {
        Observation::Observed(value.to_owned())
    } else {
        Observation::InvalidOutput
    }
}
