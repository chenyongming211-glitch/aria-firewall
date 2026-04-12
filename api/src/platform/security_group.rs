use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::metadata::{ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "direction": "ingress",
    "ethertype": "ipv4",
    "src_selector": "0.0.0.0/0",
    "dst_selector": "10.0.0.0/24",
    "protocol": "tcp",
    "port_range": "443",
    "action": "allow",
    "priority": 100,
    "log_enabled": false,
    "audit_mode": false
}))]
pub struct SecurityRuleSpec {
    /// Rule direction such as `ingress` or `egress`.
    #[schema(example = "ingress")]
    pub direction: String,
    /// EtherType family such as `ipv4` or `ipv6`.
    #[schema(example = "ipv4")]
    pub ethertype: String,
    /// Optional source selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "0.0.0.0/0")]
    pub src_selector: Option<String>,
    /// Optional destination selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "10.0.0.0/24")]
    pub dst_selector: Option<String>,
    /// Optional protocol selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "tcp")]
    pub protocol: Option<String>,
    /// Optional port range expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "443")]
    pub port_range: Option<String>,
    /// Rule action.
    #[schema(example = "allow")]
    pub action: String,
    /// Lower values are evaluated first.
    #[schema(example = 100)]
    pub priority: u32,
    /// Whether matching should emit an explicit policy log.
    #[schema(example = false)]
    pub log_enabled: bool,
    /// Whether the rule runs in audit-only mode.
    #[schema(example = false)]
    pub audit_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "tenant_id": "tenant-0001",
    "name": "frontend",
    "description": "Frontend ingress policy set",
    "rules": [
        {
            "direction": "ingress",
            "ethertype": "ipv4",
            "src_selector": "0.0.0.0/0",
            "dst_selector": "10.0.0.0/24",
            "protocol": "tcp",
            "port_range": "443",
            "action": "allow",
            "priority": 100,
            "log_enabled": false,
            "audit_mode": false
        }
    ]
}))]
pub struct SecurityGroupSpec {
    /// Owning tenant identifier.
    #[schema(example = "tenant-0001")]
    pub tenant_id: String,
    /// Human-readable security group name.
    #[schema(example = "frontend")]
    pub name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "Frontend ingress policy set")]
    pub description: Option<String>,
    /// Ordered security rules.
    #[serde(default)]
    pub rules: Vec<SecurityRuleSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "phase": "ready",
    "rule_count": 1
}))]
pub struct SecurityGroupStatus {
    /// Lifecycle phase as observed by the platform.
    #[schema(example = "ready")]
    pub phase: String,
    /// Materialized rule count.
    #[schema(example = 1)]
    pub rule_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "metadata": {
        "id": "sg-0001",
        "resource_version": "1",
        "created_at": "1712649600",
        "updated_at": "1712649600",
        "labels": {"tier": "frontend"}
    },
    "spec": {
        "tenant_id": "tenant-0001",
        "name": "frontend",
        "description": "Frontend ingress policy set",
        "rules": [
            {
                "direction": "ingress",
                "ethertype": "ipv4",
                "src_selector": "0.0.0.0/0",
                "dst_selector": "10.0.0.0/24",
                "protocol": "tcp",
                "port_range": "443",
                "action": "allow",
                "priority": 100,
                "log_enabled": false,
                "audit_mode": false
            }
        ]
    },
    "status": {
        "phase": "ready",
        "rule_count": 1
    }
}))]
pub struct SecurityGroupResource {
    pub metadata: ResourceMetadata,
    pub spec: SecurityGroupSpec,
    pub status: SecurityGroupStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateSecurityGroupRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceCreateMetadata>,
    pub spec: SecurityGroupSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateSecurityGroupRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResourceUpdateMetadata>,
    pub spec: SecurityGroupSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SecurityGroupListResponse {
    pub items: Vec<SecurityGroupResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub total_count: usize,
}
