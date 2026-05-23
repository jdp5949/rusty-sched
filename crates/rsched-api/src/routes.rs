//! HTTP routes.

use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rsched_core::{
    AlertConfig, BackoffKind, Job, JobId, MisfirePolicy, RetryPolicy, RunId, Shell, Target, Trigger,
};
use rsched_scheduler::next_fire;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Build the public router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/jobs", get(list_jobs).post(create_job))
        .route(
            "/api/v1/jobs/:id",
            get(get_job).delete(delete_job).put(update_job),
        )
        .route("/api/v1/jobs/by-name/:name", get(get_job_by_name))
        .route("/api/v1/jobs/:id/pause", post(pause_job))
        .route("/api/v1/jobs/:id/resume", post(resume_job))
        .route("/api/v1/jobs/:id/trigger", post(trigger_job))
        .route("/api/v1/jobs/:id/runs", get(list_runs_for_job))
        .route("/api/v1/runs/:id", get(get_run).delete(kill_run))
        .route("/api/v1/runs/:id/logs", get(get_run_logs))
        .route("/api/v1/stats/jobs/:id", get(job_stats))
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(s): State<AppState>) -> Result<&'static str, ApiError> {
    let _n = s.store.agents().count().await?;
    Ok("ready")
}

#[derive(Debug, Deserialize)]
struct CreateJobReq {
    name: String,
    trigger: Trigger,
    cmd: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    shell: Shell,
    #[serde(default)]
    target: Option<Target>,
    #[serde(default)]
    timeout_secs: u64,
    #[serde(default)]
    sla_secs: u64,
    #[serde(default)]
    retry: Option<RetryPolicy>,
    #[serde(default)]
    misfire: MisfirePolicy,
    #[serde(default)]
    alerts: AlertConfig,
}

#[derive(Debug, Serialize)]
struct JobResp {
    job: Job,
}

async fn list_jobs(State(s): State<AppState>) -> Result<Json<Vec<Job>>, ApiError> {
    Ok(Json(s.store.jobs().list().await?))
}

async fn create_job(
    State(s): State<AppState>,
    Json(req): Json<CreateJobReq>,
) -> Result<(StatusCode, Json<JobResp>), ApiError> {
    let now = chrono::Utc::now();
    let trigger = req.trigger.clone();
    let retry = req.retry.unwrap_or(RetryPolicy {
        max_attempts: 1,
        backoff: BackoffKind::None,
    });
    let next_fire_at = match &trigger {
        Trigger::Cron { expr, timezone } => Some(next_fire(expr, timezone.as_deref(), now)?),
        Trigger::Interval { every, start_at } => {
            Some(start_at.unwrap_or(now + chrono::Duration::from_std(*every).unwrap()))
        }
        Trigger::OneShot { at } => Some(*at),
        _ => None,
    };
    let job = Job {
        id: JobId::new(),
        name: req.name,
        box_id: None,
        trigger,
        cmd: req.cmd,
        args: req.args,
        env: req.env,
        cwd: req.cwd,
        shell: req.shell,
        target: req.target.unwrap_or(Target::Any),
        retry,
        timeout_secs: req.timeout_secs,
        sla_secs: req.sla_secs,
        calendar_id: None,
        exclude_calendar_id: None,
        must_start_times: Vec::new(),
        must_complete_times: Vec::new(),
        exit_policy: rsched_core::ExitCodePolicy::default(),
        misfire: req.misfire,
        dependencies: Vec::new(),
        paused: false,
        alerts: req.alerts,
        created_at: now,
        updated_at: now,
        next_fire_at,
    };
    job.validate()?;
    s.store.jobs().insert(&job).await?;
    Ok((StatusCode::CREATED, Json(JobResp { job })))
}

async fn get_job(State(s): State<AppState>, Path(id): Path<String>) -> Result<Json<Job>, ApiError> {
    let id: JobId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad job id".into()))?;
    Ok(Json(s.store.jobs().get(id).await?))
}

