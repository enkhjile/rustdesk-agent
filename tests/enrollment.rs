use rustdesk_agent::enrollment::Enrollment;
use zeroize::Zeroizing;

fn credential() -> Zeroizing<String> {
    Zeroizing::new("test-device-credential-0123456789abcdef".into())
}

#[test]
fn enrollment_retains_identity_and_redacts_debug() {
    let enrollment = Enrollment::new("https://management.example:8443", credential()).unwrap();
    let encoded = enrollment.encode().unwrap();
    let loaded = Enrollment::decode(&encoded).unwrap();
    assert_eq!(enrollment.info(), loaded.info());
    let info = serde_json::to_string(&loaded.info()).unwrap();
    assert!(!info.contains(credential().as_str()));
    assert!(!format!("{loaded:?}").contains(credential().as_str()));
    assert_ne!(
        enrollment.info().agent_uuid,
        Enrollment::new("https://management.example", credential())
            .unwrap()
            .info()
            .agent_uuid
    );
}

#[test]
fn rejects_unsafe_origins_and_credentials() {
    for origin in [
        "http://example.com",
        "https://user:pass@example.com",
        "https://example.com/path",
        "https://example.com/?secret=x",
        "https://example.com/#x",
        " https://example.com",
        "file:///tmp",
    ] {
        assert!(Enrollment::new(origin, credential()).is_err());
    }
    for token in [
        "short".to_string(),
        "x".repeat(257),
        "x".repeat(32) + "\n",
        "x".repeat(32) + " ",
    ] {
        assert!(Enrollment::new("https://example.com", Zeroizing::new(token)).is_err());
    }
}

#[test]
fn corrupt_or_unknown_enrollment_is_not_replaced() {
    for input in [b"not json".as_slice(), br#"{"schema_version":2}"#, br#"{}"#] {
        assert!(Enrollment::decode(input).is_err());
    }
    let value = Enrollment::new("https://example.com", credential()).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&value.encode().unwrap()).unwrap();
    json["unexpected"] = true.into();
    assert!(Enrollment::decode(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn stored_identity_requires_a_standard_v4_uuid() {
    let enrollment = Enrollment::new("https://example.com", credential()).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&enrollment.encode().unwrap()).unwrap();
    // Version nibble is 4, but the variant is not RFC 4122/9562.
    value["agent_uuid"] = "00000000-0000-4000-0000-000000000000".into();
    assert!(Enrollment::decode(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn rejects_unknown_version_and_duplicate_fields_in_complete_records() {
    let enrollment = Enrollment::new("https://example.com", credential()).unwrap();
    let encoded = enrollment.encode().unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    value["schema_version"] = 2.into();
    assert!(Enrollment::decode(&serde_json::to_vec(&value).unwrap()).is_err());
    let duplicate = std::str::from_utf8(&encoded).unwrap().replace(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
    );
    assert!(Enrollment::decode(duplicate.as_bytes()).is_err());
}
