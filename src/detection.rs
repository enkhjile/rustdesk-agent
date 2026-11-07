//! Read-only CLI observations, never evidence that settings are live or applied.
use serde::Serialize;

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

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Observation {
    Observed(String),
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
    if valid {
        Observation::Observed(value.to_owned())
    } else {
        Observation::InvalidOutput
    }
}
