//! Durable desired-state acceptance; application is opt-in and never claims verification.
use crate::{
    Setting, backoff_seconds,
    enrollment::Enrollment,
    parse_desired,
    state::{DurableState, ManagementMode, SettingResult, StateStore},
};
use reqwest::{Client, ClientBuilder, Method, header};
use serde::Serialize;
use std::time::{Duration, SystemTime};
use uuid::Uuid;
use zeroize::Zeroizing;

#[path = "password_polling.rs"]
mod password_polling;

const MAX_BODY: usize = 65536;

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PollStatus {
    Accepted { revision: u64 },
    Unassigned,
    Rejected { reason: &'static str },
    HttpError { status: u16 },
    TransportError,
    Timeout,
    Delivered,
    DeliveryRejected { status: u16 },
    StorageError,
}

/// Local diagnostics only: acceptance is not application or verification.
#[derive(Debug, Serialize)]
pub struct PollObservation {
    pub status: PollStatus,
    pub delivery: Option<PollStatus>,
    pub accepted_revision: Option<u64>,
    pub next_poll_seconds: u64,
}

pub struct Poller {
    client: Client,
    enrollment: Enrollment,
    state: DurableState,
    store: Box<dyn StateStore>,
    poisoned: bool,
    report_failures: u32,
    failures: u32,
    password_failures: u32,
    password_retry_seconds: u64,
}

pub(crate) fn client_builder() -> ClientBuilder {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .https_only(true)
        // Keep the lab hostname on loopback, independent of DNS/hosts changes.
        .resolve("localhost", std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
}

impl Poller {
    /// Delay for the service loop after settings and optional password polling.
    pub fn next_poll_seconds(&self, observation: &PollObservation) -> u64 {
        observation
            .next_poll_seconds
            .max(self.password_retry_seconds)
    }

    fn authenticated_request(
        &self,
        method: Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, header::InvalidHeaderValue> {
        // Only this locally provisioned origin can receive the credential.
        let mut endpoint = self.enrollment.info().management_origin;
        endpoint.set_path(path);
        let token = Zeroizing::new(format!("Bearer {}", self.enrollment.credential()));
        let mut authorization = header::HeaderValue::from_str(&token)?;
        authorization.set_sensitive(true);
        Ok(self
            .client
            .request(method, endpoint)
            .header(header::AUTHORIZATION, authorization))
    }

    pub fn new(
        enrollment: Enrollment,
        mut store: impl StateStore + 'static,
    ) -> Result<Self, &'static str> {
        enrollment.validate()?;
        let state = DurableState::decode(
            &store.load().map_err(|_| "state_load_failed")?,
            &enrollment.info(),
        )?;
        Ok(Self {
            client: client_builder()
                .build()
                .map_err(|_| "http_client_initialization_failed")?,
            enrollment,
            state,
            store: Box::new(store),
            poisoned: false,
            report_failures: 0,
            failures: 0,
            password_failures: 0,
            password_retry_seconds: 0,
        })
    }

    /// Dropping this future cancels the in-flight request. The caller owns the
    /// stop-aware wait; never run overlapping polls for the same enrollment.
    pub async fn poll(&mut self, id: Option<String>, version: Option<String>) -> PollObservation {
        self.poll_with(
            id,
            version,
            None::<fn(Setting, &'static str) -> std::future::Ready<SettingResult>>,
        )
        .await
    }

    /// Managed application, requiring a fresh authenticated desired response.
    /// The callback must be cancellation-safe. No callback can claim `verified`.
    pub async fn poll_applying<F, Fut>(
        &mut self,
        id: Option<String>,
        version: Option<String>,
        apply: F,
    ) -> PollObservation
    where
        F: FnMut(Setting, &'static str) -> Fut,
        Fut: std::future::Future<Output = SettingResult>,
    {
        self.poll_with(id, version, Some(apply)).await
    }

    async fn poll_with<F, Fut>(
        &mut self,
        id: Option<String>,
        version: Option<String>,
        mut apply: Option<F>,
    ) -> PollObservation
    where
        F: FnMut(Setting, &'static str) -> Fut,
        Fut: std::future::Future<Output = SettingResult>,
    {
        if self.poisoned {
            return PollObservation {
                status: PollStatus::StorageError,
                delivery: None,
                accepted_revision: self.state.accepted.as_ref().map(|s| s.revision),
                next_poll_seconds: 300,
            };
        }
        // Retry the exact durable report before a fresh observation can supersede it.
        let mut delivery = if self.state.pending.is_some() {
            Some(self.deliver().await)
        } else {
            None
        };
        let (status, retry_after) = if self.poisoned {
            (PollStatus::StorageError, None)
        } else {
            self.fetch(id, version, &mut apply).await
        };
        if delivery.is_none() && self.state.pending.is_some() && !self.poisoned {
            delivery = Some(self.deliver().await);
        }
        let mut next_poll_seconds =
            if matches!(status, PollStatus::Accepted { .. } | PollStatus::Unassigned) {
                self.failures = 0;
                60
            } else {
                next_failure_delay(&mut self.failures, retry_after)
            };
        if let Some((result, retry_after)) = &delivery {
            if matches!(
                result,
                PollStatus::Delivered | PollStatus::DeliveryRejected { .. }
            ) {
                self.report_failures = 0;
            } else {
                let delay = next_failure_delay(&mut self.report_failures, *retry_after);
                next_poll_seconds =
                    if matches!(status, PollStatus::Accepted { .. } | PollStatus::Unassigned) {
                        delay
                    } else {
                        next_poll_seconds.max(delay)
                    };
            }
        }
        PollObservation {
            status: if self.poisoned {
                PollStatus::StorageError
            } else {
                status
            },
            delivery: delivery.map(|(status, _)| status),
            accepted_revision: self.state.accepted.as_ref().map(|state| state.revision),
            next_poll_seconds,
        }
    }

    fn commit(&mut self, next: DurableState) -> Result<(), ()> {
        let result = next
            .encode()
            .map_err(|_| ())
            .and_then(|bytes| self.store.save(&bytes).map_err(|_| ()));
        if result.is_err() {
            self.poisoned = true;
            return Err(());
        }
        self.state = next;
        Ok(())
    }

    async fn deliver(&mut self) -> (PollStatus, Option<u64>) {
        let Some(report) = self.state.pending.as_ref() else {
            return (PollStatus::Delivered, None);
        };
        let body = match serde_json::to_vec(report) {
            Ok(body) => body,
            Err(_) => {
                self.poisoned = true;
                return (PollStatus::StorageError, None);
            }
        };
        let response = match self
            .authenticated_request(Method::POST, "/v1/agent/status")
            .expect("validated enrollment token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => return (transport_error(error), None),
        };
        let code = response.status().as_u16();
        if code == 204 || matches!(code, 400 | 403 | 409) {
            let mut next = self.state.clone();
            next.pending = None;
            if self.commit(next).is_err() {
                return (PollStatus::StorageError, None);
            }
            return (
                if code == 204 {
                    PollStatus::Delivered
                } else {
                    PollStatus::DeliveryRejected { status: code }
                },
                None,
            );
        }
        (
            PollStatus::HttpError { status: code },
            response_retry_after(&response),
        )
    }

    async fn fetch<F, Fut>(
        &mut self,
        id: Option<String>,
        version: Option<String>,
        apply: &mut Option<F>,
    ) -> (PollStatus, Option<u64>)
    where
        F: FnMut(Setting, &'static str) -> Fut,
        Fut: std::future::Future<Output = SettingResult>,
    {
        let mode = Some(if apply.is_some() {
            ManagementMode::Managed
        } else {
            ManagementMode::ObserveOnly
        });
        let Ok(request) = self.authenticated_request(Method::GET, "/v1/agent/desired") else {
            return (
                PollStatus::Rejected {
                    reason: "invalid_device_credential",
                },
                None,
            );
        };
        let response = request
            .header(header::ACCEPT, "application/json")
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => return (transport_error(error), None),
        };
        let status = response.status();
        if status == reqwest::StatusCode::NO_CONTENT {
            // Keep an undelivered report intact until it is accepted or rejected.
            if self.state.pending.is_some() {
                return (PollStatus::Unassigned, None);
            }
            let mut next = self.state.clone();
            if let Err(reason) = next.heartbeat(id, version) {
                return (PollStatus::Rejected { reason }, None);
            }
            next.pending.as_mut().unwrap().management_mode = mode;
            next.pending.as_mut().unwrap().local_ips = crate::network::collect();
            if self.commit(next).is_err() {
                return (PollStatus::StorageError, None);
            }
            return (PollStatus::Unassigned, None);
        }
        if status != reqwest::StatusCode::OK {
            return (
                PollStatus::HttpError {
                    status: status.as_u16(),
                },
                response_retry_after(&response),
            );
        }
        let body = match read_desired_body(response).await {
            Ok(body) => body,
            Err(status) => return (status, None),
        };
        let result = parse_desired(&body).and_then(|state| {
            let mut next = self.state.clone();
            next.observe(&state, id, version)?;
            next.pending.as_mut().unwrap().management_mode = mode;
            next.pending.as_mut().unwrap().local_ips = crate::network::collect();
            if apply.is_some() {
                for (key, _) in state.pairs() {
                    next.pending
                        .as_mut()
                        .unwrap()
                        .set_result(key, SettingResult::Indeterminate);
                }
            }
            self.commit(next).map_err(|_| "state_save_failed")?;
            Ok(state)
        });
        if let (Ok(state), Some(apply)) = (&result, apply) {
            for (key, wanted) in state.pairs() {
                let outcome = apply(key, wanted).await;
                let mut next = self.state.clone();
                next.pending.as_mut().unwrap().set_result(key, outcome);
                if self.commit(next).is_err() {
                    return (PollStatus::StorageError, None);
                }
            }
        }
        (
            match result {
                Ok(state) => PollStatus::Accepted {
                    revision: state.revision,
                },
                Err(reason) => PollStatus::Rejected { reason },
            },
            None,
        )
    }
}

async fn read_desired_body(mut response: reqwest::Response) -> Result<Vec<u8>, PollStatus> {
    let json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
    if !json {
        return Err(PollStatus::Rejected {
            reason: "invalid_content_type",
        });
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BODY as u64)
    {
        return Err(PollStatus::Rejected {
            reason: "response_too_large",
        });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if chunk.len() > MAX_BODY - body.len() {
            return Err(PollStatus::Rejected {
                reason: "response_too_large",
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn transport_error(error: reqwest::Error) -> PollStatus {
    // Never serialize reqwest errors, URLs, response bodies or request headers.
    if error.is_timeout() {
        PollStatus::Timeout
    } else {
        PollStatus::TransportError
    }
}

fn response_retry_after(response: &reqwest::Response) -> Option<u64> {
    let status = response.status();
    if status != reqwest::StatusCode::TOO_MANY_REQUESTS && !status.is_server_error() {
        return None;
    }
    response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_retry_after(value, SystemTime::now()))
}

fn parse_retry_after(value: &str, now: SystemTime) -> Option<u64> {
    let seconds = value.parse::<u64>().ok().or_else(|| {
        httpdate::parse_http_date(value)
            .ok()
            .map(|date| date.duration_since(now).unwrap_or_default().as_secs())
    })?;
    Some(seconds.min(300))
}

fn next_failure_delay(failures: &mut u32, retry_after: Option<u64>) -> u64 {
    let delay = retry_delay(*failures, retry_after, Uuid::new_v4().as_u128());
    *failures = failures.saturating_add(1);
    delay
}

fn retry_delay(failures: u32, retry_after: Option<u64>, random: u128) -> u64 {
    let ceiling = backoff_seconds(failures);
    let floor = (ceiling * 4 / 5).max(5);
    let jittered = floor + (random % u128::from(ceiling - floor + 1)) as u64;
    jittered.max(retry_after.unwrap_or(0).min(300))
}

#[cfg(test)]
#[path = "management_tests.rs"]
mod tests;
