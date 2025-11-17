use std::{
    sync::LazyLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use axum_auth::AuthBearer;
use moka::sync::Cache;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{error, info, warn};
use wreq::StatusCode;
use yup_oauth2::ServiceAccountKey;

use super::error::ApiError;
use crate::{
    VERSION_INFO,
    claude_code_state::ClaudeCodeState,
    config::{CLEWDR_CONFIG, ClewdrConfig, CookieStatus, KeyStatus},
    persistence,
    services::{
        cookie_actor::CookieActorHandle,
        key_actor::{KeyActorHandle, KeyStatusInfo},
    },
};

const DB_UNAVAILABLE_MESSAGE: &str = "Database storage is unavailable";

/// Cache entry for cookie status responses
#[derive(Clone)]
struct CookieStatusCache {
    data: Value,
    timestamp: u64,
}

/// Query parameters for cookie status endpoint
#[derive(Deserialize)]
pub struct CookieStatusQuery {
    #[serde(default)]
    refresh: bool,
}

/// Global cache for cookie status responses (TTL: 5 minutes)
static COOKIES_CACHE: LazyLock<Cache<String, CookieStatusCache>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(300)) // 5 minutes
        .build()
});

/// Cache key for cookie status
const COOKIE_STATUS_CACHE_KEY: &str = "all_cookies";

#[derive(Deserialize)]
pub struct VertexCredentialPayload {
    pub credential: ServiceAccountKey,
}

#[derive(Deserialize)]
pub struct VertexCredentialDeletePayload {
    pub client_email: String,
}

#[derive(Serialize)]
pub struct VertexCredentialInfo {
    pub client_email: String,
    pub project_id: Option<String>,
    pub private_key_id: Option<String>,
}

async fn ensure_db_writable() -> Result<(), ApiError> {
    let storage = persistence::storage();
    if !storage.is_enabled() {
        return Ok(());
    }

    match storage.status().await {
        Ok(status) => {
            let is_healthy = status
                .get("healthy")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_healthy {
                return Ok(());
            }

            if let Some(detail) = status
                .get("error")
                .and_then(|v| v.as_str())
                .or_else(|| status.get("last_error").and_then(|v| v.as_str()))
            {
                warn!("Database health check failed: {detail}");
            }
        }
        Err(e) => {
            warn!("Database status fetch failed: {}", e);
        }
    }

    Err(ApiError::service_unavailable(DB_UNAVAILABLE_MESSAGE))
}

/// API endpoint to submit a new cookie
/// Validates and adds the cookie to the cookie manager
///
/// # Arguments
/// * `s` - Application state containing event sender
/// * `t` - Auth bearer token for admin authentication
/// * `c` - Cookie status to be submitted
///
/// # Returns
/// * `StatusCode` - HTTP status code indicating success or failure
pub async fn api_post_cookie(
    State(s): State<CookieActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(mut c): Json<CookieStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    ensure_db_writable().await?;
    c.reset_time = None;
    info!("Cookie accepted: {}", c.cookie);
    match s.submit(c).await {
        Ok(_) => {
            info!("Cookie submitted successfully");
            // Clear cache to ensure fresh data on next request
            COOKIES_CACHE.invalidate(COOKIE_STATUS_CACHE_KEY);
            info!("Cookie status cache invalidated after adding new cookie");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            error!("Failed to submit cookie: {}", e);
            Err(ApiError::internal(format!(
                "Failed to submit cookie: {}",
                e
            )))
        }
    }
}

pub async fn api_post_key(
    State(s): State<KeyActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<KeyStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    if !c.key.validate() {
        warn!("Invalid key: {}", c.key);
        return Err(ApiError::bad_request("Invalid key"));
    }
    ensure_db_writable().await?;
    info!("Key accepted: {}", c.key);
    match s.submit(c).await {
        Ok(_) => {
            info!("Key submitted successfully");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            error!("Failed to submit key: {}", e);
            Err(ApiError::internal(format!("Failed to submit key: {}", e)))
        }
    }
}

pub async fn api_get_vertex_credentials(
    AuthBearer(t): AuthBearer,
) -> Result<Json<Vec<VertexCredentialInfo>>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    let infos = CLEWDR_CONFIG
        .load()
        .vertex
        .credential_list()
        .into_iter()
        .map(|cred| VertexCredentialInfo {
            client_email: cred.client_email.clone(),
            project_id: cred.project_id.clone(),
            private_key_id: cred.private_key_id.clone(),
        })
        .collect();

    Ok(Json(infos))
}

pub async fn api_post_vertex_credential(
    AuthBearer(t): AuthBearer,
    Json(payload): Json<VertexCredentialPayload>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    ensure_db_writable().await?;
    let client_email = payload.credential.client_email.clone();
    if client_email.trim().is_empty() {
        return Err(ApiError::bad_request("client_email is required"));
    }

    CLEWDR_CONFIG.rcu(|config| {
        let mut new_config = ClewdrConfig::clone(config);
        new_config
            .vertex
            .credentials
            .retain(|cred| !cred.client_email.eq_ignore_ascii_case(&client_email));
        new_config
            .vertex
            .credentials
            .push(payload.credential.clone());
        new_config = new_config.validate();
        new_config
    });

    if let Err(e) = CLEWDR_CONFIG.load().save().await {
        error!("Failed to persist vertex credential: {}", e);
        return Err(ApiError::internal(format!(
            "Failed to persist vertex credential: {}",
            e
        )));
    }

    info!("Vertex credential accepted: {}", client_email);
    Ok(StatusCode::OK)
}

