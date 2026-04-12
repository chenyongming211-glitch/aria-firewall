use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

/// A single QoS rate-limit rule within a QosPolicy.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "ip_group_id": "ipgroup-web",
    "direction": 1,
    "rate_bps": 100000000,
    "burst_bytes": 12500000,
    "priority": 0,
    "mode": 0
}))]
pub struct QosPolicyRule {
    /// IP group ID (references an IpGroup in the same network).
    pub ip_group_id: String,
    /// Direction: 0 = ingress, 1 = egress.
    #[schema(example = 1)]
    pub direction: u8,
    /// Rate limit in bits per second.
    #[schema(example = 100000000)]
    pub rate_bps: u64,
    /// Burst size in bytes.
    #[schema(example = 12500000)]
    pub burst_bytes: u64,
    /// Priority level (lower = higher priority).
    #[schema(example = 0)]
    pub priority: u8,
    /// Mode: 0 = policing (drop), 1 = shaping (delay).
    #[schema(example = 0)]
    pub mode: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "name": "web-egress-limit",
    "rules": [
        {
            "ip_group_id": "ipgroup-web",
            "direction": 1,
            "rate_bps": 100000000,
            "burst_bytes": 12500000,
            "priority": 0,
            "mode": 0
        }
    ]
}))]
pub struct QosPolicySpec {
    /// Owning tenant.
    pub tenant_id: String,
    /// Logical network that owns the policy.
    pub network_id: String,
    /// Human-readable policy name, unique within the network.
    pub name: String,
    /// Ordered list of QoS rate-limit rules.
    pub rules: Vec<QosPolicyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready",
    "rule_count": 1
}))]
pub struct QosPolicyStatus {
    /// Lifecycle phase.
    #[schema(example = "ready")]
    pub phase: String,
    /// Number of rules in the policy.
    #[schema(example = 1)]
    pub rule_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QosPolicyResource {
    pub metadata: ResourceMetadata,
    pub spec: QosPolicySpec,
    pub status: QosPolicyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateQosPolicyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: QosPolicySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateQosPolicyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: QosPolicySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QosPolicyListResponse {
    pub items: Vec<QosPolicyResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
