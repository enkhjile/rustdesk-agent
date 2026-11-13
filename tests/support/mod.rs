//! In-memory, loopback-only test backend. No production enrollment or storage.
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_server::tls_rustls::RustlsConfig;
use rustdesk_agent::{DesiredState, parse_desired};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

pub const A: &str = "a49f96c4-968b-4e7a-a3be-a5452385584d";
pub const B: &str = "b49f96c4-968b-4e7a-a3be-a5452385584d";
type Shared = Arc<Mutex<Backend>>;

// Typed decoding rejects duplicate keys before JSON Schema sees the document.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Status {
    schema_version: u32,
    agent_uuid: String,
    rustdesk_id: Option<String>,
    rustdesk_version: Option<String>,
    desired_revision: Option<u64>,
    applied_revision: Option<u64>,
    observation_sequence: u64,
    results: Results,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    management_mode: Option<rustdesk_agent::state::ManagementMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "rustdesk_agent::network::deserialize_ips"
    )]
    local_ips: Option<Vec<std::net::IpAddr>>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Results {
    approve_mode: Option<String>,
    verification_method: Option<String>,
}

struct Backend {
    status_response: Option<StatusCode>,
    attempts: Vec<Vec<u8>>,
    response: Option<(StatusCode, HeaderMap, Vec<u8>)>,
    desired: BTreeMap<String, (DesiredState, Value)>,
    latest: BTreeMap<String, Value>,
    desired_schema: jsonschema::Validator,
    status_schema: jsonschema::Validator,
}
fn schema(source: &str) -> jsonschema::Validator {
    jsonschema::options()
        .should_validate_formats(true)
        .build(&serde_json::from_str(source).unwrap())
        .unwrap()
}
fn authenticate(headers: &HeaderMap) -> Result<&'static str, StatusCode> {
    match headers.get("authorization").and_then(|h| h.to_str().ok()) {
        Some("Bearer test-a" | "Bearer disposable-test-credential-device-a") => Ok(A),
        Some("Bearer test-b") => Ok(B),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
async fn desired(
    State(backend): State<Shared>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let device = authenticate(&headers)?;
    let backend = backend.lock().unwrap();
    if let Some(response) = &backend.response {
        let (mut parts, body) = response.clone().into_response().into_parts();
        parts.headers.remove("content-length");
        return Ok(Response::from_parts(
            parts,
            axum::body::Body::from_stream(body.into_data_stream()),
        ));
    }
    Ok(match backend.desired.get(device) {
        Some((_, value)) => Json(value.clone()).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}
async fn status(
    State(backend): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let device = authenticate(&headers)?;
    if headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::trim)
        != Some("application/json")
    {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let report: Status = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let value: Value = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mut backend = backend.lock().unwrap();
    backend.attempts.push(body.to_vec());
    if let Some(status) = backend.status_response {
        return Ok(status);
    }
    if !backend.status_schema.is_valid(&value) {
        return Err(StatusCode::BAD_REQUEST);
    }
    if report.agent_uuid != device {
        return Err(StatusCode::FORBIDDEN);
    }
    let desired = backend.desired.get(device);
    if report.desired_revision != desired.map(|(state, _)| state.revision) {
        return Err(StatusCode::CONFLICT);
    }
    let results = value["results"].as_object().unwrap();
    if let Some((state, raw)) = desired {
        let settings = raw["settings"].as_object().unwrap();
        let verified = results.values().all(|v| v == "verified");
        if !results.keys().eq(settings.keys())
            || report.applied_revision != verified.then_some(state.revision)
        {
            return Err(StatusCode::BAD_REQUEST);
        }
    } else if !results.is_empty() || report.applied_revision.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(previous) = backend.latest.get(device) {
        let sequence = previous["observation_sequence"].as_u64().unwrap();
        if report.observation_sequence < sequence
            || (report.observation_sequence == sequence && previous != &value)
        {
            return Err(StatusCode::CONFLICT);
        }
    }
    backend.latest.insert(device.to_owned(), value);
    Ok(StatusCode::NO_CONTENT)
}

pub struct Fixture {
    pub url: String,
    pub client: reqwest::Client,
    #[allow(dead_code)]
    pub certificate: reqwest::Certificate,
    backend: Shared,
    handle: axum_server::Handle<std::net::SocketAddr>,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    pub async fn start() -> Self {
        Self::start_with_routes(Router::new()).await
    }
    pub async fn start_with_routes(extra: Router) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let certificate = rcgen::generate_simple_self_signed(vec![
            "localhost".into(),
            "127.0.0.1".into(),
            "console.test".into(),
        ])
        .unwrap();
        let der = certificate.cert.der().to_vec();
        let config =
            RustlsConfig::from_der(vec![der.clone()], certificate.signing_key.serialize_der())
                .await
                .unwrap();
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
            .build()
            .unwrap();
        let backend = Arc::new(Mutex::new(Backend {
            status_response: None,
            attempts: Vec::new(),
            response: None,
            desired: BTreeMap::new(),
            latest: BTreeMap::new(),
            desired_schema: schema(include_str!("../../docs/schemas/desired-state.schema.json")),
            status_schema: schema(include_str!("../../docs/schemas/status.schema.json")),
        }));
        let app = Router::new()
            .route("/v1/agent/desired", get(desired))
            .route("/v1/agent/status", post(status))
            .layer(DefaultBodyLimit::max(65536))
            .with_state(backend.clone())
            .merge(extra);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = axum_server::Handle::new();
        let server = axum_server::from_tcp_rustls(listener, config)
            .unwrap()
            .handle(handle.clone());
        let task = tokio::spawn(async move {
            server.serve(app.into_make_service()).await.unwrap();
        });
        tokio::time::timeout(Duration::from_secs(3), handle.listening())
            .await
            .unwrap()
            .unwrap();
        Self {
            url: format!("https://{address}"),
            client,
            certificate: reqwest::Certificate::from_der(&der).unwrap(),
            backend,
            handle,
            task,
        }
    }
    #[allow(dead_code)]
    pub fn status_response(&self, status: Option<StatusCode>) {
        self.backend.lock().unwrap().status_response = status;
    }
    #[allow(dead_code)]
    pub fn attempts(&self) -> Vec<Vec<u8>> {
        self.backend.lock().unwrap().attempts.clone()
    }
    /// Fault injection for the agent HTTP client tests only.
    #[allow(dead_code)]
    pub fn respond(&self, status: StatusCode, headers: HeaderMap, body: Vec<u8>) {
        self.backend.lock().unwrap().response = Some((status, headers, body));
    }
    pub fn assign(&self, device: &str, value: Value) -> Result<(), &'static str> {
        let state = parse_desired(&serde_json::to_vec(&value).unwrap())?;
        let mut backend = self.backend.lock().unwrap();
        if ![A, B].contains(&device) || !backend.desired_schema.is_valid(&value) {
            return Err("invalid_assignment");
        }
        if let Some((previous, _)) = backend.desired.get(device)
            && (state.revision < previous.revision
                || (state.revision == previous.revision && state != *previous))
        {
            return Err("revision_conflict");
        }
        backend.desired.insert(device.to_owned(), (state, value));
        Ok(())
    }
    pub fn latest(&self, device: &str) -> Option<Value> {
        self.backend.lock().unwrap().latest.get(device).cloned()
    }
    pub async fn get(&self, token: &str) -> reqwest::Response {
        self.client
            .get(format!("{}/v1/agent/desired", self.url))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
    }
    pub async fn post(&self, value: &Value) -> reqwest::Response {
        self.post_raw(value.to_string()).await
    }
    pub async fn post_raw(&self, body: String) -> reqwest::Response {
        self.client
            .post(format!("{}/v1/agent/status", self.url))
            .bearer_auth("test-a")
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.handle.shutdown();
        self.task.abort();
    }
}
