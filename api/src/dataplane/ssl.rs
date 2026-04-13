use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SslConnEntry {
    pub seq: u64,
    pub pid: u32,
    pub tid: u32,
    pub handshake_us: f64,
    pub timestamp: u64,
    pub sni: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SslListResponse {
    pub connections: Vec<SslConnEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SslFlushResponse {
    pub flushed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SslHttpEntry {
    pub seq: u64,
    pub pid: u32,
    pub tid: u32,
    pub method: String,
    pub path: String,
    pub host: String,
    pub status_code: u16,
    pub latency_us: f64,
    pub request_ts: u64,
    pub response_ts: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SslHttpListResponse {
    pub events: Vec<SslHttpEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SslHttpFlushResponse {
    pub flushed: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "enabled": true
}))]
pub struct SslGlobalConfigResponse {
    /// Whether process-level SSL observability is enabled.
    #[schema(example = true)]
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "enabled": true
}))]
pub struct UpdateSslGlobalConfigRequest {
    /// Desired global SSL observability state.
    #[schema(example = true)]
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SslErrorEntry {
    pub seq: u64,
    pub pid: u32,
    pub tid: u32,
    pub timestamp: u64,
    pub syscall: String,
    pub ret_code: i32,
    pub error_hint: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SslErrorListResponse {
    pub errors: Vec<SslErrorEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SslErrorFlushResponse {
    pub flushed: u64,
}
