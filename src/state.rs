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
    pub client_keys: Arc<std::sync::RwLock<std::collections::HashSet<String>>>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.downstream.timeout_secs))
            .pool_max_idle_per_host(20)
            .build()?;

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

        // Seed client keys in DB if empty, and load them into memory cache
        let db_client_keys = db.get_client_keys().await.map_err(|e| anyhow::anyhow!(e))?;
        let mut client_keys_set = std::collections::HashSet::new();

        if db_client_keys.is_empty() && !config.client_keys.is_empty() {
            for ck in &config.client_keys {
                db.add_client_key(&ck.key, Some("Imported from config"), None)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                client_keys_set.insert(ck.key.clone());
            }
        } else {
            for ck in db_client_keys {
                client_keys_set.insert(ck.key);
            }
        }

        // Seed downstream keys in DB if empty, and load them into key pool
        let db_downstream_keys = db.get_downstream_keys().await.map_err(|e| anyhow::anyhow!(e))?;
        let mut downstream_keys_list = Vec::new();

        if db_downstream_keys.is_empty() && !config.downstream_keys.is_empty() {
            for dk in &config.downstream_keys {
                db.add_downstream_key(&dk.key, dk.weight as i64)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                downstream_keys_list.push(dk.clone());
            }
        } else {
            for dk in db_downstream_keys {
                downstream_keys_list.push(crate::config::DownstreamKeyConfig {
                    key: dk.key,
                    weight: dk.weight as u32,
                });
            }
        }

        let key_pool = Arc::new(KeyPool::new(&downstream_keys_list));
        let rate_limiter = Arc::new(RateLimiter::new(
            config.rate_limit.requests_per_minute,
            config.rate_limit.burst_size,
        ));

        Ok(Self {
            config,
            http_client,
            key_pool,
            rate_limiter,
            db,
            client_keys: Arc::new(std::sync::RwLock::new(client_keys_set)),
        })
    }
}
