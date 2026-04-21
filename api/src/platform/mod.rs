use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use std::collections::BTreeMap;
use utoipa::ToSchema;

pub mod metadata;
pub mod query_types;
pub mod tenant;
pub mod node;
pub mod network;
pub mod port;
pub mod security_group;
pub mod route_table;
pub mod health_check;
pub mod backend_set;
pub mod service;
pub mod ip_group;
pub mod network_policy;
pub mod qos_policy;
pub mod mirror_policy;
pub mod service_chain;
pub mod node_config;
pub mod event;
pub mod diagnose;
pub mod observe;

pub use metadata::*;
pub use query_types::*;
pub use tenant::*;
pub use node::*;
pub use network::*;
pub use port::*;
pub use security_group::*;
pub use route_table::*;
pub use health_check::*;
pub use backend_set::*;
pub use service::*;
pub use ip_group::*;
pub use network_policy::*;
pub use qos_policy::*;
pub use mirror_policy::*;
pub use service_chain::*;
pub use node_config::*;
pub use event::*;
pub use diagnose::*;
pub use observe::*;

// ── Platform-level Error & Health ──

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "code": "resource_not_found",
    "message": "tenant 'tenant-0001' was not found",
    "request_id": "req-20260409-0001",
    "details": {
        "resource": "tenant",
        "id": "tenant-0001"
    }
}))]
pub struct PlatformApiError {
    /// Stable machine-readable error code.
    #[schema(example = "resource_not_found")]
    pub code: String,
    /// Human-readable error safe to display to operators.
    #[schema(example = "tenant 'tenant-0001' was not found")]
    pub message: String,
    /// Optional request identifier propagated by the platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "req-20260409-0001")]
    pub request_id: Option<String>,
    /// Optional machine-readable context for callers and audits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "status": "ok",
    "service": "aria-controller",
    "version": "0.1.0",
    "resource_kinds": {
        "tenants": 1,
        "nodes": 2,
        "networks": 3,
        "ports": 12,
        "security_groups": 4,
        "route_tables": 3,
        "services": 2,
        "backend_sets": 2,
        "health_checks": 1
    }
}))]
pub struct ControllerHealthResponse {
    /// Overall controller health.
    #[schema(example = "ok")]
    pub status: String,
    /// Service name reporting the status.
    #[schema(example = "aria-controller")]
    pub service: String,
    /// Controller version string.
    #[schema(example = "0.1.0")]
    pub version: String,
    /// Current in-memory object counts by resource kind.
    pub resource_kinds: BTreeMap<String, usize>,
}
