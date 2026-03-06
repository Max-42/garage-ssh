use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, error};

use crate::config::AppConfig;
use crate::state::AppState;

const STATE_PATH: &str = "/data/state.json";
const CONFIG_PATH: &str = "/data/options.json";

#[derive(Clone)]
struct WebState {
    app_state: Arc<RwLock<AppState>>,
    app_config: Arc<RwLock<AppConfig>>,
}

pub async fn run(
    state: Arc<RwLock<AppState>>,
    config: Arc<RwLock<AppConfig>>,
) -> anyhow::Result<()> {
    let web_state = WebState {
        app_state: state,
        app_config: config,
    };
    
    let app = Router::new()
        .route("/", get(index_page))
        .route("/api/state", get(get_state))
        .route("/api/trusted-keys", get(get_trusted_keys))
        .route("/api/untrusted-keys", get(get_untrusted_keys))
        .route("/api/trust-key", post(trust_key))
        .route("/api/revoke-key", post(revoke_key))
        .route("/api/delete-untrusted-key", post(delete_untrusted_key))
        .route("/api/tofu/activate", post(activate_tofu))
        .route("/api/tofu/deactivate", post(deactivate_tofu))
        .route("/api/tofu/status", get(tofu_status))
        .route("/api/logs", get(get_logs))
        .route("/api/config", get(get_config))
        .with_state(web_state);
    
    let addr = "0.0.0.0:8099";
    info!("Web UI (ingress) listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

// === API Types ===

#[derive(Serialize)]
struct TrustedKeyResponse {
    id: String,
    fingerprint: String,
    key_type: String,
    username: String,
    device_name: String,
    trusted_at: String,
    last_used: Option<String>,
    use_count: u64,
}

#[derive(Serialize)]
struct UntrustedKeyResponse {
    id: String,
    fingerprint: String,
    key_type: String,
    ssh_username: String,
    first_seen: String,
    last_seen: String,
    attempt_count: u64,
    device_info: Option<DeviceInfoResponse>,
}

#[derive(Serialize)]
struct DeviceInfoResponse {
    device_name: Option<String>,
    device_model: Option<String>,
    device_os: Option<String>,
}

#[derive(Serialize)]
struct LogEntryResponse {
    timestamp: String,
    fingerprint: String,
    ssh_username: String,
    result: String,
    client_ip: String,
}

#[derive(Serialize)]
struct TofuStatusResponse {
    active: bool,
    remaining_seconds: Option<i64>,
}

#[derive(Deserialize)]
struct TrustKeyRequest {
    fingerprint: String,
    username: String,
    device_name: String,
}

#[derive(Deserialize)]
struct FingerprintRequest {
    fingerprint: String,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

// === Route Handlers ===

async fn get_state(State(web): State<WebState>) -> impl IntoResponse {
    let state = web.app_state.read().await;
    let tofu_active = state.tofu.is_active();
    
    let response = serde_json::json!({
        "trusted_keys_count": state.trusted_keys.len(),
        "untrusted_keys_count": state.untrusted_keys.len(),
        "log_entries_count": state.connection_log.len(),
        "tofu_active": tofu_active,
    });
    
    Json(response)
}

async fn get_trusted_keys(State(web): State<WebState>) -> impl IntoResponse {
    let state = web.app_state.read().await;
    let keys: Vec<TrustedKeyResponse> = state.trusted_keys.iter().map(|k| TrustedKeyResponse {
        id: k.id.to_string(),
        fingerprint: k.fingerprint.clone(),
        key_type: k.key_type.clone(),
        username: k.username.clone(),
        device_name: k.device_name.clone(),
        trusted_at: k.trusted_at.to_rfc3339(),
        last_used: k.last_used.map(|t| t.to_rfc3339()),
        use_count: k.use_count,
    }).collect();
    
    Json(keys)
}

async fn get_untrusted_keys(State(web): State<WebState>) -> impl IntoResponse {
    let state = web.app_state.read().await;
    let keys: Vec<UntrustedKeyResponse> = state.untrusted_keys.iter().map(|k| UntrustedKeyResponse {
        id: k.id.to_string(),
        fingerprint: k.fingerprint.clone(),
        key_type: k.key_type.clone(),
        ssh_username: k.ssh_username.clone(),
        first_seen: k.first_seen.to_rfc3339(),
        last_seen: k.last_seen.to_rfc3339(),
        attempt_count: k.attempt_count,
        device_info: k.last_payload.as_ref().map(|p| DeviceInfoResponse {
            device_name: p.device_name.clone(),
            device_model: p.device_model.clone(),
            device_os: p.device_os.clone(),
        }),
    }).collect();
    
    Json(keys)
}

async fn trust_key(
    State(web): State<WebState>,
    Json(req): Json<TrustKeyRequest>,
) -> impl IntoResponse {
    let username = crate::sanitize::sanitize_short_string(&req.username);
    let device_name = crate::sanitize::sanitize_short_string(&req.device_name);
    
    let mut state = web.app_state.write().await;
    match state.trust_key(&req.fingerprint, username, device_name) {
        Ok(()) => {
            if let Err(e) = state.save(STATE_PATH) {
                error!("Failed to save state: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
                    success: false,
                    message: format!("Failed to save: {}", e),
                }));
            }
            (StatusCode::OK, Json(ApiResponse {
                success: true,
                message: "Key trusted successfully".to_string(),
            }))
        }
        Err(e) => {
            (StatusCode::NOT_FOUND, Json(ApiResponse {
                success: false,
                message: format!("Error: {}", e),
            }))
        }
    }
}

async fn revoke_key(
    State(web): State<WebState>,
    Json(req): Json<FingerprintRequest>,
) -> impl IntoResponse {
    let mut state = web.app_state.write().await;
    match state.revoke_key(&req.fingerprint) {
        Ok(()) => {
            if let Err(e) = state.save(STATE_PATH) {
                error!("Failed to save state: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
                    success: false,
                    message: format!("Failed to save: {}", e),
                }));
            }
            (StatusCode::OK, Json(ApiResponse {
                success: true,
                message: "Key revoked successfully".to_string(),
            }))
        }
        Err(e) => {
            (StatusCode::NOT_FOUND, Json(ApiResponse {
                success: false,
                message: format!("Error: {}", e),
            }))
        }
    }
}

