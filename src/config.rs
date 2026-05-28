use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub downstream: DownstreamConfig,
    #[serde(default)]
    pub client_keys: Vec<ClientKeyConfig>,
    pub downstream_keys: Vec<DownstreamKeyConfig>,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownstreamConfig {
    pub openai_base_url: String,
    pub anthropic_base_url: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_base_ms")]
    pub retry_base_ms: u64,
}

fn default_timeout() -> u64 {
    120
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_base_ms() -> u64 {
    500
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientKeyConfig {
    pub key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownstreamKeyConfig {
    pub key: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_rpm")]
    pub requests_per_minute: u32,
    #[serde(default = "default_burst")]
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: default_rpm(),
            burst_size: default_burst(),
        }
    }
}

fn default_rpm() -> u32 {
    60
}

fn default_burst() -> u32 {
    60
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// Turso/libsql database URL (e.g. libsql://mydb.aws-ap-south-1.turso.io)
    #[serde(default)]
    pub url: String,
    /// Turso auth token
    #[serde(default)]
    pub token: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            token: String::new(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = std::env::var("XIAOMI_PROXY_CONFIG")
            .unwrap_or_else(|_| "config.toml".to_string());

        let mut config: Config = if Path::new(&config_path).exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            toml::from_str(&contents)?
        } else {
            anyhow::bail!("Config file not found: {}", config_path);
        };

        // Env var overrides
        if let Ok(port) = std::env::var("XIAOMI_PROXY_PORT") {
            config.server.port = port.parse()?;
        }
        if let Ok(host) = std::env::var("XIAOMI_PROXY_HOST") {
            config.server.host = host;
        }
        if let Ok(url) = std::env::var("XIAOMI_PROXY_OPENAI_URL") {
            config.downstream.openai_base_url = url;
        }
        if let Ok(url) = std::env::var("XIAOMI_PROXY_ANTHROPIC_URL") {
            config.downstream.anthropic_base_url = url;
        }
        if let Ok(url) = std::env::var("TURSO_DATABASE_URL") {
            config.database.url = url;
        }
        if let Ok(token) = std::env::var("TURSO_AUTH_TOKEN") {
            config.database.token = token;
        }

        Ok(config)
    }

    pub fn is_valid_client_key(&self, key: &str) -> bool {
        self.client_keys.iter().any(|k| k.key == key)
    }

    pub fn mask_key(key: &str) -> String {
        if key.len() <= 8 {
            return "***".to_string();
        }
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}
