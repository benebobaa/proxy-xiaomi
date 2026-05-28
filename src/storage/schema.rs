pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS request_logs (
    id              TEXT PRIMARY KEY,
    timestamp       TEXT NOT NULL DEFAULT (datetime('now')),
    client_key      TEXT NOT NULL,
    protocol        TEXT NOT NULL,
    path            TEXT NOT NULL,
    model           TEXT,
    status_code     INTEGER NOT NULL,
    latency_ms      INTEGER NOT NULL,
    prompt_tokens   INTEGER,
    completion_tokens INTEGER,
    total_tokens    INTEGER,
    is_stream       BOOLEAN NOT NULL DEFAULT 0,
    error_message   TEXT
);

CREATE INDEX IF NOT EXISTS idx_request_logs_timestamp ON request_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_request_logs_client_key ON request_logs(client_key);
CREATE INDEX IF NOT EXISTS idx_request_logs_model ON request_logs(model);

CREATE TABLE IF NOT EXISTS client_keys (
    key          TEXT PRIMARY KEY,
    description  TEXT,
    rate_limit   INTEGER,
    created_at   TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS downstream_keys (
    key          TEXT PRIMARY KEY,
    weight       INTEGER DEFAULT 1,
    created_at   TEXT DEFAULT (datetime('now'))
);
";
