use std::collections::{BTreeMap, BTreeSet};

use aria_api::{
    BackendSetStatus, HealthCheckStatus, IpGroupStatus, MessageResponse, NetworkPolicyStatus,
    NetworkStatus, NodeCapability, NodeResource, NodeStatus, PortStatus, QosPolicyStatus,
    ResourceCreateMetadata, ResourceMetadata, ResourceUpdateMetadata, RouteTableStatus,
    SecurityGroupStatus, ServiceStatus, SouthboundNodeStatusResponse, TenantStatus,
};

use super::{AppState, ControllerError};

pub(super) fn error_details(resource: &'static str, id: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("resource".to_string(), resource.to_string()),
        ("id".to_string(), id.to_string()),
    ])
}

pub(super) fn invalid_reference_details(
    resource: &'static str,
    field: &str,
    value: &str,
    referenced_resource: &'static str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("resource".to_string(), resource.to_string()),
        ("field".to_string(), field.to_string()),
        ("value".to_string(), value.to_string()),
        (
            "referenced_resource".to_string(),
            referenced_resource.to_string(),
        ),
    ])
}

pub(super) fn dependency_conflict_details(
    resource: &'static str,
    id: &str,
    dependent_resource: &'static str,
    dependent_id: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("resource".to_string(), resource.to_string()),
        ("id".to_string(), id.to_string()),
        (
            "dependent_resource".to_string(),
            dependent_resource.to_string(),
        ),
        ("dependent_id".to_string(), dependent_id.to_string()),
    ])
}

pub(super) fn metadata_from_create(input: Option<ResourceCreateMetadata>) -> ResourceMetadata {
    let metadata = input.unwrap_or(ResourceCreateMetadata {
        id: None,
        labels: BTreeMap::new(),
    });
    ResourceMetadata {
        id: metadata.id.unwrap_or_default(),
        resource_version: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
        labels: metadata.labels,
    }
}

pub(super) fn metadata_from_update(
    existing: &ResourceMetadata,
    input: Option<ResourceUpdateMetadata>,
) -> ResourceMetadata {
    match input {
        Some(metadata) => ResourceMetadata {
            id: existing.id.clone(),
            resource_version: metadata
                .resource_version
                .unwrap_or_else(|| existing.resource_version.clone()),
            created_at: existing.created_at.clone(),
            updated_at: existing.updated_at.clone(),
            labels: metadata.labels.unwrap_or_else(|| existing.labels.clone()),
        },
        None => existing.clone(),
    }
}

pub(super) fn deleted_message(resource: &str, id: &str) -> MessageResponse {
    MessageResponse {
        message: format!("Deleted {resource} {id}"),
    }
}

const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 200;

pub(super) fn parse_limit(limit: Option<usize>) -> Result<usize, ControllerError> {
    match limit {
        Some(0) => Err(ControllerError::BadRequest(
            "limit must be between 1 and 200".to_string(),
        )),
        Some(value) if value > MAX_PAGE_LIMIT => Err(ControllerError::BadRequest(format!(
            "limit must be between 1 and {MAX_PAGE_LIMIT}"
        ))),
        Some(value) => Ok(value),
        None => Ok(DEFAULT_PAGE_LIMIT),
    }
}

pub(super) fn parse_page_token(page_token: Option<&str>) -> Result<usize, ControllerError> {
    match page_token {
        Some(token) if token.trim().is_empty() => Err(ControllerError::BadRequest(
            "page_token must be a non-negative integer offset".to_string(),
        )),
        Some(token) => token.parse::<usize>().map_err(|_| {
            ControllerError::BadRequest(
                "page_token must be a non-negative integer offset".to_string(),
            )
        }),
        None => Ok(0),
    }
}

pub(super) fn parse_label_selector(
    selector: Option<&str>,
) -> Result<BTreeMap<String, String>, ControllerError> {
    let Some(selector) = selector.map(str::trim) else {
        return Ok(BTreeMap::new());
    };
    if selector.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut labels = BTreeMap::new();
    for clause in selector.split(',') {
        let clause = clause.trim();
        let Some((key, value)) = clause.split_once('=') else {
            return Err(ControllerError::BadRequest(format!(
                "label_selector clause '{clause}' must use key=value syntax"
            )));
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(ControllerError::BadRequest(
                "label_selector requires non-empty key and value".to_string(),
            ));
        }
        labels.insert(key.to_string(), value.to_string());
    }

    Ok(labels)
}

pub(super) fn labels_match(
    labels: &BTreeMap<String, String>,
    selector: &BTreeMap<String, String>,
) -> bool {
    selector
        .iter()
        .all(|(key, value)| labels.get(key) == Some(value))
}

pub(super) fn optional_eq(filter: Option<&str>, value: &str) -> bool {
    match filter {
        Some(filter) => filter == value,
        None => true,
    }
}

pub(super) fn optional_option_eq(filter: Option<&str>, value: Option<&str>) -> bool {
    match filter {
        Some(filter) => value == Some(filter),
        None => true,
    }
}

