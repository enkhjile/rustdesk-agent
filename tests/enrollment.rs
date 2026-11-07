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
