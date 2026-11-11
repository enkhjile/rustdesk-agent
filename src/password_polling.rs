use super::{PollStatus, Poller, next_failure_delay, response_retry_after};
use crate::password::{Attempt, Operation, Outcome};
use reqwest::{Method, header};
use std::time::UNIX_EPOCH;
use zeroize::Zeroizing;

impl Poller {
    /// Called only in managed mode. Secret delivery is independent of policies.
    /// Once intent is persisted, neither an interrupted command nor a lost
    /// acknowledgement permits replay. Only the non-secret outcome is retried.
    pub async fn poll_password<F, Fut>(&mut self, apply: F) -> PollStatus
    where
        F: FnOnce(Zeroizing<String>) -> Fut,
        Fut: std::future::Future<Output = Outcome>,
    {
        let mut retry_after = None;
        let status = self.poll_password_once(apply, &mut retry_after).await;
        if matches!(status, PollStatus::Delivered | PollStatus::Unassigned) {
            self.password_failures = 0;
            self.password_retry_seconds = 0;
        } else {
            self.password_retry_seconds =
                next_failure_delay(&mut self.password_failures, retry_after);
        }
        status
    }

    async fn poll_password_once<F, Fut>(
        &mut self,
        apply: F,
        retry_after: &mut Option<u64>,
    ) -> PollStatus
    where
        F: FnOnce(Zeroizing<String>) -> Fut,
        Fut: std::future::Future<Output = Outcome>,
    {
        if self.poisoned {
            return PollStatus::StorageError;
        }
        if self
            .state
            .password_attempt
            .as_ref()
            .is_some_and(|a| !a.acknowledged)
        {
            return self.deliver_password(retry_after).await;
        }
        let mut response = match self
            .authenticated_request(Method::POST, "/v1/agent/password/claim")
            .expect("validated enrollment credential")
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => return PollStatus::TransportError,
        };
        let status = response.status().as_u16();
        if status == 204 {
            return PollStatus::Unassigned;
        }
        if status != 200 {
            *retry_after = response_retry_after(&response);
            return PollStatus::HttpError { status };
        }
        if response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(str::trim)
            != Some("application/json")
        {
            return PollStatus::Rejected {
                reason: "invalid_password_content_type",
            };
        }
        let mut body = Zeroizing::new(Vec::new());
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if body.len() + chunk.len() <= 4096 => {
                    body.extend_from_slice(&chunk)
                }
                Ok(None) => break,
                _ => {
                    return PollStatus::Rejected {
                        reason: "invalid_password_body",
                    };
                }
            }
        }
        let operation = match Operation::parse(&body) {
            Ok(operation) => operation,
            Err(reason) => return PollStatus::Rejected { reason },
        };
        drop(body);
        let now = match UNIX_EPOCH.elapsed() {
            Ok(now) => now.as_secs(),
            Err(_) => {
                return PollStatus::Rejected {
                    reason: "invalid_clock",
                };
            }
        };
        let attempt = match Attempt::begin(self.state.password_attempt.as_ref(), &operation, now) {
            Ok(attempt) => attempt,
            Err(reason) => return PollStatus::Rejected { reason },
        };
        let execute = attempt.outcome != Outcome::Expired;
        let mut next = self.state.clone();
        next.password_attempt = Some(attempt);
        if self.commit(next).is_err() {
            return PollStatus::StorageError;
        }
        if execute {
            let outcome = apply(operation.password.expect("unexpired password payload")).await;
            let mut next = self.state.clone();
            next.password_attempt
                .as_mut()
                .expect("persisted password intent")
                .outcome = outcome;
            if self.commit(next).is_err() {
                return PollStatus::StorageError;
            }
        }
        self.deliver_password(retry_after).await
    }

    async fn deliver_password(&mut self, retry_after: &mut Option<u64>) -> PollStatus {
        let attempt = self
            .state
            .password_attempt
            .as_ref()
            .expect("pending password result");
        let body = serde_json::json!({
            "schema_version": 1,
            "operation_id": attempt.operation_id,
            "sequence": attempt.sequence,
            "outcome": attempt.outcome,
        })
        .to_string();
        let response = match self
            .authenticated_request(Method::POST, "/v1/agent/password/result")
            .expect("validated enrollment credential")
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => return PollStatus::TransportError,
        };
        let status = response.status().as_u16();
        if status != 204 {
            *retry_after = response_retry_after(&response);
            return PollStatus::DeliveryRejected { status };
        }
        let mut next = self.state.clone();
        next.password_attempt
            .as_mut()
            .expect("pending password result")
            .acknowledged = true;
        if self.commit(next).is_err() {
            return PollStatus::StorageError;
        }
        PollStatus::Delivered
    }
}
