use super::*;
use axum::http::{HeaderMap, StatusCode};
use serde_json::json;
use zeroize::Zeroizing;

#[path = "../tests/support/mod.rs"]
#[allow(dead_code)]
mod support;

fn enrollment(origin: &str) -> Enrollment {
    Enrollment::decode(serde_json::to_string(&json!({"schema_version":1,"agent_uuid":support::A,"management_origin":format!("{}/", origin.trim_end_matches('/')),"credential":"disposable-test-credential-device-a"})).unwrap().as_bytes()).unwrap()
}
fn poller(fixture: &support::Fixture) -> Poller {
    let mut poller = Poller::new(
        enrollment(&fixture.url),
        crate::state::tests::MemoryStore::new(&enrollment(&fixture.url).info()),
    )
    .unwrap();
    poller.client = client_builder()
        .add_root_certificate(fixture.certificate.clone())
        .build()
        .unwrap();
    poller
}
fn respond(fixture: &support::Fixture, code: u16, body: &str) {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    fixture.respond(
        StatusCode::from_u16(code).unwrap(),
        headers,
        body.as_bytes().to_vec(),
    );
}
fn desired(revision: u64, mode: &str) -> String {
    json!({"schema_version":1,"revision":revision,"settings":{"approve_mode":mode}}).to_string()
}

#[tokio::test]
async fn polls_authenticated_state_without_applying_and_retains_watermark() {
    let fixture = support::Fixture::start().await;
    fixture
        .assign(
            support::A,
            serde_json::from_str(&desired(7, "click")).unwrap(),
        )
        .unwrap();
    let mut poller = poller(&fixture);
    for _ in 0..2 {
        let observation = poller.poll(None, None).await;
        assert_eq!(observation.status, PollStatus::Accepted { revision: 7 });
        assert_eq!(observation.accepted_revision, Some(7));
        assert_eq!(observation.next_poll_seconds, 60);
    }
    for (body, reason) in [
        (desired(6, "click"), "stale_revision"),
        (desired(7, "password"), "revision_conflict"),
    ] {
        respond(&fixture, 200, &body);
        assert_eq!(
            poller.poll(None, None).await.status,
            PollStatus::Rejected { reason }
        );
    }
    respond(&fixture, 204, "");
    let observation = poller.poll(None, None).await;
    assert_eq!(observation.status, PollStatus::Unassigned);
    assert_eq!(observation.accepted_revision, Some(7));
    assert_eq!(observation.next_poll_seconds, 60);
    assert_eq!(
        fixture.latest(support::A).unwrap()["applied_revision"],
        serde_json::Value::Null
    );
}

#[tokio::test]
async fn rejects_bad_payloads_and_wrong_content_type_without_acceptance() {
    let fixture = support::Fixture::start().await;
    let mut poller = poller(&fixture);
    for body in [
        "not json".into(),
        desired(0, "click"),
        desired(1, "unknown"),
        r#"{"schema_version":1,"revision":1,"revision":2,"settings":{"approve_mode":"click"}}"#
            .into(),
    ] {
        respond(&fixture, 200, &body);
        let observation = poller.poll(None, None).await;
        assert!(matches!(observation.status, PollStatus::Rejected { .. }));
        assert_eq!(observation.accepted_revision, None);
    }
    fixture.respond(
        StatusCode::OK,
        HeaderMap::new(),
        desired(1, "click").into_bytes(),
    );
    assert_eq!(
        poller.poll(None, None).await.status,
        PollStatus::Rejected {
            reason: "invalid_content_type"
        }
    );
    respond(&fixture, 200, &" ".repeat(65537));
    assert_eq!(
        poller.poll(None, None).await.status,
        PollStatus::Rejected {
            reason: "response_too_large"
        }
    );
}

#[tokio::test]
async fn rejects_untrusted_tls_and_invalid_credentials() {
    let fixture = support::Fixture::start().await;
    let mut untrusted = Poller::new(
        enrollment(&fixture.url),
        crate::state::tests::MemoryStore::new(&enrollment(&fixture.url).info()),
    )
    .unwrap();
    assert_eq!(
        untrusted.poll(None, None).await.status,
        PollStatus::TransportError
    );
    let mut wrong = poller(&fixture);
    wrong.enrollment = Enrollment::new(
        &fixture.url,
        Zeroizing::new("different-disposable-test-credential".into()),
    )
    .unwrap();
    assert_eq!(
        wrong.poll(None, None).await.status,
        PollStatus::HttpError { status: 401 }
    );
}

