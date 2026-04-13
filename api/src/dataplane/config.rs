use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "iface": "eth0",
    "max_port_policies": 16384
}))]
pub struct SystemStartRequest {
    /// Physical or virtual interface to manage as the standalone firewall instance.
    #[schema(example = "eth0")]
    pub iface: String,
    /// Maximum number of port bitmap-backed policies to allocate for the instance.
    #[schema(example = 16384)]
    #[serde(default = "default_max_port_policies")]
    pub max_port_policies: u32,
}

fn default_max_port_policies() -> u32 {
    16384
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "message": "Added policy: web -> db (ingress)"
}))]
pub struct MessageResponse {
    /// Short operator-facing status message describing the completed action.
    #[schema(example = "Added policy: web -> db (ingress)")]
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "conntrack": true,
    "monitoring": true,
    "acl": true,
    "qos": true,
    "mirror": false,
    "tcprt": true,
    "ssl": false,
    "num_cpus": 8
}))]
pub struct ConfigResponse {
    /// Whether conntrack collection is enabled for the instance.
    #[schema(example = true)]
    pub conntrack: bool,
    /// Whether base monitoring counters are enabled.
    #[schema(example = true)]
    pub monitoring: bool,
    /// Whether ACL enforcement is enabled.
    #[schema(example = true)]
    pub acl: bool,
    /// Whether QoS enforcement is enabled.
    #[schema(example = true)]
    pub qos: bool,
    /// Whether mirror rule evaluation is enabled.
    #[schema(example = false)]
    pub mirror: bool,
    /// Whether TCP-RT observability is enabled.
    #[schema(example = true)]
    pub tcprt: bool,
    /// Whether SSL observability is enabled for the instance.
    #[schema(example = false)]
    pub ssl: bool,
    /// Number of CPUs provisioned for per-CPU maps.
    #[schema(example = 8)]
    pub num_cpus: u16,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "qos": true,
    "mirror": true,
    "ssl": false
}))]
pub struct UpdateConfigRequest {
    /// Toggle conntrack collection.
    pub conntrack: Option<bool>,
    /// Toggle base monitoring counters.
    pub monitoring: Option<bool>,
    /// Toggle ACL enforcement.
    pub acl: Option<bool>,
    /// Toggle QoS enforcement.
    pub qos: Option<bool>,
    /// Toggle mirror rule evaluation.
    pub mirror: Option<bool>,
    /// Toggle TCP-RT observability.
    pub tcprt: Option<bool>,
    /// Toggle SSL observability for the instance.
    pub ssl: Option<bool>,
}
