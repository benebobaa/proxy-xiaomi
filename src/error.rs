use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Unauthorized: invalid API key")]
    Unauthorized,
    #[error("Rate limit exceeded")]
    #[allow(dead_code)]
    RateLimited,
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Downstream error: {0}")]
    Downstream(String),
    #[error("No available downstream keys")]
    NoKeysAvailable,
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Downstream(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::BadRequest(e.to_string())
    }
}

impl From<String> for AppError {
    fn from(e: String) -> Self {
        AppError::Internal(anyhow::anyhow!(e))
    }
}

impl From<axum::http::Error> for AppError {
    fn from(e: axum::http::Error) -> Self {
        AppError::Internal(anyhow::anyhow!(e))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication_error"),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limit_error"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "invalid_request_error"),
            AppError::Downstream(_) => (StatusCode::BAD_GATEWAY, "downstream_error"),
            AppError::NoKeysAvailable => (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"),
            AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };

        let body = json!({
            "error": {
                "message": self.to_string(),
                "type": error_type,
            }
        });

        (status, axum::Json(body)).into_response()
    }
}
