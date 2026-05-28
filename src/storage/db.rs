use tracing::info;

use crate::storage::models::{UsageSummary, ClientKeyModel, DownstreamKeyModel};
use crate::storage::schema::SCHEMA;

pub struct Db {
    conn: libsql::Connection,
}

impl Db {
    pub async fn new(url: &str, token: &str) -> anyhow::Result<Self> {
        let db = if url.starts_with("libsql://") || url.starts_with("http://") || url.starts_with("https://") {
            libsql::Builder::new_remote(url.to_string(), token.to_string()).build().await?
        } else {
            let path = if url.is_empty() { "local.db" } else { url };
            libsql::Builder::new_local(path).build().await?
        };

        let conn = db.connect()?;
        conn.execute_batch(SCHEMA).await?;

        info!(url = %url, "LibSQL database initialized");
        Ok(Self { conn })
    }

    pub async fn record_request(
        &self,
        client_key: &str,
        protocol: &str,
        path: &str,
        model: Option<&str>,
        status_code: u16,
        latency_ms: u64,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
        total_tokens: Option<u32>,
        is_stream: bool,
    ) -> Result<(), String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn
            .execute(
                "INSERT INTO request_logs (id, client_key, protocol, path, model, status_code, latency_ms, prompt_tokens, completion_tokens, total_tokens, is_stream) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                libsql::params![
                    id,
                    client_key.to_string(),
                    protocol.to_string(),
                    path.to_string(),
                    model.map(|s| s.to_string()),
                    status_code as i64,
                    latency_ms as i64,
                    prompt_tokens.map(|v| v as i64),
                    completion_tokens.map(|v| v as i64),
                    total_tokens.map(|v| v as i64),
                    is_stream,
                ],
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub async fn query_usage(
        &self,
        from: &str,
        to: &str,
        key: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<UsageSummary>, String> {
        let mut sql = String::from(
            "SELECT date(timestamp) as date, client_key, model, \
             COUNT(*) as request_count, \
             COALESCE(SUM(prompt_tokens), 0) as total_prompt_tokens, \
             COALESCE(SUM(completion_tokens), 0) as total_completion_tokens, \
             COALESCE(SUM(total_tokens), 0) as total_tokens \
             FROM request_logs WHERE date(timestamp) >= ?1 AND date(timestamp) <= ?2",
        );
        let mut params: Vec<libsql::Value> = vec![
            libsql::Value::from(from.to_string()),
            libsql::Value::from(to.to_string()),
        ];

        if let Some(k) = key {
            sql.push_str(&format!(" AND client_key = ?{}", params.len() + 1));
            params.push(libsql::Value::from(k.to_string()));
        }
        if let Some(m) = model {
            sql.push_str(&format!(" AND model = ?{}", params.len() + 1));
            params.push(libsql::Value::from(m.to_string()));
        }

        sql.push_str(" GROUP BY date(timestamp), client_key, model ORDER BY date(timestamp) DESC");

        let stmt = self.conn.prepare(&sql).await.map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params).await.map_err(|e| e.to_string())?;

        let mut summaries = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            summaries.push(UsageSummary {
                date: row.get::<String>(0).map_err(|e| e.to_string())?,
                client_key: row.get::<String>(1).map_err(|e| e.to_string())?,
                model: row.get::<Option<String>>(2).map_err(|e| e.to_string())?,
                request_count: row.get::<i64>(3).map_err(|e| e.to_string())?,
                total_prompt_tokens: row.get::<i64>(4).map_err(|e| e.to_string())?,
                total_completion_tokens: row.get::<i64>(5).map_err(|e| e.to_string())?,
                total_tokens: row.get::<i64>(6).map_err(|e| e.to_string())?,
            });
        }
        Ok(summaries)
    }

    pub async fn get_client_keys(&self) -> Result<Vec<ClientKeyModel>, String> {
        let stmt = self.conn.prepare("SELECT key, description, rate_limit, created_at FROM client_keys ORDER BY created_at DESC").await.map_err(|e| e.to_string())?;
        let mut rows = stmt.query(()).await.map_err(|e| e.to_string())?;
        let mut keys = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            keys.push(ClientKeyModel {
                key: row.get::<String>(0).map_err(|e| e.to_string())?,
                description: row.get::<Option<String>>(1).map_err(|e| e.to_string())?,
                rate_limit: row.get::<Option<i64>>(2).map_err(|e| e.to_string())?,
                created_at: row.get::<Option<String>>(3).map_err(|e| e.to_string())?,
            });
        }
        Ok(keys)
    }

    pub async fn add_client_key(&self, key: &str, description: Option<&str>, rate_limit: Option<i64>) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO client_keys (key, description, rate_limit) VALUES (?1, ?2, ?3)",
                libsql::params![key.to_string(), description.map(|s| s.to_string()), rate_limit],
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub async fn delete_client_key(&self, key: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM client_keys WHERE key = ?1", libsql::params![key.to_string()])
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub async fn get_downstream_keys(&self) -> Result<Vec<DownstreamKeyModel>, String> {
        let stmt = self.conn.prepare("SELECT key, weight, created_at FROM downstream_keys ORDER BY created_at DESC").await.map_err(|e| e.to_string())?;
        let mut rows = stmt.query(()).await.map_err(|e| e.to_string())?;
        let mut keys = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            keys.push(DownstreamKeyModel {
                key: row.get::<String>(0).map_err(|e| e.to_string())?,
                weight: row.get::<i64>(1).map_err(|e| e.to_string())?,
                created_at: row.get::<Option<String>>(2).map_err(|e| e.to_string())?,
            });
        }
        Ok(keys)
    }

    pub async fn add_downstream_key(&self, key: &str, weight: i64) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO downstream_keys (key, weight) VALUES (?1, ?2)",
                libsql::params![key.to_string(), weight],
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub async fn delete_downstream_key(&self, key: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM downstream_keys WHERE key = ?1", libsql::params![key.to_string()])
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
