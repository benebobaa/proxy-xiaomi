use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;

use crate::state::AppState;

/// HTTP Basic Auth middleware for the `/admin/*` routes.
///
/// If `XIAOMI_PROXY_ADMIN_USERNAME` / `XIAOMI_PROXY_ADMIN_PASSWORD` are not
/// configured the middleware is a no-op (all requests pass through), so
/// deployments without those env vars continue to work the same as before.
///
/// When credentials are configured every request to a protected route must
/// supply a valid `Authorization: Basic <base64(user:pass)>` header. Browsers
/// that receive a 401 with `WWW-Authenticate: Basic realm="Admin Console"` will
/// automatically open the native login dialog.
pub async fn basic_auth(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let username = &state.config.admin.username;
    let password = &state.config.admin.password;

    // If not configured, skip auth entirely.
    if username.is_empty() || password.is_empty() {
        return next.run(req).await;
    }

    // Build the expected base64-encoded credential string.
    let expected_raw = format!("{}:{}", username, password);
    let expected_b64 = base64::engine::general_purpose::STANDARD.encode(expected_raw.as_bytes());

    // Extract the Authorization header value.
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .map(|s| s.trim().to_string());

    match provided {
        Some(b64) if b64 == expected_b64 => next.run(req).await,
        _ => unauthorized_response(),
    }
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [
            (header::WWW_AUTHENTICATE, "Basic realm=\"Admin Console\", charset=\"UTF-8\""),
            (header::CONTENT_TYPE, "application/json"),
        ],
        r#"{"error":{"message":"Unauthorized: admin credentials required","type":"authentication_error"}}"#,
    )
        .into_response()
}
