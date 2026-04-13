use aria_api::{
    CreateNodeRequest, MessageResponse, NodeListQuery, NodeListResponse, NodeResource,
    PlatformApiError, UpdateNodeRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, enrich_node_resource, enrich_node_resources, labels_match,
        metadata_from_create, metadata_from_update, node_status, optional_eq, paginate,
        parse_label_selector,
    },
    AppState, ControllerError,
};

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