#[tokio::test]
async fn never_follows_redirects_and_bounds_error_retries() {
    let fixture = support::Fixture::start().await;
    let destination = support::Fixture::start().await;
    destination
        .assign(
            support::A,
            serde_json::from_str(&desired(1, "click")).unwrap(),
        )
        .unwrap();
    let mut poller = poller(&fixture);
    let mut headers = HeaderMap::new();
    headers.insert(
        "location",
        format!("{}/v1/agent/desired", destination.url)
            .parse()
            .unwrap(),
    );
    fixture.respond(StatusCode::TEMPORARY_REDIRECT, headers, vec![]);
    assert_eq!(
        poller.poll(None, None).await.status,
        PollStatus::HttpError { status: 307 }
    );
    for code in [401, 403, 429, 500, 503] {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "999999".parse().unwrap());
        fixture.respond(StatusCode::from_u16(code).unwrap(), headers, vec![]);
        let observation = poller.poll(None, None).await;
        assert_eq!(observation.status, PollStatus::HttpError { status: code });
        assert!((5..=300).contains(&observation.next_poll_seconds));
        if code == 429 || code == 503 {
            assert_eq!(observation.next_poll_seconds, 300);
        }
    }
    respond(&fixture, 204, "");
    assert_eq!(poller.poll(None, None).await.next_poll_seconds, 60);
    respond(&fixture, 500, "");
    assert_eq!(poller.poll(None, None).await.next_poll_seconds, 5);
}

#[test]
fn retry_delay_and_retry_after_are_bounded() {
    for failures in [0, 1, 2, 6, u32::MAX] {
        for random in [0, 1, u128::MAX] {
            let seconds = retry_delay(failures, None, random);
            assert!((5..=300).contains(&seconds));
        }
    }
    assert_ne!(retry_delay(100, None, 0), retry_delay(100, None, 1));
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    assert_eq!(parse_retry_after("999999", now), Some(300));
    assert_eq!(parse_retry_after("invalid", now), None);
    assert_eq!(
        parse_retry_after(
            &httpdate::fmt_http_date(now + Duration::from_secs(120)),
            now
        ),
        Some(120)
    );
    assert_eq!(
        parse_retry_after(&httpdate::fmt_http_date(now - Duration::from_secs(1)), now),
        Some(0)
    );
}

#[tokio::test]
async fn stalled_connection_times_out_and_poll_can_be_cancelled() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("https://{}", listener.local_addr().unwrap());
    let mut poller = Poller::new(
        enrollment(&origin),
        crate::state::tests::MemoryStore::new(&enrollment(&origin).info()),
    )
    .unwrap();
    // Keep the TCP peer open without completing TLS.
    let peer = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    assert_eq!(poller.poll(None, None).await.status, PollStatus::Timeout);
    assert!(poller.state.accepted.as_ref().is_none());
    peer.abort();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("https://{}", listener.local_addr().unwrap());
    let mut poller = Poller::new(
        enrollment(&origin),
        crate::state::tests::MemoryStore::new(&enrollment(&origin).info()),
    )
    .unwrap();
    let (stop_tx, mut stop) = tokio::sync::watch::channel(false);
    let peer = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        stop_tx.send(true).unwrap();
        std::future::pending::<()>().await;
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        tokio::select! {
            biased;
            _ = stop.changed() => (),
            _ = poller.poll(None, None) => panic!("request should still be waiting for TLS"),
        }
    })
    .await
    .unwrap();
    assert!(poller.state.accepted.as_ref().is_none());
    assert_eq!(poller.failures, 0);
    peer.abort();
}

