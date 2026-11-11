use super::*;

#[tokio::test]
async fn password_retry_after_extends_service_delay() {
    let extra = axum::Router::new().route(
        "/v1/agent/password/claim",
        axum::routing::post(|| async { (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "300")]) }),
    );
    let fixture = support::Fixture::start_with_routes(extra).await;
    let mut client = poller(&fixture);
    let management = client.poll(None, None).await;
    assert_eq!(
        client
            .poll_password(|_| async { panic!("must not execute") })
            .await,
        PollStatus::HttpError { status: 429 }
    );
    assert_eq!(client.next_poll_seconds(&management), 300);
}

#[tokio::test]
async fn password_claim_backoff_survives_healthy_settings_and_resets_on_204() {
    use std::sync::{Arc, Mutex};
    let response = Arc::new(Mutex::new(StatusCode::SERVICE_UNAVAILABLE));
    let served = response.clone();
    let extra = axum::Router::new().route(
        "/v1/agent/password/claim",
        axum::routing::post(move || {
            let served = served.clone();
            async move { *served.lock().unwrap() }
        }),
    );
    let fixture = support::Fixture::start_with_routes(extra).await;
    let mut client = poller(&fixture);
    for failures in 0..9 {
        let observation = client.poll(None, None).await;
        assert_eq!(observation.status, PollStatus::Unassigned);
        assert_eq!(
            client
                .poll_password(|_| async { panic!("must not execute") })
                .await,
            PollStatus::HttpError { status: 503 }
        );
        let ceiling = backoff_seconds(failures);
        assert!(
            ((ceiling * 4 / 5).max(60)..=ceiling.max(60))
                .contains(&client.next_poll_seconds(&observation))
        );
    }
    *response.lock().unwrap() = StatusCode::NO_CONTENT;
    let observation = client.poll(None, None).await;
    assert_eq!(
        client
            .poll_password(|_| async { panic!("must not execute") })
            .await,
        PollStatus::Unassigned
    );
    assert_eq!(client.next_poll_seconds(&observation), 60);
    *response.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    client
        .poll_password(|_| async { panic!("must not execute") })
        .await;
    assert_eq!(client.next_poll_seconds(&observation), 60);
}

#[tokio::test]
async fn password_retry_after_filters_status_and_invalid_headers() {
    for (code, value, expected) in [
        (503, "999999", 300),
        (503, "invalid", 60),
        (401, "300", 60),
        (403, "300", 60),
        (429, "0", 60),
    ] {
        let extra = axum::Router::new().route(
            "/v1/agent/password/claim",
            axum::routing::post(move || async move {
                (
                    StatusCode::from_u16(code).unwrap(),
                    [("retry-after", value)],
                )
            }),
        );
        let fixture = support::Fixture::start_with_routes(extra).await;
        let mut client = poller(&fixture);
        let observation = client.poll(None, None).await;
        assert_eq!(
            client
                .poll_password(|_| async { panic!("must not execute") })
                .await,
            PollStatus::HttpError { status: code }
        );
        assert_eq!(client.next_poll_seconds(&observation), expected);
    }
}

#[tokio::test]
async fn password_result_retries_honor_dates_without_reexecuting() {
    use std::sync::{Arc, Mutex};
    let receipts = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let received = receipts.clone();
    let id = Uuid::new_v4();
    let extra = axum::Router::new()
        .route("/v1/agent/password/claim", axum::routing::post(move || async move {
            axum::Json(json!({"schema_version":1,"operation_id":id,"sequence":1,
                "expires_at":SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()+900,
                "password":"dummy-test-password"}))
        }))
        .route("/v1/agent/password/result", axum::routing::post(move |body: axum::body::Bytes| {
            let received = received.clone();
            async move {
                let mut received = received.lock().unwrap();
                received.push(serde_json::from_slice(&body).unwrap());
                match received.len() {
                    1 => (StatusCode::SERVICE_UNAVAILABLE, [("retry-after", "999999".to_owned())]),
                    2 => (StatusCode::TOO_MANY_REQUESTS, [("retry-after",
                        httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(120)))]),
                    3..=6 => (StatusCode::SERVICE_UNAVAILABLE, [("retry-after", "invalid".to_owned())]),
                    _ => (StatusCode::NO_CONTENT, [("retry-after", "300".to_owned())]),
                }
            }
        }));
    let fixture = support::Fixture::start_with_routes(extra).await;
    let mut client = poller(&fixture);
    let observation = client.poll(None, None).await;
    assert_eq!(
        client
            .poll_password(|_| async { crate::password::Outcome::CommandAccepted })
            .await,
        PollStatus::DeliveryRejected { status: 503 }
    );
    assert_eq!(client.next_poll_seconds(&observation), 300);
    assert_eq!(
        client
            .poll_password(|_| async { panic!("must not replay") })
            .await,
        PollStatus::DeliveryRejected { status: 429 }
    );
    assert!((115..=120).contains(&client.next_poll_seconds(&observation)));
    for _ in 0..4 {
        assert_eq!(
            client
                .poll_password(|_| async { panic!("must not replay") })
                .await,
            PollStatus::DeliveryRejected { status: 503 }
        );
    }
    assert_eq!(
        client
            .poll_password(|_| async { panic!("must not replay") })
            .await,
        PollStatus::Delivered
    );
    assert_eq!(client.next_poll_seconds(&observation), 60);
    // Successful acknowledgement resets the failure counter, too.
    assert!(matches!(
        client
            .poll_password(|_| async { panic!("must not replay") })
            .await,
        PollStatus::Rejected {
            reason: "password_operation_conflict"
        }
    ));
    assert_eq!(client.next_poll_seconds(&observation), 60);
    let received = receipts.lock().unwrap();
    assert_eq!(received.len(), 7);
    assert!(received.iter().all(|receipt| receipt == &received[0]));
}

#[tokio::test]
async fn password_transport_backoff_never_shortens_management_delay() {
    let fixture = support::Fixture::start().await;
    let enrolled = enrollment(&fixture.url);
    let store = crate::state::tests::MemoryStore::new(&enrolled.info());
    // Default client rejects the fixture's untrusted TLS certificate.
    let mut client = Poller::new(enrolled, store).unwrap();
    let mut observation = PollObservation {
        status: PollStatus::Unassigned,
        delivery: None,
        accepted_revision: None,
        next_poll_seconds: 60,
    };
    for _ in 0..8 {
        assert_eq!(
            client
                .poll_password(|_| async { panic!("must not execute") })
                .await,
            PollStatus::TransportError
        );
    }
    assert!((240..=300).contains(&client.next_poll_seconds(&observation)));
    observation.next_poll_seconds = 300;
    assert_eq!(client.next_poll_seconds(&observation), 300);
}
