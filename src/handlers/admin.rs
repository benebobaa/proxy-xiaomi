use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    pub key: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub model: Option<String>,
}

pub async fn get_usage(
    State(state): State<AppState>,
    Query(query): Query<UsageQuery>,
) -> Result<impl IntoResponse, AppError> {
    let from = query.from.as_deref().unwrap_or("1970-01-01");
    let to = query.to.as_deref().unwrap_or("9999-12-31");

    let usage = state.db.query_usage(from, to, query.key.as_deref(), query.model.as_deref()).await?;

    Ok(Json(json!({
        "period": { "from": from, "to": to },
        "records": usage,
    })))
}

pub async fn list_keys(
    State(state): State<AppState>,
) -> impl IntoResponse {
    Json(json!({
        "downstream_keys": state.key_pool.key_stats(),
        "total": state.key_pool.key_count(),
        "healthy": state.key_pool.healthy_key_count(),
    }))
}
