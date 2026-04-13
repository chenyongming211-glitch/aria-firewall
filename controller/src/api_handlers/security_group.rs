use aria_api::{
    CreateSecurityGroupRequest, MessageResponse, PlatformApiError, SecurityGroupListQuery,
    SecurityGroupListResponse, SecurityGroupResource, SecurityGroupStatus,
    UpdateSecurityGroupRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update, optional_eq,
        paginate, parse_label_selector, security_group_status,
    },
    AppState, ControllerError,
};

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
