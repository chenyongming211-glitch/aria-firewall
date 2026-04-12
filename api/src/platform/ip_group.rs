use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "name": "web-servers",
    "cidrs": ["10.0.1.0/24", "10.0.2.0/24"]
}))]
pub struct IpGroupSpec {
    /// Owning tenant.
    #[schema(example = "tenant-0001")]
    pub tenant_id: String,
    /// Logical network that owns the group.
    #[schema(example = "network-0001")]
    pub network_id: String,
    /// Human-readable group name, unique within the network.
    #[schema(example = "web-servers")]
    pub name: String,
    /// CIDR prefixes belonging to this group.
    pub cidrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready",
    "cidr_count": 2
}))]
pub struct IpGroupStatus {
    /// Lifecycle phase.
    #[schema(example = "ready")]
    pub phase: String,
    /// Number of CIDR prefixes in the group.
    #[schema(example = 1)]
    pub cidr_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "ipgroup-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {}
    },
    "spec": {
        "tenant_id": "tenant-0001",
        "network_id": "network-0001",
        "name": "web-servers",
        "cidrs": ["10.0.1.0/24", "10.0.2.0/24"]
    },
    "status": {
        "phase": "ready",
        "cidr_count": 2
    }
}))]
pub struct IpGroupResource {
    pub metadata: ResourceMetadata,
    pub spec: IpGroupSpec,
    pub status: IpGroupStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateIpGroupRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: IpGroupSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateIpGroupRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: IpGroupSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IpGroupListResponse {
    pub items: Vec<IpGroupResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