#[tokio::test]
async fn diagnostics_never_include_response_content_or_credentials() {
    let fixture = support::Fixture::start().await;
    let mut poller = poller(&fixture);
    respond(&fixture, 200, "disposable-test-credential-device-a");
    let observation = poller.poll(None, None).await;
    for text in [
        serde_json::to_string(&observation).unwrap(),
        format!("{observation:?}"),
    ] {
        assert!(!text.contains("disposable-test-credential"));
        assert!(!text.contains(&fixture.url));
        assert!(!text.contains("applied_revision"));
    }
}

fn restored_poller(
    fixture: &support::Fixture,
    store: impl crate::state::StateStore + 'static,
) -> Poller {
    let mut poller = Poller::new(enrollment(&fixture.url), store).unwrap();
    poller.client = client_builder()
        .add_root_certificate(fixture.certificate.clone())
        .build()
        .unwrap();
    poller
}

#[tokio::test]
async fn managed_poll_reports_unverified_partial_results_and_retries_without_reapplying() {
    use crate::{Setting, state::SettingResult};
    let fixture = support::Fixture::start().await;
    fixture
        .assign(
            support::A,
            json!({"schema_version":1,"revision":7,"settings":{
                "approve_mode":"click", "verification_method":"use-temporary-password"
            }}),
        )
        .unwrap();
    let store = crate::state::tests::MemoryStore::new(&enrollment(&fixture.url).info());
    let mut poller = restored_poller(&fixture, store.clone());
    fixture.status_response(Some(StatusCode::SERVICE_UNAVAILABLE));
    let mut calls = vec![];
    let result = poller
        .poll_applying(None, None, |key, _| {
            let saved =
                DurableState::decode(&store.0.lock().unwrap(), &enrollment(&fixture.url).info())
                    .unwrap();
            assert_eq!(saved.accepted.unwrap().revision, 7);
            calls.push(key);
            std::future::ready(if key == Setting::ApproveMode {
                SettingResult::UnverifiedCliMatch
            } else {
                SettingResult::WriteFailed
            })
        })
        .await;
    assert_eq!(result.delivery, Some(PollStatus::HttpError { status: 503 }));
    assert_eq!(calls.len(), 2);
    let first = fixture.attempts()[0].clone();
    let report: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(report["results"]["approve_mode"], "unverified_cli_match");
    assert_eq!(report["results"]["verification_method"], "write_failed");
    assert!(report["applied_revision"].is_null());
    drop(poller);
    let mut poller = restored_poller(&fixture, store);
    fixture.status_response(None);
    respond(&fixture, 204, "");
    poller
        .poll_applying(None, None, |_, _| {
            panic!("report delivery must not apply cached desired state");
            #[allow(unreachable_code)]
            std::future::ready(SettingResult::WriteFailed)
        })
        .await;
    assert_eq!(fixture.attempts()[1], first);
}

#[tokio::test]
async fn report_retry_survives_restart_and_new_observation_supersedes_it() {
    use crate::state::tests::MemoryStore;
    let fixture = support::Fixture::start().await;
    fixture
        .assign(
            support::A,
            serde_json::from_str(&desired(7, "click")).unwrap(),
        )
        .unwrap();
    fixture.status_response(Some(StatusCode::SERVICE_UNAVAILABLE));
    let store = MemoryStore::new(&enrollment(&fixture.url).info());
    let mut poller = restored_poller(&fixture, store.clone());
    let result = poller.poll(Some("123".into()), Some("1.4.7".into())).await;
    assert_eq!(result.delivery, Some(PollStatus::HttpError { status: 503 }));
    let first = fixture.attempts()[0].clone();
    let report: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(report["observation_sequence"], 1);
    assert_eq!(report["results"]["approve_mode"], "unsupported");
    assert_eq!(report["applied_revision"], serde_json::Value::Null);
    drop(poller);
    let mut poller = restored_poller(&fixture, store.clone());
    fixture.status_response(None);
    respond(&fixture, 204, "");
    assert_eq!(
        poller.poll(None, None).await.delivery,
        Some(PollStatus::Delivered)
    );
    assert_eq!(fixture.attempts()[1], first);
    assert!(poller.state.pending.is_none());
    drop(poller);
    let mut poller = restored_poller(&fixture, store);
    respond(&fixture, 200, &desired(6, "click"));
    assert_eq!(
        poller.poll(None, None).await.status,
        PollStatus::Rejected {
            reason: "stale_revision"
        }
    );
    respond(&fixture, 200, &desired(7, "password"));
    assert_eq!(
        poller.poll(None, None).await.status,
        PollStatus::Rejected {
            reason: "revision_conflict"
        }
    );
    respond(&fixture, 200, &desired(7, "click"));
    poller.poll(None, None).await;
    let report: serde_json::Value =
        serde_json::from_slice(fixture.attempts().last().unwrap()).unwrap();
    assert_eq!(report["observation_sequence"], 2);
}

