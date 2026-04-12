use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use std::collections::BTreeMap;
use utoipa::ToSchema;

use crate::{DesiredStatePublishRecord, SouthboundSyncStatus};
use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "node-sh-01",
    "mgmt_address": "10.10.0.11",
    "az": "cn-east-1a"
}))]
pub struct NodeSpec {
    /// Human-readable node name.
    #[schema(example = "node-sh-01")]
    pub name: String,
    /// Management address used for controller-to-agent coordination.
    #[schema(example = "10.10.0.11")]
    pub mgmt_address: String,
    /// Availability zone or fault domain label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "cn-east-1a")]
    pub az: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "registered",
    "agent_version": "0.9.0",
    "kernel_version": "6.8.0-71-generic",
    "capabilities": ["encap", "nat", "qos_shaping", "socket_lb", "tc", "trace_ringbuf", "xdp"],
    "desired_generation": "7",
    "last_applied_generation": "7",
    "last_seen_at": "1712649915",
    "last_reconcile_at": "1712649915",
    "last_error": null,
    "last_publish_summary": {
        "generation": "7",
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
    "pending_object_counts": {},
    "changed_kinds": [],
    "has_deletes": false,
    "sync_status": {
        "state": "in_sync",
        "reconcile_required": false,
        "reasons": []
    }
}))]
pub struct NodeStatus {
    /// Lifecycle phase as reported by the platform.
    #[schema(example = "registered")]
    pub phase: String,
    /// Currently observed agent version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "0.9.0")]
    pub agent_version: Option<String>,
    /// Observed kernel version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "6.8.0-71-generic")]
    pub kernel_version: Option<String>,
    /// Datapath capabilities surfaced by the node.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Latest controller desired-state generation visible in the node view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "7")]
    pub desired_generation: Option<String>,
    /// Most recently applied generation reported by the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "7")]
    pub last_applied_generation: Option<String>,
    /// Last southbound observation timestamp for this node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "1712649915")]
    pub last_seen_at: Option<String>,
    /// Last reconcile timestamp reported by the agent health heartbeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "1712649915")]
    pub last_reconcile_at: Option<String>,
    /// Sticky runtime error, if any, surfaced by the node health report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "wal_replay_failed")]
    pub last_error: Option<String>,
    /// Latest lightweight publish summary mirrored from the southbound state plane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_publish_summary: Option<DesiredStatePublishRecord>,
    /// Lightweight per-kind object counts that still need reconcile for the current desired generation.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pending_object_counts: BTreeMap<String, usize>,
    /// Lightweight per-kind change summary derived from pending reconcile work for the current generation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_kinds: Vec<String>,
    /// Whether the current desired generation includes explicit deletes.
    #[serde(default)]
    pub has_deletes: bool,
    /// Derived controller-side sync summary mirrored from the southbound status plane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_status: Option<SouthboundSyncStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "node-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {"rack": "r1"}
    },
    "spec": {
        "name": "node-sh-01",
        "mgmt_address": "10.10.0.11",
        "az": "cn-east-1a"
    },
    "status": {
        "phase": "registered",
        "agent_version": "0.9.0",
        "kernel_version": "6.8.0-71-generic",
        "capabilities": ["nat", "tc", "xdp"],
        "desired_generation": "7",
        "last_applied_generation": "7",
        "last_seen_at": "1712649915",
        "last_publish_summary": {
            "generation": "7",
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
        "pending_object_counts": {},
        "changed_kinds": [],
        "has_deletes": false,
        "sync_status": {
            "state": "in_sync",
            "reconcile_required": false,
            "reasons": []
        }
    }
}))]
pub struct NodeResource {
    pub metadata: ResourceMetadata,
    pub spec: NodeSpec,
    pub status: NodeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateNodeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: NodeSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateNodeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: NodeSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeListResponse {
    pub items: Vec<NodeResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
