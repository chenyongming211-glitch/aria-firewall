use aria_api::{
    CreateHealthCheckRequest, HealthCheckListQuery, HealthCheckListResponse, HealthCheckResource,
    MessageResponse, PlatformApiError, UpdateHealthCheckRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, health_check_status, labels_match, metadata_from_create,
        metadata_from_update, optional_eq, optional_option_eq, paginate, parse_label_selector,
    },
    AppState, ControllerError,
};

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