struct FailSave {
    store: crate::state::tests::MemoryStore,
    remaining: usize,
}

#[tokio::test]
async fn managed_rejects_unsafe_inputs_and_reobserves_equal_and_higher_revisions() {
    use crate::state::SettingResult;
    let fixture = support::Fixture::start().await;
    let mut poller = poller(&fixture);
    let mut calls = 0;
    for revision in [7, 7, 8] {
        respond(&fixture, 200, &desired(revision, "click"));
        assert_eq!(
            poller
                .poll_applying(None, None, |_, _| {
                    calls += 1;
                    std::future::ready(SettingResult::UnverifiedCliMatch)
                })
                .await
                .status,
            PollStatus::Accepted { revision }
        );
    }
    assert_eq!(calls, 3); // Equal revisions still inspect drift; higher revisions progress.
    for (code, body) in [
        (204, "".into()),
        (401, "".into()),
        (403, "".into()),
        (503, "".into()),
        (200, "not json".into()),
        (200, desired(7, "click")),
        (200, desired(8, "password")),
    ] {
        respond(&fixture, code, &body);
        poller
            .poll_applying(None, None, |_, _| {
                calls += 1;
                std::future::ready(SettingResult::WriteFailed)
            })
            .await;
    }
    assert_eq!(calls, 3);
    poller.enrollment = enrollment("https://example.test");
    assert_eq!(
        poller
            .poll_applying(None, None, |_, _| {
                calls += 1;
                std::future::ready(SettingResult::WriteFailed)
            })
            .await
            .status,
        PollStatus::Rejected {
            reason: "lab_requires_loopback_origin"
        }
    );
    assert_eq!(calls, 3);
}

