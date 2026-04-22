use aria_api::{
    ApiError, DiagnoseRequest, DiagnoseResponse, PlatformApiError, PlatformDiagnoseRequest,
};
use axum::{extract::State, Json};
use reqwest::Url;

use super::{AppState, ControllerError};

#[utoipa::path(
    post,
    path = "/api/v1/diagnose",
    operation_id = "diagnosePlatform",
    tag = "diagnose",
    request_body = PlatformDiagnoseRequest,
    responses(
        (status = 200, description = "Run structured diagnose through the controller proxy", body = DiagnoseResponse),
        (status = 400, description = "Invalid diagnose request", body = PlatformApiError),
        (status = 404, description = "Node or instance not found", body = PlatformApiError),
        (status = 503, description = "Agent diagnose upstream unavailable", body = PlatformApiError)
    )
)]
pub async fn diagnose(
    State(store): State<AppState>,
    Json(req): Json<PlatformDiagnoseRequest>,
) -> Result<Json<DiagnoseResponse>, ControllerError> {
    let node_id = req.node_id.trim().to_string();
    if node_id.is_empty() {
        return Err(ControllerError::BadRequest(
            "node_id must be non-empty".to_string(),
        ));
    }

    let instance = req.instance.trim().to_string();
    if instance.is_empty() {
        return Err(ControllerError::BadRequest(
            "instance must be non-empty".to_string(),
        ));
    }

    let dst_ip = req.dst_ip.trim().to_string();
    if dst_ip.is_empty() {
        return Err(ControllerError::BadRequest(
            "dst_ip must be non-empty".to_string(),
        ));
    }

    let node = store
        .get_node(&node_id)
        .await
        .ok_or(ControllerError::NotFound {
            resource: "node",
            id: node_id.clone(),
        })?;
    let southbound = store.southbound_status(&node_id).await?;
    let address = management_address(&node, &southbound).ok_or(
        ControllerError::UpstreamUnavailable(
            format!("node '{node_id}' does not have a management address"),
        ),
    )?;
    let url = build_agent_diagnose_url(&address, &instance).map_err(|error| {
        ControllerError::UpstreamUnavailable(format!(
            "node '{node_id}' management address '{address}' is not usable: {error}"
        ))
    })?;

    let upstream_request = DiagnoseRequest {
        dst_ip,
        dst_port: req.dst_port,
        chain: req
            .chain
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        time_window_seconds: req.time_window_seconds,
    };

    let response = reqwest::Client::new()
        .post(url)
        .json(&upstream_request)
        .send()
        .await
        .map_err(|error| {
            ControllerError::UpstreamUnavailable(format!(
                "failed to reach node '{node_id}' diagnose API: {error}"
            ))
        })?;

    let status = response.status();
    if status.is_success() {
        let body = response.json::<DiagnoseResponse>().await.map_err(|error| {
            ControllerError::UpstreamUnavailable(format!(
                "failed to decode diagnose response from node '{node_id}': {error}"
            ))
        })?;
        return Ok(Json(body));
    }

    let upstream_error = response.json::<ApiError>().await.ok();
    match status.as_u16() {
        400 => Err(ControllerError::BadRequest(
            upstream_error
                .map(|error| error.error)
                .unwrap_or_else(|| "agent rejected diagnose request".to_string()),
        )),
        404 => Err(ControllerError::NotFound {
            resource: "instance",
            id: format!("{node_id}/{instance}"),
        }),
        503 => Err(ControllerError::UpstreamUnavailable(
            upstream_error.map(|error| error.error).unwrap_or_else(|| {
                format!("instance '{instance}' on node '{node_id}' is not ready")
            }),
        )),
        _ => {
            let suffix = upstream_error
                .map(|error| format!(": {}", error.error))
                .unwrap_or_default();
            Err(ControllerError::UpstreamUnavailable(format!(
                "node '{node_id}' diagnose API returned status {}{}",
                status.as_u16(),
                suffix
            )))
        }
    }
}

fn management_address(
    node: &aria_api::NodeResource,
    southbound: &aria_api::SouthboundNodeStatusResponse,
) -> Option<String> {
    southbound
        .registration
        .as_ref()
        .and_then(|registration| {
            registration
                .info
                .addresses
                .iter()
                .find(|address| {
                    address.kind.eq_ignore_ascii_case("management")
                        && !address.value.trim().is_empty()
                })
                .map(|address| address.value.trim().to_string())
        })
        .or_else(|| {
            let address = node.spec.mgmt_address.trim();
            (!address.is_empty()).then(|| address.to_string())
        })
}

fn build_agent_diagnose_url(address: &str, instance: &str) -> Result<Url, String> {
    let normalized = if address.starts_with("http://") || address.starts_with("https://") {
        address.to_string()
    } else {
        format!("http://{address}")
    };

    let mut url = Url::parse(&normalized).map_err(|error| error.to_string())?;
    if url.host_str().is_none() {
        return Err("missing host".to_string());
    }
    if url.port().is_none() {
        url.set_port(Some(8080))
            .map_err(|_| "cannot set default agent port 8080".to_string())?;
    }
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| "cannot set diagnose path".to_string())?;
    segments.clear();
    segments.extend(["api", "v1", instance, "diagnose"]);
    drop(segments);

    Ok(url)
}
