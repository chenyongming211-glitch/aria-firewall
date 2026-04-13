use aria_api::{
    CreateTenantRequest, MessageResponse, PlatformApiError, TenantListQuery, TenantListResponse,
    TenantResource, UpdateTenantRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update, optional_eq,
        paginate, parse_label_selector, tenant_status,
    },
    AppState, ControllerError,
};

#[utoipa::path(
    get,
    path = "/api/v1/tenants",
    operation_id = "listTenants",
    tag = "tenants",
    params(TenantListQuery),
    responses(
        (status = 200, description = "List tenants", body = TenantListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_tenants(
    State(store): State<AppState>,
    Query(query): Query<TenantListQuery>,
) -> Result<Json<TenantListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_tenants()
        .await
        .into_iter()
        .filter(|tenant| {
            labels_match(&tenant.metadata.labels, &selector)
                && optional_eq(query.status.as_deref(), &tenant.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(TenantListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/tenants",
    operation_id = "createTenant",
    tag = "tenants",
    request_body = CreateTenantRequest,
    responses(
        (status = 201, description = "Create tenant", body = TenantResource),
        (status = 409, description = "Tenant already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn create_tenant(
    State(store): State<AppState>,
    Json(request): Json<CreateTenantRequest>,
) -> Result<(StatusCode, Json<TenantResource>), ControllerError> {
    let resource = TenantResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: tenant_status(),
    };
    let created = store.create_tenant(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/tenants/{id}",
    operation_id = "getTenant",
    tag = "tenants",
    params(("id" = String, Path, description = "Tenant ID")),
    responses(
        (status = 200, description = "Get tenant", body = TenantResource),
        (status = 404, description = "Tenant not found", body = PlatformApiError)
    )
)]
pub async fn get_tenant(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TenantResource>, ControllerError> {
    let resource = store
        .get_tenant(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "tenant",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/tenants/{id}",
    operation_id = "updateTenant",
    tag = "tenants",
    params(("id" = String, Path, description = "Tenant ID")),
    request_body = UpdateTenantRequest,
    responses(
        (status = 200, description = "Update tenant", body = TenantResource),
        (status = 404, description = "Tenant not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn update_tenant(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateTenantRequest>,
) -> Result<Json<TenantResource>, ControllerError> {
    let existing = store
        .get_tenant(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "tenant",
            id: id.clone(),
        })?;
    let resource = TenantResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: existing.status,
    };
    let updated = store.update_tenant(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/tenants/{id}",
    operation_id = "deleteTenant",
    tag = "tenants",
    params(("id" = String, Path, description = "Tenant ID")),
    responses(
        (status = 200, description = "Delete tenant", body = MessageResponse),
        (status = 404, description = "Tenant not found", body = PlatformApiError),
        (status = 409, description = "Tenant still has dependent resources", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_tenant(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_tenant(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "tenant",
            id: id.clone(),
        })?;
    store.delete_tenant(&id).await?;
    Ok(Json(deleted_message("tenant", &id)))
}
