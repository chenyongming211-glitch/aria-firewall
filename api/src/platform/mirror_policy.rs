use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

/// A single mirror rule within a MirrorPolicy.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_ip_group_id": "any",
    "dst_ip_group_id": "ipgroup-db",
    "proto": 6,
    "direction": 1,
    "target_ifindex": 3,
    "target_iface": "eth1",
    "is_global": false
}))]
pub struct MirrorPolicyRule {
    /// Source IP group ID, or `"any"` for wildcard.
    #[schema(example = "any")]
    pub src_ip_group_id: String,
    /// Destination IP group ID, or `"any"` for wildcard.
    #[schema(example = "ipgroup-db")]
    pub dst_ip_group_id: String,
    /// L4 protocol number: 0 = any, 6 = TCP, 17 = UDP.
    #[schema(example = 6)]
    pub proto: u8,
    /// Traffic direction: 0 = ingress, 1 = egress.
    #[schema(example = 1)]
    pub direction: u8,
    /// Target interface ifindex that receives mirrored packets.
    #[schema(example = 3)]
    pub target_ifindex: u32,
    /// Target interface name (informational).
    #[schema(example = "eth1")]
    pub target_iface: String,
    /// If true, rule is written to MIRROR_GLOBAL (ignores src/dst/proto).
    #[schema(example = false)]
    #[serde(default)]
    pub is_global: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "name": "db-mirror",
    "rules": [
        {
            "src_ip_group_id": "any",
            "dst_ip_group_id": "ipgroup-db",
            "proto": 6,
            "direction": 1,
            "target_ifindex": 3,
            "target_iface": "eth1",
            "is_global": false
        }
    ]
}))]
pub struct MirrorPolicySpec {
    /// Owning tenant.
    pub tenant_id: String,
    /// Logical network that owns the policy.
    pub network_id: String,
    /// Human-readable policy name, unique within the network.
    pub name: String,
    /// Mirror rules.
    pub rules: Vec<MirrorPolicyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready",
    "rule_count": 1
}))]
pub struct MirrorPolicyStatus {
    /// Lifecycle phase.
    #[schema(example = "ready")]
    pub phase: String,
    /// Number of rules in the policy.
    #[schema(example = 1)]
    pub rule_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MirrorPolicyResource {
    pub metadata: ResourceMetadata,
    pub spec: MirrorPolicySpec,
    pub status: MirrorPolicyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateMirrorPolicyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: MirrorPolicySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateMirrorPolicyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: MirrorPolicySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MirrorPolicyListResponse {
    pub items: Vec<MirrorPolicyResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
