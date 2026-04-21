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
    "severity": "warning",
    "title": "HTTP 5xx ratio elevated",
    "summary": "HTTP 5xx ratio is 12.5% for the requested destination",
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
    "summary": "Observed elevated HTTP failures and degraded transport quality for 10.0.10.20:443",
    "evidence": [
        {
            "evidence_type": "tcprt",
            "severity": "warning",
            "title": "Transport quality degraded",
            "summary": "Average NQA score is below the healthy threshold",
            "metrics": {
                "instances": 1,
                "avg_nqa_score": 72.0
            }
        }
    ],
    "candidate_causes": [
        "NQA=72 (<80)"
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
