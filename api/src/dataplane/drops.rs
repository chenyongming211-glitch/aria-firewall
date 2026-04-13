use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DropStatsEntry {
    pub reason: String,
    pub direction: String,
    pub proto: String,
    pub src_group: String,
    pub src_id: u32,
    pub dst_group: String,
    pub dst_id: u32,
    pub packets: u64,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DropStatsResponse {
    pub drops: Vec<DropStatsEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DropFlushResponse {
    pub flushed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[schema(example = json!({
    "instance": "eth0",
    "iface": "eth0",
    "ifindex": 2,
    "reason": 38,
    "top": 20,
    "include_unattributed": false
}))]
pub struct KernelDropQuery {
    /// Optional managed instance name filter.
    #[schema(example = "eth0")]
    pub instance: Option<String>,
    /// Optional interface name filter.
    #[schema(example = "eth0")]
    pub iface: Option<String>,
    /// Optional interface index filter.
    #[schema(example = 2)]
    pub ifindex: Option<u32>,
    /// Optional numeric kernel drop reason code filter.
    #[schema(example = 38)]
    pub reason: Option<u16>,
    /// Maximum number of aggregated results to return.
    #[schema(example = 20)]
    pub top: Option<usize>,
    /// Include drop entries that could not be mapped back to a managed instance.
    #[schema(example = false)]
    #[serde(default)]
    pub include_unattributed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KernelDropStatsEntry {
    pub instance: Option<String>,
    pub iface: Option<String>,
    pub ifindex: u32,
    pub reason_code: Option<u16>,
    pub reason: String,
    pub proto: String,
    pub packets: u64,
    pub bytes: u64,
    pub last_seen_ns: u64,
    pub last_location: Option<u64>,
    pub location: Option<String>,
    pub location_hint: Option<String>,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct KernelDropStatsResponse {
    pub drops: Vec<KernelDropStatsEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct KernelDropFlushResponse {
    pub flushed: u64,
}