async fn delete_untrusted_key(
    State(web): State<WebState>,
    Json(req): Json<FingerprintRequest>,
) -> impl IntoResponse {
    let mut state = web.app_state.write().await;
    match state.delete_untrusted_key(&req.fingerprint) {
        Ok(()) => {
            if let Err(e) = state.save(STATE_PATH) {
                error!("Failed to save state: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse {
                    success: false,
                    message: format!("Failed to save: {}", e),
                }));
            }
            (StatusCode::OK, Json(ApiResponse {
                success: true,
                message: "Untrusted key deleted".to_string(),
            }))
        }
        Err(e) => {
            (StatusCode::NOT_FOUND, Json(ApiResponse {
                success: false,
                message: format!("Error: {}", e),
            }))
        }
    }
}

async fn activate_tofu(State(web): State<WebState>) -> impl IntoResponse {
    let timeout = {
        let config = web.app_config.read().await;
        config.tofu_timeout_sec
    };
    
    let mut state = web.app_state.write().await;
    state.tofu.activate(timeout);
    
    if let Err(e) = state.save(STATE_PATH) {
        error!("Failed to save state: {}", e);
    }
    
    Json(ApiResponse {
        success: true,
        message: format!("TOFU mode activated for {} seconds", timeout),
    })
}

async fn deactivate_tofu(State(web): State<WebState>) -> impl IntoResponse {
    let mut state = web.app_state.write().await;
    state.tofu.deactivate();
    
    if let Err(e) = state.save(STATE_PATH) {
        error!("Failed to save state: {}", e);
    }
    
    Json(ApiResponse {
        success: true,
        message: "TOFU mode deactivated".to_string(),
    })
}

async fn tofu_status(State(web): State<WebState>) -> impl IntoResponse {
    let state = web.app_state.read().await;
    let active = state.tofu.is_active();
    let remaining = if active {
        state.tofu.activated_at.map(|at| {
            let elapsed = chrono::Utc::now().signed_duration_since(at).num_seconds();
            (state.tofu.timeout_sec as i64 - elapsed).max(0)
        })
    } else {
        None
    };
    
    Json(TofuStatusResponse {
        active,
        remaining_seconds: remaining,
    })
}

async fn get_logs(State(web): State<WebState>) -> impl IntoResponse {
    let state = web.app_state.read().await;
    // Return last 200 log entries, most recent first
    let logs: Vec<LogEntryResponse> = state.connection_log
        .iter()
        .rev()
        .take(200)
        .map(|e| LogEntryResponse {
            timestamp: e.timestamp.to_rfc3339(),
            fingerprint: e.fingerprint.clone(),
            ssh_username: e.ssh_username.clone(),
            result: format!("{}", e.result),
            client_ip: e.client_ip.clone(),
        })
        .collect();
    
    Json(logs)
}

async fn get_config(State(web): State<WebState>) -> impl IntoResponse {
    let config = web.app_config.read().await;
    Json(serde_json::json!({
        "ssh_port": config.ssh_port,
        "geofence_radius_km": config.geofence_radius_km,
        "home_latitude": config.home_latitude,
        "home_longitude": config.home_longitude,
        "tofu_timeout_sec": config.tofu_timeout_sec,
        "untrusted_key_retention_days": config.untrusted_key_retention_days,
        "expected_json_version": config.expected_json_version,
        "log_level": config.log_level,
    }))
}

async fn index_page(State(web): State<WebState>) -> impl IntoResponse {
    Html(include_str!("../web/index.html"))
}
