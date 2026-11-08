//! Read-only desired-state polling. No settings adapter or status transport.
use crate::{RevisionGuard, backoff_seconds, enrollment::Enrollment, parse_desired};
use reqwest::{Client, ClientBuilder, header};
use serde::Serialize;
use std::time::{Duration, SystemTime};
use uuid::Uuid;
use zeroize::Zeroizing;

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
}

/// Local diagnostics only: acceptance is not application or verification.
#[derive(Debug, Serialize)]
pub struct PollObservation {
    pub status: PollStatus,
    pub accepted_revision: Option<u64>,
    pub next_poll_seconds: u64,
}

pub struct Poller {
    client: Client,
    enrollment: Enrollment,
    guard: RevisionGuard,
    failures: u32,
}

fn client_builder() -> ClientBuilder {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
}

impl Poller {
    pub fn new(enrollment: Enrollment) -> Result<Self, &'static str> {
        enrollment.validate()?;
        Ok(Self {
            client: client_builder()
                .build()
                .map_err(|_| "http_client_initialization_failed")?,
            enrollment,
            guard: RevisionGuard::default(),
            failures: 0,
        })
    }

    /// Dropping this future cancels the in-flight request. The caller owns the
    /// stop-aware wait; never run overlapping polls for the same enrollment.
    pub async fn poll(&mut self) -> PollObservation {
        let (status, retry_after) = self.fetch().await;
        let next_poll_seconds =
            if matches!(status, PollStatus::Accepted { .. } | PollStatus::Unassigned) {
                self.failures = 0;
                60
            } else {
                let seconds = retry_delay(self.failures, retry_after, Uuid::new_v4().as_u128());
                self.failures = self.failures.saturating_add(1);
                seconds
            };
        PollObservation {
            status,
            accepted_revision: self.guard.accepted().map(|state| state.revision),
            next_poll_seconds,
        }
    }

    async fn fetch(&mut self) -> (PollStatus, Option<u64>) {
        // Only this locally provisioned origin can receive the credential.
        let mut endpoint = self.enrollment.info().management_origin;
        endpoint.set_path("/v1/agent/desired");
        let token = Zeroizing::new(format!("Bearer {}", self.enrollment.credential()));
        let Ok(mut authorization) = header::HeaderValue::from_str(&token) else {
            return (
                PollStatus::Rejected {
                    reason: "invalid_device_credential",
                },
                None,
            );
        };
        authorization.set_sensitive(true);
        let response = self
            .client
            .get(endpoint)
            .header(header::AUTHORIZATION, authorization)
            .header(header::ACCEPT, "application/json")
            .send()
            .await;
        let mut response = match response {
            Ok(response) => response,
            Err(error) => return (transport_error(error), None),
        };
        let status = response.status();
        if status == reqwest::StatusCode::NO_CONTENT {
            return (PollStatus::Unassigned, None);
        }
        if status != reqwest::StatusCode::OK {
            let retry_after =
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                    response
                        .headers()
                        .get(header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| parse_retry_after(value, SystemTime::now()))
                } else {
                    None
                };
            return (
                PollStatus::HttpError {
                    status: status.as_u16(),
                },
                retry_after,
            );
        }
        let json = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
        if !json {
            return (
                PollStatus::Rejected {
                    reason: "invalid_content_type",
                },
                None,
            );
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BODY as u64)
        {
            return (
                PollStatus::Rejected {
                    reason: "response_too_large",
                },
                None,
            );
        }
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if chunk.len() <= MAX_BODY - body.len() => {
                    body.extend_from_slice(&chunk)
                }
                Ok(Some(_)) => {
                    return (
                        PollStatus::Rejected {
                            reason: "response_too_large",
                        },
                        None,
                    );
                }
                Ok(None) => break,
                Err(error) => return (transport_error(error), None),
            }
        }
        let result = parse_desired(&body).and_then(|state| {
            self.guard.accept(&state)?;
            Ok(state.revision)
        });
        (
            match result {
                Ok(revision) => PollStatus::Accepted { revision },
                Err(reason) => PollStatus::Rejected { reason },
            },
            None,
        )
    }
}

fn transport_error(error: reqwest::Error) -> PollStatus {
    // Never serialize reqwest errors, URLs, response bodies or request headers.
    if error.is_timeout() {
        PollStatus::Timeout
    } else {
        PollStatus::TransportError
    }
}

fn parse_retry_after(value: &str, now: SystemTime) -> Option<u64> {
    let seconds = value.parse::<u64>().ok().or_else(|| {
        httpdate::parse_http_date(value)
            .ok()
            .map(|date| date.duration_since(now).unwrap_or_default().as_secs())
    })?;
    Some(seconds.min(300))
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
