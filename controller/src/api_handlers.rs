use std::collections::BTreeMap;

use aria_api::{
    ControllerHealthResponse, CreateNetworkRequest, CreateNodeRequest, CreatePortRequest,
    CreateRouteTableRequest, CreateSecurityGroupRequest, CreateTenantRequest, MessageResponse,
    NetworkListResponse, NetworkResource, NetworkStatus, NodeListResponse, NodeResource,
    NodeStatus, PlatformApiError, PortListResponse, PortResource, PortStatus,
    ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata, RouteTableListResponse,
    RouteTableResource, RouteTableStatus, SecurityGroupListResponse, SecurityGroupResource,
    SecurityGroupStatus, TenantListResponse, TenantResource, TenantStatus, UpdateNetworkRequest,
    UpdateNodeRequest, UpdatePortRequest, UpdateRouteTableRequest, UpdateSecurityGroupRequest,
    UpdateTenantRequest,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::store::{SharedStore, StoreError};

pub(crate) type AppState = SharedStore;

#[derive(Debug)]
pub(crate) enum ControllerError {
    BadRequest(String),
    Conflict { resource: &'static str, id: String },
    NotFound { resource: &'static str, id: String },
}

impl From<StoreError> for ControllerError {
    fn from(value: StoreError) -> Self {
        match value {
            StoreError::AlreadyExists { resource, id } => Self::Conflict { resource, id },
            StoreError::NotFound { resource, id } => Self::NotFound { resource, id },
        }
    }
}

impl IntoResponse for ControllerError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self {
            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                "invalid_request".to_string(),
                message,
                None,
            ),
            Self::Conflict { resource, id } => (
                StatusCode::CONFLICT,
                "resource_conflict".to_string(),
                format!("{resource} '{id}' already exists"),
                Some(error_details(resource, &id)),
            ),
            Self::NotFound { resource, id } => (
                StatusCode::NOT_FOUND,
                "resource_not_found".to_string(),
                format!("{resource} '{id}' was not found"),
                Some(error_details(resource, &id)),
            ),
        };

        (
            status,
            Json(PlatformApiError {
                code,
                message,
                request_id: None,
                details,
            }),
        )
            .into_response()
    }
}

fn error_details(resource: &'static str, id: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("resource".to_string(), resource.to_string()),
        ("id".to_string(), id.to_string()),
    ])
}

fn metadata_from_create(input: Option<ResourceCreateMetadata>) -> ResourceMetadata {
    let metadata = input.unwrap_or(ResourceCreateMetadata {
        id: None,
        labels: BTreeMap::new(),
    });
    ResourceMetadata {
        id: metadata.id.unwrap_or_default(),
        resource_version: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
        labels: metadata.labels,
    }
}

fn metadata_from_update(
    existing: &ResourceMetadata,
    input: Option<ResourceUpdateMetadata>,
) -> ResourceMetadata {
    match input {
        Some(metadata) => ResourceMetadata {
            id: existing.id.clone(),
            resource_version: metadata
                .resource_version
                .unwrap_or_else(|| existing.resource_version.clone()),
            created_at: existing.created_at.clone(),
            updated_at: existing.updated_at.clone(),
            labels: metadata.labels.unwrap_or_else(|| existing.labels.clone()),
        },
        None => existing.clone(),
    }
}

fn deleted_message(resource: &str, id: &str) -> MessageResponse {
    MessageResponse {
        message: format!("Deleted {resource} {id}"),
    }
}

fn tenant_status() -> TenantStatus {
    TenantStatus {
        phase: "ready".to_string(),
    }
}

fn node_status() -> NodeStatus {
    NodeStatus {
        phase: "registered".to_string(),
        agent_version: None,
        kernel_version: None,
        capabilities: Vec::new(),
    }
}

fn network_status() -> NetworkStatus {
    NetworkStatus {
        phase: "ready".to_string(),
    }
}

fn port_status() -> PortStatus {
    PortStatus {
        phase: "pending".to_string(),
        attachment_state: Some("detached".to_string()),
    }
}

