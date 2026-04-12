use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "id": "backend-0001",
    "target_type": "port_ref",
    "target_ref": "port-0002",
    "ip": null,
    "node_id": "node-0002",
    "port": 443,
    "weight": 100,
    "admin_state": "enabled",
    "locality": "remote"
}))]
pub struct BackendTargetSpec {
    /// Stable backend member identifier within the backend set.
    #[schema(example = "backend-0001")]
    pub id: String,
    /// Backend target form such as `port_ref` or `ip`.
    #[schema(example = "port_ref")]
    pub target_type: String,
    /// Optional reference to a platform object such as a Port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "port-0002")]
    pub target_ref: Option<String>,
    /// Optional direct IP target for legacy or external backends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "10.0.2.15")]
    pub ip: Option<String>,
    /// Optional node hint for cross-node forwarding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "node-0002")]
    pub node_id: Option<String>,
    /// Backend service port.
    #[schema(example = 443)]
    pub port: u16,
    /// Relative scheduling weight.
    #[schema(example = 100)]
    pub weight: u16,
    /// Operator-facing administrative state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "enabled")]
    pub admin_state: Option<String>,
    /// Placement hint for node-local vs cross-node forwarding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "remote")]
    pub locality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "name": "api-backends",
    "health_check_id": "hc-0001",
    "policy": "maglev",
    "backends": [
        {
            "id": "backend-0001",
            "target_type": "port_ref",
            "target_ref": "port-0002",
            "ip": null,
            "node_id": "node-0002",
            "port": 443,
            "weight": 100,
            "admin_state": "enabled",
            "locality": "remote"
        }
    ]
}))]
pub struct BackendSetSpec {
    /// Owning tenant for the backend set.
    #[schema(example = "tenant-0001")]
    pub tenant_id: String,
    /// Network scope for the backend set.
    #[schema(example = "network-0001")]
    pub network_id: String,
    /// Human-readable backend set name.
    #[schema(example = "api-backends")]
    pub name: String,
    /// Optional health check policy reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "hc-0001")]
    pub health_check_id: Option<String>,
    /// Backend selection policy such as `round_robin` or `maglev`.
    #[schema(example = "maglev")]
    pub policy: String,
    /// Ordered backend members available to the scheduler.
    #[serde(default)]
    pub backends: Vec<BackendTargetSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready",
    "backend_count": 2,
    "healthy_backends": 0
}))]
pub struct BackendSetStatus {
    /// Lifecycle phase as observed by the platform.
    #[schema(example = "ready")]
    pub phase: String,
    /// Total number of configured backends.
    #[schema(example = 2)]
    pub backend_count: usize,
    /// Number of backends currently reported healthy.
    #[schema(example = 0)]
    pub healthy_backends: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "bset-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {"scope": "prod"}
    },
    "spec": {
        "tenant_id": "tenant-0001",
        "network_id": "network-0001",
        "name": "api-backends",
        "health_check_id": "hc-0001",
        "policy": "maglev",
        "backends": [
            {
                "id": "backend-0001",
                "target_type": "port_ref",
                "target_ref": "port-0002",
                "ip": null,
                "node_id": "node-0002",
                "port": 443,
                "weight": 100,
                "admin_state": "enabled",
                "locality": "remote"
            }
        ]
    },
    "status": {
        "phase": "ready",
        "backend_count": 1,
        "healthy_backends": 0
    }
}))]
pub struct BackendSetResource {
    pub metadata: ResourceMetadata,
    pub spec: BackendSetSpec,
    pub status: BackendSetStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateBackendSetRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: BackendSetSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateBackendSetRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: BackendSetSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackendSetListResponse {
    pub items: Vec<BackendSetResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
