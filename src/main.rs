mod config;
mod error;
mod handlers;
mod middleware;
mod proxy;
mod router;
mod state;
mod storage;

use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("xiaomi_proxy=info,tower_http=info")),
        )
        .init();

    info!("Starting Xiaomi Token Plan Proxy");

    // Load configuration
    let config = config::Config::load()?;
    let addr = format!("{}:{}", config.server.host, config.server.port);

    info!(
        host = %config.server.host,
        port = config.server.port,
        downstream_keys = config.downstream_keys.len(),
        client_keys = config.client_keys.len(),
        "Configuration loaded"
    );

    // Build application state
    let state = state::AppState::new(config).await?;

    // Build router
    let app = router::build_router(state);

    // Bind and serve
    let listener = TcpListener::bind(&addr).await?;
    info!(addr = %addr, "Proxy server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received");
}
