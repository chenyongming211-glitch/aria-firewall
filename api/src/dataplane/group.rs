use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "id": 1,
    "name": "web",
    "cidrs": ["10.0.1.0/24", "10.0.2.0/24"]
}))]
pub struct GroupEntry {
    /// Stable numeric identifier allocated to the group.
    #[schema(example = 1)]
    pub id: u32,
    /// Human-readable group name referenced by ACL, QoS, and mirror rules.
    #[schema(example = "web")]
    pub name: String,
    /// CIDR members contained in the group.
    pub cidrs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "groups": [
        {"id": 1, "name": "web", "cidrs": ["10.0.1.0/24"]},
        {"id": 2, "name": "db", "cidrs": ["10.0.10.0/24"]}
    ]
}))]
pub struct GroupsResponse {
    /// Configured address groups for the instance.
    pub groups: Vec<GroupEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "web",
    "cidr": "10.0.1.0/24"
}))]
pub struct AddGroupRequest {
    /// Group name to create or extend.
    #[schema(example = "web")]
    pub name: String,
    /// CIDR to add to the group.
    #[schema(example = "10.0.1.0/24")]
    pub cidr: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "id": 1,
    "name": "web"
}))]
pub struct AddGroupResponse {
    /// Stable numeric identifier assigned to the group.
    #[schema(example = 1)]
    pub id: u32,
    /// Name of the created or extended group.
    #[schema(example = "web")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "id": 1,
    "name": "web",
    "cidrs": ["10.0.1.0/24"],
    "ingress_packets": 128,
    "ingress_bytes": 8192,
    "egress_packets": 256,
    "egress_bytes": 16384
}))]
pub struct GroupWithStatsEntry {
    /// Stable numeric identifier allocated to the group.
    #[schema(example = 1)]
    pub id: u32,
    /// Human-readable group name.
    #[schema(example = "web")]
    pub name: String,
    /// CIDR members contained in the group.
    pub cidrs: Vec<String>,
    /// Total ingress packets matched to the group.
    #[schema(example = 128)]
    #[serde(default)]
    pub ingress_packets: u64,
    /// Total ingress bytes matched to the group.
    #[schema(example = 8192)]
    #[serde(default)]
    pub ingress_bytes: u64,
    /// Total egress packets matched to the group.
    #[schema(example = 256)]
    #[serde(default)]
    pub egress_packets: u64,
    /// Total egress bytes matched to the group.
    #[schema(example = 16384)]
    #[serde(default)]
    pub egress_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "groups": [
        {
            "id": 1,
            "name": "web",
            "cidrs": ["10.0.1.0/24"],
            "ingress_packets": 128,
            "ingress_bytes": 8192,
            "egress_packets": 256,
            "egress_bytes": 16384
        }
    ]
}))]
pub struct GroupsWithStatsResponse {
    /// Groups enriched with aggregated per-direction traffic counters.
    pub groups: Vec<GroupWithStatsEntry>,
}
