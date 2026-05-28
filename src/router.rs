use axum::http::StatusCode;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post, delete};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::middleware::{auth, basic_auth, logging, rate_limit};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    // --- Proxy API routes (client-key auth + rate-limit) ---
    let proxy_routes = Router::new()
        .route("/v1/chat/completions", post(handlers::openai::chat_completions))
        .route("/v1/completions", post(handlers::openai::completions))
        .route("/v1/embeddings", post(handlers::openai::embeddings))
        .route("/v1/models", get(handlers::openai::list_models))
        .route("/anthropic/v1/messages", post(handlers::anthropic::messages))
        // Rate-limit then client-key auth
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit,
        ))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::auth));

    // --- Admin routes (HTTP Basic Auth only) ---
    // The JSON API endpoints (/admin/usage, /admin/keys, etc.) are also
    // protected by Basic Auth here. The client-key Bearer check is intentionally
    // NOT applied to admin routes — the Basic Auth credential IS the admin gate.
    let admin_routes = Router::new()
        .route("/admin/dashboard", get(handlers::admin::dashboard))
        .route("/admin/usage", get(handlers::admin::get_usage))
        .route("/admin/keys", get(handlers::admin::list_keys))
        .route("/admin/client-keys", get(handlers::admin::get_client_keys).post(handlers::admin::add_client_key))
        .route("/admin/client-keys/{key}", delete(handlers::admin::delete_client_key))
        .route("/admin/downstream-keys", post(handlers::admin::add_downstream_key))
        .route("/admin/downstream-keys/{key}", delete(handlers::admin::delete_downstream_key))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            basic_auth,
        ));

    // --- Health check (no auth) ---
    let health_routes = Router::new().route("/health", get(health_check));

    Router::new()
        .merge(proxy_routes)
        .merge(admin_routes)
        .merge(health_routes)
        .layer(middleware::from_fn(logging::logging))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}
