use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct TenantListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "env=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct NodeListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "rack=r1")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "registered")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "out_of_sync")]
    pub sync_state: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct NetworkListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tier=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct PortListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "role=frontend")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "node-0001")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "pending")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct SecurityGroupListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tier=frontend")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct RouteTableListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "scope=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct HealthCheckListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "scope=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tcp")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct BackendSetListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "scope=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "hc-0001")]
    pub health_check_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct ServiceListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "scope=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "bset-0001")]
    pub backend_set_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "internal")]
    pub exposure_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct IpGroupListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "scope=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct NetworkPolicyListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "scope=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "allow")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct QosPolicyListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "scope=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct MirrorPolicyListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = 50, minimum = 1, maximum = 200)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "50")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "scope=prod")]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "tenant-0001")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "network-0001")]
    pub network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "ready")]
    pub status: Option<String>,
}
