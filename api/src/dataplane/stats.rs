use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "groups": 4,
    "policies": 12,
    "qos_rules": 2,
    "mirror_rules": 1,
    "conntrack_v4": 38,
    "conntrack_v6": 0
}))]
pub struct StatsOverview {
    /// Number of configured groups.
    #[schema(example = 4)]
    pub groups: usize,
    /// Number of configured ACL policies.
    #[schema(example = 12)]
    pub policies: usize,
    /// Number of configured QoS rules.
    #[schema(example = 2)]
    pub qos_rules: usize,
    /// Number of configured mirror rules.
    #[schema(example = 1)]
    pub mirror_rules: usize,
    /// Active IPv4 conntrack entries.
    #[schema(example = 38)]
    pub conntrack_v4: u64,
    /// Active IPv6 conntrack entries.
    #[schema(example = 0)]
    pub conntrack_v6: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RuleStatsEntry {
    pub src_group: String,
    pub src_id: u32,
    pub dst_group: String,
    pub dst_id: u32,
    pub proto: String,
    pub direction: String,
    pub packets: u64,
    pub bytes: u64,
    #[serde(default)]
    pub dropped_packets: u64,
    #[serde(default)]
    pub dropped_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RuleStatsResponse {
    pub rules: Vec<RuleStatsEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FlowEntry {
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub proto: String,
    pub packets: u64,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FlowStatsResponse {
    pub flows: Vec<FlowEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GroupStatsEntry {
    pub group: String,
    pub group_id: u32,
    pub direction: String,
    pub packets: u64,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GroupStatsResponse {
    pub groups: Vec<GroupStatsEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LbStatsEntry {
    pub service_id: u32,
    pub backend_slot: u16,
    pub lb_algo: String,
    pub affinity_hit: bool,
    pub packets: u64,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LbStatsResponse {
    pub entries: Vec<LbStatsEntry>,
}
