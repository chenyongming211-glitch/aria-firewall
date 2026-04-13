use aria_api::{
    CreateNetworkPolicyRequest, MessageResponse, NetworkPolicyListQuery,
    NetworkPolicyListResponse, NetworkPolicyResource, NetworkPolicyStatus, PlatformApiError,
    UpdateNetworkPolicyRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use super::{
    helpers::{
        deleted_message, labels_match, metadata_from_create, metadata_from_update,
        network_policy_status, optional_eq, paginate, parse_label_selector,
    },
    AppState, ControllerError,
};

fn matches_action_filter(filter: Option<&str>, action: u8) -> bool {
    match filter {
        Some("allow") => action == 0,
        Some("deny") => action == 1,
        Some(filter) => filter.parse::<u8>() == Ok(action),
        None => true,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/network-policies",
    operation_id = "listNetworkPolicies",
    tag = "network-policies",
    params(NetworkPolicyListQuery),
    responses(
        (status = 200, description = "List network policies", body = NetworkPolicyListResponse),
        (status = 400, description = "Invalid list query", body = PlatformApiError)
    )
)]
pub async fn list_network_policies(
    State(store): State<AppState>,
    Query(query): Query<NetworkPolicyListQuery>,
) -> Result<Json<NetworkPolicyListResponse>, ControllerError> {
    let selector = parse_label_selector(query.label_selector.as_deref())?;
    let items = store
        .list_network_policies()
        .await
        .into_iter()
        .filter(|policy| {
            labels_match(&policy.metadata.labels, &selector)
                && optional_eq(query.tenant_id.as_deref(), &policy.spec.tenant_id)
                && optional_eq(query.network_id.as_deref(), &policy.spec.network_id)
                && optional_eq(query.status.as_deref(), &policy.status.phase)
                && match query.action.as_deref() {
                    Some(action) => policy
                        .spec
                        .rules
                        .iter()
                        .any(|rule| matches_action_filter(Some(action), rule.action)),
                    None => true,
                }
        })
        .collect::<Vec<_>>();
    let (items, next_page_token, total_count) =
        paginate(items, query.limit, query.page_token.as_deref())?;
    Ok(Json(NetworkPolicyListResponse {
        items,
        next_page_token,
        total_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/network-policies",
    operation_id = "createNetworkPolicy",
    tag = "network-policies",
    request_body = CreateNetworkPolicyRequest,
    responses(
        (status = 201, description = "Create network policy", body = NetworkPolicyResource),
        (status = 400, description = "Invalid network policy references", body = PlatformApiError),
        (status = 409, description = "Network policy already exists", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn create_network_policy(
    State(store): State<AppState>,
    Json(request): Json<CreateNetworkPolicyRequest>,
) -> Result<(StatusCode, Json<NetworkPolicyResource>), ControllerError> {
    let rule_count = request.spec.rules.len();
    let resource = NetworkPolicyResource {
        metadata: metadata_from_create(request.metadata),
        spec: request.spec,
        status: network_policy_status(rule_count),
    };
    let created = store.create_network_policy(resource).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/network-policies/{id}",
    operation_id = "getNetworkPolicy",
    tag = "network-policies",
    params(("id" = String, Path, description = "Network policy ID")),
    responses(
        (status = 200, description = "Get network policy", body = NetworkPolicyResource),
        (status = 404, description = "Network policy not found", body = PlatformApiError)
    )
)]
pub async fn get_network_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NetworkPolicyResource>, ControllerError> {
    let resource = store
        .get_network_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network_policy",
            id,
        })?;
    Ok(Json(resource))
}

#[utoipa::path(
    put,
    path = "/api/v1/network-policies/{id}",
    operation_id = "updateNetworkPolicy",
    tag = "network-policies",
    params(("id" = String, Path, description = "Network policy ID")),
    request_body = UpdateNetworkPolicyRequest,
    responses(
        (status = 200, description = "Update network policy", body = NetworkPolicyResource),
        (status = 400, description = "Invalid network policy references", body = PlatformApiError),
        (status = 404, description = "Network policy not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn update_network_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateNetworkPolicyRequest>,
) -> Result<Json<NetworkPolicyResource>, ControllerError> {
    let existing = store
        .get_network_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network_policy",
            id: id.clone(),
        })?;
    let rule_count = request.spec.rules.len();
    let resource = NetworkPolicyResource {
        metadata: metadata_from_update(&existing.metadata, request.metadata),
        spec: request.spec,
        status: NetworkPolicyStatus {
            phase: existing.status.phase,
            rule_count,
        },
    };
    let updated = store.update_network_policy(&id, resource).await?;
    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/network-policies/{id}",
    operation_id = "deleteNetworkPolicy",
    tag = "network-policies",
    params(("id" = String, Path, description = "Network policy ID")),
    responses(
        (status = 200, description = "Delete network policy", body = MessageResponse),
        (status = 404, description = "Network policy not found", body = PlatformApiError),
        (status = 500, description = "Internal controller error", body = PlatformApiError)
    )
)]
pub async fn delete_network_policy(
    State(store): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessageResponse>, ControllerError> {
    store
        .get_network_policy(&id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "network_policy",
            id: id.clone(),
        })?;
    store.delete_network_policy(&id).await?;
    Ok(Json(deleted_message("network_policy", &id)))
}
