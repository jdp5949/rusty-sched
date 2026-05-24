//! Authentication middleware + login/api-key routes.
//!
//! Two credentials supported:
//!   - `rsched_session` cookie (set after `/api/v1/auth/login`)
//!   - `Authorization: Bearer <api_key>` header
//!
//! On success, the request gains an [`AuthCtx`] extension carrying the
//! resolved user id + role. Use the [`AuthUser`] / [`RequireRole`] extractors
//! in handlers.

use crate::{ApiError, AppState};
use axum::extract::{FromRequestParts, State};
use axum::http::header::{AUTHORIZATION, COOKIE, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::Utc;
use rand::rngs::OsRng;
use rand::RngCore;
use rsched_core::{ApiKeyId, Role, UserId};
use serde::{Deserialize, Serialize};

const SESSION_COOKIE: &str = "rsched_session";
const SESSION_TTL_HOURS: i64 = 12;

/// Resolved request authentication context.
#[derive(Debug, Clone, Copy)]
pub struct AuthCtx {
    /// Authenticated user.
    pub user_id: UserId,
    /// User's role.
    pub role: Role,
}

/// Axum extractor for the [`AuthCtx`]. Returns `Unauthorized` if missing.
pub struct AuthUser(pub AuthCtx);

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthCtx>()
            .copied()
            .map(AuthUser)
            .ok_or(ApiError::Unauthorized)
    }
}

/// Require that the request was authenticated and the user has write access.
pub struct RequireWrite(pub AuthCtx);

#[axum::async_trait]
impl<S> FromRequestParts<S> for RequireWrite
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts
            .extensions
            .get::<AuthCtx>()
            .copied()
            .ok_or(ApiError::Unauthorized)?;
        if !ctx.role.can_write() {
            return Err(ApiError::Forbidden);
        }
        Ok(RequireWrite(ctx))
    }
}

/// Require admin access.
pub struct RequireAdmin(pub AuthCtx);

#[axum::async_trait]
impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts
            .extensions
            .get::<AuthCtx>()
            .copied()
            .ok_or(ApiError::Unauthorized)?;
        if !ctx.role.can_admin() {
            return Err(ApiError::Forbidden);
        }
        Ok(RequireAdmin(ctx))
    }
}

/// Generate a cryptographically random URL-safe token of `byte_len` bytes (encoded base64-url-no-pad).
pub fn random_token(byte_len: usize) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let mut buf = vec![0u8; byte_len];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Authentication middleware. Attempts to resolve a session cookie or
/// `Authorization: Bearer` API key. On success, injects [`AuthCtx`]. On
/// failure, leaves the request unauthenticated (handlers decide whether
/// they require auth via the [`AuthUser`] extractor).
pub async fn middleware(
    State(state): State<AppState>,
    mut req: axum::extract::Request,
    next: Next,
) -> Response {
    if let Some(ctx) = resolve(&state, req.headers()).await {
        req.extensions_mut().insert(ctx);
    }
    next.run(req).await
}

async fn resolve(state: &AppState, headers: &HeaderMap) -> Option<AuthCtx> {
    // 1. Bearer token.
    if let Some(authz) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(token) = authz.strip_prefix("Bearer ") {
            if let Some(ctx) = resolve_api_key(state, token).await {
                return Some(ctx);
            }
        }
    }
    // 2. Session cookie.
    if let Some(cookie_header) = headers.get(COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(token) = parse_cookie(cookie_header, SESSION_COOKIE) {
            if let Some(ctx) = resolve_session(state, &token).await {
                return Some(ctx);
            }
        }
    }
    None
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
    for part in header.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if k == name {
                return Some(v.to_string());
            }
        }
    }
    None
}

async fn resolve_session(state: &AppState, token: &str) -> Option<AuthCtx> {
    let (uid, _exp, _csrf) = state
        .store
        .sessions()
        .get_valid(token, Utc::now())
        .await
        .ok()??;
    let user = state.store.users().get(uid).await.ok()?;
    if user.disabled {
        return None;
    }
    Some(AuthCtx {
        user_id: user.id,
        role: user.role,
    })
}

