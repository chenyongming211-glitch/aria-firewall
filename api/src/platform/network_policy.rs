use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

/// A single ACL rule within a NetworkPolicy.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NetworkPolicyRule {
    /// Source IP group ID (references an IpGroup in the same network).
    pub src_ip_group_id: String,
    /// Destination IP group ID (references an IpGroup in the same network).
    pub dst_ip_group_id: String,
    /// Protocol: 0 = wildcard, 6 = TCP, 17 = UDP, 1 = ICMP.
    #[schema(example = 6)]
    pub proto: u8,
    /// Direction: 0 = ingress, 1 = egress.
    #[schema(example = 0)]
    pub direction: u8,
    /// Action: 0 = allow, 1 = deny.
    #[schema(example = 1)]
    pub action: u8,
    /// Port filter expression, e.g. "80,443" or "1000-2000". None = all ports.
    pub ports: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "name": "deny-web-to-db",
    "rules": [
        {
            "src_ip_group_id": "ipgroup-web",
            "dst_ip_group_id": "ipgroup-db",
            "proto": 6,
            "direction": 1,
            "action": 1,
            "ports": "3306"
        }
    ]
}))]
pub struct NetworkPolicySpec {
    /// Owning tenant.
    pub tenant_id: String,
    /// Logical network that owns the policy.
    pub network_id: String,
    /// Human-readable policy name, unique within the network.
    pub name: String,
    /// Ordered list of ACL rules.
    pub rules: Vec<NetworkPolicyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready",
    "rule_count": 1
}))]
pub struct NetworkPolicyStatus {
    /// Lifecycle phase.
    pub phase: String,
    /// Number of rules in the policy.
    pub rule_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NetworkPolicyResource {
    pub metadata: ResourceMetadata,
    pub spec: NetworkPolicySpec,
    pub status: NetworkPolicyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateNetworkPolicyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: NetworkPolicySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateNetworkPolicyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: NetworkPolicySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NetworkPolicyListResponse {
    pub items: Vec<NetworkPolicyResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
