mod support;

use serde_json::{Value, json};
use support::{A, B, Fixture};

fn desired(revision: u64, mode: &str) -> Value {
    json!({"schema_version":1,"revision":revision,"settings":{"approve_mode":mode}})
}
fn report(sequence: u64) -> Value {
    json!({"schema_version":1,"agent_uuid":A,"rustdesk_id":null,
        "rustdesk_version":null,"desired_revision":7,"applied_revision":7,
        "observation_sequence":sequence,"results":{"approve_mode":"verified"}})
}

#[tokio::test]
async fn heartbeat_schema_requires_null_revision_and_empty_results() {
    let fixture = Fixture::start().await;
    let mut heartbeat = report(1);
    heartbeat["desired_revision"] = Value::Null;
    heartbeat["applied_revision"] = Value::Null;
    heartbeat["results"] = json!({});
    assert_eq!(fixture.post(&heartbeat).await.status(), 204);
    assert_eq!(fixture.post(&heartbeat).await.status(), 204);
    for (key, value) in [
        ("results", json!({"approve_mode":"unsupported"})),
        ("applied_revision", json!(7)),
    ] {
        let mut bad = heartbeat.clone();
        bad[key] = value;
        assert_eq!(fixture.post(&bad).await.status(), 400);
    }
    let mut missing = heartbeat.clone();
    missing.as_object_mut().unwrap().remove("desired_revision");
    assert_eq!(fixture.post(&missing).await.status(), 400);
    assert_eq!(
        fixture
            .post_raw(heartbeat.to_string().replace(
                "\"desired_revision\":null",
                "\"desired_revision\":null,\"desired_revision\":null"
            ))
            .await
            .status(),
        400
    );
    fixture.assign(A, desired(7, "click")).unwrap();
    assert_eq!(fixture.post(&heartbeat).await.status(), 409);
}

#[tokio::test]
async fn device_isolation_and_authentication() {
    let fixture = Fixture::start().await;
    fixture.assign(A, desired(7, "click")).unwrap();
    let response = fixture.get("test-a").await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap(), desired(7, "click"));
    assert_eq!(fixture.get("test-b").await.status(), 204);
    fixture.assign(B, desired(1, "password")).unwrap();
    assert_eq!(
        fixture.get("test-b").await.json::<Value>().await.unwrap(),
        desired(1, "password")
    );
    assert_eq!(fixture.get("wrong").await.status(), 401);
    let mut forged = report(1);
    forged["agent_uuid"] = json!(B);
    assert_eq!(fixture.post(&forged).await.status(), 403);
}

#[tokio::test]
async fn revisions_are_immutable() {
    let fixture = Fixture::start().await;
    fixture.assign(A, desired(7, "click")).unwrap();
    fixture.assign(A, desired(7, "click")).unwrap();
    assert!(fixture.assign(A, desired(6, "click")).is_err());
    assert!(fixture.assign(A, desired(7, "password")).is_err());
    fixture.assign(A, desired(8, "password")).unwrap();
    assert_eq!(
        fixture.get("test-a").await.json::<Value>().await.unwrap(),
        desired(8, "password")
    );
}

#[tokio::test]
async fn reports_are_idempotent_and_ordered() {
    let fixture = Fixture::start().await;
    fixture.assign(A, desired(7, "click")).unwrap();
    assert_eq!(fixture.post(&report(1)).await.status(), 204);
    assert_eq!(fixture.post(&report(1)).await.status(), 204);
    let mut newer = report(2);
    newer["applied_revision"] = Value::Null;
    newer["results"]["approve_mode"] = json!("mismatch");
    assert_eq!(fixture.post(&newer).await.status(), 204);
    assert_eq!(fixture.post(&report(1)).await.status(), 409);
    assert_eq!(fixture.post(&report(2)).await.status(), 409);
    assert_eq!(fixture.latest(A), Some(newer.clone()));
    fixture.assign(A, desired(8, "password")).unwrap();
    assert_eq!(fixture.post(&newer).await.status(), 409);
    let mut next_revision = report(1);
    next_revision["desired_revision"] = json!(8);
    next_revision["applied_revision"] = json!(8);
    assert_eq!(fixture.post(&next_revision).await.status(), 409);
    next_revision["observation_sequence"] = json!(3);
    assert_eq!(fixture.post(&next_revision).await.status(), 204);
    assert_eq!(fixture.latest(A), Some(next_revision));
}

#[tokio::test]
async fn invalid_reports_cannot_change_status() {
    let fixture = Fixture::start().await;
    fixture.assign(A, desired(7, "click")).unwrap();
    for (key, value, status) in [
        ("extra", json!(true), 400),
        ("applied_revision", json!(6), 400),
        ("results", json!({"approve_mode":"mismatch"}), 400),
        ("results", json!({"verification_method":"verified"}), 400),
        ("observation_sequence", json!(0), 400),
        ("agent_uuid", json!("invalid-uuid"), 400),
        ("desired_revision", json!(8), 409),
    ] {
        let mut value_report = report(1);
        value_report[key] = value;
        assert_eq!(fixture.post(&value_report).await.status(), status);
    }
    for body in [
        report(1).to_string().replace(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
        ),
        report(1).to_string().replace(
            "\"approve_mode\":\"verified\"",
            "\"approve_mode\":\"verified\",\"approve_mode\":\"verified\"",
        ),
        "not json".to_owned(),
    ] {
        assert_eq!(fixture.post_raw(body).await.status(), 400);
    }
    assert_eq!(fixture.post_raw("x".repeat(65537)).await.status(), 413);
    assert_eq!(
        fixture
            .client
            .get(format!("{}/commands", fixture.url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(fixture.latest(A), None);
}

#[tokio::test]
async fn desired_state_uses_the_schema_and_rust_parser() {
    let fixture = Fixture::start().await;
    for settings in [
        json!({}),
        json!({"approve_mode":null}),
        json!({"command":"anything"}),
    ] {
        let mut state = desired(7, "click");
        state["settings"] = settings;
        assert!(fixture.assign(A, state).is_err());
    }
    assert_eq!(fixture.get("test-a").await.status(), 204);
}

#[tokio::test]
async fn untrusted_certificate_is_rejected() {
    let fixture = Fixture::start().await;
    let untrusted = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();
    let error = untrusted
        .get(format!("{}/v1/agent/desired", fixture.url))
        .send()
        .await
        .unwrap_err();
    assert!(error.is_connect());
    // Prove the listener is healthy with the explicitly trusted client.
    assert_eq!(fixture.get("test-a").await.status(), 204);
}
