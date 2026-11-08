//! Upstream proposal only; not linked into rustdesk-agent.
use hbb_common::tokio;
use std::{collections::HashMap, future::Future, time::Duration};

#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    Usage,
    Denied,
    IpcUnavailable,
    UnsupportedValue,
}

impl ReadError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage => 2,
            Self::Denied => 3,
            Self::IpcUnavailable => 4,
            Self::UnsupportedValue => 5,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::Usage => "usage: --get-settings-json",
            Self::Denied => "settings read denied",
            Self::IpcUnavailable => "live settings unavailable",
            Self::UnsupportedValue => "unsupported live setting value",
        }
    }
}

/// The callback must perform one live Options(None) request, without cache I/O.
pub async fn read_settings<F, Fut>(
    args: &[String],
    installed: bool,
    elevated: bool,
    disabled: bool,
    request: F,
) -> Result<String, ReadError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<HashMap<String, String>, ()>>,
{
    if args.len() != 1 || args[0] != "--get-settings-json" {
        return Err(ReadError::Usage);
    }
    if !installed || !elevated || disabled {
        return Err(ReadError::Denied);
    }
    let options = tokio::time::timeout(Duration::from_secs(1), request())
        .await
        .map_err(|_| ReadError::IpcUnavailable)?
        .map_err(|_| ReadError::IpcUnavailable)?;
    let approve = options.get("approve-mode");
    let verification = options.get("verification-method");
    if approve.is_some_and(|v| !matches!(v.as_str(), "" | "password" | "click" | "password-click"))
        || verification.is_some_and(|v| {
            !matches!(
                v.as_str(),
                "" | "use-temporary-password" | "use-permanent-password" | "use-both-passwords"
            )
        })
    {
        return Err(ReadError::UnsupportedValue);
    }
    Ok(serde_json::json!({
        "schema_version": 1,
        "source": "live_ipc",
        "settings": {"approve_mode": approve, "verification_method": verification}
    })
    .to_string())
}
