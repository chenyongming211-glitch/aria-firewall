use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};

use super::common::{err_response, AppState};

#[utoipa::path(
    post,
    path = "/api/v1/{instance}/diagnose",
    tag = "diagnose",
    summary = "Run structured diagnose for a managed instance",
    operation_id = "diagnoseInstance",
    params(
        ("instance" = String, Path, description = "Managed instance name")
    ),
    request_body = aria_api::DiagnoseRequest,
    responses(
        (status = 200, description = "Structured diagnose result", body = aria_api::DiagnoseResponse),
        (status = 400, description = "Validation error", body = aria_api::ApiError),
        (status = 404, description = "Instance not found", body = aria_api::ApiError),
        (status = 500, description = "Internal server error", body = aria_api::ApiError),
        (status = 503, description = "Instance not ready", body = aria_api::ApiError)
    )
)]
pub async fn diagnose(
    State(cp): State<AppState>,
    Path(instance): Path<String>,
    Json(req): Json<aria_api::DiagnoseRequest>,
) -> impl IntoResponse {
    match cp.diagnose_instance(&instance, &req).await {
        Ok(response) => Ok(Json(response)),
        Err(error) => Err(err_response(error)),
    }
}
