//! Thin reqwest client over the rusty-sched REST API.

use anyhow::{Context, Result};
use rsched_core::{Job, Run};
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

    /// Create a job.
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

    /// Replace a job (PUT).
    pub async fn update_job<T: Serialize>(&self, id: &str, spec: &T) -> Result<Value> {
        let r = self
            .http
            .put(format!("{}/api/v1/jobs/{}", self.base, id))
            .json(spec)
            .send()
            .await?
            .error_for_status()
            .context("update_job")?;
        Ok(r.json().await?)
    }

    /// Fetch a job by id.
    pub async fn get_job(&self, id: &str) -> Result<Job> {
        let r = self
            .http
            .get(format!("{}/api/v1/jobs/{}", self.base, id))
            .send()
            .await?
            .error_for_status()?;
        Ok(r.json().await?)
    }

    /// Look up a job by name. Returns id+full job.
    pub async fn get_job_by_name(&self, name: &str) -> Result<Job> {
        let r = self
            .http
            .get(format!("{}/api/v1/jobs/by-name/{}", self.base, name))
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("lookup job by name: {name}"))?;
        Ok(r.json().await?)
    }

    /// Delete a job by id.
    pub async fn delete_job(&self, id: &str) -> Result<()> {
        self.http
            .delete(format!("{}/api/v1/jobs/{}", self.base, id))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
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

    /// Most recent runs for a job (server caps at 100).
    pub async fn runs_for(&self, id: &str) -> Result<Vec<Run>> {
        let r = self
            .http
            .get(format!("{}/api/v1/jobs/{}/runs", self.base, id))
            .send()
            .await?
            .error_for_status()?;
        Ok(r.json().await?)
    }

    /// Resolve a "NAME_OR_ID" argument to a job id string.
    /// Tries to parse as ULID first; falls back to name lookup.
    pub async fn resolve(&self, name_or_id: &str) -> Result<String> {
        if name_or_id.parse::<rsched_core::JobId>().is_ok() {
            return Ok(name_or_id.to_string());
        }
        let job = self.get_job_by_name(name_or_id).await?;
        Ok(job.id.to_string())
    }
}
