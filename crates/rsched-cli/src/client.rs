//! Thin reqwest client over the rusty-sched REST API.

use anyhow::{Context, Result};
use rsched_core::Job;
use serde::Serialize;
use serde_json::Value;

/// HTTP client.
pub struct ApiClient {
    base: String,
    http: reqwest::Client,
}

impl ApiClient {
    /// Construct from a base URL (`http://host:port`).
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            http: reqwest::Client::new(),
        }
    }

    /// List all jobs.
    pub async fn list_jobs(&self) -> Result<Vec<Job>> {
        let r = self
            .http
            .get(format!("{}/api/v1/jobs", self.base))
            .send()
            .await?
            .error_for_status()?;
        Ok(r.json().await?)
    }

    /// Create a job from a spec value.
    pub async fn create_job<T: Serialize>(&self, spec: &T) -> Result<Value> {
        let r = self
            .http
            .post(format!("{}/api/v1/jobs", self.base))
            .json(spec)
            .send()
            .await?
            .error_for_status()
            .context("create_job")?;
        Ok(r.json().await?)
    }

    /// Manually trigger a job.
    pub async fn trigger(&self, id: &str) -> Result<Value> {
        let r = self
            .http
            .post(format!("{}/api/v1/jobs/{}/trigger", self.base, id))
            .send()
            .await?
            .error_for_status()?;
        Ok(r.json().await?)
    }

    /// Pause.
    pub async fn pause(&self, id: &str) -> Result<()> {
        self.http
            .post(format!("{}/api/v1/jobs/{}/pause", self.base, id))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Resume.
    pub async fn resume(&self, id: &str) -> Result<()> {
        self.http
            .post(format!("{}/api/v1/jobs/{}/resume", self.base, id))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}
