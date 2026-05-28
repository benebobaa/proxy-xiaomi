use axum::body::Body;
use axum::http::{header, HeaderMap, Response, StatusCode};
use bytes::Bytes;
use tracing::warn;

use crate::config::Config;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone, Copy)]
pub enum Protocol {
    OpenAI,
    Anthropic,
}

impl Protocol {
    pub fn base_url<'a>(&self, config: &'a Config) -> &'a str {
        match self {
            Protocol::OpenAI => &config.downstream.openai_base_url,
            Protocol::Anthropic => &config.downstream.anthropic_base_url,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "openai",
            Protocol::Anthropic => "anthropic",
        }
    }
}

/// Forward a non-streaming request to downstream.
pub async fn forward_request(
    state: &AppState,
    protocol: Protocol,
    path: &str,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(Bytes, StatusCode, HeaderMap), AppError> {
    let (resp, _acquired) = crate::proxy::retry::with_retry(
        state.config.downstream.max_retries,
        state.config.downstream.retry_base_ms,
        &state.key_pool,
        |downstream_key| {
            let url = format!("{}{}", protocol.base_url(&state.config), path);
            let downstream_headers = build_downstream_headers(&headers, &downstream_key, protocol);

            state
                .http_client
                .post(&url)
                .headers(downstream_headers)
                .body(body.clone())
                .send()
        },
    )
    .await?;

    let status = resp.status();
    let resp_headers = resp.headers().clone();
    let resp_body = resp.bytes().await?;

    Ok((resp_body, status, resp_headers))
}

/// Forward a streaming request to downstream, returning a streaming Response.
pub async fn forward_streaming(
    state: &AppState,
    protocol: Protocol,
    path: &str,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response<Body>, AppError> {
    let url = format!(
        "{}{}",
        protocol.base_url(&state.config),
        path
    );

    // For streaming, we don't retry — just acquire a key and go
    let acquired = state.key_pool.acquire_key()?;
    let downstream_key = acquired.key.clone();

    let downstream_headers = build_downstream_headers(&headers, &downstream_key, protocol);

    let resp = state
        .http_client
        .post(&url)
        .headers(downstream_headers)
        .body(body)
        .send()
        .await?;

    if !resp.status().is_success() {
        state.key_pool.report_failure(&acquired);
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        warn!(status = status.as_u16(), body = %err_body, "Downstream streaming error");
        return Err(AppError::Downstream(format!(
            "HTTP {}: {}",
            status.as_u16(),
            err_body
        )));
    }

    state.key_pool.report_success(&acquired);

    let status = resp.status();
    let resp_headers = resp.headers().clone();

    // Pipe the downstream SSE stream directly to the client
    let byte_stream = resp.bytes_stream();
    let body = Body::from_stream(byte_stream);

    let mut response = Response::builder().status(status);
    for (key, value) in &resp_headers {
        response = response.header(key, value);
    }
    Ok(response.body(body)?)
}

fn build_downstream_headers(
    original: &HeaderMap,
    downstream_key: &str,
    protocol: Protocol,
) -> HeaderMap {
    let mut headers = original.clone();

    // Remove client auth
    headers.remove(header::AUTHORIZATION);
    headers.remove("x-api-key");

    // Remove host header to allow target server routing
    headers.remove(header::HOST);

    // Remove content-length header to let reqwest calculate it for the new body
    headers.remove(header::CONTENT_LENGTH);

    // Set downstream auth
    let auth_value = format!("Bearer {}", downstream_key).parse().unwrap();
    headers.insert(header::AUTHORIZATION, auth_value);

    // Anthropic uses x-api-key header
    if matches!(protocol, Protocol::Anthropic) {
        if let Ok(val) = downstream_key.parse() {
            headers.insert("x-api-key", val);
        }
    }

    headers
}
