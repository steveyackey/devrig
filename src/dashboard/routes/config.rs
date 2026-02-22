use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::DashboardState;

#[derive(Serialize)]
pub struct ConfigResponse {
    pub content: String,
    pub hash: String,
}

#[derive(Deserialize)]
pub struct ConfigUpdateRequest {
    pub content: String,
    pub hash: String,
}

#[derive(Serialize)]
pub struct ConfigErrorResponse {
    pub error: String,
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn get_config(State(state): State<DashboardState>) -> impl IntoResponse {
    let config_path = match &state.config_path {
        Some(p) => p.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ConfigErrorResponse {
                    error: "config path not available".to_string(),
                }),
            )
                .into_response();
        }
    };

    match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => {
            let hash = compute_hash(&content);
            Json(ConfigResponse { content, hash }).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigErrorResponse {
                error: format!("failed to read config: {}", e),
            }),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct ConfigValidateRequest {
    pub content: String,
}

#[derive(Serialize)]
pub struct ConfigValidateResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn validate_config(Json(req): Json<ConfigValidateRequest>) -> impl IntoResponse {
    match req.content.parse::<toml::Table>() {
        Ok(_) => Json(ConfigValidateResponse {
            valid: true,
            error: None,
        })
        .into_response(),
        Err(e) => Json(ConfigValidateResponse {
            valid: false,
            error: Some(format!("{}", e)),
        })
        .into_response(),
    }
}

pub async fn update_config(
    State(state): State<DashboardState>,
    Json(req): Json<ConfigUpdateRequest>,
) -> impl IntoResponse {
    let config_path = match &state.config_path {
        Some(p) => p.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ConfigErrorResponse {
                    error: "config path not available".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Validate TOML syntax
    if let Err(e) = req.content.parse::<toml::Table>() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ConfigErrorResponse {
                error: format!("invalid TOML: {}", e),
            }),
        )
            .into_response();
    }

    // Check optimistic concurrency: read current content and compare hash
    let current_content = match tokio::fs::read_to_string(&config_path).await {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ConfigErrorResponse {
                    error: format!("failed to read current config: {}", e),
                }),
            )
                .into_response();
        }
    };

    let current_hash = compute_hash(&current_content);
    if req.hash != current_hash {
        return (
            StatusCode::CONFLICT,
            Json(ConfigErrorResponse {
                error: "config has been modified externally; please reload".to_string(),
            }),
        )
            .into_response();
    }

    // Write the new content
    match tokio::fs::write(&config_path, req.content.as_bytes()).await {
        Ok(()) => {
            let new_hash = compute_hash(&req.content);
            Json(ConfigResponse {
                content: req.content,
                hash: new_hash,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigErrorResponse {
                error: format!("failed to write config: {}", e),
            }),
        )
            .into_response(),
    }
}
