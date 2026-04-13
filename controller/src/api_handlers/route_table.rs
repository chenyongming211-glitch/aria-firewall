use aria_api::{
    CreateRouteTableRequest, MessageResponse, PlatformApiError, RouteTableListQuery,
    RouteTableListResponse, RouteTableResource, UpdateRouteTableRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update, optional_eq,
        paginate, parse_label_selector, route_table_status,
    },
    AppState, ControllerError,
};

#[utoipa::path(
    get,
    path = "/api/v1/route-tables",
    operation_id = "listRouteTables",
    tag = "route-tables",
    params(RouteTableListQuery),
    responses(
        (status = 200, description = "List route tables", body = RouteTableListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_route_tables(
    State(store): State<AppState>,
    Query(query): Query<RouteTableListQuery>,
) -> Result<Json<RouteTableListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_route_tables()
        .await
        .into_iter()
        .filter(|route_table| {
            labels_match(&route_table.metadata.labels, &selector)
                && optional_eq(query.network_id.as_deref(), &route_table.spec.network_id)
                && optional_eq(query.status.as_deref(), &route_table.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(RouteTableListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/route-tables",
    operation_id = "createRouteTable",
    tag = "route-tables",
    request_body = CreateRouteTableRequest,
    responses(
        (status = 201, description = "Create route table", body = RouteTableResource),
        (status = 400, description = "Invalid route table references", body = PlatformApiError),
        (status = 409, description = "Route table already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn create_route_table(
    State(store): State<AppState>,
    Json(request): Json<CreateRouteTableRequest>,
) -> Result<(StatusCode, Json<RouteTableResource>), ControllerError> {
    let resource = RouteTableResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: route_table_status(),
    };
    let created = store.create_route_table(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/route-tables/{id}",
    operation_id = "getRouteTable",
    tag = "route-tables",
    params(("id" = String, Path, description = "Route table ID")),
    responses(
        (status = 200, description = "Get route table", body = RouteTableResource),
        (status = 404, description = "Route table not found", body = PlatformApiError)
    )
)]
pub async fn get_route_table(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RouteTableResource>, ControllerError> {
    let resource = store
        .get_route_table(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "route_table",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/route-tables/{id}",
    operation_id = "updateRouteTable",
    tag = "route-tables",
    params(("id" = String, Path, description = "Route table ID")),
    request_body = UpdateRouteTableRequest,
    responses(
        (status = 200, description = "Update route table", body = RouteTableResource),
        (status = 400, description = "Invalid route table references", body = PlatformApiError),
        (status = 404, description = "Route table not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn update_route_table(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateRouteTableRequest>,
) -> Result<Json<RouteTableResource>, ControllerError> {
    let existing = store
        .get_route_table(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "route_table",
            id: id.clone(),
        })?;
    let resource = RouteTableResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_route_table(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/route-tables/{id}",
    operation_id = "deleteRouteTable",
    tag = "route-tables",
    params(("id" = String, Path, description = "Route table ID")),
    responses(
        (status = 200, description = "Delete route table", body = MessageResponse),
        (status = 404, description = "Route table not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_route_table(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store.delete_route_table(&id).await?;
    Ok(Json(deleted_message("route_table", &id)))
}
