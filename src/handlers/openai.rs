use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Extension;
use axum::Json;
use bytes::Bytes;
use serde_json::Value;
use std::time::Instant;
use tracing::info;

use crate::error::AppError;
use crate::middleware::auth::ClientKey;
use crate::proxy::forwarder::{self, Protocol};
use crate::state::AppState;

pub async fn chat_completions(
    State(state): State<AppState>,
    Extension(client_key): Extension<ClientKey>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, AppError> {
    let is_streaming = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let body_bytes = serde_json::to_vec(&body)?;
    let start = Instant::now();

    if is_streaming {
        let response = forwarder::forward_streaming(
            &state,
            Protocol::OpenAI,
            "/chat/completions",
            headers,
            Bytes::from(body_bytes),
        )
        .await?;

        info!(
            model = %model,
            client_key = %crate::config::Config::mask_key(&client_key.0),
            stream = true,
            "Streaming chat completion started"
        );

        Ok(response)
    } else {
        let (resp_body, status, _resp_headers): (Bytes, StatusCode, _) =
            forwarder::forward_request(
                &state,
                Protocol::OpenAI,
                "/chat/completions",
                headers,
                Bytes::from(body_bytes),
            )
            .await?;

        let latency = start.elapsed();

        let usage = extract_openai_usage(&resp_body);
        info!(
            model = %model,
            client_key = %crate::config::Config::mask_key(&client_key.0),
            status = status.as_u16(),
            latency_ms = latency.as_millis() as u64,
            prompt_tokens = usage.0,
            completion_tokens = usage.1,
            "Chat completion completed"
        );

        let db = state.db.clone();
        let ck = client_key.0.clone();
        let m = model.clone();
        let (pt, ct, tt) = usage;
        tokio::spawn(async move {
            let _ = db
                .record_request(
                    &ck,
                    "openai",
                    "/chat/completions",
                    Some(&m),
                    status.as_u16(),
                    latency.as_millis() as u64,
                    pt,
                    ct,
                    tt,
                    false,
                )
                .await;
        });

        Ok((
            status,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            resp_body,
        )
            .into_response())
    }
}

pub async fn completions(
    State(state): State<AppState>,
    Extension(_client_key): Extension<ClientKey>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, AppError> {
    let body_bytes = serde_json::to_vec(&body)?;
    let (resp_body, status, _resp_headers): (Bytes, StatusCode, _) =
        forwarder::forward_request(&state, Protocol::OpenAI, "/completions", headers, Bytes::from(body_bytes))
            .await?;

    Ok((
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        resp_body,
    )
        .into_response())
}

pub async fn embeddings(
    State(state): State<AppState>,
    Extension(_client_key): Extension<ClientKey>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, AppError> {
    let body_bytes = serde_json::to_vec(&body)?;
    let (resp_body, status, _resp_headers): (Bytes, StatusCode, _) =
        forwarder::forward_request(&state, Protocol::OpenAI, "/embeddings", headers, Bytes::from(body_bytes))
            .await?;

    Ok((
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        resp_body,
    )
        .into_response())
}

pub async fn list_models(
    State(state): State<AppState>,
    Extension(_client_key): Extension<ClientKey>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let (resp_body, status, _resp_headers): (Bytes, StatusCode, _) =
        forwarder::forward_request(&state, Protocol::OpenAI, "/models", headers, Bytes::new())
            .await?;

    Ok((
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        resp_body,
    )
        .into_response())
}

fn extract_openai_usage(body: &Bytes) -> (Option<u32>, Option<u32>, Option<u32>) {
    match serde_json::from_slice::<Value>(body) {
        Ok(json) => {
            let usage = json.get("usage");
            let pt = usage
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let ct = usage
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let tt = usage
                .and_then(|u| u.get("total_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            (pt, ct, tt)
        }
        Err(_) => (None, None, None),
    }
}
