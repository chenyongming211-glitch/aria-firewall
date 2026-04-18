use aria_api::{
    CreateServiceChainRequest, MessageResponse, ServiceChainListQuery, ServiceChainListResponse,
    ServiceChainResource, ServiceChainStatus, PlatformApiError, UpdateServiceChainRequest,
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

fn service_chain_status(hop_count: usize) -> ServiceChainStatus {
    ServiceChainStatus {
        phase: "ready".to_string(),
        hop_count,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/service-chains",
    operation_id = "listServiceChains",
    tag = "service-chains",
    params(ServiceChainListQuery),
    responses(
        (status = 200, description = "List service chains", body = ServiceChainListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_service_chains(
    State(store): State<AppState>,
    Query(query): Query<ServiceChainListQuery>,
) -> Result<Json<ServiceChainListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_service_chains()
        .await
        .into_iter()
        .filter(|sc| {
            labels_match(&sc.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &sc.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &sc.spec.network_id)
                && optional_eq(query.status.as_deref(), &sc.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(ServiceChainListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/service-chains",
    operation_id = "createServiceChain",
    tag = "service-chains",
    request_body = CreateServiceChainRequest,
    responses(
        (status = 201, description = "Create service chain", body = ServiceChainResource),
        (status = 400, description = "Invalid service chain references", body = PlatformApiError),
    )
)]
pub async fn create_service_chain(
    State(store): State<AppState>,
    Json(request): Json<CreateServiceChainRequest>,
) -> Result<(StatusCode, Json<ServiceChainResource>), ControllerError> {
    let hop_count = request.spec.hops.len();
    let resource = ServiceChainResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: service_chain_status(hop_count),
    };
    let created = store.create_service_chain(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/service-chains/{id}",
    operation_id = "getServiceChain",
    tag = "service-chains",
    params(("id" = String, Path, description = "Service chain ID")),
    responses(
        (status = 200, description = "Get service chain", body = ServiceChainResource),
        (status = 404, description = "Service chain not found", body = PlatformApiError)
    )
)]
pub async fn get_service_chain(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ServiceChainResource>, ControllerError> {
    let resource = store
        .get_service_chain(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "service_chain",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/service-chains/{id}",
    operation_id = "updateServiceChain",
    tag = "service-chains",
    params(("id" = String, Path, description = "Service chain ID")),
    request_body = UpdateServiceChainRequest,
    responses(
        (status = 200, description = "Update service chain", body = ServiceChainResource),
        (status = 400, description = "Invalid service chain references", body = PlatformApiError),
    )
)]
pub async fn update_service_chain(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateServiceChainRequest>,
) -> Result<Json<ServiceChainResource>, ControllerError> {
    let existing = store
        .get_service_chain(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "service_chain",
            id: id.clone(),
        })?;
    let hop_count = request.spec.hops.len();
    let resource = ServiceChainResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: ServiceChainStatus {
            phase: existing.status.phase,
            hop_count,
        },
    };
    let updated = store.update_service_chain(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/service-chains/{id}",
    operation_id = "deleteServiceChain",
    tag = "service-chains",
    params(("id" = String, Path, description = "Service chain ID")),
    responses(
        (status = 200, description = "Delete service chain", body = MessageResponse),
        (status = 404, description = "Service chain not found", body = PlatformApiError)
    )
)]
pub async fn delete_service_chain(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_service_chain(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "service_chain",
            id: id.clone(),
        })?;
    store.delete_service_chain(&id).await?;
    Ok(Json(deleted_message("service_chain", &id)))
}
