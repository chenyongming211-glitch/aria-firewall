use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "network_id": "network-0001",
    "segment_id": "segment-0001",
    "instance_id": "instance-0001",
    "node_id": "node-0001",
    "mac_address": "fa:16:3e:11:22:33",
    "fixed_ips": ["10.0.0.10/24"],
    "security_group_ids": ["sg-0001"],
    "allowed_address_pairs": ["10.0.0.20"],
    "anti_spoof_enabled": true,
    "admin_state_up": true
}))]
pub struct PortSpec {
    /// Owning tenant identifier.
    #[schema(example = "tenant-0001")]
    pub tenant_id: String,
    /// Logical network identifier the port belongs to.
    #[schema(example = "network-0001")]
    pub network_id: String,
    /// Optional segment identifier under the network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "segment-0001")]
    pub segment_id: Option<String>,
    /// Optional bound instance identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "instance-0001")]
    pub instance_id: Option<String>,
    /// Optional hosting node identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "node-0001")]
    pub node_id: Option<String>,
    /// Port MAC address.
    #[schema(example = "fa:16:3e:11:22:33")]
    pub mac_address: String,
    /// Fixed addresses assigned to the port.
    #[serde(default)]
    pub fixed_ips: Vec<String>,
    /// Attached security groups.
    #[serde(default)]
    pub security_group_ids: Vec<String>,
    /// Additional addresses allowed beyond the fixed set.
    #[serde(default)]
    pub allowed_address_pairs: Vec<String>,
    /// Whether anti-spoof protection is enabled.
    #[schema(example = true)]
    pub anti_spoof_enabled: bool,
    /// Administrative up/down state.
    #[schema(example = true)]
    pub admin_state_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "pending",
    "attachment_state": "detached"
}))]
pub struct PortStatus {
    /// Lifecycle phase as observed by the platform.
    #[schema(example = "pending")]
    pub phase: String,
    /// High-level attachment state for the port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "detached")]
    pub attachment_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "port-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {"role": "frontend"}
    },
    "spec": {
        "tenant_id": "tenant-0001",
        "network_id": "network-0001",
        "segment_id": "segment-0001",
        "instance_id": "instance-0001",
        "node_id": "node-0001",
        "mac_address": "fa:16:3e:11:22:33",
        "fixed_ips": ["10.0.0.10/24"],
        "security_group_ids": ["sg-0001"],
        "allowed_address_pairs": ["10.0.0.20"],
        "anti_spoof_enabled": true,
        "admin_state_up": true
    },
    "status": {
        "phase": "pending",
        "attachment_state": "detached"
    }
}))]
pub struct PortResource {
    pub metadata: ResourceMetadata,
    pub spec: PortSpec,
    pub status: PortStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePortRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: PortSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdatePortRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: PortSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PortListResponse {
    pub items: Vec<PortResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
