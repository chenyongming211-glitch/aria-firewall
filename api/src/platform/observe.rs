use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use crate::EventEnvelope;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[schema(example = json!({
    "event_type": "http",
    "instance_id": "tap-a",
    "service_id": "svc-web",
    "src_ip": "10.0.1.10",
    "dst_ip": "10.0.10.20",
    "dst_port": 443,
    "protocol": "tcp",
    "time_range": 300,
    "limit": 100
}))]
pub struct ObserveQuery {
    /// Optional event type filter such as `flow`, `tcprt`, `ssl`, `http`, `drop`, `kernel_drop`, or `lb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "http")]
    pub event_type: Option<String>,
    /// Optional instance ID filter for events tied to a managed tap instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "tap-a")]
    pub instance_id: Option<String>,
    /// Optional service ID filter for service/LB related events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "svc-web")]
    pub service_id: Option<String>,
    /// Optional source IP filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "10.0.1.10")]
    pub src_ip: Option<String>,
    /// Optional destination IP filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "10.0.10.20")]
    pub dst_ip: Option<String>,
    /// Optional destination port filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 443)]
    pub dst_port: Option<u16>,
    /// Optional L4 protocol filter such as `tcp`, `udp`, or `icmp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "tcp")]
    pub protocol: Option<String>,
    /// Optional relative lookback window in seconds. Events without a timestamp are excluded when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 300)]
    pub time_range: Option<u64>,
    /// Maximum number of events to return after filtering and sorting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 100)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "events": [
        {
            "event_id": "evt-http-1",
            "event_type": "http",
            "timestamp": "1713676800",
            "node_id": "local",
            "direction": "unknown",
            "verdict": "observe",
            "hook": "uprobe",
            "instance_id": "system",
            "dst_ip": "10.0.10.20",
            "dst_port": 443,
            "protocol": "tcp",
            "payload": {
                "method": "GET",
                "path": "/healthz",
                "status_code": 200
            }
        }
    ]
}))]
pub struct ObserveResponse {
    pub events: Vec<EventEnvelope>,
}
