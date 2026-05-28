use axum::extract::State;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::debug;

use crate::error::AppError;
use crate::middleware::auth::ClientKey;
use crate::state::AppState;

pub async fn rate_limit(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    let client_key = req
        .extensions()
        .get::<ClientKey>()
        .map(|k| k.0.clone())
        .unwrap_or_default();

    match state.rate_limiter.check(&client_key) {
        Ok(remaining) => {
            debug!(remaining = remaining, "Rate limit check passed");
            let mut response = next.run(req).await;
            let headers = response.headers_mut();
            headers.insert(
                "x-ratelimit-remaining",
                HeaderValue::from_str(&remaining.to_string()).unwrap(),
            );
            Ok(response)
        }
        Err(retry_after) => {
            let body = serde_json::json!({
                "error": {
                    "message": "Rate limit exceeded",
                    "type": "rate_limit_error",
                }
            });
            let response = (
                StatusCode::TOO_MANY_REQUESTS,
                [("retry-after", format!("{:.0}", retry_after.ceil()))],
                axum::Json(body),
            )
                .into_response();
            Ok(response)
        }
    }
}