pub(super) fn paginate<T>(
    items: Vec<T>,
    limit: Option<usize>,
    page_token: Option<&str>,
) -> Result<(Vec<T>, Option<String>, usize), ControllerError> {
    let limit = parse_limit(limit)?;
    let offset = parse_page_token(page_token)?;
    let total_count = items.len();
    if offset >= total_count {
        return Ok((Vec::new(), None, total_count));
    }

    let page_items = items
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next_offset = offset + page_items.len();
    let next_page_token = (next_offset < total_count).then(|| next_offset.to_string());
    Ok((page_items, next_page_token, total_count))
}

pub(super) fn tenant_status() -> TenantStatus {
    TenantStatus {
        phase: "ready".to_string(),
    }
}

pub(super) fn node_status() -> NodeStatus {
    NodeStatus {
        phase: "registered".to_string(),
        agent_version: None,
        kernel_version: None,
        capabilities: Vec::new(),
        desired_generation: None,
        last_applied_generation: None,
        last_seen_at: None,
        last_reconcile_at: None,
        last_error: None,
        last_publish_summary: None,
        pending_object_counts: BTreeMap::new(),
        changed_kinds: Vec::new(),
        has_deletes: false,
        sync_status: None,
    }
}

pub(super) fn network_status() -> NetworkStatus {
    NetworkStatus {
        phase: "ready".to_string(),
    }
}

pub(super) fn port_status() -> PortStatus {
    PortStatus {
        phase: "pending".to_string(),
        attachment_state: Some("detached".to_string()),
    }
}

pub(super) fn security_group_status(rule_count: usize) -> SecurityGroupStatus {
    SecurityGroupStatus {
        phase: "ready".to_string(),
        rule_count,
    }
}

pub(super) fn route_table_status() -> RouteTableStatus {
    RouteTableStatus {
        phase: "ready".to_string(),
    }
}

pub(super) fn ip_group_status(cidr_count: usize) -> IpGroupStatus {
    IpGroupStatus {
        phase: "ready".to_string(),
        cidr_count,
    }
}

pub(super) fn network_policy_status(rule_count: usize) -> NetworkPolicyStatus {
    NetworkPolicyStatus {
        phase: "ready".into(),
        rule_count,
    }
}

pub(super) fn qos_policy_status(rule_count: usize) -> QosPolicyStatus {
    QosPolicyStatus {
        phase: "ready".into(),
        rule_count,
    }
}

pub(super) fn health_check_status() -> HealthCheckStatus {
    HealthCheckStatus {
        phase: "ready".to_string(),
    }
}

pub(super) fn backend_set_status(backend_count: usize) -> BackendSetStatus {
    BackendSetStatus {
        phase: "ready".to_string(),
        backend_count,
        healthy_backends: 0,
    }
}

pub(super) fn service_status() -> ServiceStatus {
    ServiceStatus {
        phase: "ready".to_string(),
        exposure_state: Some("reserved".to_string()),
    }
}

fn node_capabilities_from_report(capability: &NodeCapability) -> Vec<String> {
    let mut capabilities = BTreeSet::new();
    capabilities.extend(capability.supported_hooks.iter().cloned());

    if capability.supports_socket_lb {
        capabilities.insert("socket_lb".to_string());
    }
    if capability.supports_trace_ringbuf {
        capabilities.insert("trace_ringbuf".to_string());
    }
    if capability.supports_nat {
        capabilities.insert("nat".to_string());
    }
    if capability.supports_lb {
        capabilities.insert("lb".to_string());
    }
    if capability.supports_encap {
        capabilities.insert("encap".to_string());
    }
    if capability.supports_qos_shaping {
        capabilities.insert("qos_shaping".to_string());
    }

    capabilities.into_iter().collect()
}

fn apply_southbound_node_status(status: &mut NodeStatus, southbound: SouthboundNodeStatusResponse) {
    status.desired_generation = Some(southbound.desired_generation);
    status.last_applied_generation = southbound.last_applied_generation;
    status.last_seen_at = southbound.last_seen_at;
    status.last_publish_summary = southbound.last_desired_state;
    status.pending_object_counts = southbound.pending_object_counts;
    status.changed_kinds = southbound.changed_kinds;
    status.has_deletes = southbound.has_deletes;
    status.sync_status = Some(southbound.sync_status);

    if let Some(registration) = southbound.registration {
        status.agent_version = Some(registration.info.agent_version);
        status.kernel_version = Some(registration.info.kernel_version);
        status.capabilities = node_capabilities_from_report(&registration.capability);
    }

    if let Some(health) = southbound.last_health {
        status.last_reconcile_at = health.last_reconcile_at;
        status.last_error = health.last_error;
    }
}

pub(super) async fn enrich_node_resource(
    store: &AppState,
    mut resource: NodeResource,
) -> Result<NodeResource, ControllerError> {
    let southbound = store.southbound_status(&resource.metadata.id).await?;
    apply_southbound_node_status(&mut resource.status, southbound);
    Ok(resource)
}

pub(super) async fn enrich_node_resources(
    store: &AppState,
    resources: Vec<NodeResource>,
) -> Result<Vec<NodeResource>, ControllerError> {
    let mut enriched = Vec::with_capacity(resources.len());
    for resource in resources {
        enriched.push(enrich_node_resource(store, resource).await?);
    }
    Ok(enriched)
}
