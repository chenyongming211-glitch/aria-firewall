use aria_api::{
    CreateMirrorPolicyRequest, MessageResponse, MirrorPolicyListQuery, MirrorPolicyListResponse,
    MirrorPolicyResource, MirrorPolicyStatus, PlatformApiError, UpdateMirrorPolicyRequest,
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

fn mirror_policy_status(rule_count: usize) -> MirrorPolicyStatus {
    MirrorPolicyStatus {
        phase: "ready".to_string(),
        rule_count,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/mirror-policies",
    operation_id = "listMirrorPolicies",
    tag = "mirror-policies",
    params(MirrorPolicyListQuery),
    responses(
        (status = 200, description = "List mirror policies", body = MirrorPolicyListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_mirror_policies(
    State(store): State<AppState>,
    Query(query): Query<MirrorPolicyListQuery>,
) -> Result<Json<MirrorPolicyListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_mirror_policies()
        .await
        .into_iter()
        .filter(|mp| {
            labels_match(&mp.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &mp.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &mp.spec.network_id)
                && optional_eq(query.status.as_deref(), &mp.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(MirrorPolicyListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/mirror-policies",
    operation_id = "createMirrorPolicy",
    tag = "mirror-policies",
    request_body = CreateMirrorPolicyRequest,
    responses(
        (status = 201, description = "Create mirror policy", body = MirrorPolicyResource),
        (status = 400, description = "Invalid mirror policy references", body = PlatformApiError),
    )
)]
pub async fn create_mirror_policy(
    State(store): State<AppState>,
    Json(request): Json<CreateMirrorPolicyRequest>,
) -> Result<(StatusCode, Json<MirrorPolicyResource>), ControllerError> {
    let rule_count = request.spec.rules.len();
    let resource = MirrorPolicyResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: mirror_policy_status(rule_count),
    };
    let created = store.create_mirror_policy(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/mirror-policies/{id}",
    operation_id = "getMirrorPolicy",
    tag = "mirror-policies",
    params(("id" = String, Path, description = "Mirror policy ID")),
    responses(
        (status = 200, description = "Get mirror policy", body = MirrorPolicyResource),
        (status = 404, description = "Mirror policy not found", body = PlatformApiError)
    )
)]
pub async fn get_mirror_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MirrorPolicyResource>, ControllerError> {
    let resource = store
        .get_mirror_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "mirror_policy",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/mirror-policies/{id}",
    operation_id = "updateMirrorPolicy",
    tag = "mirror-policies",
    params(("id" = String, Path, description = "Mirror policy ID")),
    request_body = UpdateMirrorPolicyRequest,
    responses(
        (status = 200, description = "Update mirror policy", body = MirrorPolicyResource),
        (status = 400, description = "Invalid mirror policy references", body = PlatformApiError),
    )
)]
pub async fn update_mirror_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateMirrorPolicyRequest>,
) -> Result<Json<MirrorPolicyResource>, ControllerError> {
    let existing = store
        .get_mirror_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "mirror_policy",
            id: id.clone(),
        })?;
    let rule_count = request.spec.rules.len();
    let resource = MirrorPolicyResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: MirrorPolicyStatus {
            phase: existing.status.phase,
            rule_count,
        },
    };
    let updated = store.update_mirror_policy(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/mirror-policies/{id}",
    operation_id = "deleteMirrorPolicy",
    tag = "mirror-policies",
    params(("id" = String, Path, description = "Mirror policy ID")),
    responses(
        (status = 200, description = "Delete mirror policy", body = MessageResponse),
        (status = 404, description = "Mirror policy not found", body = PlatformApiError)
    )
)]
pub async fn delete_mirror_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_mirror_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "mirror_policy",
            id: id.clone(),
        })?;
    store.delete_mirror_policy(&id).await?;
    Ok(Json(deleted_message("mirror_policy", &id)))
}
