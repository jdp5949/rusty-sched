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
    ///
    /// Reads `RSCHED_TOKEN` from the environment; if set, every request includes
    /// `Authorization: Bearer <token>` so the CLI can authenticate against the
    /// v0.4+ RBAC-protected mutation routes. Set the token via
    /// `POST /api/v1/auth/api-keys` from the UI and export it before running CLI.
    pub fn new(base: impl Into<String>) -> Self {
        let mut builder = reqwest::Client::builder();
        if let Ok(tok) = std::env::var("RSCHED_TOKEN") {
            if !tok.trim().is_empty() {
                let mut headers = reqwest::header::HeaderMap::new();
                let value = format!("Bearer {}", tok.trim());
                if let Ok(hv) = reqwest::header::HeaderValue::from_str(&value) {
                    headers.insert(reqwest::header::AUTHORIZATION, hv);
                    builder = builder.default_headers(headers);
                }
            }
        }
        Self {
            base: base.into(),
            http: builder.build().unwrap_or_else(|_| reqwest::Client::new()),
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

    /// Kill a live run by run id. Returns Ok if server responded 204.
    pub async fn kill_run(&self, run_id: &str) -> Result<()> {
        self.http
            .delete(format!("{}/api/v1/runs/{}", self.base, run_id))
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

    /// Set a global variable (used by Autosys `value(name)` conditions).
    pub async fn set_global(&self, name: &str, value: &str) -> Result<()> {
        self.http
            .post(format!("{}/api/v1/globals", self.base))
            .json(&serde_json::json!({"name": name, "value": value}))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// List all globals as `(name, value, updated_at)`.
    pub async fn list_globals(&self) -> Result<Vec<(String, String, String)>> {
        let r = self
            .http
            .get(format!("{}/api/v1/globals", self.base))
            .send()
            .await?
            .error_for_status()?;
        let arr: Vec<serde_json::Value> = r.json().await?;
        Ok(arr
            .into_iter()
            .filter_map(|v| {
                Some((
                    v["name"].as_str()?.to_string(),
                    v["value"].as_str()?.to_string(),
                    v["updated_at"].as_str()?.to_string(),
                ))
            })
            .collect())
    }

    /// Change a run's state (Autosys CHANGE_STATUS verb).
    pub async fn change_run_state(&self, run_id: &str, state: &str) -> Result<()> {
        self.http
            .post(format!("{}/api/v1/runs/{}/state", self.base, run_id))
            .json(&serde_json::json!({"state": state}))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Delete a global by name.
    pub async fn delete_global(&self, name: &str) -> Result<()> {
        self.http
            .delete(format!("{}/api/v1/globals/{}", self.base, name))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
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