pub async fn api_delete_vertex_credential(
    AuthBearer(t): AuthBearer,
    Json(payload): Json<VertexCredentialDeletePayload>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    ensure_db_writable().await?;

    let exists = CLEWDR_CONFIG
        .load()
        .vertex
        .credential_list()
        .iter()
        .any(|cred| {
            cred.client_email
                .eq_ignore_ascii_case(&payload.client_email)
        });

    if !exists {
        return Err(ApiError::bad_request("Credential not found"));
    }

    CLEWDR_CONFIG.rcu(|config| {
        let mut new_config = ClewdrConfig::clone(config);
        new_config.vertex.credentials.retain(|cred| {
            !cred
                .client_email
                .eq_ignore_ascii_case(&payload.client_email)
        });
        new_config = new_config.validate();
        new_config
    });

    if let Err(e) = CLEWDR_CONFIG.load().save().await {
        error!("Failed to delete vertex credential: {}", e);
        return Err(ApiError::internal(format!(
            "Failed to delete vertex credential: {}",
            e
        )));
    }

    info!("Vertex credential deleted: {}", payload.client_email);
    Ok(StatusCode::NO_CONTENT)
}

/// API endpoint to retrieve all cookies and their status
/// Gets information about valid, exhausted, and invalid cookies
///
/// # Arguments
/// * `s` - Application state containing event sender
/// * `t` - Auth bearer token for admin authentication
/// * `query` - Query parameters including optional refresh flag
///
/// # Returns
/// * `Result<(HeaderMap, Json<Value>), ApiError>` - Response with cache headers and cookie status
pub async fn api_get_cookies(
    State(s): State<CookieActorHandle>,
    AuthBearer(t): AuthBearer,
    Query(query): Query<CookieStatusQuery>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    let mut headers = HeaderMap::new();

    // Check cache if not force refreshing
    if !query.refresh
        && let Some(cached) = COOKIES_CACHE.get(COOKIE_STATUS_CACHE_KEY)
    {
        headers.insert("X-Cache-Status", HeaderValue::from_static("HIT"));
        headers.insert(
            "X-Cache-Timestamp",
            HeaderValue::from_str(&cached.timestamp.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("0")),
        );
        info!("Cookie status served from cache");
        return Ok((headers, Json(cached.data)));
    }

    // Cache miss or force refresh - fetch fresh data
    match s.get_status().await {
        Ok(status) => {
            let valid = augment_utilization(status.valid, s.clone()).await;
            let exhausted = augment_utilization(status.exhausted, s.clone()).await;
            let invalid = status
                .invalid
                .into_iter()
                .map(|u| serde_json::to_value(u).unwrap_or(json!({})))
                .collect::<Vec<_>>();

            let response_data = json!({
                "valid": valid,
                "exhausted": exhausted,
                "invalid": invalid,
            });

            // Store in cache
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|e| {
                    warn!("System time error: {}, using fallback timestamp", e);
                    Duration::from_secs(0)
                })
                .as_secs();

            COOKIES_CACHE.insert(
                COOKIE_STATUS_CACHE_KEY.to_string(),
                CookieStatusCache {
                    data: response_data.clone(),
                    timestamp,
                },
            );

            headers.insert("X-Cache-Status", HeaderValue::from_static("MISS"));
            headers.insert(
                "X-Cache-Timestamp",
                HeaderValue::from_str(&timestamp.to_string())
                    .unwrap_or_else(|_| HeaderValue::from_static("0")),
            );

            if query.refresh {
                info!("Cookie status force refreshed");
            } else {
                info!("Cookie status fetched and cached");
            }

            Ok((headers, Json(response_data)))
        }
        Err(e) => Err(ApiError::internal(format!(
            "Failed to get cookie status: {}",
            e
        ))),
    }
}

pub async fn api_get_keys(
    State(s): State<KeyActorHandle>,
    AuthBearer(t): AuthBearer,
) -> Result<Json<KeyStatusInfo>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    match s.get_status().await {
        Ok(status) => Ok(Json(status)),
        Err(e) => Err(ApiError::internal(format!(
            "Failed to get keys status: {}",
            e
        ))),
    }
}

