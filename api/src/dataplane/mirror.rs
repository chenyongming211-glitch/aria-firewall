use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_group": "any",
    "src_group_id": 0,
    "dst_group": "db",
    "dst_group_id": 2,
    "proto": "tcp",
    "direction": "egress",
    "target_iface": "eth1",
    "target_ifindex": 3,
    "is_global": false
}))]
pub struct MirrorEntry {
    /// Source group name or `any`.
    #[schema(example = "any")]
    pub src_group: String,
    /// Numeric identifier of the source group.
    #[schema(example = 0)]
    pub src_group_id: u32,
    /// Destination group name or `any`.
    #[schema(example = "db")]
    pub dst_group: String,
    /// Numeric identifier of the destination group.
    #[schema(example = 2)]
    pub dst_group_id: u32,
    /// Matched L4 protocol name or protocol number.
    #[schema(example = "tcp")]
    pub proto: String,
    /// Traffic direction: `ingress` or `egress`.
    #[schema(example = "egress")]
    pub direction: String,
    /// Interface name that receives mirrored packets.
    #[schema(example = "eth1")]
    pub target_iface: String,
    /// Interface index resolved from the target interface name.
    #[schema(example = 3)]
    pub target_ifindex: u32,
    /// Whether the rule applies globally across all groups.
    #[schema(example = false)]
    pub is_global: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "rules": [
        {
            "src_group": "any",
            "src_group_id": 0,
            "dst_group": "db",
            "dst_group_id": 2,
            "proto": "tcp",
            "direction": "egress",
            "target_iface": "eth1",
            "target_ifindex": 3,
            "is_global": false
        }
    ]
}))]
pub struct MirrorListResponse {
    /// Configured mirror rules for the instance.
    pub rules: Vec<MirrorEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_group": "any",
    "dst_group": "db",
    "proto": "tcp",
    "direction": "egress",
    "target": "eth1"
}))]
pub struct AddMirrorRequest {
    /// Source group name or `any`.
    #[schema(example = "any")]
    #[serde(default = "default_any")]
    pub src_group: String,
    /// Destination group name or `any`.
    #[schema(example = "db")]
    #[serde(default = "default_any")]
    pub dst_group: String,
    /// Protocol name (`tcp`, `udp`, `icmp`, `any`) or protocol number.
    #[schema(example = "tcp")]
    #[serde(default = "default_any_proto")]
    pub proto: String,
    /// Traffic direction: `ingress` or `egress`.
    #[schema(example = "egress")]
    pub direction: String,
    /// Target interface name that should receive mirrored packets.
    #[schema(example = "eth1")]
    pub target: String,
}

fn default_any() -> String {
    "any".to_string()
}

fn default_any_proto() -> String {
    "any".to_string()
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_group": "any",
    "dst_group": "db",
    "proto": "tcp",
    "direction": "egress"
}))]
pub struct DeleteMirrorRequest {
    /// Source group name or `any`.
    #[schema(example = "any")]
    #[serde(default = "default_any")]
    pub src_group: String,
    /// Destination group name or `any`.
    #[schema(example = "db")]
    #[serde(default = "default_any")]
    pub dst_group: String,
    /// Protocol name (`tcp`, `udp`, `icmp`, `any`) or protocol number.
    #[schema(example = "tcp")]
    #[serde(default = "default_any_proto")]
    pub proto: String,
    /// Traffic direction: `ingress` or `egress`.
    #[schema(example = "egress")]
    pub direction: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MirrorStatsEntry {
    pub src_group: String,
    pub src_id: u32,
    pub dst_group: String,
    pub dst_id: u32,
    pub proto: String,
    pub direction: String,
    pub mirrored_packets: u64,
    pub mirrored_bytes: u64,
    pub errors: u64,
    pub is_global: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MirrorStatsResponse {
    pub rules: Vec<MirrorStatsEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MirrorWithStatsEntry {
    pub src_group: String,
    pub src_group_id: u32,
    pub dst_group: String,
    pub dst_group_id: u32,
    pub proto: String,
    pub direction: String,
    pub target_iface: String,
    pub target_ifindex: u32,
    pub is_global: bool,
    #[serde(default)]
    pub mirrored_packets: u64,
    #[serde(default)]
    pub mirrored_bytes: u64,
    #[serde(default)]
    pub errors: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MirrorWithStatsResponse {
    pub rules: Vec<MirrorWithStatsEntry>,
}
