use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tap": "tapfw0",
    "role": "in"
}))]
pub struct TapBindingEntry {
    /// Tap interface name bound to the hop.
    #[schema(example = "tapfw0")]
    pub tap: String,
    /// Logical role of the tap within the hop.
    #[schema(example = "in")]
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceHopEntry {
    /// Friendly hop name.
    #[schema(example = "fw-west")]
    pub name: String,
    /// Hop type such as `bridge` or `proxy`.
    #[schema(example = "bridge")]
    pub hop_type: String,
    /// Tap bindings associated with the hop.
    pub taps: Vec<TapBindingEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "frontend-to-db",
    "description": "Traffic chain from frontend to database",
    "hops": [
        {
            "name": "fw-west",
            "hop_type": "bridge",
            "taps": [{"tap": "tapfw0", "role": "in"}]
        }
    ]
}))]
pub struct ServiceChainEntry {
    /// Stable service chain name.
    #[schema(example = "frontend-to-db")]
    pub name: String,
    /// Optional operator-facing description.
    #[schema(example = "Traffic chain from frontend to database")]
    #[serde(default)]
    pub description: String,
    /// Ordered list of hops in the chain.
    pub hops: Vec<ServiceHopEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DataplaneServiceChainListResponse {
    pub chains: Vec<ServiceChainEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "frontend-to-db",
    "description": "Traffic chain from frontend to database",
    "hops": [
        {
            "name": "fw-west",
            "hop_type": "bridge",
            "taps": [{"tap": "tapfw0", "role": "in"}]
        },
        {
            "name": "db-service",
            "hop_type": "proxy",
            "taps": [{"tap": "tapdb0", "role": "out"}]
        }
    ]
}))]
pub struct DataplaneCreateServiceChainRequest {
    /// Stable service chain name.
    #[schema(example = "frontend-to-db")]
    pub name: String,
    /// Optional operator-facing description.
    #[schema(example = "Traffic chain from frontend to database")]
    #[serde(default)]
    pub description: String,
    /// Ordered list of hops in the chain.
    pub hops: Vec<ServiceHopEntry>,
}