/// API endpoint to delete a specific cookie
/// Removes the cookie from all collections in the cookie manager
///
/// # Arguments
/// * `s` - Application state containing event sender
/// * `t` - Auth bearer token for admin authentication
/// * `c` - Cookie status to be deleted
///
/// # Returns
/// * `Result<StatusCode, (StatusCode, Json<serde_json::Value>)>` - Success status or error
pub async fn api_delete_cookie(
    State(s): State<CookieActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<CookieStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    ensure_db_writable().await?;

    match s.delete_cookie(c.to_owned()).await {
        Ok(_) => {
            info!("Cookie deleted successfully: {}", c.cookie);
            // Clear cache to ensure fresh data on next request
            COOKIES_CACHE.invalidate(COOKIE_STATUS_CACHE_KEY);
            info!("Cookie status cache invalidated");
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            error!("Failed to delete cookie: {}", e);
            Err(ApiError::internal(format!(
                "Failed to delete cookie: {}",
                e
            )))
        }
    }
}

pub async fn api_delete_key(
    State(s): State<KeyActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<KeyStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    if !c.key.validate() {
        warn!("Invalid key: {}", c.key);
        return Err(ApiError::bad_request("Invalid key"));
    }

    ensure_db_writable().await?;

    match s.delete_key(c.to_owned()).await {
        Ok(_) => {
            info!("Key deleted successfully: {}", c.key);
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            error!("Failed to delete key: {}", e);
            Err(ApiError::internal(format!("Failed to delete key: {}", e)))
        }
    }
}

/// API endpoint to get the application version information
///
/// # Returns
/// * `String` - Version information string
pub async fn api_version() -> String {
    VERSION_INFO.to_string()
}

/// API endpoint to verify authentication
/// Checks if the provided token is valid for admin access
///
/// # Arguments
/// * `t` - Auth bearer token to verify
///
/// # Returns
/// * `StatusCode` - OK if authorized, UNAUTHORIZED otherwise
pub async fn api_auth(AuthBearer(t): AuthBearer) -> StatusCode {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return StatusCode::UNAUTHORIZED;
    }
    info!("Auth token accepted,");
    StatusCode::OK
}

const MODEL_LIST: [&str; 10] = [
    "claude-3-7-sonnet-20250219",
    "claude-3-7-sonnet-20250219-thinking",
    "claude-sonnet-4-20250514",
    "claude-sonnet-4-20250514-thinking",
    "claude-sonnet-4-5-20250929",
    "claude-sonnet-4-5-20250929-thinking",
    "claude-opus-4-20250514",
    "claude-opus-4-20250514-thinking",
    "claude-opus-4-1-20250805",
    "claude-opus-4-1-20250805-thinking",
];

/// API endpoint to get the list of available models
/// Retrieves the list of models from the configuration
pub async fn api_get_models() -> Json<Value> {
    let data: Vec<Value> = MODEL_LIST
        .iter()
        .map(|model| {
            json!({
                "id": model,
                "object": "model",
                "created": 0,
                "owned_by": "clewdr",
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "object": "list",
        "data": data,
    }))
}

// ------------------------------
// Ephemeral org usage enrichment
// ------------------------------
use futures::{StreamExt, stream};
use http::HeaderValue;

async fn augment_utilization(cookies: Vec<CookieStatus>, handle: CookieActorHandle) -> Vec<Value> {
    let concurrency = 5usize;
    stream::iter(cookies.into_iter().map(move |cookie| {
        let handle = handle.clone();
        async move {
            let base = serde_json::to_value(&cookie).unwrap_or(json!({}));
            match fetch_usage_percent(cookie, handle).await {
                Some((
                    five_hour,
                    five_reset,
                    seven_day,
                    seven_reset,
                    seven_day_opus,
                    opus_reset,
                )) => {
                    let mut obj = base;
                    obj["session_utilization"] = json!(five_hour);
                    obj["session_resets_at"] = json!(five_reset);
                    obj["seven_day_utilization"] = json!(seven_day);
                    obj["seven_day_resets_at"] = json!(seven_reset);
                    obj["seven_day_opus_utilization"] = json!(seven_day_opus);
                    obj["seven_day_opus_resets_at"] = json!(opus_reset);
                    obj
                }
                None => base,
            }
        }
    }))
    .buffer_unordered(concurrency)
    .collect::<Vec<_>>()
    .await
}

async fn fetch_usage_percent(
    cookie: CookieStatus,
    handle: CookieActorHandle,
) -> Option<(
    u32,
    Option<String>,
    u32,
    Option<String>,
    u32,
    Option<String>,
)> {
    let mut state = ClaudeCodeState::from_cookie(handle, cookie).ok()?;
    let usage = state.fetch_usage_metrics().await.ok()?;
    state.return_cookie(None).await;
    let five = usage
        .get("five_hour")
        .and_then(|o| o.get("utilization"))
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as u32)
        .unwrap_or(0);
    let five_reset = usage
        .get("five_hour")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let seven = usage
        .get("seven_day")
        .and_then(|o| o.get("utilization"))
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as u32)
        .unwrap_or(0);
    let seven_reset = usage
        .get("seven_day")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let seven_opus = usage
        .get("seven_day_opus")
        .and_then(|o| o.get("utilization"))
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as u32)
        .unwrap_or(0);
    let opus_reset = usage
        .get("seven_day_opus")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Some((five, five_reset, seven, seven_reset, seven_opus, opus_reset))
}
