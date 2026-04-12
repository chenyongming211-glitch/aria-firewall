use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "name": "tcp-ready",
    "protocol": "tcp",
    "interval_seconds": 5,
    "timeout_seconds": 2,
    "healthy_threshold": 3,
    "unhealthy_threshold": 2,
    "target_port": 443,
    "request_template": null
}))]
pub struct HealthCheckSpec {
    /// Owning tenant for the health check policy.
    #[schema(example = "tenant-0001")]
    pub tenant_id: String,
    /// Optional network scope. Omit for tenant-global reusable checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "network-0001")]
    pub network_id: Option<String>,
    /// Human-readable health check name.
    #[schema(example = "tcp-ready")]
    pub name: String,
    /// Active probe protocol.
    #[schema(example = "tcp")]
    pub protocol: String,
    /// Probe interval in seconds.
    #[schema(example = 5)]
    pub interval_seconds: u32,
    /// Probe timeout in seconds.
    #[schema(example = 2)]
    pub timeout_seconds: u32,
    /// Consecutive successes required to mark a backend healthy.
    #[schema(example = 3)]
    pub healthy_threshold: u32,
    /// Consecutive failures required to mark a backend unhealthy.
    #[schema(example = 2)]
    pub unhealthy_threshold: u32,
    /// Optional override target port for the probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 443)]
    pub target_port: Option<u16>,
    /// Optional request template for future HTTP/TLS probes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "GET /healthz HTTP/1.1\\r\\nHost: app.internal\\r\\n\\r\\n")]
    pub request_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready"
}))]
pub struct HealthCheckStatus {
    /// Lifecycle phase as observed by the platform.
    #[schema(example = "ready")]
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "hc-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {"scope": "prod"}
    },
    "spec": {
        "tenant_id": "tenant-0001",
        "network_id": "network-0001",
        "name": "tcp-ready",
        "protocol": "tcp",
        "interval_seconds": 5,
        "timeout_seconds": 2,
        "healthy_threshold": 3,
        "unhealthy_threshold": 2,
        "target_port": 443,
        "request_template": null
    },
    "status": {
        "phase": "ready"
    }
}))]
pub struct HealthCheckResource {
    pub metadata: ResourceMetadata,
    pub spec: HealthCheckSpec,
    pub status: HealthCheckStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateHealthCheckRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: HealthCheckSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateHealthCheckRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: HealthCheckSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthCheckListResponse {
    pub items: Vec<HealthCheckResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
