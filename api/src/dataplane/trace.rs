use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_ip": "10.0.1.10",
    "dst_ip": "10.0.10.20",
    "src_port": 52344,
    "dst_port": 443,
    "proto": "tcp"
}))]
pub struct TraceStartRequest {
    /// Source IP filter; empty string matches any source.
    #[schema(example = "10.0.1.10")]
    #[serde(default)]
    pub src_ip: String,
    /// Destination IP filter; empty string matches any destination.
    #[schema(example = "10.0.10.20")]
    #[serde(default)]
    pub dst_ip: String,
    /// Source port filter; `0` matches any source port.
    #[schema(example = 52344)]
    #[serde(default)]
    pub src_port: u16,
    /// Destination port filter; `0` matches any destination port.
    #[schema(example = 443)]
    #[serde(default)]
    pub dst_port: u16,
    /// Protocol filter such as `tcp`, `udp`, or empty string for any protocol.
    #[schema(example = "tcp")]
    #[serde(default)]
    pub proto: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TraceEventEntry {
    pub seq: u64,
    pub timestamp: u64,
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub proto: String,
    pub hook: String,
    pub result: String,
    pub direction: String,
    pub src_group: String,
    pub src_id: u32,
    pub dst_group: String,
    pub dst_id: u32,
    pub pkt_len: u32,
    pub ct_state: String,
    pub drop_reason: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TraceResponse {
    pub events: Vec<TraceEventEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TraceFlushResponse {
    pub flushed: u64,
}
