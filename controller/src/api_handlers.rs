use std::collections::{BTreeMap, BTreeSet};

use aria_api::{
    BackendSetListQuery, BackendSetListResponse, BackendSetResource, BackendSetStatus,
    ControllerHealthResponse, CreateBackendSetRequest, CreateHealthCheckRequest,
    CreateIpGroupRequest, CreateNetworkPolicyRequest, CreateNetworkRequest, CreateNodeRequest,
    CreatePortRequest, CreateRouteTableRequest, CreateSecurityGroupRequest, CreateServiceRequest,
    CreateTenantRequest, HealthCheckListQuery, HealthCheckListResponse, HealthCheckResource,
    HealthCheckStatus, IpGroupListQuery, IpGroupListResponse, IpGroupResource, IpGroupStatus,
    MessageResponse, NetworkListQuery, NetworkListResponse, NetworkPolicyListQuery,
    NetworkPolicyListResponse, NetworkPolicyResource, NetworkPolicyStatus, NetworkResource,
    NetworkStatus, NodeCapability, NodeListQuery, NodeListResponse, NodeResource, NodeStatus,
    PlatformApiError, PortListQuery, PortListResponse, PortResource, PortStatus, ResourceCreateMetadata
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::{
    request_id,
    store::{SharedStore, StoreError},
};

pub(crate) type AppState = SharedStore;

#[derive(Debug)]
pub(crate) enum ControllerError {
    BadRequest(String),
    Conflict {
        resource: &'static str,
        id: String,
    },
    DependencyConflict {
        resource: &'static str,
        id: String,
        dependent_resource: &'static str,
        dependent_id: String,
    },
    Internal(String),
    InvalidReference {
        resource: &'static str,
        field: &'static str,
        value: String,
        referenced_resource: &'static str,
    },
    NotFound {
        resource: &'static str,
        id: String,
    },
}

impl From<StoreError> for ControllerError {
    fn from(value: StoreError) -> Self {
        match value {
            StoreError::AlreadyExists { resource, id } => Self::Conflict { resource, id },
            StoreError::BadRequest(message) => Self::BadRequest(message),
            StoreError::DependencyConflict {
                resource,
                id,
                dependent_resource,
                dependent_id,
            } => Self::DependencyConflict {
                resource,
                id,
                dependent_resource,
                dependent_id,
            },
            StoreError::Internal(message) => Self::Internal(message),
            StoreError::InvalidReference {
                resource,
                field,
                value,
                referenced_resource,
            } => Self::InvalidReference {
                resource,
                field,
                value,
                referenced_resource,
            },
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
            Self::DependencyConflict {
                resource,
                id,
                dependent_resource,
                dependent_id,
            } => (
                StatusCode::CONFLICT,
                "dependency_conflict".to_string(),
                format!(
                    "{resource} '{id}' is still referenced by {dependent_resource} '{dependent_id}'"
                ),
                Some(dependency_conflict_details(
                    resource,
                    &id,
                    dependent_resource,
                    &dependent_id,
                )),
            ),
            Self::Internal(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error".to_string(),
                message,
                None,
            ),
            Self::InvalidReference {
                resource,
                field,
                value,
                referenced_resource,
            } => (
                StatusCode::BAD_REQUEST,
                "invalid_reference".to_string(),
                format!(
                    "{resource} field '{field}' references missing {referenced_resource} '{value}'"
                ),
                Some(invalid_reference_details(
                    resource,
                    field,
                    &value,
                    referenced_resource,
                )),
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
                request_id: request_id::current_request_id(),
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

fn invalid_reference_details(
    resource: &'static str,
    field: &'static str,
    value: &str,
    referenced_resource: &'static str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("resource".to_string(), resource.to_string()),
        ("field".to_string(), field.to_string()),
        ("value".to_string(), value.to_string()),
        (
            "referenced_resource".to_string(),
            referenced_resource.to_string(),
        ),
    ])
}

fn dependency_conflict_details(
    resource: &'static str,
    id: &str,
    dependent_resource: &'static str,
    dependent_id: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("resource".to_string(), resource.to_string()),
        ("id".to_string(), id.to_string()),
        (
            "dependent_resource".to_string(),
            dependent_resource.to_string(),
        ),
        ("dependent_id".to_string(), dependent_id.to_string()),
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

const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 200;

fn parse_limit(limit: Option<usize>) -> Result<usize, ControllerError> {
    match limit {
        Some(0) => Err(ControllerError::BadRequest(
            "limit must be between 1 and 200".to_string(),
        )),
        Some(value) if value > MAX_PAGE_LIMIT => Err(ControllerError::BadRequest(format!(
            "limit must be between 1 and {MAX_PAGE_LIMIT}"
        ))),
        Some(value) => Ok(value),
        None => Ok(DEFAULT_PAGE_LIMIT),
    }
}

fn parse_page_token(page_token: Option<&str>) -> Result<usize, ControllerError> {
    match page_token {
        Some(token) if token.trim().is_empty() => Err(ControllerError::BadRequest(
            "page_token must be a non-negative integer offset".to_string(),
        )),
        Some(token) => token.parse::<usize>().map_err(|_| {
            ControllerError::BadRequest(
                "page_token must be a non-negative integer offset".to_string(),
            )
        }),
        None => Ok(0),
    }
}

fn parse_label_selector(
    selector: Option<&str>,
) -> Result<BTreeMap<String, String>, ControllerError> {
    let Some(selector) = selector.map(str::trim) else {
        return Ok(BTreeMap::new());
    };
    if selector.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut labels = BTreeMap::new();
    for clause in selector.split(',') {
        let clause = clause.trim();
        let Some((key, value)) = clause.split_once('=') else {
            return Err(ControllerError::BadRequest(format!(
                "label_selector clause '{clause}' must use key=value syntax"
            )));
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(ControllerError::BadRequest(
                "label_selector requires non-empty key and value".to_string(),
            ));
        }
        labels.insert(key.to_string(), value.to_string());
    }

    Ok(labels)
}

fn labels_match(labels: &BTreeMap<String, String>, selector: &BTreeMap<String, String>) -> bool {
    selector
        .iter()
        .all(|(key, value)| labels.get(key) == Some(value))
}

fn optional_eq(filter: Option<&str>, value: &str) -> bool {
    match filter {
        Some(filter) => filter == value,
        None => true,
    }
}

fn optional_option_eq(filter: Option<&str>, value: Option<&str>) -> bool {
    match filter {
        Some(filter) => value == Some(filter),
        None => true,
    }
}

fn paginate<T>(
    items: Vec<T>,
    limit: Option<usize>,
    page_token: Option<&str>,
) -> Result<(Vec<T>, Option<String>, usize), ControllerError> {
    let limit = parse_limit(limit)?;
    let offset = parse_page_token(page_token)?;
    let total_count = items.len();
    if offset >= total_count {
        return Ok((Vec::new(), None, total_count));
    }

    let page_items = items
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next_offset = offset + page_items.len();
    let next_page_token = (next_offset < total_count).then(|| next_offset.to_string());
    Ok((page_items, next_page_token, total_count))
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
        desired_generation: None,
        last_applied_generation: None,
        last_seen_at: None,
        last_reconcile_at: None,
        last_error: None,
        last_publish_summary: None,
        pending_object_counts: BTreeMap::new(),
        changed_kinds: Vec::new(),
        has_deletes: false,
        sync_status: None,
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

fn health_check_status() -> HealthCheckStatus {
    HealthCheckStatus {
        phase: "ready".to_string(),
    }
}

fn backend_set_status(backend_count: usize) -> BackendSetStatus {
    BackendSetStatus {
        phase: "ready".to_string(),
        backend_count,
        healthy_backends: 0,
    }
}

fn service_status() -> ServiceStatus {
    ServiceStatus {
        phase: "ready".to_string(),
        exposure_state: Some("reserved".to_string()),
    }
}

fn node_capabilities_from_report(capability: &NodeCapability) -> Vec<String> {
    let mut capabilities = BTreeSet::new();
    capabilities.extend(capability.supported_hooks.iter().cloned());

    if capability.supports_socket_lb {
        capabilities.insert("socket_lb".to_string());
    }
    if capability.supports_trace_ringbuf {
        capabilities.insert("trace_ringbuf".to_string());
    }
    if capability.supports_nat {
        capabilities.insert("nat".to_string());
    }
    if capability.supports_lb {
        capabilities.insert("lb".to_string());
    }
    if capability.supports_encap {
        capabilities.insert("encap".to_string());
    }
    if capability.supports_qos_shaping {
        capabilities.insert("qos_shaping".to_string());
    }

    capabilities.into_iter().collect()
}

fn apply_southbound_node_status(status: &mut NodeStatus, southbound: SouthboundNodeStatusResponse) {
    status.desired_generation = Some(southbound.desired_generation);
    status.last_applied_generation = southbound.last_applied_generation;
    status.last_seen_at = southbound.last_seen_at;
    status.last_publish_summary = southbound.last_desired_state;
    status.pending_object_counts = southbound.pending_object_counts;
    status.changed_kinds = southbound.changed_kinds;
    status.has_deletes = southbound.has_deletes;
    status.sync_status = Some(southbound.sync_status);

    if let Some(registration) = southbound.registration {
        status.agent_version = Some(registration.info.agent_version);
        status.kernel_version = Some(registration.info.kernel_version);
        status.capabilities = node_capabilities_from_report(&registration.capability);
    }

    if let Some(health) = southbound.last_health {
        status.last_reconcile_at = health.last_reconcile_at;
        status.last_error = health.last_error;
    }
}

async fn enrich_node_resource(
    store: &AppState,
    mut resource: NodeResource,
) -> Result<NodeResource, ControllerError> {
    let southbound = store.southbound_status(&resource.metadata.id).await?;
    apply_southbound_node_status(&mut resource.status, southbound);
    Ok(resource)
}

async fn enrich_node_resources(
    store: &AppState,
    resources: Vec<NodeResource>,
) -> Result<Vec<NodeResource>, ControllerError> {
    let mut enriched = Vec::with_capacity(resources.len());
    for resource in resources {
        enriched.push(enrich_node_resource(store, resource).await?);
    }
    Ok(enriched)
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
    params(TenantListQuery),
    responses(
        (status = 200, description = "List tenants", body = TenantListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_tenants(
    State(store): State<AppState>,
    Query(query): Query<TenantListQuery>,
) -> Result<Json<TenantListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_tenants()
        .await
        .into_iter()
        .filter(|tenant| {
            labels_match(&tenant.metadata.labels, &selector)
                && optional_eq(query.status.as_deref(), &tenant.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(TenantListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/tenants",
    operation_id = "createTenant",
    tag = "tenants",
    request_body = CreateTenantRequest,
    responses(
        (status = 201, description = "Create tenant", body = TenantResource),
        (status = 409, description = "Tenant already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 404, description = "Tenant not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 404, description = "Tenant not found", body = PlatformApiError),
        (status = 409, description = "Tenant still has dependent resources", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_tenant(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_tenant(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "tenant",
            id: id.clone(),
        })?;
    store.delete_tenant(&id).await?;
    Ok(Json(deleted_message("tenant", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/nodes",
    operation_id = "listNodes",
    tag = "nodes",
    params(NodeListQuery),
    responses(
        (status = 200, description = "List nodes", body = NodeListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_nodes(
    State(store): State<AppState>,
    Query(query): Query<NodeListQuery>,
) -> Result<Json<NodeListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_nodes()
        .await
        .into_iter()
        .filter(|node| {
            labels_match(&node.metadata.labels, &selector)
                && optional_eq(query.status.as_deref(), &node.status.phase)
        })
        .collect::<Vec<_>>();
    let items = enrich_node_resources(&store, items)
        .await?
        .into_iter()
        .filter(|node| {
            optional_eq(
                query.sync_state.as_deref(),
                node.status
                    .sync_status
                    .as_ref()
                    .map(|status| status.state.as_str())
                    .unwrap_or(""),
            )
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(NodeListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/nodes",
    operation_id = "createNode",
    tag = "nodes",
    request_body = CreateNodeRequest,
    responses(
        (status = 201, description = "Create node", body = NodeResource),
        (status = 409, description = "Node already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
    let created = enrich_node_resource(&store, store.create_node(resource).await?).await?;
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
    let resource = enrich_node_resource(&store, resource).await?;
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
        (status = 404, description = "Node not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
    let updated = enrich_node_resource(&store, store.update_node(&id, resource).await?).await?;
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
        (status = 404, description = "Node not found", body = PlatformApiError),
        (status = 409, description = "Node still has dependent resources", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_node(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.get_node(&id).await.ok_or(ControllerError::NotFound {
        resource: "node",
        id: id.clone(),
    })?;
    store.delete_node(&id).await?;
    Ok(Json(deleted_message("node", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/networks",
    operation_id = "listNetworks",
    tag = "networks",
    params(NetworkListQuery),
    responses(
        (status = 200, description = "List networks", body = NetworkListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_networks(
    State(store): State<AppState>,
    Query(query): Query<NetworkListQuery>,
) -> Result<Json<NetworkListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_networks()
        .await
        .into_iter()
        .filter(|network| {
            labels_match(&network.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &network.spec.tenant_id)
                && optional_eq(query.status.as_deref(), &network.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(NetworkListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/networks",
    operation_id = "createNetwork",
    tag = "networks",
    request_body = CreateNetworkRequest,
    responses(
        (status = 201, description = "Create network", body = NetworkResource),
        (status = 400, description = "Invalid network references", body = PlatformApiError),
        (status = 409, description = "Network already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 400, description = "Invalid network references", body = PlatformApiError),
        (status = 404, description = "Network not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 404, description = "Network not found", body = PlatformApiError),
        (status = 409, description = "Network still has dependent resources", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_network(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_network(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network",
            id: id.clone(),
        })?;
    store.delete_network(&id).await?;
    Ok(Json(deleted_message("network", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/ports",
    operation_id = "listPorts",
    tag = "ports",
    params(PortListQuery),
    responses(
        (status = 200, description = "List ports", body = PortListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_ports(
    State(store): State<AppState>,
    Query(query): Query<PortListQuery>,
) -> Result<Json<PortListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_ports()
        .await
        .into_iter()
        .filter(|port| {
            labels_match(&port.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &port.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &port.spec.network_id)
                && optional_eq(
                    query.node_id.as_deref(),
                    port.spec.node_id.as_deref().unwrap_or(""),
                )
                && optional_eq(query.status.as_deref(), &port.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(PortListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/ports",
    operation_id = "createPort",
    tag = "ports",
    request_body = CreatePortRequest,
    responses(
        (status = 201, description = "Create port", body = PortResource),
        (status = 400, description = "Invalid port references", body = PlatformApiError),
        (status = 409, description = "Port already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 400, description = "Invalid port references", body = PlatformApiError),
        (status = 404, description = "Port not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 404, description = "Port not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_port(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_port(&id).await?;
    Ok(Json(deleted_message("port", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/security-groups",
    operation_id = "listSecurityGroups",
    tag = "security-groups",
    params(SecurityGroupListQuery),
    responses(
        (status = 200, description = "List security groups", body = SecurityGroupListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
// --- NetworkPolicy (Phase 3.5b: ACL platform migration) ---

fn network_policy_status(rule_count: usize) -> NetworkPolicyStatus {
    NetworkPolicyStatus {
        phase: "ready".into(),
        rule_count,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/network-policies",
    operation_id = "listNetworkPolicies",
    tag = "network-policies",
    params(NetworkPolicyListQuery),
    responses(
        (status = 200, description = "List network policies", body = NetworkPolicyListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_network_policies(
    State(store): State<AppState>,
    Query(query): Query<NetworkPolicyListQuery>,
) -> Result<Json<NetworkPolicyListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_network_policies()
        .await
        .into_iter()
        .filter(|np| {
            labels_match(&np.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &np.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &np.spec.network_id)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(NetworkPolicyListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/network-policies",
    operation_id = "createNetworkPolicy",
    tag = "network-policies",
    request_body = CreateNetworkPolicyRequest,
    responses(
        (status = 201, description = "Create network policy", body = NetworkPolicyResource),
        (status = 400, description = "Invalid network policy references", body = PlatformApiError),
        (status = 409, description = "Network policy already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn create_network_policy(
    State(store): State<AppState>,
    Json(request): Json<CreateNetworkPolicyRequest>,
) -> Result<(StatusCode, Json<NetworkPolicyResource>), ControllerError> {
    let rule_count = request.spec.rules.len();
    let resource = NetworkPolicyResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: network_policy_status(rule_count),
    };
    let created = store.create_network_policy(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/network-policies/{id}",
    operation_id = "getNetworkPolicy",
    tag = "network-policies",
    params(("id" = String, Path, description = "Network policy ID")),
    responses(
        (status = 200, description = "Get network policy", body = NetworkPolicyResource),
        (status = 404, description = "Network policy not found", body = PlatformApiError)
    )
)]
pub async fn get_network_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NetworkPolicyResource>, ControllerError> {
    let resource = store
        .get_network_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network_policy",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/network-policies/{id}",
    operation_id = "updateNetworkPolicy",
    tag = "network-policies",
    params(("id" = String, Path, description = "Network policy ID")),
    request_body = UpdateNetworkPolicyRequest,
    responses(
        (status = 200, description = "Update network policy", body = NetworkPolicyResource),
        (status = 400, description = "Invalid network policy references", body = PlatformApiError),
        (status = 404, description = "Network policy not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn update_network_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateNetworkPolicyRequest>,
) -> Result<Json<NetworkPolicyResource>, ControllerError> {
    let existing = store
        .get_network_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network_policy",
            id: id.clone(),
        })?;
    let rule_count = request.spec.rules.len();
    let resource = NetworkPolicyResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: NetworkPolicyStatus {
            phase: existing.status.phase,
            rule_count,
        },
    };
    let updated = store.update_network_policy(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/network-policies/{id}",
    operation_id = "deleteNetworkPolicy",
    tag = "network-policies",
    params(("id" = String, Path, description = "Network policy ID")),
    responses(
        (status = 200, description = "Delete network policy", body = MessageResponse),
        (status = 404, description = "Network policy not found", body = PlatformApiError),
        (status = 409, description = "Network policy still has dependent resources", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_network_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_network_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network_policy",
            id: id.clone(),
        })?;
    store.delete_network_policy(&id).await?;
    Ok(Json(deleted_message("network_policy", &id)))
}

// --- IpGroup (Phase 3.5a: Groups platform migration) ---

fn ip_group_status(cidr_count: usize) -> IpGroupStatus {
    IpGroupStatus {
        phase: "ready".into(),
        cidr_count,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/ip-groups",
    operation_id = "listIpGroups",
    tag = "ip-groups",
    params(IpGroupListQuery),
    responses(
        (status = 200, description = "List IP groups", body = IpGroupListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_ip_groups(
    State(store): State<AppState>,
    Query(query): Query<IpGroupListQuery>,
) -> Result<Json<IpGroupListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_ip_groups()
        .await
        .into_iter()
        .filter(|ip_group| {
            labels_match(&ip_group.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &ip_group.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &ip_group.spec.network_id)
                && optional_eq(query.status.as_deref(), &ip_group.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(IpGroupListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/ip-groups",
    operation_id = "createIpGroup",
    tag = "ip-groups",
    request_body = CreateIpGroupRequest,
    responses(
        (status = 201, description = "Create IP group", body = IpGroupResource),
        (status = 400, description = "Invalid IP group references", body = PlatformApiError),
        (status = 409, description = "IP group already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn create_ip_group(
    State(store): State<AppState>,
    Json(request): Json<CreateIpGroupRequest>,
) -> Result<(StatusCode, Json<IpGroupResource>), ControllerError> {
    let cidr_count = request.spec.cidrs.len();
    let resource = IpGroupResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: ip_group_status(cidr_count),
    };
    let created = store.create_ip_group(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/ip-groups/{id}",
    operation_id = "getIpGroup",
    tag = "ip-groups",
    params(("id" = String, Path, description = "IP group ID")),
    responses(
        (status = 200, description = "Get IP group", body = IpGroupResource),
        (status = 404, description = "IP group not found", body = PlatformApiError)
    )
)]
pub async fn get_ip_group(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<IpGroupResource>, ControllerError> {
    let resource = store
        .get_ip_group(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "ip_group",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/ip-groups/{id}",
    operation_id = "updateIpGroup",
    tag = "ip-groups",
    params(("id" = String, Path, description = "IP group ID")),
    request_body = UpdateIpGroupRequest,
    responses(
        (status = 200, description = "Update IP group", body = IpGroupResource),
        (status = 400, description = "Invalid IP group references", body = PlatformApiError),
        (status = 404, description = "IP group not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn update_ip_group(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateIpGroupRequest>,
) -> Result<Json<IpGroupResource>, ControllerError> {
    let existing = store
        .get_ip_group(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "ip_group",
            id: id.clone(),
        })?;
    let cidr_count = request.spec.cidrs.len();
    let resource = IpGroupResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: IpGroupStatus {
            phase: existing.status.phase,
            cidr_count,
        },
    };
    let updated = store.update_ip_group(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/ip-groups/{id}",
    operation_id = "deleteIpGroup",
    tag = "ip-groups",
    params(("id" = String, Path, description = "IP group ID")),
    responses(
        (status = 200, description = "Delete IP group", body = MessageResponse),
        (status = 404, description = "IP group not found", body = PlatformApiError),
        (status = 409, description = "IP group still has dependent resources", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_ip_group(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_ip_group(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "ip_group",
            id: id.clone(),
        })?;
    store.delete_ip_group(&id).await?;
    Ok(Json(deleted_message("ip_group", &id)))
}

pub async fn list_security_groups(
    State(store): State<AppState>,
    Query(query): Query<SecurityGroupListQuery>,
) -> Result<Json<SecurityGroupListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_security_groups()
        .await
        .into_iter()
        .filter(|security_group| {
            labels_match(&security_group.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &security_group.spec.tenant_id)
                && optional_eq(query.status.as_deref(), &security_group.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(SecurityGroupListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/security-groups",
    operation_id = "createSecurityGroup",
    tag = "security-groups",
    request_body = CreateSecurityGroupRequest,
    responses(
        (status = 201, description = "Create security group", body = SecurityGroupResource),
        (status = 400, description = "Invalid security group references", body = PlatformApiError),
        (status = 409, description = "Security group already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 400, description = "Invalid security group references", body = PlatformApiError),
        (status = 404, description = "Security group not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 404, description = "Security group not found", body = PlatformApiError),
        (status = 409, description = "Security group still has dependent resources", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_security_group(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_security_group(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "security_group",
            id: id.clone(),
        })?;
    store.delete_security_group(&id).await?;
    Ok(Json(deleted_message("security_group", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/route-tables",
    operation_id = "listRouteTables",
    tag = "route-tables",
    params(RouteTableListQuery),
    responses(
        (status = 200, description = "List route tables", body = RouteTableListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_route_tables(
    State(store): State<AppState>,
    Query(query): Query<RouteTableListQuery>,
) -> Result<Json<RouteTableListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_route_tables()
        .await
        .into_iter()
        .filter(|route_table| {
            labels_match(&route_table.metadata.labels, &selector)
                && optional_eq(query.network_id.as_deref(), &route_table.spec.network_id)
                && optional_eq(query.status.as_deref(), &route_table.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(RouteTableListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/route-tables",
    operation_id = "createRouteTable",
    tag = "route-tables",
    request_body = CreateRouteTableRequest,
    responses(
        (status = 201, description = "Create route table", body = RouteTableResource),
        (status = 400, description = "Invalid route table references", body = PlatformApiError),
        (status = 409, description = "Route table already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 400, description = "Invalid route table references", body = PlatformApiError),
        (status = 404, description = "Route table not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
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
        (status = 404, description = "Route table not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_route_table(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_route_table(&id).await?;
    Ok(Json(deleted_message("route_table", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/health-checks",
    operation_id = "listHealthChecks",
    tag = "health-checks",
    params(HealthCheckListQuery),
    responses(
        (status = 200, description = "List health checks", body = HealthCheckListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_health_checks(
    State(store): State<AppState>,
    Query(query): Query<HealthCheckListQuery>,
) -> Result<Json<HealthCheckListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_health_checks()
        .await
        .into_iter()
        .filter(|health_check| {
            labels_match(&health_check.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &health_check.spec.tenant_id)
                && optional_option_eq(
                    query.network_id.as_deref(),
                    health_check.spec.network_id.as_deref(),
                )
                && optional_eq(query.protocol.as_deref(), &health_check.spec.protocol)
                && optional_eq(query.status.as_deref(), &health_check.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(HealthCheckListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/health-checks",
    operation_id = "createHealthCheck",
    tag = "health-checks",
    request_body = CreateHealthCheckRequest,
    responses(
        (status = 201, description = "Create health check", body = HealthCheckResource),
        (status = 400, description = "Invalid health check references", body = PlatformApiError),
        (status = 409, description = "Health check already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn create_health_check(
    State(store): State<AppState>,
    Json(request): Json<CreateHealthCheckRequest>,
) -> Result<(StatusCode, Json<HealthCheckResource>), ControllerError> {
    let resource = HealthCheckResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: health_check_status(),
    };
    let created = store.create_health_check(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/health-checks/{id}",
    operation_id = "getHealthCheck",
    tag = "health-checks",
    params(("id" = String, Path, description = "Health check ID")),
    responses(
        (status = 200, description = "Get health check", body = HealthCheckResource),
        (status = 404, description = "Health check not found", body = PlatformApiError)
    )
)]
pub async fn get_health_check(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<HealthCheckResource>, ControllerError> {
    let resource = store
        .get_health_check(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "health_check",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/health-checks/{id}",
    operation_id = "updateHealthCheck",
    tag = "health-checks",
    params(("id" = String, Path, description = "Health check ID")),
    request_body = UpdateHealthCheckRequest,
    responses(
        (status = 200, description = "Update health check", body = HealthCheckResource),
        (status = 400, description = "Invalid health check references", body = PlatformApiError),
        (status = 404, description = "Health check not found", body = PlatformApiError),
        (status = 409, description = "Health check still referenced", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn update_health_check(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateHealthCheckRequest>,
) -> Result<Json<HealthCheckResource>, ControllerError> {
    let existing = store
        .get_health_check(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "health_check",
            id: id.clone(),
        })?;
    let resource = HealthCheckResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_health_check(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/health-checks/{id}",
    operation_id = "deleteHealthCheck",
    tag = "health-checks",
    params(("id" = String, Path, description = "Health check ID")),
    responses(
        (status = 200, description = "Delete health check", body = MessageResponse),
        (status = 404, description = "Health check not found", body = PlatformApiError),
        (status = 409, description = "Health check still referenced", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_health_check(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_health_check(&id).await?;
    Ok(Json(deleted_message("health_check", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/backend-sets",
    operation_id = "listBackendSets",
    tag = "backend-sets",
    params(BackendSetListQuery),
    responses(
        (status = 200, description = "List backend sets", body = BackendSetListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_backend_sets(
    State(store): State<AppState>,
    Query(query): Query<BackendSetListQuery>,
) -> Result<Json<BackendSetListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_backend_sets()
        .await
        .into_iter()
        .filter(|backend_set| {
            labels_match(&backend_set.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &backend_set.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &backend_set.spec.network_id)
                && optional_option_eq(
                    query.health_check_id.as_deref(),
                    backend_set.spec.health_check_id.as_deref(),
                )
                && optional_eq(query.status.as_deref(), &backend_set.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(BackendSetListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/backend-sets",
    operation_id = "createBackendSet",
    tag = "backend-sets",
    request_body = CreateBackendSetRequest,
    responses(
        (status = 201, description = "Create backend set", body = BackendSetResource),
        (status = 400, description = "Invalid backend set references", body = PlatformApiError),
        (status = 409, description = "Backend set already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn create_backend_set(
    State(store): State<AppState>,
    Json(request): Json<CreateBackendSetRequest>,
) -> Result<(StatusCode, Json<BackendSetResource>), ControllerError> {
    let backend_count = request.spec.backends.len();
    let resource = BackendSetResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: backend_set_status(backend_count),
    };
    let created = store.create_backend_set(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/backend-sets/{id}",
    operation_id = "getBackendSet",
    tag = "backend-sets",
    params(("id" = String, Path, description = "Backend set ID")),
    responses(
        (status = 200, description = "Get backend set", body = BackendSetResource),
        (status = 404, description = "Backend set not found", body = PlatformApiError)
    )
)]
pub async fn get_backend_set(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BackendSetResource>, ControllerError> {
    let resource = store
        .get_backend_set(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "backend_set",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/backend-sets/{id}",
    operation_id = "updateBackendSet",
    tag = "backend-sets",
    params(("id" = String, Path, description = "Backend set ID")),
    request_body = UpdateBackendSetRequest,
    responses(
        (status = 200, description = "Update backend set", body = BackendSetResource),
        (status = 400, description = "Invalid backend set references", body = PlatformApiError),
        (status = 404, description = "Backend set not found", body = PlatformApiError),
        (status = 409, description = "Backend set still referenced", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn update_backend_set(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateBackendSetRequest>,
) -> Result<Json<BackendSetResource>, ControllerError> {
    let existing = store
        .get_backend_set(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "backend_set",
            id: id.clone(),
        })?;
    let backend_count = request.spec.backends.len();
    let resource = BackendSetResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: backend_set_status(backend_count),
    };
    let updated = store.update_backend_set(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/backend-sets/{id}",
    operation_id = "deleteBackendSet",
    tag = "backend-sets",
    params(("id" = String, Path, description = "Backend set ID")),
    responses(
        (status = 200, description = "Delete backend set", body = MessageResponse),
        (status = 404, description = "Backend set not found", body = PlatformApiError),
        (status = 409, description = "Backend set still referenced", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_backend_set(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_backend_set(&id).await?;
    Ok(Json(deleted_message("backend_set", &id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/services",
    operation_id = "listServices",
    tag = "services",
    params(ServiceListQuery),
    responses(
        (status = 200, description = "List services", body = ServiceListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_services(
    State(store): State<AppState>,
    Query(query): Query<ServiceListQuery>,
) -> Result<Json<ServiceListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_services()
        .await
        .into_iter()
        .filter(|service| {
            labels_match(&service.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &service.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &service.spec.network_id)
                && optional_option_eq(
                    query.backend_set_id.as_deref(),
                    service.spec.backend_set_id.as_deref(),
                )
                && optional_eq(query.exposure_type.as_deref(), &service.spec.exposure_type)
                && optional_eq(query.status.as_deref(), &service.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(ServiceListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/services",
    operation_id = "createService",
    tag = "services",
    request_body = CreateServiceRequest,
    responses(
        (status = 201, description = "Create service", body = ServiceResource),
        (status = 400, description = "Invalid service references", body = PlatformApiError),
        (status = 409, description = "Service already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn create_service(
    State(store): State<AppState>,
    Json(request): Json<CreateServiceRequest>,
) -> Result<(StatusCode, Json<ServiceResource>), ControllerError> {
    let resource = ServiceResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: service_status(),
    };
    let created = store.create_service(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/services/{id}",
    operation_id = "getService",
    tag = "services",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Get service", body = ServiceResource),
        (status = 404, description = "Service not found", body = PlatformApiError)
    )
)]
pub async fn get_service(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ServiceResource>, ControllerError> {
    let resource = store
        .get_service(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "service",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/services/{id}",
    operation_id = "updateService",
    tag = "services",
    params(("id" = String, Path, description = "Service ID")),
    request_body = UpdateServiceRequest,
    responses(
        (status = 200, description = "Update service", body = ServiceResource),
        (status = 400, description = "Invalid service references", body = PlatformApiError),
        (status = 404, description = "Service not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn update_service(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateServiceRequest>,
) -> Result<Json<ServiceResource>, ControllerError> {
    let existing = store
        .get_service(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "service",
            id: id.clone(),
        })?;
    let resource = ServiceResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_service(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/services/{id}",
    operation_id = "deleteService",
    tag = "services",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Delete service", body = MessageResponse),
        (status = 404, description = "Service not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_service(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_service(&id).await?;
    Ok(Json(deleted_message("service", &id)))
}
