use aria_api::{
    CreateServiceRequest, MessageResponse, PlatformApiError, ServiceListQuery,
    ServiceListResponse, ServiceResource, UpdateServiceRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update, optional_eq,
        optional_option_eq, paginate, parse_label_selector, service_status,
    },
    AppState, ControllerError,
};

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
