use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "name": "prod-vpc",
    "network_type": "l3",
    "ipv4_enabled": true,
    "ipv6_enabled": false,
    "route_mode": "native"
}))]
pub struct NetworkSpec {
    /// Owning tenant identifier.
    #[schema(example = "tenant-0001")]
    pub tenant_id: String,
    /// Human-readable network name.
    #[schema(example = "prod-vpc")]
    pub name: String,
    /// Network mode such as `l2`, `l3`, or `overlay`.
    #[schema(example = "l3")]
    pub network_type: String,
    /// Whether IPv4 addressing is enabled.
    #[schema(example = true)]
    pub ipv4_enabled: bool,
    /// Whether IPv6 addressing is enabled.
    #[schema(example = false)]
    pub ipv6_enabled: bool,
    /// Route execution mode such as `native`, `overlay`, or `hybrid`.
    #[schema(example = "native")]
    pub route_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready"
}))]
pub struct NetworkStatus {
    /// Lifecycle phase as observed by the platform.
    #[schema(example = "ready")]
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "network-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {"tier": "prod"}
    },
    "spec": {
        "tenant_id": "tenant-0001",
        "name": "prod-vpc",
        "network_type": "l3",
        "ipv4_enabled": true,
        "ipv6_enabled": false,
        "route_mode": "native"
    },
    "status": {
        "phase": "ready"
    }
}))]
pub struct NetworkResource {
    pub metadata: ResourceMetadata,
    pub spec: NetworkSpec,
    pub status: NetworkStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateNetworkRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: NetworkSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateNetworkRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: NetworkSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NetworkListResponse {
    pub items: Vec<NetworkResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
