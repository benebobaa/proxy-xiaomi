use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RequestLog {
    pub id: String,
    pub timestamp: String,
    pub client_key: String,
    pub protocol: String,
    pub path: String,
    pub model: Option<String>,
    pub status_code: u16,
    pub latency_ms: u64,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub is_stream: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub date: String,
    pub client_key: String,
    pub model: Option<String>,
    pub request_count: i64,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_tokens: i64,
}
