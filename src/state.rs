use std::sync::Arc;
use crate::config::Config;
use crate::proxy::key_pool::KeyPool;
use crate::proxy::rate_limiter::RateLimiter;
use crate::storage::db::Db;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub http_client: reqwest::Client,
    pub key_pool: Arc<KeyPool>,
    pub rate_limiter: Arc<RateLimiter>,
    pub db: Arc<Db>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.downstream.timeout_secs))
            .pool_max_idle_per_host(20)
            .build()?;

        let key_pool = Arc::new(KeyPool::new(&config.downstream_keys));
        let rate_limiter = Arc::new(RateLimiter::new(
            config.rate_limit.requests_per_minute,
            config.rate_limit.burst_size,
        ));

        // Ensure parent directory exists for SQLite if using local file
        let is_remote = config.database.url.starts_with("libsql://")
            || config.database.url.starts_with("http://")
            || config.database.url.starts_with("https://");

        if !is_remote && !config.database.url.is_empty() {
            if let Some(parent) = std::path::Path::new(&config.database.url).parent() {
                if parent.as_os_str() != "" {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }

        let db = Arc::new(Db::new(&config.database.url, &config.database.token).await?);

        Ok(Self {
            config,
            http_client,
            key_pool,
            rate_limiter,
            db,
        })
    }
}
