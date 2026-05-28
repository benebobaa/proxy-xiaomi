use axum::extract::State;
use axum::http::{header, Request};
use axum::middleware::Next;
use axum::response::Response;
use tracing::debug;

use crate::config::Config;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct ClientKey(pub String);

pub async fn auth(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    // Extract key first, then drop the borrow on req
    let key = {
        let auth_header = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        let api_key_header = req
            .headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok());

        auth_header
            .or(api_key_header)
            .map(|k| k.to_string())
    };

    match key {
        Some(k) if state.client_keys.read().unwrap().contains(&k) => {
            debug!(client_key = %Config::mask_key(&k), "Authenticated request");
            req.extensions_mut().insert(ClientKey(k));
            Ok(next.run(req).await)
        }
        _ => Err(AppError::Unauthorized),
    }
}
