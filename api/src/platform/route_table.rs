use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "destination": "0.0.0.0/0",
    "next_hop_type": "gateway",
    "next_hop_ref": "gw-0001",
    "preference": 100,
    "scope": "global",
    "status": "active"
}))]
pub struct RouteSpec {
    /// Destination prefix.
    #[schema(example = "0.0.0.0/0")]
    pub destination: String,
    /// Next hop type such as `local`, `node`, `gateway`, or `service`.
    #[schema(example = "gateway")]
    pub next_hop_type: String,
    /// Identifier or opaque reference to the selected next hop.
    #[schema(example = "gw-0001")]
    pub next_hop_ref: String,
    /// Route preference within the table.
    #[schema(example = 100)]
    pub preference: u32,
    /// Optional route scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "global")]
    pub scope: Option<String>,
    /// Optional status string for precomputed entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "active")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "network_id": "network-0001",
    "name": "prod-main",
    "routes": [
        {
            "destination": "0.0.0.0/0",
            "next_hop_type": "gateway",
            "next_hop_ref": "gw-0001",
            "preference": 100,
            "scope": "global",
            "status": "active"
        }
    ],
    "default_route": "gw-0001"
}))]
pub struct RouteTableSpec {
    /// Logical network identifier that owns the table.
    #[schema(example = "network-0001")]
    pub network_id: String,
    /// Human-readable route table name.
    #[schema(example = "prod-main")]
    pub name: String,
    /// Statically configured routes.
    #[serde(default)]
    pub routes: Vec<RouteSpec>,
    /// Optional default route reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "gw-0001")]
    pub default_route: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready"
}))]
pub struct RouteTableStatus {
    /// Lifecycle phase as observed by the platform.
    #[schema(example = "ready")]
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "rt-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {"scope": "prod"}
    },
    "spec": {
        "network_id": "network-0001",
        "name": "prod-main",
        "routes": [
            {
                "destination": "0.0.0.0/0",
                "next_hop_type": "gateway",
                "next_hop_ref": "gw-0001",
                "preference": 100,
                "scope": "global",
                "status": "active"
            }
        ],
        "default_route": "gw-0001"
    },
    "status": {
        "phase": "ready"
    }
}))]
pub struct RouteTableResource {
    pub metadata: ResourceMetadata,
    pub spec: RouteTableSpec,
    pub status: RouteTableStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateRouteTableRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: RouteTableSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateRouteTableRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: RouteTableSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RouteTableListResponse {
    pub items: Vec<RouteTableResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
