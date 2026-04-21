use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};

use super::common::{err_response, AppState};

#[utoipa::path(
    get,
    path = "/api/v1/observe/events",
    tag = "observe",
    summary = "List normalized observability events across available data sources",
    operation_id = "listObserveEvents",
    params(
        ("event_type" = Option<String>, Query, description = "Filter by event type"),
        ("src_ip" = Option<String>, Query, description = "Filter by source IP"),
        ("dst_ip" = Option<String>, Query, description = "Filter by destination IP"),
        ("dst_port" = Option<u16>, Query, description = "Filter by destination port"),
        ("limit" = Option<usize>, Query, description = "Maximum number of events to return")
    ),
    responses(
        (status = 200, description = "Normalized observability events", body = aria_api::ObserveResponse),
        (status = 400, description = "Validation error", body = aria_api::ApiError),
        (status = 404, description = "Instance not found", body = aria_api::ApiError),
        (status = 500, description = "Internal server error", body = aria_api::ApiError),
        (status = 503, description = "Observability source not ready", body = aria_api::ApiError)
    )
)]
pub async fn list_observe_events(
    State(cp): State<AppState>,
    Query(query): Query<aria_api::ObserveQuery>,
) -> impl IntoResponse {
    match cp.observe_events(&query).await {
        Ok(response) => Ok(Json(response)),
        Err(error) => Err(err_response(error)),
    }
}
