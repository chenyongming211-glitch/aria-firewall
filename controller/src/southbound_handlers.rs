use aria_api::{
    ApplyStatusReport, ApplyStatusResponse, DesiredStateEnvelope, HeartbeatResponse,
    NodeHealthReport, NodeRegisterRequest, NodeRegisterResponse, PlatformApiError,
    SouthboundNodeStatusResponse,
};
use axum::{
    extract::{Path, State},
    Json,
};

use crate::api_handlers::{AppState, ControllerError};

#[utoipa::path(
    post,
    path = "/api/v1/southbound/nodes/{id}/register",
    operation_id = "registerSouthboundNode",
    tag = "southbound",
    params(("id" = String, Path, description = "Node ID")),
    request_body = NodeRegisterRequest,
    responses(
        (status = 200, description = "Register node and capability profile", body = NodeRegisterResponse),
        (status = 400, description = "Path node ID and payload node ID mismatch", body = PlatformApiError),
        (status = 404, description = "Node not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn register_node(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<NodeRegisterRequest>,
) -> Result<Json<NodeRegisterResponse>, ControllerError> {
    if request.info.node_id != id {
        return Err(ControllerError::BadRequest(format!(
            "path node id '{}' does not match payload node id '{}'",
            id, request.info.node_id
        )));
    }

    let status = store
        .record_registration(&id, request.info, request.capability)
        .await?;
    Ok(Json(NodeRegisterResponse {
        accepted: true,
        node_id: status.node_id,
        desired_generation: status.desired_generation.clone(),
        full_sync_required: true,
        desired_state_url: format!("/api/v1/southbound/nodes/{id}/desired-state"),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/southbound/nodes/{id}/desired-state",
    operation_id = "getSouthboundDesiredState",
    tag = "southbound",
    params(("id" = String, Path, description = "Node ID")),
    responses(
        (status = 200, description = "Fetch desired-state snapshot for node", body = DesiredStateEnvelope),
        (status = 404, description = "Node not found", body = PlatformApiError)
    )
)]
pub async fn desired_state(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DesiredStateEnvelope>, ControllerError> {
    Ok(Json(store.desired_state_for_node(&id).await?))
}

#[utoipa::path(
    post,
    path = "/api/v1/southbound/nodes/{id}/apply-status",
    operation_id = "reportSouthboundApplyStatus",
    tag = "southbound",
    params(("id" = String, Path, description = "Node ID")),
    request_body = ApplyStatusReport,
    responses(
        (status = 200, description = "Report apply result", body = ApplyStatusResponse),
        (status = 404, description = "Node not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn apply_status(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(report): Json<ApplyStatusReport>,
) -> Result<Json<ApplyStatusResponse>, ControllerError> {
    let generation = report.generation.clone();
    store.record_apply_status(&id, report).await?;
    Ok(Json(ApplyStatusResponse {
        accepted: true,
        node_id: id,
        generation,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/southbound/nodes/{id}/heartbeat",
    operation_id = "reportSouthboundHeartbeat",
    tag = "southbound",
    params(("id" = String, Path, description = "Node ID")),
    request_body = NodeHealthReport,
    responses(
        (status = 200, description = "Report node heartbeat", body = HeartbeatResponse),
        (status = 404, description = "Node not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn heartbeat(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(report): Json<NodeHealthReport>,
) -> Result<Json<HeartbeatResponse>, ControllerError> {
    let status = store.record_health(&id, report).await?;
    Ok(Json(HeartbeatResponse {
        accepted: true,
        node_id: id,
        observed_generation: status.desired_generation,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/southbound/nodes/{id}/status",
    operation_id = "getSouthboundNodeStatus",
    tag = "southbound",
    params(("id" = String, Path, description = "Node ID")),
    responses(
        (status = 200, description = "Get latest southbound node runtime status", body = SouthboundNodeStatusResponse),
        (status = 404, description = "Node not found", body = PlatformApiError)
    )
)]
pub async fn status(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SouthboundNodeStatusResponse>, ControllerError> {
    Ok(Json(store.southbound_status(&id).await?))
}