fn security_group_status(rule_count: usize) -> SecurityGroupStatus {
    SecurityGroupStatus {
        phase: "ready".to_string(),
        rule_count,
    }
}

fn route_table_status() -> RouteTableStatus {
    RouteTableStatus {
        phase: "ready".to_string(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    operation_id = "getControllerHealth",
    tag = "platform",
    responses((status = 200, description = "Get controller health", body = ControllerHealthResponse))
)]
pub async fn health(State(store): State<AppState>) -> Json<ControllerHealthResponse> {
    Json(ControllerHealthResponse {
        status: "ok".to_string(),
        service: "aria-controller".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        resource_kinds: store.resource_counts().await,
    })
}

#[utoipa::path(
    get,
    path = "/api/v1/tenants",
    operation_id = "listTenants",
    tag = "tenants",
    responses((status = 200, description = "List tenants", body = TenantListResponse))
)]
pub async fn list_tenants(State(store): State<AppState>) -> Json<TenantListResponse> {
    let items = store.list_tenants().await;
    Json(TenantListResponse {
        total_count: items.len(),
        items,
        next_page_token: None,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/tenants",
    operation_id = "createTenant",
    tag = "tenants",
    request_body = CreateTenantRequest,
    responses(
        (status = 201, description = "Create tenant", body = TenantResource),
        (status = 409, description = "Tenant already exists", body = PlatformApiError)
    )
)]
pub async fn create_tenant(
    State(store): State<AppState>,
    Json(request): Json<CreateTenantRequest>,
) -> Result<(StatusCode, Json<TenantResource>), ControllerError> {
    let resource = TenantResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: tenant_status(),
    };
    let created = store.create_tenant(resource).await?;
    store.bump_generation();
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/tenants/{id}",
    operation_id = "getTenant",
    tag = "tenants",
    params(("id" = String, Path, description = "Tenant ID")),
    responses(
        (status = 200, description = "Get tenant", body = TenantResource),
        (status = 404, description = "Tenant not found", body = PlatformApiError)
    )
)]
pub async fn get_tenant(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TenantResource>, ControllerError> {
    let resource = store
        .get_tenant(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "tenant",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/tenants/{id}",
    operation_id = "updateTenant",
    tag = "tenants",
    params(("id" = String, Path, description = "Tenant ID")),
    request_body = UpdateTenantRequest,
    responses(
        (status = 200, description = "Update tenant", body = TenantResource),
        (status = 404, description = "Tenant not found", body = PlatformApiError)
    )
)]
pub async fn update_tenant(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateTenantRequest>,
) -> Result<Json<TenantResource>, ControllerError> {
    let existing = store
        .get_tenant(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "tenant",
            id: id.clone(),
        })?;
    let resource = TenantResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_tenant(&id, resource).await?;
    store.bump_generation();
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/tenants/{id}",
    operation_id = "deleteTenant",
    tag = "tenants",
    params(("id" = String, Path, description = "Tenant ID")),
    responses(
        (status = 200, description = "Delete tenant", body = MessageResponse),
        (status = 404, description = "Tenant not found", body = PlatformApiError)
    )
)]
pub async fn delete_tenant(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_tenant(&id).await?;
    store.bump_generation();
    Ok(Json(deleted_message("tenant", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/nodes",
    operation_id = "listNodes",
    tag = "nodes",
    responses((status = 200, description = "List nodes", body = NodeListResponse))
)]
pub async fn list_nodes(State(store): State<AppState>) -> Json<NodeListResponse> {
    let items = store.list_nodes().await;
    Json(NodeListResponse {
        total_count: items.len(),
        items,
        next_page_token: None,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/nodes",
    operation_id = "createNode",
    tag = "nodes",
    request_body = CreateNodeRequest,
    responses(
        (status = 201, description = "Create node", body = NodeResource),
        (status = 409, description = "Node already exists", body = PlatformApiError)
    )
)]
pub async fn create_node(
    State(store): State<AppState>,
    Json(request): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<NodeResource>), ControllerError> {
    let resource = NodeResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: node_status(),
    };
    let created = store.create_node(resource).await?;
    store.bump_generation();
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/nodes/{id}",
    operation_id = "getNode",
    tag = "nodes",
    params(("id" = String, Path, description = "Node ID")),
    responses(
        (status = 200, description = "Get node", body = NodeResource),
        (status = 404, description = "Node not found", body = PlatformApiError)
    )
)]
pub async fn get_node(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NodeResource>, ControllerError> {
    let resource = store.get_node(&id).await.ok_or(ControllerError::NotFound {
        resource: "node",
        id,
    })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/nodes/{id}",
    operation_id = "updateNode",
    tag = "nodes",
    params(("id" = String, Path, description = "Node ID")),
    request_body = UpdateNodeRequest,
    responses(
        (status = 200, description = "Update node", body = NodeResource),
        (status = 404, description = "Node not found", body = PlatformApiError)
    )
)]
pub async fn update_node(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateNodeRequest>,
) -> Result<Json<NodeResource>, ControllerError> {
    let existing = store.get_node(&id).await.ok_or(ControllerError::NotFound {
        resource: "node",
        id: id.clone(),
    })?;
    let resource = NodeResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_node(&id, resource).await?;
    store.bump_generation();
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/nodes/{id}",
    operation_id = "deleteNode",
    tag = "nodes",
    params(("id" = String, Path, description = "Node ID")),
    responses(
        (status = 200, description = "Delete node", body = MessageResponse),
        (status = 404, description = "Node not found", body = PlatformApiError)
    )
)]
pub async fn delete_node(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_node(&id).await?;
    store.clear_southbound_runtime(&id).await;
    store.bump_generation();
    Ok(Json(deleted_message("node", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/networks",
    operation_id = "listNetworks",
    tag = "networks",
    responses((status = 200, description = "List networks", body = NetworkListResponse))
)]
pub async fn list_networks(State(store): State<AppState>) -> Json<NetworkListResponse> {
    let items = store.list_networks().await;
    Json(NetworkListResponse {
        total_count: items.len(),
        items,
        next_page_token: None,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/networks",
    operation_id = "createNetwork",
    tag = "networks",
    request_body = CreateNetworkRequest,
    responses(
        (status = 201, description = "Create network", body = NetworkResource),
        (status = 409, description = "Network already exists", body = PlatformApiError)
    )
)]
pub async fn create_network(
    State(store): State<AppState>,
    Json(request): Json<CreateNetworkRequest>,
) -> Result<(StatusCode, Json<NetworkResource>), ControllerError> {
    let resource = NetworkResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: network_status(),
    };
    let created = store.create_network(resource).await?;
    store.bump_generation();
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/networks/{id}",
    operation_id = "getNetwork",
    tag = "networks",
    params(("id" = String, Path, description = "Network ID")),
    responses(
        (status = 200, description = "Get network", body = NetworkResource),
        (status = 404, description = "Network not found", body = PlatformApiError)
    )
)]
pub async fn get_network(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NetworkResource>, ControllerError> {
    let resource = store
        .get_network(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/networks/{id}",
    operation_id = "updateNetwork",
    tag = "networks",
    params(("id" = String, Path, description = "Network ID")),
    request_body = UpdateNetworkRequest,
    responses(
        (status = 200, description = "Update network", body = NetworkResource),
        (status = 404, description = "Network not found", body = PlatformApiError)
    )
)]
pub async fn update_network(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateNetworkRequest>,
) -> Result<Json<NetworkResource>, ControllerError> {
    let existing = store
        .get_network(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network",
            id: id.clone(),
        })?;
    let resource = NetworkResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_network(&id, resource).await?;
    store.bump_generation();
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/networks/{id}",
    operation_id = "deleteNetwork",
    tag = "networks",
    params(("id" = String, Path, description = "Network ID")),
    responses(
        (status = 200, description = "Delete network", body = MessageResponse),
        (status = 404, description = "Network not found", body = PlatformApiError)
    )
)]
pub async fn delete_network(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_network(&id).await?;
    store.bump_generation();
    Ok(Json(deleted_message("network", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/ports",
    operation_id = "listPorts",
    tag = "ports",
    responses((status = 200, description = "List ports", body = PortListResponse))
)]
pub async fn list_ports(State(store): State<AppState>) -> Json<PortListResponse> {
    let items = store.list_ports().await;
    Json(PortListResponse {
        total_count: items.len(),
        items,
        next_page_token: None,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/ports",
    operation_id = "createPort",
    tag = "ports",
    request_body = CreatePortRequest,
    responses(
        (status = 201, description = "Create port", body = PortResource),
        (status = 409, description = "Port already exists", body = PlatformApiError)
    )
)]
pub async fn create_port(
    State(store): State<AppState>,
    Json(request): Json<CreatePortRequest>,
) -> Result<(StatusCode, Json<PortResource>), ControllerError> {
    let resource = PortResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: port_status(),
    };
    let created = store.create_port(resource).await?;
    store.bump_generation();
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/ports/{id}",
    operation_id = "getPort",
    tag = "ports",
    params(("id" = String, Path, description = "Port ID")),
    responses(
        (status = 200, description = "Get port", body = PortResource),
        (status = 404, description = "Port not found", body = PlatformApiError)
    )
)]
pub async fn get_port(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PortResource>, ControllerError> {
    let resource = store.get_port(&id).await.ok_or(ControllerError::NotFound {
        resource: "port",
        id,
    })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/ports/{id}",
    operation_id = "updatePort",
    tag = "ports",
    params(("id" = String, Path, description = "Port ID")),
    request_body = UpdatePortRequest,
    responses(
        (status = 200, description = "Update port", body = PortResource),
        (status = 404, description = "Port not found", body = PlatformApiError)
    )
)]
pub async fn update_port(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdatePortRequest>,
) -> Result<Json<PortResource>, ControllerError> {
    let existing = store.get_port(&id).await.ok_or(ControllerError::NotFound {
        resource: "port",
        id: id.clone(),
    })?;
    let resource = PortResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_port(&id, resource).await?;
    store.bump_generation();
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/ports/{id}",
    operation_id = "deletePort",
    tag = "ports",
    params(("id" = String, Path, description = "Port ID")),
    responses(
        (status = 200, description = "Delete port", body = MessageResponse),
        (status = 404, description = "Port not found", body = PlatformApiError)
    )
)]
pub async fn delete_port(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_port(&id).await?;
    store.bump_generation();
    Ok(Json(deleted_message("port", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/security-groups",
    operation_id = "listSecurityGroups",
    tag = "security-groups",
    responses((status = 200, description = "List security groups", body = SecurityGroupListResponse))
)]
pub async fn list_security_groups(
    State(store): State<AppState>,
) -> Json<SecurityGroupListResponse> {
    let items = store.list_security_groups().await;
    Json(SecurityGroupListResponse {
        total_count: items.len(),
        items,
        next_page_token: None,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/security-groups",
    operation_id = "createSecurityGroup",
    tag = "security-groups",
    request_body = CreateSecurityGroupRequest,
    responses(
        (status = 201, description = "Create security group", body = SecurityGroupResource),
        (status = 409, description = "Security group already exists", body = PlatformApiError)
    )
)]
pub async fn create_security_group(
    State(store): State<AppState>,
    Json(request): Json<CreateSecurityGroupRequest>,
) -> Result<(StatusCode, Json<SecurityGroupResource>), ControllerError> {
    let rule_count = request.spec.rules.len();
    let resource = SecurityGroupResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: security_group_status(rule_count),
    };
    let created = store.create_security_group(resource).await?;
    store.bump_generation();
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/security-groups/{id}",
    operation_id = "getSecurityGroup",
    tag = "security-groups",
    params(("id" = String, Path, description = "Security group ID")),
    responses(
        (status = 200, description = "Get security group", body = SecurityGroupResource),
        (status = 404, description = "Security group not found", body = PlatformApiError)
    )
)]
pub async fn get_security_group(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SecurityGroupResource>, ControllerError> {
    let resource = store
        .get_security_group(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "security_group",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/security-groups/{id}",
    operation_id = "updateSecurityGroup",
    tag = "security-groups",
    params(("id" = String, Path, description = "Security group ID")),
    request_body = UpdateSecurityGroupRequest,
    responses(
        (status = 200, description = "Update security group", body = SecurityGroupResource),
        (status = 404, description = "Security group not found", body = PlatformApiError)
    )
)]
pub async fn update_security_group(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateSecurityGroupRequest>,
) -> Result<Json<SecurityGroupResource>, ControllerError> {
    let existing = store
        .get_security_group(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "security_group",
            id: id.clone(),
        })?;
    let rule_count = request.spec.rules.len();
    let resource = SecurityGroupResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: SecurityGroupStatus {
            phase: existing.status.phase,
            rule_count,
        },
    };
    let updated = store.update_security_group(&id, resource).await?;
    store.bump_generation();
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/security-groups/{id}",
    operation_id = "deleteSecurityGroup",
    tag = "security-groups",
    params(("id" = String, Path, description = "Security group ID")),
    responses(
        (status = 200, description = "Delete security group", body = MessageResponse),
        (status = 404, description = "Security group not found", body = PlatformApiError)
    )
)]
pub async fn delete_security_group(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_security_group(&id).await?;
    store.bump_generation();
    Ok(Json(deleted_message("security_group", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/route-tables",
    operation_id = "listRouteTables",
    tag = "route-tables",
    responses((status = 200, description = "List route tables", body = RouteTableListResponse))
)]
pub async fn list_route_tables(State(store): State<AppState>) -> Json<RouteTableListResponse> {
    let items = store.list_route_tables().await;
    Json(RouteTableListResponse {
        total_count: items.len(),
        items,
        next_page_token: None,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/route-tables",
    operation_id = "createRouteTable",
    tag = "route-tables",
    request_body = CreateRouteTableRequest,
    responses(
        (status = 201, description = "Create route table", body = RouteTableResource),
        (status = 409, description = "Route table already exists", body = PlatformApiError)
    )
)]
pub async fn create_route_table(
    State(store): State<AppState>,
    Json(request): Json<CreateRouteTableRequest>,
) -> Result<(StatusCode, Json<RouteTableResource>), ControllerError> {
    let resource = RouteTableResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: route_table_status(),
    };
    let created = store.create_route_table(resource).await?;
    store.bump_generation();
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/route-tables/{id}",
    operation_id = "getRouteTable",
    tag = "route-tables",
    params(("id" = String, Path, description = "Route table ID")),
    responses(
        (status = 200, description = "Get route table", body = RouteTableResource),
        (status = 404, description = "Route table not found", body = PlatformApiError)
    )
)]
pub async fn get_route_table(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RouteTableResource>, ControllerError> {
    let resource = store
        .get_route_table(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "route_table",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/route-tables/{id}",
    operation_id = "updateRouteTable",
    tag = "route-tables",
    params(("id" = String, Path, description = "Route table ID")),
    request_body = UpdateRouteTableRequest,
    responses(
        (status = 200, description = "Update route table", body = RouteTableResource),
        (status = 404, description = "Route table not found", body = PlatformApiError)
    )
)]
pub async fn update_route_table(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateRouteTableRequest>,
) -> Result<Json<RouteTableResource>, ControllerError> {
    let existing = store
        .get_route_table(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "route_table",
            id: id.clone(),
        })?;
    let resource = RouteTableResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_route_table(&id, resource).await?;
    store.bump_generation();
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/route-tables/{id}",
    operation_id = "deleteRouteTable",
    tag = "route-tables",
    params(("id" = String, Path, description = "Route table ID")),
    responses(
        (status = 200, description = "Delete route table", body = MessageResponse),
        (status = 404, description = "Route table not found", body = PlatformApiError)
    )
)]
pub async fn delete_route_table(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_route_table(&id).await?;
    store.bump_generation();
    Ok(Json(deleted_message("route_table", &id)))
}
