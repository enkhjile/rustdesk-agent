use super::*;
use axum::http::{HeaderMap, StatusCode};
use serde_json::json;
use zeroize::Zeroizing;

#[path = "../tests/support/mod.rs"]
#[allow(dead_code)]
mod support;

fn enrollment(origin: &str) -> Enrollment {
    Enrollment::new(
        origin,
        Zeroizing::new("disposable-test-credential-device-a".into()),
    )
    .unwrap()
}
fn poller(fixture: &support::Fixture) -> Poller {
    let mut poller = Poller::new(enrollment(&fixture.url)).unwrap();
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
        let observation = poller.poll().await;
        assert_eq!(observation.status, PollStatus::Accepted { revision: 7 });
        assert_eq!(observation.accepted_revision, Some(7));
        assert_eq!(observation.next_poll_seconds, 60);
    }
    for (body, reason) in [
        (desired(6, "click"), "stale_revision"),
        (desired(7, "password"), "revision_conflict"),
    ] {
        respond(&fixture, 200, &body);
        assert_eq!(poller.poll().await.status, PollStatus::Rejected { reason });
    }
    respond(&fixture, 204, "");
    let observation = poller.poll().await;
    assert_eq!(observation.status, PollStatus::Unassigned);
    assert_eq!(observation.accepted_revision, Some(7));
    assert_eq!(observation.next_poll_seconds, 60);
    assert!(fixture.latest(support::A).is_none());
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
        let observation = poller.poll().await;
        assert!(matches!(observation.status, PollStatus::Rejected { .. }));
        assert_eq!(observation.accepted_revision, None);
    }
    fixture.respond(
        StatusCode::OK,
        HeaderMap::new(),
        desired(1, "click").into_bytes(),
    );
    assert_eq!(
        poller.poll().await.status,
        PollStatus::Rejected {
            reason: "invalid_content_type"
        }
    );
    respond(&fixture, 200, &" ".repeat(65537));
    assert_eq!(
        poller.poll().await.status,
        PollStatus::Rejected {
            reason: "response_too_large"
        }
    );
}

#[tokio::test]
async fn rejects_untrusted_tls_and_invalid_credentials() {
    let fixture = support::Fixture::start().await;
    let mut untrusted = Poller::new(enrollment(&fixture.url)).unwrap();
    assert_eq!(untrusted.poll().await.status, PollStatus::TransportError);
    let mut wrong = poller(&fixture);
    wrong.enrollment = Enrollment::new(
        &fixture.url,
        Zeroizing::new("different-disposable-test-credential".into()),
    )
    .unwrap();
    assert_eq!(
        wrong.poll().await.status,
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
        poller.poll().await.status,
        PollStatus::HttpError { status: 307 }
    );
    for code in [401, 403, 429, 500, 503] {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "999999".parse().unwrap());
        fixture.respond(StatusCode::from_u16(code).unwrap(), headers, vec![]);
        let observation = poller.poll().await;
        assert_eq!(observation.status, PollStatus::HttpError { status: code });
        assert!((5..=300).contains(&observation.next_poll_seconds));
        if code == 429 || code == 503 {
            assert_eq!(observation.next_poll_seconds, 300);
        }
    }
    respond(&fixture, 204, "");
    assert_eq!(poller.poll().await.next_poll_seconds, 60);
    respond(&fixture, 500, "");
    assert_eq!(poller.poll().await.next_poll_seconds, 5);
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
    let mut poller = Poller::new(enrollment(&origin)).unwrap();
    // Keep the TCP peer open without completing TLS.
    let peer = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    assert_eq!(poller.poll().await.status, PollStatus::Timeout);
    assert!(poller.guard.accepted().is_none());
    peer.abort();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("https://{}", listener.local_addr().unwrap());
    let mut poller = Poller::new(enrollment(&origin)).unwrap();
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
            _ = poller.poll() => panic!("request should still be waiting for TLS"),
        }
    })
    .await
    .unwrap();
    assert!(poller.guard.accepted().is_none());
    assert_eq!(poller.failures, 0);
    peer.abort();
}

#[tokio::test]
async fn diagnostics_never_include_response_content_or_credentials() {
    let fixture = support::Fixture::start().await;
    let mut poller = poller(&fixture);
    respond(&fixture, 200, "disposable-test-credential-device-a");
    let observation = poller.poll().await;
    for text in [
        serde_json::to_string(&observation).unwrap(),
        format!("{observation:?}"),
    ] {
        assert!(!text.contains("disposable-test-credential"));
        assert!(!text.contains(&fixture.url));
        assert!(!text.contains("applied_revision"));
    }
}
