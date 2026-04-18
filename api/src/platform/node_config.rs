use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "node_id": "node-0001",
    "conntrack_enabled": true,
    "monitoring_enabled": true,
    "acl_enabled": true,
    "qos_enabled": false,
    "mirror_enabled": false,
    "tcprt_enabled": false,
    "lb_enabled": false,
    "ssl_enabled": false,
    "ct_tcp_established_ns": 432000000000000_u64,
    "ct_tcp_new_ns": 120000000000_u64,
    "ct_udp_ns": 30000000000_u64,
    "ct_icmp_ns": 30000000000_u64
}))]
pub struct NodeConfigSpec {
    /// Target node identifier.
    #[schema(example = "node-0001")]
    pub node_id: String,
    /// Enable or disable conntrack for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = true)]
    pub conntrack_enabled: Option<bool>,
    /// Enable or disable monitoring for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = true)]
    pub monitoring_enabled: Option<bool>,
    /// Enable or disable ACL enforcement for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = true)]
    pub acl_enabled: Option<bool>,
    /// Enable or disable QoS enforcement for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = false)]
    pub qos_enabled: Option<bool>,
    /// Enable or disable mirror rule evaluation for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = false)]
    pub mirror_enabled: Option<bool>,
    /// Enable or disable TCP-RT observability for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = false)]
    pub tcprt_enabled: Option<bool>,
    /// Enable or disable load-balancing for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = false)]
    pub lb_enabled: Option<bool>,
    /// Enable or disable SSL observability for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = false)]
    pub ssl_enabled: Option<bool>,
    /// Conntrack TCP established timeout in nanoseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 432000000000000_u64)]
    pub ct_tcp_established_ns: Option<u64>,
    /// Conntrack TCP new connection timeout in nanoseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 120000000000_u64)]
    pub ct_tcp_new_ns: Option<u64>,
    /// Conntrack UDP timeout in nanoseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 30000000000_u64)]
    pub ct_udp_ns: Option<u64>,
    /// Conntrack ICMP timeout in nanoseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 30000000000_u64)]
    pub ct_icmp_ns: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready"
}))]
pub struct NodeConfigStatus {
    /// Lifecycle phase.
    #[schema(example = "ready")]
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeConfigResource {
    pub metadata: ResourceMetadata,
    pub spec: NodeConfigSpec,
    pub status: NodeConfigStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateNodeConfigRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: NodeConfigSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateNodeConfigRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: NodeConfigSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeConfigListResponse {
    pub items: Vec<NodeConfigResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
