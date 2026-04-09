use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use utoipa::ToSchema;

use crate::{
    NetworkResource, PortResource, RouteTableResource, SecurityGroupResource, TenantResource,
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "kind": "management",
    "value": "10.10.0.11"
}))]
pub struct NodeAddress {
    /// Address role such as `management`, `storage`, or `overlay`.
    #[schema(example = "management")]
    pub kind: String,
    /// Address value in string form.
    #[schema(example = "10.10.0.11")]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "node_id": "node-0001",
    "hostname": "node-sh-01",
    "agent_version": "0.9.0",
    "kernel_version": "6.8.0-71-generic",
    "addresses": [{"kind": "management", "value": "10.10.0.11"}],
    "labels": {"rack": "r1"}
}))]
pub struct NodeInfo {
    /// Stable node identifier known to the controller.
    #[schema(example = "node-0001")]
    pub node_id: String,
    /// Hostname reported by the agent.
    #[schema(example = "node-sh-01")]
    pub hostname: String,
    /// Running agent version.
    #[schema(example = "0.9.0")]
    pub agent_version: String,
    /// Kernel version observed by the agent.
    #[schema(example = "6.8.0-71-generic")]
    pub kernel_version: String,
    /// Advertised addresses for the node.
    #[serde(default)]
    pub addresses: Vec<NodeAddress>,
    /// Node labels surfaced to the controller.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "supported_hooks": ["xdp", "tc"],
    "supports_xdp": true,
    "supports_tc": true,
    "supports_socket_lb": false,
    "supports_trace_ringbuf": true,
    "supports_nat": true,
    "supports_lb": false,
    "supports_encap": false,
    "supports_qos_shaping": true,
    "limits": {"max_ports": 4096, "max_maps": 128},
    "observability_profile": "full"
}))]
pub struct NodeCapability {
    /// Hook families supported by the agent.
    #[serde(default)]
    pub supported_hooks: Vec<String>,
    /// Whether XDP datapath is supported.
    pub supports_xdp: bool,
    /// Whether TC datapath is supported.
    pub supports_tc: bool,
    /// Whether socket-LB style steering is supported.
    pub supports_socket_lb: bool,
    /// Whether ring-buffer trace backend is supported.
    pub supports_trace_ringbuf: bool,
    /// Whether NAT datapath features are supported.
    pub supports_nat: bool,
    /// Whether LB datapath features are supported.
    pub supports_lb: bool,
    /// Whether encapsulation datapath features are supported.
    pub supports_encap: bool,
    /// Whether QoS shaping is supported.
    pub supports_qos_shaping: bool,
    /// Arbitrary numeric limits reported by the node.
    #[serde(default)]
    pub limits: BTreeMap<String, u64>,
    /// Optional coarse observability profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "full")]
    pub observability_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "info": {
        "node_id": "node-0001",
        "hostname": "node-sh-01",
        "agent_version": "0.9.0",
        "kernel_version": "6.8.0-71-generic",
        "addresses": [{"kind": "management", "value": "10.10.0.11"}],
        "labels": {"rack": "r1"}
    },
    "capability": {
        "supported_hooks": ["xdp", "tc"],
        "supports_xdp": true,
        "supports_tc": true,
        "supports_socket_lb": false,
        "supports_trace_ringbuf": true,
        "supports_nat": true,
        "supports_lb": false,
        "supports_encap": false,
        "supports_qos_shaping": true,
        "limits": {"max_ports": 4096},
        "observability_profile": "full"
    }
}))]
pub struct NodeRegisterRequest {
    /// Static node identity and runtime information.
    pub info: NodeInfo,
    /// Datapath capability report for this agent.
    pub capability: NodeCapability,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "accepted": true,
    "node_id": "node-0001",
    "desired_generation": "5",
    "full_sync_required": true,
    "desired_state_url": "/api/v1/southbound/nodes/node-0001/desired-state"
}))]
pub struct NodeRegisterResponse {
    /// Whether the registration was accepted.
    pub accepted: bool,
    /// Effective node identifier recognized by the controller.
    #[schema(example = "node-0001")]
    pub node_id: String,
    /// Current desired generation for the node.
    #[schema(example = "5")]
    pub desired_generation: String,
    /// Whether the agent should perform a full sync.
    pub full_sync_required: bool,
    /// URL where the current desired state can be fetched.
    #[schema(example = "/api/v1/southbound/nodes/node-0001/desired-state")]
    pub desired_state_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "resource_kind": "port",
    "id": "port-0001"
}))]
pub struct DesiredStateDeleteRef {
    /// Deleted resource kind.
    #[schema(example = "port")]
    pub resource_kind: String,
    /// Deleted resource identifier.
    #[schema(example = "port-0001")]
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "generation": "5",
    "full_sync": true,
    "issued_at": "1712649900",
    "node_id": "node-0001",
    "object_counts": {
        "tenants": 1,
        "networks": 1,
        "ports": 2,
        "security_groups": 1,
        "route_tables": 1,
        "deletes": 0
    },
    "tenants": [],
    "networks": [],
    "ports": [],
    "security_groups": [],
    "route_tables": [],
    "deletes": []
}))]
pub struct DesiredStateEnvelope {
    /// Desired-state generation identifier.
    #[schema(example = "5")]
    pub generation: String,
    /// Whether this envelope is a full snapshot.
    pub full_sync: bool,
    /// Controller issue timestamp.
    #[schema(example = "1712649900")]
    pub issued_at: String,
    /// Target node identifier.
    #[schema(example = "node-0001")]
    pub node_id: String,
    /// Per-kind object counts contained in the envelope.
    #[serde(default)]
    pub object_counts: BTreeMap<String, usize>,
    /// Tenant objects relevant to the node.
    #[serde(default)]
    pub tenants: Vec<TenantResource>,
    /// Network objects relevant to the node.
    #[serde(default)]
    pub networks: Vec<NetworkResource>,
    /// Port objects relevant to the node.
    #[serde(default)]
    pub ports: Vec<PortResource>,
    /// Security-group objects relevant to the node.
    #[serde(default)]
    pub security_groups: Vec<SecurityGroupResource>,
    /// Route-table objects relevant to the node.
    #[serde(default)]
    pub route_tables: Vec<RouteTableResource>,
    /// Explicit deletes for incremental protocols.
    #[serde(default)]
    pub deletes: Vec<DesiredStateDeleteRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "generation": "5",
    "issued_at": "1712649900",
    "full_sync": true,
    "object_counts": {
        "tenants": 1,
        "networks": 1,
        "ports": 2,
        "security_groups": 1,
        "route_tables": 1,
        "deletes": 0
    }
}))]
pub struct DesiredStatePublishRecord {
    /// Published desired-state generation identifier.
    #[schema(example = "5")]
    pub generation: String,
    /// First controller publish timestamp for this generation snapshot.
    #[schema(example = "1712649900")]
    pub issued_at: String,
    /// Whether the publication corresponds to a full snapshot.
    pub full_sync: bool,
    /// Per-kind object counts contained in the published snapshot.
    #[serde(default)]
    pub object_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "resource_kind": "port",
    "id": "port-0001",
    "reason": "missing target network"
}))]
pub struct ApplyObjectFailure {
    /// Failed resource kind.
    #[schema(example = "port")]
    pub resource_kind: String,
    /// Failed resource identifier.
    #[schema(example = "port-0001")]
    pub id: String,
    /// Short machine-meaningful or operator-facing reason.
    #[schema(example = "missing target network")]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "generation": "5",
    "status": "applied",
    "applied_at": "1712649910",
    "compiled_objects": {"ports": 4, "security_groups": 2},
    "failed_objects": [],
    "warnings": [],
    "degraded_reasons": []
}))]
pub struct ApplyStatusReport {
    /// Generation the agent attempted to apply.
    #[schema(example = "5")]
    pub generation: String,
    /// Apply status such as `applied`, `partial`, or `failed`.
    #[schema(example = "applied")]
    pub status: String,
    /// Agent-side apply timestamp.
    #[schema(example = "1712649910")]
    pub applied_at: String,
    /// Count of objects compiled by kind.
    #[serde(default)]
    pub compiled_objects: BTreeMap<String, usize>,
    /// Per-object failures, if any.
    #[serde(default)]
    pub failed_objects: Vec<ApplyObjectFailure>,
    /// Non-fatal warnings emitted by the agent.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Declared degraded reasons, if any.
    #[serde(default)]
    pub degraded_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "accepted": true,
    "node_id": "node-0001",
    "generation": "5"
}))]
pub struct ApplyStatusResponse {
    /// Whether the report was accepted.
    pub accepted: bool,
    /// Node identifier.
    #[schema(example = "node-0001")]
    pub node_id: String,
    /// Generation acknowledged by the controller.
    #[schema(example = "5")]
    pub generation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "agent_uptime": 1234,
    "datapath_ready": true,
    "attached_ports": 4,
    "event_queue_depth": 0,
    "wal_health": "ok",
    "last_reconcile_at": "1712649915",
    "last_error": null
}))]
pub struct NodeHealthReport {
    /// Agent uptime in seconds.
    pub agent_uptime: u64,
    /// Whether datapath and attach state are currently healthy.
    pub datapath_ready: bool,
    /// Number of currently attached ports.
    pub attached_ports: usize,
    /// Event or telemetry backlog depth.
    pub event_queue_depth: usize,
    /// WAL health summary.
    #[schema(example = "ok")]
    pub wal_health: String,
    /// Last reconcile timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "1712649915")]
    pub last_reconcile_at: Option<String>,
    /// Last fatal or sticky error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "accepted": true,
    "node_id": "node-0001",
    "observed_generation": "5"
}))]
pub struct HeartbeatResponse {
    /// Whether the heartbeat was accepted.
    pub accepted: bool,
    /// Node identifier.
    #[schema(example = "node-0001")]
    pub node_id: String,
    /// Controller generation currently visible to the node.
    #[schema(example = "5")]
    pub observed_generation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "state": "in_sync",
    "reconcile_required": false,
    "reasons": []
}))]
pub struct SouthboundSyncStatus {
    /// Derived controller-side sync state for this node.
    #[schema(example = "in_sync")]
    pub state: String,
    /// Whether the controller expects the node to reconcile again.
    pub reconcile_required: bool,
    /// Operator-facing reasons that explain the derived state.
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "node_id": "node-0001",
    "desired_generation": "5",
    "last_applied_generation": "5",
    "last_seen_at": "1712649915",
    "pending_object_counts": {},
    "last_desired_state": {
        "generation": "5",
        "issued_at": "1712649900",
        "full_sync": true,
        "object_counts": {
            "tenants": 1,
            "networks": 1,
            "ports": 2,
            "security_groups": 1,
            "route_tables": 1,
            "deletes": 0
        }
    },
    "sync_status": {
        "state": "in_sync",
        "reconcile_required": false,
        "reasons": []
    },
    "registration": {
        "info": {
            "node_id": "node-0001",
            "hostname": "node-sh-01",
            "agent_version": "0.9.0",
            "kernel_version": "6.8.0-71-generic",
            "addresses": [{"kind": "management", "value": "10.10.0.11"}],
            "labels": {"rack": "r1"}
        },
        "capability": {
            "supported_hooks": ["xdp", "tc"],
            "supports_xdp": true,
            "supports_tc": true,
            "supports_socket_lb": false,
            "supports_trace_ringbuf": true,
            "supports_nat": true,
            "supports_lb": false,
            "supports_encap": false,
            "supports_qos_shaping": true,
            "limits": {"max_ports": 4096},
            "observability_profile": "full"
        }
    },
    "last_apply_status": null,
    "last_health": null
}))]
pub struct SouthboundNodeStatusResponse {
    /// Node identifier.
    #[schema(example = "node-0001")]
    pub node_id: String,
    /// Current desired generation for the node.
    #[schema(example = "5")]
    pub desired_generation: String,
    /// Last successfully reported generation, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "5")]
    pub last_applied_generation: Option<String>,
    /// Last controller-side observation timestamp, if the node has reported any southbound activity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "1712649915")]
    pub last_seen_at: Option<String>,
    /// Lightweight per-kind object counts that still need reconcile for the current desired generation.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pending_object_counts: BTreeMap<String, usize>,
    /// Latest desired-state publication recorded by the controller, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_desired_state: Option<DesiredStatePublishRecord>,
    /// Derived controller-side sync summary for this node.
    pub sync_status: SouthboundSyncStatus,
    /// Latest registration payload, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration: Option<NodeRegisterRequest>,
    /// Latest apply-status report, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_apply_status: Option<ApplyStatusReport>,
    /// Latest health report, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_health: Option<NodeHealthReport>,
}
