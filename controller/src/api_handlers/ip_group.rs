use aria_api::{
    CreateIpGroupRequest, IpGroupListQuery, IpGroupListResponse, IpGroupResource, IpGroupStatus,
    MessageResponse, PlatformApiError, UpdateIpGroupRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, ip_group_status, labels_match, metadata_from_create,
        metadata_from_update, optional_eq, paginate, parse_label_selector,
    },
    AppState, ControllerError,
};

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
        (status = 409, description = "IP group still referenced", body = PlatformApiError),
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
        (status = 409, description = "IP group still referenced", body = PlatformApiError),
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
