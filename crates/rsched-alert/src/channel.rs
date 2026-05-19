//! Alert delivery channels.

use crate::{AlertError, AlertPayload};
use async_trait::async_trait;
use rsched_core::AlertChannel;
use serde_json::json;
use tracing::warn;

/// Trait for an alert delivery target.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Deliver one payload.
    async fn deliver(&self, payload: &AlertPayload) -> Result<(), AlertError>;
}

/// Slack incoming webhook channel.
pub struct SlackChannel {
    client: reqwest::Client,
    webhook_url: String,
}

impl SlackChannel {
    /// Construct.
    pub fn new(webhook_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            webhook_url: webhook_url.into(),
        }
    }
}

#[async_trait]
impl Channel for SlackChannel {
    async fn deliver(&self, p: &AlertPayload) -> Result<(), AlertError> {
        let text = format!(
            ":rotating_light: *{:?}* — job `{}` (attempt {}) state `{:?}`{}",
            p.event,
            p.job_name,
            p.attempt,
            p.state,
            p.message
                .as_ref()
                .map(|m| format!(" — {m}"))
                .unwrap_or_default(),
        );
        let body = json!({"text": text});
        let resp = self
            .client
            .post(&self.webhook_url)
            .json(&body)
            .send()
            .await?;
        let _ = resp.error_for_status()?;
        Ok(())
    }
}

/// Generic webhook channel (POST JSON of [`AlertPayload`]).
pub struct WebhookChannel {
    client: reqwest::Client,
    url: String,
}

impl WebhookChannel {
    /// Construct.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.into(),
        }
    }
}

#[async_trait]
impl Channel for WebhookChannel {
    async fn deliver(&self, p: &AlertPayload) -> Result<(), AlertError> {
        let resp = self.client.post(&self.url).json(p).send().await?;
        let _ = resp.error_for_status()?;
        Ok(())
    }
}

/// Try to deliver `payload` across each configured channel; log+swallow per-
/// channel failures so a single bad URL doesn't block the others.
pub async fn deliver_all(channels: &[AlertChannel], payload: &AlertPayload) {
    for ch in channels {
        let result = match ch {
            AlertChannel::Slack { webhook_url } => {
                SlackChannel::new(webhook_url).deliver(payload).await
            }
            AlertChannel::Webhook { url, .. } => WebhookChannel::new(url).deliver(payload).await,
            AlertChannel::Email { .. } => Err(AlertError::Unsupported("email (M6.1)")),
        };
        if let Err(e) = result {
            warn!(error = %e, "alert delivery failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{AlertEvent, JobId, RunId, RunState};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn payload() -> AlertPayload {
        AlertPayload {
            event: AlertEvent::OnFailure,
            job_id: JobId::new(),
            job_name: "nightly-etl".into(),
            run_id: RunId::new(),
            state: RunState::Failed,
            exit_code: Some(1),
            attempt: 2,
            started_at: None,
            finished_at: None,
            host: "test".into(),
            message: Some("disk full".into()),
        }
    }

    #[tokio::test]
    async fn slack_posts_text() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&srv)
            .await;
        let ch = SlackChannel::new(format!("{}/hook", srv.uri()));
        ch.deliver(&payload()).await.unwrap();
    }

    #[tokio::test]
    async fn webhook_posts_payload() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/recv"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&srv)
            .await;
        let ch = WebhookChannel::new(format!("{}/recv", srv.uri()));
        ch.deliver(&payload()).await.unwrap();
    }

    #[tokio::test]
    async fn slack_propagates_http_error() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&srv)
            .await;
        let ch = SlackChannel::new(srv.uri());
        assert!(ch.deliver(&payload()).await.is_err());
    }

    #[tokio::test]
    async fn deliver_all_continues_past_failure() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&srv)
            .await;
        let channels = vec![
            AlertChannel::Slack {
                webhook_url: "http://127.0.0.1:1/dead".into(),
            },
            AlertChannel::Webhook {
                url: srv.uri(),
                secret: None,
            },
        ];
        // should not panic / propagate
        deliver_all(&channels, &payload()).await;
    }
}
