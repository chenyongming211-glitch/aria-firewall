use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use std::collections::BTreeMap;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "prod",
    "description": "Primary production tenant",
    "quotas": {
        "networks": 32,
        "ports": 2048
    }
}))]
pub struct TenantSpec {
    /// Human-readable tenant name.
    #[schema(example = "prod")]
    pub name: String,
    /// Optional tenant description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "Primary production tenant")]
    pub description: Option<String>,
    /// Optional quota hints enforced by higher-level control logic.
    #[serde(default)]
    pub quotas: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready"
}))]
pub struct TenantStatus {
    /// Lifecycle phase as observed by the controller.
    #[schema(example = "ready")]
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "tenant-0001",
        "resource_version": "3",
        "created_at": "1712649600",
        "updated_at": "1712649655",
        "labels": {"env": "prod"}
    },
    "spec": {
        "name": "prod",
        "description": "Primary production tenant",
        "quotas": {"networks": 32, "ports": 2048}
    },
    "status": {
        "phase": "ready"
    }
}))]
pub struct TenantResource {
    pub metadata: ResourceMetadata,
    pub spec: TenantSpec,
    pub status: TenantStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTenantRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: TenantSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateTenantRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: TenantSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TenantListResponse {
    pub items: Vec<TenantResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
