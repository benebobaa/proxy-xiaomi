use axum::http::StatusCode;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::middleware::{auth, logging, rate_limit};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    // OpenAI-compatible routes
    let openai_routes = Router::new()
        .route("/v1/chat/completions", post(handlers::openai::chat_completions))
        .route("/v1/completions", post(handlers::openai::completions))
        .route("/v1/embeddings", post(handlers::openai::embeddings))
        .route("/v1/models", get(handlers::openai::list_models));

    // Anthropic-compatible routes
    let anthropic_routes = Router::new()
        .route("/anthropic/v1/messages", post(handlers::anthropic::messages));

    // Admin routes
    let admin_routes = Router::new()
        .route("/admin/usage", get(handlers::admin::get_usage))
        .route("/admin/keys", get(handlers::admin::list_keys));

    // Health check
    let health_routes = Router::new().route("/health", get(health_check));

    Router::new()
        .merge(openai_routes)
        .merge(anthropic_routes)
        .merge(admin_routes)
        .merge(health_routes)
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit,
        ))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::auth))
        .layer(middleware::from_fn(logging::logging))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}