async fn resolve_api_key(state: &AppState, plaintext: &str) -> Option<AuthCtx> {
    let active = state.store.api_keys().all_active(Utc::now()).await.ok()?;
    for (kid, uid, hash) in active {
        if verify(plaintext, &hash).unwrap_or(false) {
            let _ = state.store.api_keys().touch(kid).await;
            let user = state.store.users().get(uid).await.ok()?;
            if user.disabled {
                continue;
            }
            return Some(AuthCtx {
                user_id: user.id,
                role: user.role,
            });
        }
    }
    None
}

// ----- Routes ----------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct LoginReq {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct LoginResp {
    user_id: String,
    username: String,
    role: String,
}

pub(crate) async fn login(
    State(s): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Response, ApiError> {
    let Some((user, pw_hash)) = s.store.users().get_by_username(&req.username).await? else {
        return Err(ApiError::Unauthorized);
    };
    if user.disabled {
        return Err(ApiError::Unauthorized);
    }
    if !verify(&req.password, &pw_hash).unwrap_or(false) {
        return Err(ApiError::Unauthorized);
    }
    let token = random_token(32);
    let exp = Utc::now() + chrono::Duration::hours(SESSION_TTL_HOURS);
    s.store
        .sessions()
        .insert(&token, user.id, exp, None, None)
        .await?;
    s.store
        .audit()
        .record(
            Some(&user.id.to_string()),
            "auth.login",
            "user",
            Some(&user.id.to_string()),
            None,
        )
        .await?;
    let resp = LoginResp {
        user_id: user.id.to_string(),
        username: user.username.clone(),
        role: user.role.as_str().into(),
    };
    let cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        SESSION_TTL_HOURS * 3600
    );
    let mut response = (StatusCode::OK, Json(resp)).into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("valid cookie"),
    );
    Ok(response)
}

