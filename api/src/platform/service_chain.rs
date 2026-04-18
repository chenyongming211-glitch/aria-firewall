use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

/// A single tap binding within a service chain hop.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tap": "tapfw0",
    "role": "in"
}))]
pub struct ServiceChainTapBinding {
    /// Tap interface name.
    #[schema(example = "tapfw0")]
    pub tap: String,
    /// Tap role: "in", "out", or "bidirectional".
    #[schema(example = "in")]
    pub role: String,
}

/// A single hop within a service chain.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "fw-west",
    "hop_type": "bridge",
    "taps": [{"tap": "tapfw0", "role": "in"}]
}))]
pub struct ServiceChainHop {
    /// Hop name.
    #[schema(example = "fw-west")]
    pub name: String,
    /// Hop type: "bridge" or "proxy".
    #[schema(example = "bridge")]
    pub hop_type: String,
    /// Tap bindings associated with the hop.
    pub taps: Vec<ServiceChainTapBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "name": "frontend-to-db",
    "description": "Traffic chain from frontend to database",
    "hops": [
        {
            "name": "fw-west",
            "hop_type": "bridge",
            "taps": [{"tap": "tapfw0", "role": "in"}]
        }
    ]
}))]
pub struct ServiceChainSpec {
    /// Owning tenant.
    pub tenant_id: String,
    /// Logical network that owns the service chain.
    pub network_id: String,
    /// Human-readable service chain name, unique within the network.
    pub name: String,
    /// Optional operator-facing description.
    #[serde(default)]
    pub description: String,
    /// Ordered list of hops in the chain.
    pub hops: Vec<ServiceChainHop>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready",
    "hop_count": 1
}))]
pub struct ServiceChainStatus {
    /// Lifecycle phase.
    #[schema(example = "ready")]
    pub phase: String,
    /// Number of hops in the chain.
    #[schema(example = 1)]
    pub hop_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceChainResource {
    pub metadata: ResourceMetadata,
    pub spec: ServiceChainSpec,
    pub status: ServiceChainStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateServiceChainRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: ServiceChainSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateServiceChainRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: ServiceChainSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceChainListResponse {
    pub items: Vec<ServiceChainResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