async fn delete_job(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: JobId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad job id".into()))?;
    s.store.jobs().delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn pause_job(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: JobId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad job id".into()))?;
    s.store.jobs().set_paused(id, true).await?;
    Ok(StatusCode::OK)
}

async fn resume_job(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: JobId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad job id".into()))?;
    s.store.jobs().set_paused(id, false).await?;
    Ok(StatusCode::OK)
}

async fn trigger_job(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<rsched_core::Run>, ApiError> {
    let id: JobId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad job id".into()))?;
    // Verify job exists.
    let _job = s.store.jobs().get(id).await?;
    // Set next_fire_at = now so the scheduler tick will dispatch on its next
    // iteration. This works uniformly for any trigger kind (cron / manual /
    // dep / etc.) — manual + dep just normally have next_fire_at = NULL.
    s.store
        .jobs()
        .set_next_fire(id, Some(chrono::Utc::now()))
        .await?;
    // Return a stub run record so the CLI has something to print; the actual
    // Run row is created by the tick loop when it picks the job up (within ~1s).
    let run = rsched_core::Run::new(id, 1);
    Ok(Json(run))
}

async fn get_job_by_name(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Job>, ApiError> {
    Ok(Json(s.store.jobs().get_by_name(&name).await?))
}

async fn update_job(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateJobReq>,
) -> Result<Json<JobResp>, ApiError> {
    let id: JobId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad job id".into()))?;
    let existing = s.store.jobs().get(id).await?;
    let now = chrono::Utc::now();
    let trigger = req.trigger.clone();
    let retry = req.retry.unwrap_or(existing.retry.clone());
    let next_fire_at = match &trigger {
        Trigger::Cron { expr, timezone } => Some(next_fire(expr, timezone.as_deref(), now)?),
        Trigger::Interval { every, start_at } => {
            Some(start_at.unwrap_or(now + chrono::Duration::from_std(*every).unwrap()))
        }
        Trigger::OneShot { at } => Some(*at),
        _ => None,
    };
    let job = Job {
        id,
        name: req.name,
        box_id: existing.box_id,
        trigger,
        cmd: req.cmd,
        args: req.args,
        env: req.env,
        cwd: req.cwd,
        shell: req.shell,
        target: req.target.unwrap_or(existing.target),
        retry,
        timeout_secs: req.timeout_secs,
        sla_secs: req.sla_secs,
        calendar_id: existing.calendar_id,
        exclude_calendar_id: existing.exclude_calendar_id,
        must_start_times: existing.must_start_times,
        must_complete_times: existing.must_complete_times,
        exit_policy: existing.exit_policy,
        misfire: req.misfire,
        dependencies: existing.dependencies,
        paused: existing.paused,
        alerts: req.alerts,
        created_at: existing.created_at,
        updated_at: now,
        next_fire_at,
    };
    job.validate()?;
    s.store.jobs().update(&job).await?;
    Ok(Json(JobResp { job }))
}

async fn list_runs_for_job(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<rsched_core::Run>>, ApiError> {
    let id: JobId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad job id".into()))?;
    Ok(Json(s.store.runs().list_for_job(id, 100).await?))
}

async fn get_run(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<rsched_core::Run>, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad run id".into()))?;
    Ok(Json(s.store.runs().get(id).await?))
}

async fn kill_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl axum::response::IntoResponse {
    if state.registry.kill(&run_id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

#[derive(Debug, Deserialize)]
struct LogQuery {
    from_seq: Option<i64>,
    #[serde(default = "default_log_limit")]
    limit: i64,
}

fn default_log_limit() -> i64 {
    500
}

#[derive(Debug, Serialize)]
struct LogRowResp {
    seq: i64,
    ts: String,
    stream: String,
    chunk: String,
}

async fn get_run_logs(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<LogQuery>,
) -> Result<Json<Vec<LogRowResp>>, ApiError> {
    let rows = s.store.run_logs().fetch(&id, q.from_seq, q.limit).await?;
    let resp = rows
        .into_iter()
        .map(|r| LogRowResp {
            seq: r.seq,
            ts: r.ts,
            stream: r.stream,
            chunk: String::from_utf8_lossy(&r.chunk).into_owned(),
        })
        .collect();
    Ok(Json(resp))
}

async fn job_stats(
    State(s): State<AppState>,
    Path(job_id): Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match s.store.runs().job_stats(&job_id).await {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request};
    use rsched_store::Store;
    use tower::ServiceExt;

    async fn fresh_state() -> AppState {
        rsched_store::init_drivers();
        let pool = rsched_store::open_pool("sqlite::memory:").await.unwrap();
        let store = Store::with_url(pool, "sqlite::memory:");
        store.migrate().await.unwrap();
        AppState::new(store)
    }

    fn req_json(method: Method, uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn healthz_ok() {
        let app = router(fresh_state().await);
        let r = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
    }

    #[tokio::test]
    async fn create_list_get_job() {
        let app = router(fresh_state().await);
        let body = serde_json::json!({
            "name": "my-job",
            "trigger": {"kind":"cron","expr":"*/5 * * * *"},
            "cmd": "echo hi"
        });
        let r = app
            .clone()
            .oneshot(req_json(Method::POST, "/api/v1/jobs", body))
            .await
            .unwrap();
        assert_eq!(r.status(), 201);

        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/jobs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let bytes = axum::body::to_bytes(r.into_body(), 65536).await.unwrap();
        let jobs: Vec<Job> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "my-job");
    }

    #[tokio::test]
    async fn pause_resume_trigger() {
        let app = router(fresh_state().await);
        let body = serde_json::json!({
            "name": "x",
            "trigger": {"kind":"manual"},
            "cmd": "echo"
        });
        let r = app
            .clone()
            .oneshot(req_json(Method::POST, "/api/v1/jobs", body))
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(r.into_body(), 65536).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id = v["job"]["id"].as_str().unwrap().to_string();

        // pause
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/jobs/{id}/pause"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);

        // trigger
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/jobs/{id}/trigger"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);

        // list runs
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/jobs/{id}/runs"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(r.into_body(), 65536).await.unwrap();
        let runs: Vec<rsched_core::Run> = serde_json::from_slice(&bytes).unwrap();
        // trigger no longer creates a Run row directly — it sets next_fire_at
        // and the tick loop creates the Run. So we expect 0 here.
        assert_eq!(runs.len(), 0);
    }

    #[tokio::test]
    async fn invalid_cron_rejected() {
        let app = router(fresh_state().await);
        let body = serde_json::json!({
            "name": "bad",
            "trigger": {"kind":"cron","expr":""},
            "cmd": "echo"
        });
        let r = app
            .oneshot(req_json(Method::POST, "/api/v1/jobs", body))
            .await
            .unwrap();
        assert!(r.status().is_client_error() || r.status().is_server_error());
    }

    #[tokio::test]
    async fn get_run_and_logs() {
        use rsched_core::Run;
        let state = fresh_state().await;
        let body = serde_json::json!({"name":"log-job","trigger":{"kind":"manual"},"cmd":"echo"});
        let app = router(state.clone());
        let r = app
            .clone()
            .oneshot(req_json(Method::POST, "/api/v1/jobs", body))
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(r.into_body(), 65536).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let job_id: rsched_core::JobId = v["job"]["id"].as_str().unwrap().parse().unwrap();
        let run = Run::new(job_id, 1);
        state.store.runs().insert(&run).await.unwrap();
        let run_id = run.id.to_string();
        state
            .store
            .run_logs()
            .append(&run_id, 0, "2026-01-01T00:00:00Z", "stdout", b"hello")
            .await
            .unwrap();
        state
            .store
            .run_logs()
            .append(&run_id, 1, "2026-01-01T00:00:01Z", "stderr", b"err")
            .await
            .unwrap();
        // GET /runs/:id
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/runs/{run_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        // GET /runs/:id/logs
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/runs/{run_id}/logs"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let bytes = axum::body::to_bytes(r.into_body(), 65536).await.unwrap();
        let logs: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0]["chunk"].as_str().unwrap(), "hello");
        assert_eq!(logs[1]["stream"].as_str().unwrap(), "stderr");
        // from_seq=1
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/runs/{run_id}/logs?from_seq=1"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(r.into_body(), 65536).await.unwrap();
        let logs: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["seq"].as_i64().unwrap(), 1);
    }

    #[tokio::test]
    async fn get_run_404() {
        let app = router(fresh_state().await);
        let r = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/runs/01ARZ3NDEKTSV4RRFFQ69G5FAV")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 404);
    }
}
