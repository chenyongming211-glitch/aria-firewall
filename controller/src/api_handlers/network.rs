use aria_api::{
    CreateNetworkRequest, MessageResponse, NetworkListQuery, NetworkListResponse, NetworkResource,
    PlatformApiError, UpdateNetworkRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update, network_status,
        optional_eq, paginate, parse_label_selector,
    },
    AppState, ControllerError,
};

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
