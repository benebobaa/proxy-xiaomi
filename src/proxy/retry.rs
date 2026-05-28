use axum::http::StatusCode;
use rand::Rng;
use tracing::warn;

use crate::error::AppError;
use crate::proxy::key_pool::{AcquiredKey, KeyPool};

pub async fn with_retry<F, Fut>(
    max_retries: u32,
    base_delay_ms: u64,
    key_pool: &KeyPool,
    mut make_request: F,
) -> Result<(reqwest::Response, AcquiredKey), AppError>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let mut last_error: Option<AppError> = None;

    for attempt in 0..=max_retries {
        let acquired = key_pool.acquire_key()?;

        match make_request(acquired.key.clone()).await {
            Ok(resp) if resp.status().is_success() => {
                key_pool.report_success(&acquired);
                return Ok((resp, acquired));
            }
            Ok(resp) if is_retryable(resp.status()) => {
                key_pool.report_failure(&acquired);
                let delay = calculate_delay(base_delay_ms, attempt);
                warn!(
                    attempt = attempt,
                    status = resp.status().as_u16(),
                    delay_ms = delay,
                    "Retryable downstream error, retrying"
                );
                last_error = Some(AppError::Downstream(format!(
                    "HTTP {}",
                    resp.status().as_u16()
                )));
                if attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
            Ok(resp) => {
                // Non-retryable error — return the response as-is for the handler to deal with
                key_pool.report_success(&acquired);
                return Ok((resp, acquired));
            }
            Err(e) => {
                key_pool.report_failure(&acquired);
                let delay = calculate_delay(base_delay_ms, attempt);
                warn!(
                    attempt = attempt,
                    error = %e,
                    delay_ms = delay,
                    "Request error, retrying"
                );
                last_error = Some(AppError::Downstream(e.to_string()));
                if attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::Internal(anyhow::anyhow!("exhausted retries"))))
}

fn is_retryable(status: StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 502 | 503 | 504)
}

fn calculate_delay(base_ms: u64, attempt: u32) -> u64 {
    let exponential = base_ms * 2u64.pow(attempt);
    let jitter = exponential / 4;
    let mut rng = rand::thread_rng();
    rng.gen_range((exponential.saturating_sub(jitter))..=(exponential + jitter))
}
