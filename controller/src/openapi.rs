use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api_handlers::health,
        crate::api_handlers::list_tenants,
        crate::api_handlers::create_tenant,
        crate::api_handlers::get_tenant,
        crate::api_handlers::update_tenant,
        crate::api_handlers::delete_tenant,
        crate::api_handlers::list_nodes,
        crate::api_handlers::create_node,
        crate::api_handlers::get_node,
        crate::api_handlers::update_node,
        crate::api_handlers::delete_node,
        crate::api_handlers::list_networks,
        crate::api_handlers::create_network,
        crate::api_handlers::get_network,
        crate::api_handlers::update_network,
        crate::api_handlers::delete_network,
        crate::api_handlers::list_ports,
        crate::api_handlers::create_port,
        crate::api_handlers::get_port,
        crate::api_handlers::update_port,
        crate::api_handlers::delete_port,
        crate::api_handlers::list_security_groups,
        crate::api_handlers::create_security_group,
        crate::api_handlers::get_security_group,
        crate::api_handlers::update_security_group,
        crate::api_handlers::delete_security_group,
        crate::api_handlers::list_route_tables,
        crate::api_handlers::create_route_table,
        crate::api_handlers::get_route_table,
        crate::api_handlers::update_route_table,
        crate::api_handlers::delete_route_table,
        crate::southbound_handlers::register_node,
        crate::southbound_handlers::desired_state,
        crate::southbound_handlers::apply_status,
        crate::southbound_handlers::heartbeat,
        crate::southbound_handlers::status
    ),
    components(
        schemas(
            aria_api::PlatformApiError,
            aria_api::ControllerHealthResponse,
            aria_api::ResourceMetadata,
            aria_api::ResourceCreateMetadata,
            aria_api::ResourceUpdateMetadata,
            aria_api::TenantSpec,
            aria_api::TenantStatus,
            aria_api::TenantResource,
            aria_api::TenantListQuery,
            aria_api::CreateTenantRequest,
            aria_api::UpdateTenantRequest,
            aria_api::TenantListResponse,
            aria_api::NodeSpec,
            aria_api::NodeStatus,
            aria_api::NodeResource,
            aria_api::NodeListQuery,
            aria_api::CreateNodeRequest,
            aria_api::UpdateNodeRequest,
            aria_api::NodeListResponse,
            aria_api::NetworkSpec,
            aria_api::NetworkStatus,
            aria_api::NetworkResource,
            aria_api::NetworkListQuery,
            aria_api::CreateNetworkRequest,
            aria_api::UpdateNetworkRequest,
            aria_api::NetworkListResponse,
            aria_api::PortSpec,
            aria_api::PortStatus,
            aria_api::PortResource,
            aria_api::PortListQuery,
            aria_api::CreatePortRequest,
            aria_api::UpdatePortRequest,
            aria_api::PortListResponse,
            aria_api::SecurityRuleSpec,
            aria_api::SecurityGroupSpec,
            aria_api::SecurityGroupStatus,
            aria_api::SecurityGroupResource,
            aria_api::SecurityGroupListQuery,
            aria_api::CreateSecurityGroupRequest,
            aria_api::UpdateSecurityGroupRequest,
            aria_api::SecurityGroupListResponse,
            aria_api::RouteSpec,
            aria_api::RouteTableSpec,
            aria_api::RouteTableStatus,
            aria_api::RouteTableResource,
            aria_api::RouteTableListQuery,
            aria_api::CreateRouteTableRequest,
            aria_api::UpdateRouteTableRequest,
            aria_api::RouteTableListResponse,
            aria_api::MessageResponse,
            aria_api::NodeAddress,
            aria_api::NodeInfo,
            aria_api::NodeCapability,
            aria_api::NodeRegisterRequest,
            aria_api::NodeRegisterResponse,
            aria_api::DesiredStateDeleteRef,
            aria_api::DesiredStatePublishRecord,
            aria_api::DesiredStateEnvelope,
            aria_api::ApplyObjectFailure,
            aria_api::ApplyStatusReport,
            aria_api::ApplyStatusResponse,
            aria_api::NodeHealthReport,
            aria_api::HeartbeatResponse,
            aria_api::SouthboundSyncStatus,
            aria_api::SouthboundNodeStatusResponse
        )
    ),
    tags(
        (name = "platform", description = "Controller health and platform-level status"),
        (name = "tenants", description = "Tenant resources"),
        (name = "nodes", description = "Node registrations and status"),
        (name = "networks", description = "Logical network resources"),
        (name = "ports", description = "Port and attachment-facing resources"),
        (name = "security-groups", description = "Security group resources"),
        (name = "route-tables", description = "Route table resources"),
        (name = "southbound", description = "Controller-agent desired-state and status exchange")
    )
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::ApiDoc;
    use utoipa::OpenApi;

    #[test]
    fn openapi_contains_platform_paths_and_components() {
        let doc = serde_json::to_value(ApiDoc::openapi()).expect("openapi should serialize");

        assert!(doc.pointer("/paths/~1api~1v1~1health").is_some());
        assert!(doc.pointer("/paths/~1api~1v1~1tenants").is_some());
        assert!(doc.pointer("/paths/~1api~1v1~1nodes").is_some());
        assert!(doc.pointer("/paths/~1api~1v1~1networks").is_some());
        assert!(doc.pointer("/paths/~1api~1v1~1ports").is_some());
        assert!(doc.pointer("/paths/~1api~1v1~1security-groups").is_some());
        assert!(doc.pointer("/paths/~1api~1v1~1route-tables").is_some());
        assert!(doc
            .pointer("/paths/~1api~1v1~1southbound~1nodes~1{id}~1register")
            .is_some());
        assert!(doc
            .pointer("/paths/~1api~1v1~1southbound~1nodes~1{id}~1desired-state")
            .is_some());
        assert!(doc
            .pointer("/paths/~1api~1v1~1southbound~1nodes~1{id}~1apply-status")
            .is_some());

        assert!(doc
            .pointer("/components/schemas/PlatformApiError")
            .is_some());
        assert!(doc.pointer("/components/schemas/TenantResource").is_some());
        assert!(doc.pointer("/components/schemas/NodeResource").is_some());
        assert!(doc
            .pointer("/components/schemas/NodeStatus/properties/desired_generation")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/NodeStatus/properties/pending_object_counts")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/NodeStatus/properties/changed_kinds")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/NodeStatus/properties/has_deletes")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/NodeStatus/properties/sync_status")
            .is_some());
        assert!(doc.pointer("/components/schemas/NetworkResource").is_some());
        assert!(doc.pointer("/components/schemas/PortResource").is_some());
        assert!(doc
            .pointer("/components/schemas/SecurityGroupResource")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/RouteTableResource")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/DesiredStateEnvelope")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/DesiredStatePublishRecord")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/SouthboundSyncStatus")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/NodeRegisterRequest")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/NodeHealthReport")
            .is_some());

        assert_eq!(
            doc.pointer("/paths/~1api~1v1~1tenants/get/operationId")
                .and_then(|value| value.as_str()),
            Some("listTenants")
        );
        assert_eq!(
            doc.pointer("/paths/~1api~1v1~1security-groups/post/operationId")
                .and_then(|value| value.as_str()),
            Some("createSecurityGroup")
        );
        assert_eq!(
            doc.pointer("/paths/~1api~1v1~1route-tables~1{id}/put/operationId")
                .and_then(|value| value.as_str()),
            Some("updateRouteTable")
        );
        assert_eq!(
            doc.pointer("/paths/~1api~1v1~1southbound~1nodes~1{id}~1register/post/operationId")
                .and_then(|value| value.as_str()),
            Some("registerSouthboundNode")
        );

        let tenant_params = doc
            .pointer("/paths/~1api~1v1~1tenants/get/parameters")
            .and_then(|value| value.as_array())
            .expect("tenant list parameters should exist");
        assert!(tenant_params
            .iter()
            .any(|param| { param.get("name").and_then(|value| value.as_str()) == Some("limit") }));
        assert!(tenant_params.iter().any(|param| {
            param.get("name").and_then(|value| value.as_str()) == Some("label_selector")
        }));

        let node_params = doc
            .pointer("/paths/~1api~1v1~1nodes/get/parameters")
            .and_then(|value| value.as_array())
            .expect("node list parameters should exist");
        assert!(node_params.iter().any(|param| {
            param.get("name").and_then(|value| value.as_str()) == Some("sync_state")
        }));

        let port_params = doc
            .pointer("/paths/~1api~1v1~1ports/get/parameters")
            .and_then(|value| value.as_array())
            .expect("port list parameters should exist");
        assert!(port_params.iter().any(|param| {
            param.get("name").and_then(|value| value.as_str()) == Some("network_id")
        }));
        assert!(port_params.iter().any(|param| {
            param.get("name").and_then(|value| value.as_str()) == Some("node_id")
        }));
        assert!(doc
            .pointer(
                "/components/schemas/SouthboundNodeStatusResponse/properties/last_desired_state"
            )
            .is_some());
        assert!(doc
            .pointer(
                "/components/schemas/SouthboundNodeStatusResponse/properties/pending_object_counts"
            )
            .is_some());
        assert!(doc
            .pointer("/components/schemas/SouthboundNodeStatusResponse/properties/changed_kinds")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/SouthboundNodeStatusResponse/properties/has_deletes")
            .is_some());
        assert!(doc
            .pointer("/components/schemas/SouthboundNodeStatusResponse/properties/sync_status")
            .is_some());
    }
}
