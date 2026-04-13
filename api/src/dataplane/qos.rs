use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "group": "web",
    "group_id": 1,
    "direction": "egress",
    "rate_bps": 100000000,
    "burst_bytes": 1048576,
    "priority": 3,
    "mode": "policing"
}))]
pub struct QosEntry {
    /// Group name the QoS rule applies to.
    #[schema(example = "web")]
    pub group: String,
    /// Numeric identifier of the matched group.
    #[schema(example = 1)]
    pub group_id: u32,
    /// Traffic direction: `ingress` or `egress`.
    #[schema(example = "egress")]
    pub direction: String,
    /// Rate limit in bits per second after unit parsing.
    #[schema(example = 100000000)]
    pub rate_bps: u64,
    /// Burst budget in bytes.
    #[schema(example = 1048576)]
    pub burst_bytes: u64,
    /// Scheduling priority applied to matched packets.
    #[schema(example = 3)]
    pub priority: u8,
    /// Enforcement mode, typically `policing` or `shaping`.
    #[schema(example = "policing")]
    #[serde(default = "default_mode_string")]
    pub mode: String,
}

fn default_mode_string() -> String {
    "policing".to_string()
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "rules": [
        {
            "group": "web",
            "group_id": 1,
            "direction": "egress",
            "rate_bps": 100000000,
            "burst_bytes": 1048576,
            "priority": 3,
            "mode": "policing"
        }
    ]
}))]
pub struct QosListResponse {
    /// Configured QoS rules for the instance.
    pub rules: Vec<QosEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "group": "web",
    "direction": "egress",
    "rate": "100mbit",
    "burst": "1mb",
    "priority": 3,
    "mode": "policing"
}))]
pub struct AddQosRequest {
    /// Group name the QoS rule applies to.
    #[schema(example = "web")]
    pub group: String,
    /// Traffic direction: `ingress` or `egress`.
    #[schema(example = "egress")]
    pub direction: String,
    /// Human-readable rate value such as `100mbit` or `10gbit`.
    #[schema(example = "100mbit")]
    pub rate: String,
    /// Optional burst size such as `1mb`; empty string keeps the default.
    #[schema(example = "1mb")]
    #[serde(default)]
    pub burst: String,
    /// Scheduling priority applied to matched packets.
    #[schema(example = 3)]
    #[serde(default)]
    pub priority: u8,
    /// Enforcement mode, typically `policing` or `shaping`.
    #[schema(example = "policing")]
    #[serde(default = "default_mode_string")]
    pub mode: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "group": "web",
    "direction": "egress"
}))]
pub struct DeleteQosRequest {
    /// Group name the QoS rule applies to.
    #[schema(example = "web")]
    pub group: String,
    /// Traffic direction: `ingress` or `egress`.
    #[schema(example = "egress")]
    pub direction: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QosStatsEntry {
    pub group: String,
    pub group_id: u32,
    pub direction: String,
    pub passed_packets: u64,
    pub passed_bytes: u64,
    pub dropped_packets: u64,
    pub dropped_bytes: u64,
    pub shaped_packets: u64,
    pub shaped_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QosStatsResponse {
    pub rules: Vec<QosStatsEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QosWithStatsEntry {
    pub group: String,
    pub group_id: u32,
    pub direction: String,
    pub rate_bps: u64,
    pub burst_bytes: u64,
    pub priority: u8,
    pub mode: String,
    #[serde(default)]
    pub passed_packets: u64,
    #[serde(default)]
    pub passed_bytes: u64,
    #[serde(default)]
    pub dropped_packets: u64,
    #[serde(default)]
    pub dropped_bytes: u64,
    #[serde(default)]
    pub shaped_packets: u64,
    #[serde(default)]
    pub shaped_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QosWithStatsResponse {
    pub rules: Vec<QosWithStatsEntry>,
}
