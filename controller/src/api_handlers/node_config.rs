use aria_api::{
    CreateNodeConfigRequest, MessageResponse, NodeConfigListQuery, NodeConfigListResponse,
    NodeConfigResource, NodeConfigStatus, PlatformApiError, UpdateNodeConfigRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update, optional_eq,
        paginate, parse_label_selector,
    },
    AppState, ControllerError,
};

fn node_config_status() -> NodeConfigStatus {
    NodeConfigStatus {
        phase: "ready".to_string(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/node-configs",
    operation_id = "listNodeConfigs",
    tag = "node-configs",
    params(NodeConfigListQuery),
    responses(
        (status = 200, description = "List node configs", body = NodeConfigListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_node_configs(
    State(store): State<AppState>,
    Query(query): Query<NodeConfigListQuery>,
) -> Result<Json<NodeConfigListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_node_configs()
        .await
        .into_iter()
        .filter(|nc| {
            labels_match(&nc.metadata.labels, &selector)
                && optional_eq(query.node_id.as_deref(), &nc.spec.node_id)
                && optional_eq(query.status.as_deref(), &nc.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(NodeConfigListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/node-configs",
    operation_id = "createNodeConfig",
    tag = "node-configs",
    request_body = CreateNodeConfigRequest,
    responses(
        (status = 201, description = "Create node config", body = NodeConfigResource),
        (status = 400, description = "Invalid node config references", body = PlatformApiError),
    )
)]
pub async fn create_node_config(
    State(store): State<AppState>,
    Json(request): Json<CreateNodeConfigRequest>,
) -> Result<(StatusCode, Json<NodeConfigResource>), ControllerError> {
    let resource = NodeConfigResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: node_config_status(),
    };
    let created = store.create_node_config(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/node-configs/{id}",
    operation_id = "getNodeConfig",
    tag = "node-configs",
    params(("id" = String, Path, description = "Node config ID")),
    responses(
        (status = 200, description = "Get node config", body = NodeConfigResource),
        (status = 404, description = "Node config not found", body = PlatformApiError)
    )
)]
pub async fn get_node_config(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NodeConfigResource>, ControllerError> {
    let resource = store
        .get_node_config(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "node_config",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/node-configs/{id}",
    operation_id = "updateNodeConfig",
    tag = "node-configs",
    params(("id" = String, Path, description = "Node config ID")),
    request_body = UpdateNodeConfigRequest,
    responses(
        (status = 200, description = "Update node config", body = NodeConfigResource),
        (status = 400, description = "Invalid node config references", body = PlatformApiError),
    )
)]
pub async fn update_node_config(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateNodeConfigRequest>,
) -> Result<Json<NodeConfigResource>, ControllerError> {
    let existing = store
        .get_node_config(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "node_config",
            id: id.clone(),
        })?;
    let resource = NodeConfigResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: NodeConfigStatus {
            phase: existing.status.phase,
        },
    };
    let updated = store.update_node_config(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/node-configs/{id}",
    operation_id = "deleteNodeConfig",
    tag = "node-configs",
    params(("id" = String, Path, description = "Node config ID")),
    responses(
        (status = 200, description = "Delete node config", body = MessageResponse),
        (status = 404, description = "Node config not found", body = PlatformApiError)
    )
)]
pub async fn delete_node_config(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_node_config(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "node_config",
            id: id.clone(),
        })?;
    store.delete_node_config(&id).await?;
    Ok(Json(deleted_message("node_config", &id)))
}
