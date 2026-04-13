use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_group": "web",
    "src_group_id": 1,
    "dst_group": "db",
    "dst_group_id": 2,
    "proto": "tcp",
    "action": "allow",
    "direction": "ingress",
    "ports": "5432",
    "bitmap_idx": 7
}))]
pub struct PolicyEntry {
    /// Source group name or `any`.
    #[schema(example = "web")]
    pub src_group: String,
    /// Numeric identifier of the source group.
    #[schema(example = 1)]
    pub src_group_id: u32,
    /// Destination group name or `any`.
    #[schema(example = "db")]
    pub dst_group: String,
    /// Numeric identifier of the destination group.
    #[schema(example = 2)]
    pub dst_group_id: u32,
    /// Matched L4 protocol name or protocol number.
    #[schema(example = "tcp")]
    pub proto: String,
    /// Rule action, typically `allow` or `drop`.
    #[schema(example = "allow")]
    pub action: String,
    /// Traffic direction: `ingress` or `egress`.
    #[schema(example = "ingress")]
    pub direction: String,
    /// Optional port filter expression.
    #[schema(example = "5432")]
    pub ports: Option<String>,
    /// Optional bitmap index used for expanded port matching.
    #[schema(example = 7)]
    pub bitmap_idx: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "policies": [
        {
            "src_group": "web",
            "src_group_id": 1,
            "dst_group": "db",
            "dst_group_id": 2,
            "proto": "tcp",
            "action": "allow",
            "direction": "ingress",
            "ports": "5432",
            "bitmap_idx": 7
        }
    ]
}))]
pub struct PoliciesResponse {
    /// Configured policies for the instance.
    pub policies: Vec<PolicyEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_group": "web",
    "dst_group": "db",
    "proto": "tcp",
    "action": "allow",
    "direction": "ingress",
    "ports": "5432"
}))]
pub struct AddPolicyRequest {
    /// Source group name or `any`.
    #[schema(example = "web")]
    pub src_group: String,
    /// Destination group name or `any`.
    #[schema(example = "db")]
    pub dst_group: String,
    /// Protocol name (`tcp`, `udp`, `icmp`, `any`) or protocol number.
    #[schema(example = "tcp")]
    pub proto: String,
    /// Action to apply when the rule matches.
    #[schema(example = "allow")]
    pub action: String,
    /// Traffic direction: `ingress`, `egress`, or `both`.
    #[schema(example = "ingress")]
    #[serde(default = "default_direction")]
    pub direction: String,
    /// Optional port filter expression such as `80,443`, `1000-2000`, or `all`.
    #[schema(example = "5432")]
    pub ports: Option<String>,
}

fn default_direction() -> String {
    "ingress".to_string()
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_group": "web",
    "dst_group": "db",
    "proto": "tcp",
    "direction": "ingress"
}))]
pub struct DeletePolicyRequest {
    /// Source group name or `any`.
    #[schema(example = "web")]
    pub src_group: String,
    /// Destination group name or `any`.
    #[schema(example = "db")]
    pub dst_group: String,
    /// Protocol name (`tcp`, `udp`, `icmp`, `any`) or protocol number.
    #[schema(example = "tcp")]
    pub proto: String,
    /// Traffic direction: `ingress`, `egress`, or `both`.
    #[schema(example = "ingress")]
    #[serde(default = "default_direction")]
    pub direction: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "policies": [
        {
            "src_group": "web",
            "dst_group": "db",
            "proto": "tcp",
            "action": "allow",
            "direction": "ingress",
            "ports": "5432"
        },
        {
            "src_group": "web",
            "dst_group": "any",
            "proto": "udp",
            "action": "drop",
            "direction": "egress",
            "ports": "53"
        }
    ]
}))]
pub struct BatchAddPoliciesRequest {
    /// Policies to create in order.
    pub policies: Vec<AddPolicyRequest>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "added": 2,
    "errors": []
}))]
pub struct BatchPoliciesResponse {
    /// Number of policies successfully added.
    #[schema(example = 2)]
    pub added: usize,
    /// Validation or apply errors for entries that could not be created.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "src_group": "web",
    "src_group_id": 1,
    "dst_group": "db",
    "dst_group_id": 2,
    "proto": "tcp",
    "action": "allow",
    "direction": "ingress",
    "ports": "5432",
    "bitmap_idx": 7,
    "packets": 1024,
    "bytes": 65536,
    "dropped_packets": 0,
    "dropped_bytes": 0
}))]
pub struct PolicyWithStatsEntry {
    /// Source group name or `any`.
    #[schema(example = "web")]
    pub src_group: String,
    /// Numeric identifier of the source group.
    #[schema(example = 1)]
    pub src_group_id: u32,
    /// Destination group name or `any`.
    #[schema(example = "db")]
    pub dst_group: String,
    /// Numeric identifier of the destination group.
    #[schema(example = 2)]
    pub dst_group_id: u32,
    /// Matched L4 protocol name or protocol number.
    #[schema(example = "tcp")]
    pub proto: String,
    /// Rule action, typically `allow` or `drop`.
    #[schema(example = "allow")]
    pub action: String,
    /// Traffic direction: `ingress` or `egress`.
    #[schema(example = "ingress")]
    pub direction: String,
    /// Optional port filter expression.
    #[schema(example = "5432")]
    pub ports: Option<String>,
    /// Optional bitmap index used for expanded port matching.
    #[schema(example = 7)]
    pub bitmap_idx: Option<u32>,
    /// Total packets matched by the rule.
    #[schema(example = 1024)]
    #[serde(default)]
    pub packets: u64,
    /// Total bytes matched by the rule.
    #[schema(example = 65536)]
    #[serde(default)]
    pub bytes: u64,
    /// Total packets dropped by the rule.
    #[schema(example = 0)]
    #[serde(default)]
    pub dropped_packets: u64,
    /// Total bytes dropped by the rule.
    #[schema(example = 0)]
    #[serde(default)]
    pub dropped_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "policies": [
        {
            "src_group": "web",
            "src_group_id": 1,
            "dst_group": "db",
            "dst_group_id": 2,
            "proto": "tcp",
            "action": "allow",
            "direction": "ingress",
            "ports": "5432",
            "bitmap_idx": 7,
            "packets": 1024,
            "bytes": 65536,
            "dropped_packets": 0,
            "dropped_bytes": 0
        }
    ]
}))]
pub struct PoliciesWithStatsResponse {
    /// Policies enriched with aggregated hit and drop counters.
    pub policies: Vec<PolicyWithStatsEntry>,
}
