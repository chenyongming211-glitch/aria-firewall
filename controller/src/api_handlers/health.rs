use aria_api::ControllerHealthResponse;
use axum::{extract::State, Json};

use super::AppState;

#[utoipa::path(
    get,
    path = "/api/v1/health",
    operation_id = "getControllerHealth",
    tag = "platform",
    responses((status = 200, description = "Get controller health", body = ControllerHealthResponse))
)]
pub async fn health(State(store): State<AppState>) -> Json<ControllerHealthResponse> {
    Json(ControllerHealthResponse {
        status: "ok".to_string(),
        service: "aria-controller".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        resource_kinds: store.resource_counts().await,
    })
}
