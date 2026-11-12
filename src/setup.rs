//! Authenticated, retryable binding of an already persisted device identity.
use crate::{enrollment::Enrollment, management::client_builder};

pub async fn bind(enrollment: &Enrollment) -> Result<(), &'static str> {
    let client = client_builder()
        .build()
        .map_err(|_| "Cannot initialize HTTPS")?;
    bind_with(&client, enrollment).await
}

async fn bind_with(client: &reqwest::Client, enrollment: &Enrollment) -> Result<(), &'static str> {
    let info = enrollment.info();
    let response = client
        .post(info.management_origin.join("v1/agent/enroll").unwrap())
        .bearer_auth(enrollment.credential())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(serde_json::json!({"agent_uuid": info.agent_uuid}).to_string())
        .send()
        .await
        .map_err(|_| "Console enrollment request failed; check HTTPS connectivity and retry")?;
    match response.status().as_u16() {
        204 => Ok(()),
        401 | 403 => {
            Err("Console rejected enrollment; check the device credential and device status")
        }
        409 => Err("Console credential is already bound to another agent"),
        _ => Err("Console did not accept enrollment; check its version and retry"),
    }
}

#[cfg(windows)]
pub async fn wait_for_check_in(enrollment: &Enrollment) -> Result<(), &'static str> {
    let client = client_builder()
        .build()
        .map_err(|_| "Cannot initialize HTTPS")?;
    for _ in 0..15 {
        let response = client
            .get(
                enrollment
                    .info()
                    .management_origin
                    .join("v1/agent/enroll")
                    .unwrap(),
            )
            .bearer_auth(enrollment.credential())
            .send()
            .await
            .map_err(|_| "Cannot confirm Console check-in; service remains installed")?;
        match response.status().as_u16() {
            204 => return Ok(()),
            202 => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
            _ => return Err("Console check-in rejected; service remains installed"),
        }
    }
    Err(
        "Console check-in not yet confirmed; service remains installed. Check Console before assigning policies",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Bytes,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn binding_uses_persisted_identity_and_rejects_redirects_and_errors() {
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let tls = axum_server::tls_rustls::RustlsConfig::from_der(
            vec![certificate.cert.der().to_vec()],
            certificate.signing_key.serialize_der(),
        )
        .await
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let capture = observed.clone();
        let code = Arc::new(Mutex::new(StatusCode::NO_CONTENT));
        let status = code.clone();
        let app = Router::new().route(
            "/v1/agent/enroll",
            post(move |headers: HeaderMap, body: Bytes| {
                let capture = capture.clone();
                let status = status.clone();
                async move {
                    capture.lock().unwrap().push((headers, body));
                    (
                        *status.lock().unwrap(),
                        [("location", "https://localhost:1/secret")],
                    )
                }
            }),
        );
        let server = tokio::spawn(
            axum_server::from_tcp_rustls(listener, tls)
                .unwrap()
                .serve(app.into_make_service()),
        );
        let client = client_builder()
            .add_root_certificate(reqwest::Certificate::from_der(certificate.cert.der()).unwrap())
            .build()
            .unwrap();
        let enrollment = Enrollment::new(
            &format!("https://localhost:{port}"),
            zeroize::Zeroizing::new("disposable-setup-credential-for-tests".into()),
        )
        .unwrap();
        for expected in [204, 204, 401, 403, 409, 302, 500, 200] {
            *code.lock().unwrap() = StatusCode::from_u16(expected).unwrap();
            assert_eq!(
                bind_with(&client, &enrollment).await.is_ok(),
                expected == 204
            );
        }
        let requests = observed.lock().unwrap();
        assert_eq!(requests.len(), 8);
        for (headers, body) in requests.iter() {
            assert_eq!(
                headers["authorization"],
                "Bearer disposable-setup-credential-for-tests"
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(body).unwrap(),
                serde_json::json!({"agent_uuid":enrollment.info().agent_uuid})
            );
        }
        server.abort();
    }
}
