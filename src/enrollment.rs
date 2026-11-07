//! Validated local enrollment. No server registration or credential rotation.
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Enrollment {
    schema_version: u32,
    agent_uuid: Uuid,
    management_origin: Url,
    credential: Zeroizing<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct EnrollmentInfo {
    pub agent_uuid: Uuid,
    pub management_origin: Url,
}

impl std::fmt::Debug for Enrollment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enrollment")
            .field("info", &self.info())
            .field("credential", &"[REDACTED]")
            .finish()
    }
}

impl Enrollment {
    pub fn new(origin: &str, credential: Zeroizing<String>) -> Result<Self, &'static str> {
        if origin.trim() != origin || origin.len() > 2048 || origin.chars().any(char::is_control) {
            return Err("invalid_management_origin");
        }
        let value = Self {
            schema_version: 1,
            agent_uuid: Uuid::new_v4(),
            management_origin: Url::parse(origin).map_err(|_| "invalid_management_origin")?,
            credential,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), &'static str> {
        let origin = &self.management_origin;
        if self.schema_version != 1 || self.agent_uuid.get_version_num() != 4 {
            return Err("invalid_enrollment");
        }
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.as_str().len() > 2048
        {
            return Err("invalid_management_origin");
        }
        if !(32..=256).contains(&self.credential.len())
            || !self
                .credential
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
        {
            return Err("invalid_device_credential");
        }
        Ok(())
    }
    pub fn info(&self) -> EnrollmentInfo {
        EnrollmentInfo {
            agent_uuid: self.agent_uuid,
            management_origin: self.management_origin.clone(),
        }
    }
    /// Plaintext containing the credential: only pass to protected storage.
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, &'static str> {
        serde_json::to_vec(self)
            .map(Zeroizing::new)
            .map_err(|_| "invalid_enrollment")
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > 4096 {
            return Err("invalid_enrollment");
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| "invalid_enrollment")?;
        value.validate()?;
        Ok(value)
    }
}
