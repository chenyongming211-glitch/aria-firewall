use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "event_id": "evt-20260421-0001",
    "event_type": "http",
    "timestamp": "1713676800",
    "node_id": "node-0001",
    "direction": "ingress",
    "verdict": "pass",
    "hook": "uprobe",
    "instance_id": "tap-a",
    "service_id": "svc-web",
    "src_ip": "10.0.1.10",
    "dst_ip": "10.0.10.20",
    "src_port": 52344,
    "dst_port": 443,
    "protocol": "tcp",
    "payload": {
        "method": "GET",
        "path": "/healthz",
        "status_code": 200
    }
}))]
pub struct EventEnvelope {
    #[schema(example = "evt-20260421-0001")]
    pub event_id: String,
    #[schema(example = "http")]
    pub event_type: String,
    #[schema(example = "1713676800")]
    pub timestamp: String,
    #[schema(example = "node-0001")]
    pub node_id: String,
    #[schema(example = "ingress")]
    pub direction: String,
    #[schema(example = "pass")]
    pub verdict: String,
    #[schema(example = "uprobe")]
    pub hook: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dst_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dst_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    pub payload: serde_json::Value,
}
