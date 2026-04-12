use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "https",
    "port": 443,
    "target_port": 8443
}))]
pub struct ServicePortSpec {
    /// Optional listener name for multi-port services.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "https")]
    pub name: Option<String>,
    /// Frontend service port exposed on the VIP.
    #[schema(example = 443)]
    pub port: u16,
    /// Optional backend target port override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 8443)]
    pub target_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "name": "api-service",
    "vip": "10.0.10.20",
    "protocol": "tcp",
    "ports": [
        {"name": "https", "port": 443, "target_port": 8443}
    ],
    "backend_set_id": "bset-0001",
    "session_affinity": "client_ip",
    "lb_policy": "maglev",
    "exposure_type": "internal"
}))]
pub struct ServiceSpec {
    /// Owning tenant for the service.
    #[schema(example = "tenant-0001")]
    pub tenant_id: String,
    /// Network scope for the VIP.
    #[schema(example = "network-0001")]
    pub network_id: String,
    /// Human-readable service name.
    #[schema(example = "api-service")]
    pub name: String,
    /// Virtual IP exposed by the service.
    #[schema(example = "10.0.10.20")]
    pub vip: String,
    /// Service protocol, initially `tcp` or `udp`.
    #[schema(example = "tcp")]
    pub protocol: String,
    /// Listener ports exposed by the service.
    #[serde(default)]
    pub ports: Vec<ServicePortSpec>,
    /// Optional backend set reference bound to the VIP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "bset-0001")]
    pub backend_set_id: Option<String>,
    /// Optional session affinity policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "client_ip")]
    pub session_affinity: Option<String>,
    /// Load-balancing policy used by the service.
    #[schema(example = "maglev")]
    pub lb_policy: String,
    /// Exposure type such as `internal` or `floating-ip-backed`.
    #[schema(example = "internal")]
    pub exposure_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready",
    "exposure_state": "reserved"
}))]
pub struct ServiceStatus {
    /// Lifecycle phase as observed by the platform.
    #[schema(example = "ready")]
    pub phase: String,
    /// Current exposure state for the VIP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "reserved")]
    pub exposure_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "svc-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {"scope": "prod"}
    },
    "spec": {
        "tenant_id": "tenant-0001",
        "network_id": "network-0001",
        "name": "api-service",
        "vip": "10.0.10.20",
        "protocol": "tcp",
        "ports": [
            {"name": "https", "port": 443, "target_port": 8443}
        ],
        "backend_set_id": "bset-0001",
        "session_affinity": "client_ip",
        "lb_policy": "maglev",
        "exposure_type": "internal"
    },
    "status": {
        "phase": "ready",
        "exposure_state": "reserved"
    }
}))]
pub struct ServiceResource {
    pub metadata: ResourceMetadata,
    pub spec: ServiceSpec,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateServiceRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: ServiceSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateServiceRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: ServiceSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceListResponse {
    pub items: Vec<ServiceResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