pub(crate) async fn logout(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(cookie_header) = headers.get(COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(token) = parse_cookie(cookie_header, SESSION_COOKIE) {
            let _ = s.store.sessions().delete(&token).await;
        }
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_static("rsched_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    Ok(response)
}

#[derive(Debug, Serialize)]
pub(crate) struct MeResp {
    user_id: String,
    username: String,
    role: String,
}

pub(crate) async fn me(
    State(s): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> Result<Json<MeResp>, ApiError> {
    let u = s.store.users().get(ctx.user_id).await?;
    Ok(Json(MeResp {
        user_id: u.id.to_string(),
        username: u.username,
        role: u.role.as_str().into(),
    }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateApiKeyReq {
    name: String,
    #[serde(default)]
    expires_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateApiKeyResp {
    id: String,
    /// The plaintext token — only shown ONCE at creation.
    token: String,
    name: String,
}

pub(crate) async fn create_api_key(
    State(s): State<AppState>,
    AuthUser(ctx): AuthUser,
    Json(req): Json<CreateApiKeyReq>,
) -> Result<(StatusCode, Json<CreateApiKeyResp>), ApiError> {
    let plaintext = random_token(32);
    let key_hash =
        hash(&plaintext, DEFAULT_COST).map_err(|e| ApiError::Validation(format!("bcrypt: {e}")))?;
    let id = ApiKeyId::new();
    s.store
        .api_keys()
        .insert(id, ctx.user_id, &req.name, &key_hash, req.expires_at)
        .await?;
    s.store
        .audit()
        .record(
            Some(&ctx.user_id.to_string()),
            "apikey.create",
            "api_key",
            Some(&id.to_string()),
            Some(&format!(r#"{{"name":"{}"}}"#, req.name.replace('"', "'"))),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResp {
            id: id.to_string(),
            token: plaintext,
            name: req.name,
        }),
    ))
}

pub(crate) async fn list_api_keys(
    State(s): State<AppState>,
    AuthUser(ctx): AuthUser,
) -> Result<Json<Vec<rsched_core::ApiKey>>, ApiError> {
    Ok(Json(s.store.api_keys().list_for_user(ctx.user_id).await?))
}

pub(crate) async fn delete_api_key(
    State(s): State<AppState>,
    AuthUser(ctx): AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    let kid: ApiKeyId = id
        .parse()
        .map_err(|_| ApiError::Validation("bad api key id".into()))?;
    // For now only allow deletion of own keys; admins can list/delete any later.
    let keys = s.store.api_keys().list_for_user(ctx.user_id).await?;
    if !keys.iter().any(|k| k.id == kid) {
        return Err(ApiError::Forbidden);
    }
    s.store.api_keys().delete(kid).await?;
    s.store
        .audit()
        .record(
            Some(&ctx.user_id.to_string()),
            "apikey.delete",
            "api_key",
            Some(&id),
            None,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- User management (admin only) -----------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct CreateUserReq {
    username: String,
    password: String,
    role: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UserResp {
    id: String,
    username: String,
    role: String,
    disabled: bool,
}

pub(crate) async fn create_user(
    State(s): State<AppState>,
    RequireAdmin(ctx): RequireAdmin,
    Json(req): Json<CreateUserReq>,
) -> Result<(StatusCode, Json<UserResp>), ApiError> {
    let role = Role::parse(&req.role)
        .ok_or_else(|| ApiError::Validation(format!("bad role: {}", req.role)))?;
    if req.password.len() < 8 {
        return Err(ApiError::Validation(
            "password must be at least 8 characters".into(),
        ));
    }
    let pw_hash =
        hash(&req.password, DEFAULT_COST).map_err(|e| ApiError::Validation(e.to_string()))?;
    let id = UserId::new();
    s.store
        .users()
        .insert(id, &req.username, &pw_hash, role)
        .await?;
    s.store
        .audit()
        .record(
            Some(&ctx.user_id.to_string()),
            "user.create",
            "user",
            Some(&id.to_string()),
            Some(&format!(
                r#"{{"username":"{}","role":"{}"}}"#,
                req.username.replace('"', "'"),
                role.as_str()
            )),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(UserResp {
            id: id.to_string(),
            username: req.username,
            role: role.as_str().into(),
            disabled: false,
        }),
    ))
}

pub(crate) async fn list_users(
    State(s): State<AppState>,
    RequireAdmin(_): RequireAdmin,
) -> Result<Json<Vec<UserResp>>, ApiError> {
    let users = s.store.users().list().await?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| UserResp {
                id: u.id.to_string(),
                username: u.username,
                role: u.role.as_str().into(),
                disabled: u.disabled,
            })
            .collect(),
    ))
}

pub(crate) async fn list_audit(
    State(s): State<AppState>,
    RequireAdmin(_): RequireAdmin,
) -> Result<Json<Vec<rsched_store::AuditEntry>>, ApiError> {
    Ok(Json(s.store.audit().recent(200).await?))
}

/// Seed a default admin user on first boot if no users exist yet.
///
/// Password sourced from `RSCHED_ADMIN_PASSWORD` env var. Falls back to a
/// random token printed to stdout / tracing — caller must capture it from
/// the log on first boot. Returns the created user id (None if users already
/// existed).
pub async fn seed_admin_if_empty(state: &AppState) -> Result<Option<UserId>, ApiError> {
    let n = state.store.users().count().await?;
    if n > 0 {
        return Ok(None);
    }
    let password = std::env::var("RSCHED_ADMIN_PASSWORD").unwrap_or_else(|_| random_token(16));
    let pw_hash = hash(&password, DEFAULT_COST).map_err(|e| ApiError::Validation(e.to_string()))?;
    let id = UserId::new();
    state
        .store
        .users()
        .insert(id, "admin", &pw_hash, Role::Admin)
        .await?;
    if std::env::var("RSCHED_ADMIN_PASSWORD").is_err() {
        // Generated password — must surface to the operator.
        tracing::warn!(
            target: "rsched::auth",
            user = "admin",
            password = %password,
            "first-run admin user created — set RSCHED_ADMIN_PASSWORD env var on next start to suppress this warning"
        );
    } else {
        tracing::info!(
            target: "rsched::auth",
            user = "admin",
            "first-run admin user created from RSCHED_ADMIN_PASSWORD env var"
        );
    }
    Ok(Some(id))
}
