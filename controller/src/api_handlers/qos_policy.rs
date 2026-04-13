use aria_api::{
    CreateQosPolicyRequest, MessageResponse, PlatformApiError, QosPolicyListQuery,
    QosPolicyListResponse, QosPolicyResource, QosPolicyStatus, UpdateQosPolicyRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update, optional_eq,
        paginate, parse_label_selector, qos_policy_status,
    },
    AppState, ControllerError,
};

#[utoipa::path(
    get,
    path = "/api/v1/qos-policies",
    operation_id = "listQosPolicies",
    tag = "qos-policies",
    params(QosPolicyListQuery),
    responses(
        (status = 200, description = "List QoS policies", body = QosPolicyListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_qos_policies(
    State(store): State<AppState>,
    Query(query): Query<QosPolicyListQuery>,
) -> Result<Json<QosPolicyListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_qos_policies()
        .await
        .into_iter()
        .filter(|qp| {
            labels_match(&qp.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &qp.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &qp.spec.network_id)
                && optional_eq(query.status.as_deref(), &qp.status.phase)
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(QosPolicyListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/qos-policies",
    operation_id = "createQosPolicy",
    tag = "qos-policies",
    request_body = CreateQosPolicyRequest,
    responses(
        (status = 201, description = "Create QoS policy", body = QosPolicyResource),
        (status = 400, description = "Invalid QoS policy references", body = PlatformApiError),
    )
)]
pub async fn create_qos_policy(
    State(store): State<AppState>,
    Json(request): Json<CreateQosPolicyRequest>,
) -> Result<(StatusCode, Json<QosPolicyResource>), ControllerError> {
    let rule_count = request.spec.rules.len();
    let resource = QosPolicyResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: qos_policy_status(rule_count),
    };
    let created = store.create_qos_policy(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/qos-policies/{id}",
    operation_id = "getQosPolicy",
    tag = "qos-policies",
    params(("id" = String, Path, description = "QoS policy ID")),
    responses(
        (status = 200, description = "Get QoS policy", body = QosPolicyResource),
        (status = 404, description = "QoS policy not found", body = PlatformApiError)
    )
)]
pub async fn get_qos_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<QosPolicyResource>, ControllerError> {
    let resource = store
        .get_qos_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "qos_policy",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/qos-policies/{id}",
    operation_id = "updateQosPolicy",
    tag = "qos-policies",
    params(("id" = String, Path, description = "QoS policy ID")),
    request_body = UpdateQosPolicyRequest,
    responses(
        (status = 200, description = "Update QoS policy", body = QosPolicyResource),
        (status = 400, description = "Invalid QoS policy references", body = PlatformApiError),
    )
)]
pub async fn update_qos_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateQosPolicyRequest>,
) -> Result<Json<QosPolicyResource>, ControllerError> {
    let existing = store
        .get_qos_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "qos_policy",
            id: id.clone(),
        })?;
    let rule_count = request.spec.rules.len();
    let resource = QosPolicyResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: QosPolicyStatus {
            phase: existing.status.phase,
            rule_count,
        },
    };
    let updated = store.update_qos_policy(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/qos-policies/{id}",
    operation_id = "deleteQosPolicy",
    tag = "qos-policies",
    params(("id" = String, Path, description = "QoS policy ID")),
    responses(
        (status = 200, description = "Delete QoS policy", body = MessageResponse),
        (status = 404, description = "QoS policy not found", body = PlatformApiError)
    )
)]
pub async fn delete_qos_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_qos_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "qos_policy",
            id: id.clone(),
        })?;
    store.delete_qos_policy(&id).await?;
    Ok(Json(deleted_message("qos_policy", &id)))
}
