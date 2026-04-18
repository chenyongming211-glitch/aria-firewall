use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use std::fmt;
use utoipa::ToSchema;

mod dataplane;
mod helpers;
mod platform;
mod southbound;

pub use dataplane::*;
pub use helpers::*;
pub use platform::*;
pub use southbound::*;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "code": 400,
    "error": "Validation error: Invalid protocol 'gre'"
}))]
pub struct ApiError {
    /// HTTP-style status or application error code.
    #[schema(example = 400)]
    pub code: u16,
    /// Human-readable error message safe to display to operators.
    #[schema(example = "Validation error: Invalid protocol 'gre'")]
    pub error: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "status": "ok",
    "version": "0.10.0",
    "instances": 2,
    "wal_replay_failures": 0,
    "kernel_drop_available": true,
    "kernel_drop_mode": "kfree_skb_reasonful",
    "kernel_drop_managed_ifaces": 1,
    "kernel_drop_last_error": null
}))]
pub struct HealthResponse {
    /// Overall agent status.
    #[schema(example = "ok")]
    pub status: String,
    /// Running agent version string.
    #[schema(example = "0.10.0")]
    pub version: String,
    /// Number of managed firewall instances currently active.
    #[schema(example = 2)]
    pub instances: usize,
    /// Number of WAL lines that failed to parse or apply during the last replay.
    #[schema(example = 0)]
    #[serde(default)]
    pub wal_replay_failures: u64,
    /// Whether kernel-attributed drop observability is currently available.
    #[schema(example = true)]
    #[serde(default)]
    pub kernel_drop_available: bool,
    /// Active kernel drop collection mode when the feature is enabled.
    #[schema(example = "kfree_skb_reasonful")]
    #[serde(default)]
    pub kernel_drop_mode: Option<String>,
    /// Number of managed interfaces participating in kernel drop collection.
    #[schema(example = 1)]
    #[serde(default)]
    pub kernel_drop_managed_ifaces: usize,
    /// Last kernel drop initialization error, if any.
    #[schema(example = "failed to attach kprobe to kfree_skb_reason")]
    #[serde(default)]
    pub kernel_drop_last_error: Option<String>,
}
