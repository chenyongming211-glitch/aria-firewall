use aria_api::{
    CreatePortRequest, MessageResponse, PlatformApiError, PortListQuery, PortListResponse,
    PortResource, UpdatePortRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update, optional_eq,
        paginate, parse_label_selector, port_status,
    },
    AppState, ControllerError,
};

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
