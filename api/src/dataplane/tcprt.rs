use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtEntry {
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub handshake_us: f64,
    pub rtt_client_us: f64,
    pub rtt_server_us: f64,
    pub art_us: f64,
    pub retrans_req: u32,
    pub retrans_resp: u32,
    pub request_count: u32,
    pub state: String,
    #[serde(default)]
    pub forward_platform_us: f64,
    #[serde(default)]
    pub server_network_us: f64,
    #[serde(default)]
    pub reverse_platform_us: f64,
    #[serde(default)]
    pub fin_us: f64,
    #[serde(default)]
    pub rst_us: f64,
    #[serde(default)]
    pub close_us: f64,
    #[serde(default)]
    pub nqa_score: u8,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtResponse {
    pub flows: Vec<TcpRtEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtFlushResponse {
    pub flushed: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_ip": "10.0.1.10",
    "dst_ip": "10.0.10.20",
    "src_port": 52344,
    "dst_port": 443
}))]
pub struct TcpRtQueryTuple {
    /// Client or source IP address.
    #[schema(example = "10.0.1.10")]
    pub src_ip: String,
    /// Server or destination IP address.
    #[schema(example = "10.0.10.20")]
    pub dst_ip: String,
    /// Client or source port.
    #[schema(example = 52344)]
    pub src_port: u16,
    /// Server or destination port.
    #[schema(example = 443)]
    pub dst_port: u16,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tuples": [
        {
            "src_ip": "10.0.1.10",
            "dst_ip": "10.0.10.20",
            "src_port": 52344,
            "dst_port": 443
        },
        {
            "src_ip": "10.0.1.11",
            "dst_ip": "10.0.10.20",
            "src_port": 52345,
            "dst_port": 443
        }
    ]
}))]
pub struct TcpRtBatchQueryRequest {
    /// Tuples to query across all managed instances.
    pub tuples: Vec<TcpRtQueryTuple>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtInstanceEntry {
    pub instance: String,
    pub entry: TcpRtEntry,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtBatchQueryResponse {
    pub results: Vec<TcpRtInstanceEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "dst_ip": "10.0.10.20",
    "dst_port": 443
}))]
pub struct TcpRtFilterRequest {
    /// Service IP address to aggregate by.
    #[schema(example = "10.0.10.20")]
    pub dst_ip: String,
    /// Service port to aggregate by.
    #[schema(example = 443)]
    pub dst_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TcpRtAggregatedEntry {
    pub instance: String,
    pub flow_count: u32,
    pub avg_rtt_client_us: f64,
    pub avg_rtt_server_us: f64,
    pub avg_art_us: f64,
    pub avg_handshake_us: f64,
    pub total_retrans_req: u32,
    pub total_retrans_resp: u32,
    #[serde(default)]
    pub avg_forward_platform_us: f64,
    #[serde(default)]
    pub avg_server_network_us: f64,
    #[serde(default)]
    pub avg_reverse_platform_us: f64,
    #[serde(default)]
    pub avg_nqa_score: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtFilterResponse {
    pub dst_ip: String,
    pub dst_port: u16,
    pub instances: Vec<TcpRtAggregatedEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtHistogramBucket {
    pub le_us: f64,
    pub count: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtHistogramResponse {
    pub buckets: Vec<TcpRtHistogramBucket>,
    pub total: u64,
    pub sum_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtStateCount {
    pub state: String,
    pub count: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TcpRtStatesResponse {
    pub states: Vec<TcpRtStateCount>,
    pub total_flows: u64,
    pub anomalies: Vec<String>,
}
