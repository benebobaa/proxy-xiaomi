use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;
use tracing::info;

use crate::config::Config;
use crate::middleware::auth::ClientKey;

pub async fn logging(
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let client_key = req
        .extensions()
        .get::<ClientKey>()
        .map(|k| Config::mask_key(&k.0))
        .unwrap_or_else(|| "unknown".to_string());

    let start = Instant::now();
    let response = next.run(req).await;
    let latency = start.elapsed();

    info!(
        method = %method,
        uri = %uri,
        status = response.status().as_u16(),
        latency_ms = latency.as_millis() as u64,
        client_key = %client_key,
        "Request completed"
    );

    response
}
