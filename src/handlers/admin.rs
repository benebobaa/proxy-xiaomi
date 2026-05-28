use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::Json;
use rand::{distributions::Alphanumeric, Rng};
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

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub key: Option<String>,
    pub model: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AddClientKeyPayload {
    pub key: Option<String>,
    pub description: Option<String>,
    pub rate_limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AddDownstreamKeyPayload {
    pub key: String,
    pub weight: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDownstreamKeyPayload {
    pub weight: i64,
}

pub async fn dashboard() -> impl IntoResponse {
    Html(include_str!("admin_dashboard.html"))
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

/// Raw request logs endpoint — returns individual records (not aggregated).
pub async fn get_logs(
    State(state): State<AppState>,
    Query(query): Query<LogsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let from = query.from.as_deref().unwrap_or("1970-01-01");
    let to = query.to.as_deref().unwrap_or("9999-12-31");
    let limit = query.limit.unwrap_or(100).min(500);
    let offset = query.offset.unwrap_or(0);

    let logs = state
        .db
        .query_logs(from, to, limit, offset, query.key.as_deref(), query.model.as_deref())
        .await?;

    Ok(Json(json!({
        "logs": logs,
        "limit": limit,
        "offset": offset,
        "has_more": logs.len() as i64 == limit,
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

// Client Keys REST APIs
pub async fn get_client_keys(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let keys = state.db.get_client_keys().await?;
    Ok(Json(keys))
}

pub async fn add_client_key(
    State(state): State<AppState>,
    Json(payload): Json<AddClientKeyPayload>,
) -> Result<impl IntoResponse, AppError> {
    let key = match payload.key {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => {
            let rand_str: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(32)
                .map(char::from)
                .collect();
            format!("sk-{}", rand_str)
        }
    };

    state.db
        .add_client_key(&key, payload.description.as_deref(), payload.rate_limit)
        .await?;

    // Update dynamic cache
    state.client_keys.write().unwrap().insert(key.clone());

    Ok(Json(json!({
        "status": "success",
        "key": key,
    })))
}

pub async fn delete_client_key(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.db.delete_client_key(&key).await?;

    // Update dynamic cache
    state.client_keys.write().unwrap().remove(&key);

    Ok(Json(json!({ "status": "success" })))
}

// Downstream Keys REST APIs
pub async fn add_downstream_key(
    State(state): State<AppState>,
    Json(payload): Json<AddDownstreamKeyPayload>,
) -> Result<impl IntoResponse, AppError> {
    let key = payload.key.trim().to_string();
    let weight = payload.weight;

    state.db.add_downstream_key(&key, weight).await?;

    // Update dynamic pool
    state.key_pool.add_key(key, weight as u32);

    Ok(Json(json!({ "status": "success" })))
}

/// Update the weight of a downstream key without removing it from the pool.
pub async fn update_downstream_key(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(payload): Json<UpdateDownstreamKeyPayload>,
) -> Result<impl IntoResponse, AppError> {
    let weight = payload.weight.max(1);
    state.db.update_downstream_key_weight(&key, weight).await?;

    // Re-register with new weight in the live pool
    state.key_pool.add_key(key, weight as u32);

    Ok(Json(json!({ "status": "success" })))
}

pub async fn delete_downstream_key(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.db.delete_downstream_key(&key).await?;

    // Update dynamic pool
    state.key_pool.remove_key(&key);

    Ok(Json(json!({ "status": "success" })))
}
