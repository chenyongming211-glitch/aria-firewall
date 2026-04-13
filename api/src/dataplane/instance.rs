use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "eth0",
    "active": true
}))]
pub struct InstanceInfo {
    /// Managed instance or tap name.
    #[schema(example = "eth0")]
    pub name: String,
    /// Whether the instance currently has active data plane programs attached.
    #[schema(example = true)]
    pub active: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "instances": [
        {"name": "eth0", "active": true},
        {"name": "tapkd01", "active": false}
    ]
}))]
pub struct InstancesResponse {
    /// All managed instances known to the agent.
    pub instances: Vec<InstanceInfo>,
}
