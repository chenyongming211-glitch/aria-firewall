use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "dst_ip": "10.0.10.20",
    "dst_port": 443,
    "chain": "svc-web-chain",
    "time_window_seconds": 300
}))]
pub struct DiagnoseRequest {
    #[schema(example = "10.0.10.20")]
    pub dst_ip: String,
    #[schema(example = 443)]
    pub dst_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "svc-web-chain")]
    pub chain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = 300)]
    pub time_window_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "evidence_type": "http",
    "severity": "critical",
    "title": "HTTP 5xx ratio elevated",
    "summary": "HTTP 5xx ratio is 12.5% across 80 requests, above the unhealthy threshold of 10%",
    "metrics": {
        "requests": 80,
        "count_5xx": 10,
        "ratio_5xx": 12.5
    }
}))]
pub struct DiagnoseEvidence {
    #[schema(example = "http")]
    pub evidence_type: String,
    #[schema(example = "warning")]
    pub severity: String,
    #[schema(example = "HTTP 5xx ratio elevated")]
    pub title: String,
    #[schema(example = "HTTP 5xx ratio is 12.5% for the requested destination")]
    pub summary: String,
    pub metrics: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "diagnose_id": "diag-20260421-0001",
    "verdict": "degraded",
    "summary": "Observed degraded signals for 10.0.10.20:443: transport nqa=72.0 across 12 flows",
    "evidence": [
        {
            "evidence_type": "tcprt",
            "severity": "warning",
            "title": "Transport quality degraded",
            "summary": "Average NQA score is 72.0 across 12 flows, below the healthy threshold of 80",
            "metrics": {
                "instances": 1,
                "flow_count": 12,
                "avg_nqa_score": 72.0
            }
        }
    ],
    "candidate_causes": [
        "transport_nqa=72.0"
    ],
    "suggested_actions": [
        "Inspect TCP retransmissions and RTT trends for the destination"
    ]
}))]
pub struct DiagnoseResponse {
    #[schema(example = "diag-20260421-0001")]
    pub diagnose_id: String,
    #[schema(example = "degraded")]
    pub verdict: String,
    #[schema(example = "Observed elevated HTTP failures and degraded transport quality for 10.0.10.20:443")]
    pub summary: String,
    pub evidence: Vec<DiagnoseEvidence>,
    pub candidate_causes: Vec<String>,
    pub suggested_actions: Vec<String>,
}
