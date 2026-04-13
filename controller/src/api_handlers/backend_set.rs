use aria_api::{
    BackendSetListQuery, BackendSetListResponse, BackendSetResource, CreateBackendSetRequest,
    MessageResponse, PlatformApiError, UpdateBackendSetRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        backend_set_status, deleted_message, labels_match, metadata_from_create,
        metadata_from_update, optional_eq, optional_option_eq, paginate, parse_label_selector,
    },
    AppState, ControllerError,
};

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