#[tokio::test]
async fn managed_storage_failure_stops_writes_and_cancellation_retains_uncertainty() {
    use crate::state::SettingResult;
    let fixture = support::Fixture::start().await;
    respond(
        &fixture,
        200,
        &json!({"schema_version":1,"revision":7,"settings":{
            "approve_mode":"click", "verification_method":"use-temporary-password"
        }})
        .to_string(),
    );
    for (remaining, expected_calls) in [(0, 0), (1, 1)] {
        let store = crate::state::tests::MemoryStore::new(&enrollment(&fixture.url).info());
        let mut poller = restored_poller(&fixture, FailSave { store, remaining });
        let mut calls = 0;
        for _ in 0..2 {
            assert_eq!(
                poller
                    .poll_applying(None, None, |_, _| {
                        calls += 1;
                        std::future::ready(SettingResult::UnverifiedCliMatch)
                    })
                    .await
                    .status,
                PollStatus::StorageError
            );
        }
        assert_eq!(calls, expected_calls);
    }
    let store = crate::state::tests::MemoryStore::new(&enrollment(&fixture.url).info());
    let mut poller = restored_poller(&fixture, store.clone());
    let mut started = false;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            poller.poll_applying(None, None, |_, _| {
                started = true;
                std::future::pending::<SettingResult>()
            })
        )
        .await
        .is_err()
    );
    assert!(started);
    drop(poller);
    let mut restored = restored_poller(&fixture, store.clone());
    let pending = serde_json::to_value(restored.state.pending.as_ref().unwrap()).unwrap();
    assert_eq!(pending["results"]["approve_mode"], "indeterminate");
    assert_eq!(pending["results"]["verification_method"], "indeterminate");
    assert!(pending["applied_revision"].is_null());
    assert!(fixture.attempts().is_empty());

    let desired = serde_json::to_value(restored.state.accepted.as_ref().unwrap()).unwrap();
    fixture.assign(support::A, desired.clone()).unwrap();
    respond(&fixture, 204, "");
    fixture.status_response(Some(StatusCode::SERVICE_UNAVAILABLE));
    let mut calls = 0;
    restored
        .poll_applying(None, None, |_, _| {
            calls += 1;
            std::future::ready(SettingResult::UnverifiedCliMatch)
        })
        .await;
    let first = fixture.attempts()[0].clone();
    drop(restored);
    let mut restored = restored_poller(&fixture, store);
    fixture.status_response(None);
    restored
        .poll_applying(None, None, |_, _| {
            calls += 1;
            std::future::ready(SettingResult::UnverifiedCliMatch)
        })
        .await;
    assert_eq!(
        calls, 0,
        "interrupted reports must not reapply cached state"
    );
    assert_eq!(fixture.attempts()[1], first);
    assert_eq!(fixture.latest(support::A).unwrap(), pending);

    respond(&fixture, 200, &desired.to_string());
    restored
        .poll_applying(None, None, |_, _| {
            calls += 1;
            std::future::ready(SettingResult::UnverifiedCliMatch)
        })
        .await;
    let resumed = fixture.latest(support::A).unwrap();
    assert_eq!(calls, 2);
    assert_eq!(
        resumed["observation_sequence"].as_u64().unwrap(),
        pending["observation_sequence"].as_u64().unwrap() + 1
    );
    assert!(resumed["applied_revision"].is_null());
    assert_eq!(resumed["results"]["approve_mode"], "unverified_cli_match");
    assert_eq!(
        resumed["results"]["verification_method"],
        "unverified_cli_match"
    );
}
impl crate::state::StateStore for FailSave {
    fn load(&mut self) -> std::io::Result<Vec<u8>> {
        self.store.load()
    }
    fn save(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if self.remaining == 0 {
            return Err(std::io::Error::other("injected storage failure"));
        }
        self.remaining -= 1;
        self.store.save(bytes)
    }
}
#[tokio::test]
async fn save_failure_prevents_delivery_and_ack_failure_retries_identically() {
    let fixture = support::Fixture::start().await;
    fixture
        .assign(
            support::A,
            serde_json::from_str(&desired(7, "click")).unwrap(),
        )
        .unwrap();
    let store = crate::state::tests::MemoryStore::new(&enrollment(&fixture.url).info());
    let mut poller = restored_poller(
        &fixture,
        FailSave {
            store: store.clone(),
            remaining: 0,
        },
    );
    assert_eq!(
        poller.poll(None, None).await.status,
        PollStatus::StorageError
    );
    assert!(fixture.attempts().is_empty());
    assert_eq!(
        poller.poll(None, None).await.status,
        PollStatus::StorageError
    );
    let mut poller = restored_poller(
        &fixture,
        FailSave {
            store: store.clone(),
            remaining: 1,
        },
    );
    assert_eq!(
        poller.poll(None, None).await.status,
        PollStatus::StorageError
    );
    assert!(fixture.latest(support::A).is_some());
    let first = fixture.attempts()[0].clone();
    let mut poller = restored_poller(&fixture, store);
    respond(&fixture, 204, "");
    assert_eq!(
        poller.poll(None, None).await.delivery,
        Some(PollStatus::Delivered)
    );
    assert_eq!(fixture.attempts()[1], first);
}
#[tokio::test]
async fn terminal_report_rejections_are_not_replayed_without_new_observation() {
    for code in [400, 403, 409] {
        let fixture = support::Fixture::start().await;
        fixture
            .assign(
                support::A,
                serde_json::from_str(&desired(7, "click")).unwrap(),
            )
            .unwrap();
        fixture.status_response(Some(StatusCode::from_u16(code).unwrap()));
        let mut poller = poller(&fixture);
        assert_eq!(
            poller.poll(None, None).await.delivery,
            Some(PollStatus::DeliveryRejected { status: code })
        );
        respond(&fixture, 204, "");
        assert_eq!(poller.poll(None, None).await.delivery, None);
        assert_eq!(fixture.attempts().len(), 1);
    }
}
