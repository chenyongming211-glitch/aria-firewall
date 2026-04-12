use aria_api::{ApplyStatusReport, DesiredStateEnvelope};
use serde::{Deserialize, Serialize};

pub(crate) struct DesiredStateCacheEntry {
    cached_at: String,
    envelope: DesiredStateEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledPortBinding {
    port_id: String,
    tenant_id: String,
    network_id: String,
    segment_id: Option<String>,
    security_group_ids: Vec<String>,
    fixed_ips: Vec<String>,
    allowed_address_pairs: Vec<String>,
    mac_address: String,
    anti_spoof_enabled: bool,
    admin_state_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AntiSpoofIr {
    address: [u8; 16],
    flags: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PortIdentityIr {
    port_id: String,
    network_id: String,
    tap_id: u32,
    ifindex: u32,
    sg_program_id: u32,
    tenant_local_id: u32,
    network_local_id: u32,
    segment_local_id: u32,
    mac: [u8; 6],
    primary_ipv4: u32,
    primary_ipv6: [u8; 16],
    anti_spoof_enabled: bool,
    anti_spoof_entries: Vec<AntiSpoofIr>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledRouteTableView {
    route_table_id: String,
    network_id: String,
    route_count: usize,
    default_route: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RouteIr {
    route_table_id: String,
    port_id: String,
    tap_id: u32,
    destination: [u8; 16],
    prefix_len: u8,
    is_ipv6: bool,
    next_hop_type: u8,
    next_hop_ref: String,
    next_hop_ip: [u8; 16],
    egress_ifindex: u32,
    route_id: u16,
    priority: u8,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IpGroupIr {
    ip_group_id: String,
    numeric_id: u32,
    network_id: String,
    cidrs: Vec<IpGroupCidrIr>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IpGroupCidrIr {
    cidr: String,
    is_ipv6: bool,
    address: [u8; 16],
    prefix_len: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NetworkPolicyIr {
    policy_id: String,
    network_id: String,
    rules: Vec<NetworkPolicyRuleIr>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NetworkPolicyRuleIr {
    src_numeric_id: u32,
    dst_numeric_id: u32,
    proto: u8,
    direction: u8,
    action: u8,
    ports: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QosPolicyIr {
    policy_id: String,
    network_id: String,
    rules: Vec<QosPolicyRuleIr>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QosPolicyRuleIr {
    ip_group_numeric_id: u32,
    direction: u8,
    rate_bps: u64,
    burst_bytes: u64,
    priority: u8,
    mode: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SgRuleIr {
    port_id: String,
    tap_id: u32,
    sg_program_id: u32,
    source_security_group_id: String,
    direction: u8,
    proto: u8,
    remote_prefix: [u8; 16],
    prefix_len: u8,
    action: u8,
    priority: u8,
    port_start: u16,
    port_end: u16,
    rule_id: u16,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledHealthCheckView {
    health_check_id: String,
    tenant_id: String,
    network_id: Option<String>,
    protocol: String,
    target_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledBackendSetView {
    backend_set_id: String,
    tenant_id: String,
    network_id: String,
    health_check_id: Option<String>,
    policy: String,
    backend_count: usize,
    local_backend_count: usize,
    remote_backend_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledServiceView {
    service_id: String,
    tenant_id: String,
    network_id: String,
    backend_set_id: Option<String>,
    vip: String,
    protocol: String,
    port_count: usize,
    exposure_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HealthCheckIr {
    health_check_id: String,
    tenant_id: String,
    network_id: Option<String>,
    probe_protocol: String,
    interval_seconds: u32,
    timeout_seconds: u32,
    healthy_threshold: u32,
    unhealthy_threshold: u32,
    target_port: Option<u16>,
    has_request_template: bool,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BackendMemberIr {
    backend_id: String,
    target_type: String,
    target_ref: Option<String>,
    resolved_ip_hint: Option<String>,
    service_port: u16,
    weight: u16,
    admin_state: String,
    node_id: Option<String>,
    declared_locality: Option<String>,
    resolved_locality: String,
    forwarding_scope: String,
    resolution: String,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BackendSetIr {
    backend_set_id: String,
    tenant_id: String,
    network_id: String,
    selection_policy: String,
    health_check_id: Option<String>,
    local_backend_count: usize,
    remote_backend_count: usize,
    backends: Vec<BackendMemberIr>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceFrontendPortIr {
    name: Option<String>,
    service_port: u16,
    target_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceFrontendIr {
    service_id: String,
    tenant_id: String,
    network_id: String,
    #[serde(default = "default_service_route_mode")]
    route_mode: String,
    vip: String,
    protocol: String,
    lb_policy: String,
    session_affinity: Option<String>,
    exposure_type: String,
    #[serde(default = "default_service_forwarding_mode")]
    forwarding_mode: String,
    listener_ports: Vec<ServiceFrontendPortIr>,
    node_local_forwarding: bool,
    cross_node_forwarding: bool,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceProgramIr {
    service_id: String,
    backend_set_id: Option<String>,
    health_check_id: Option<String>,
    frontend: ServiceFrontendIr,
    backend_set: Option<BackendSetIr>,
    health_check: Option<HealthCheckIr>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompileDomainSummary {
    domain: String,
    input_objects: usize,
    compiled_objects: usize,
    failed_objects: usize,
    status: String,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledNodeState {
    generation: String,
    compiler_version: String,
    node_id: String,
    capability_profile: String,
    full_sync: bool,
    issued_at: String,
    tenant_ids: Vec<String>,
    network_ids: Vec<String>,
    security_group_ids: Vec<String>,
    port_bindings: Vec<CompiledPortBinding>,
    #[serde(default)]
    port_identities: Vec<PortIdentityIr>,
    route_tables: Vec<CompiledRouteTableView>,
    #[serde(default)]
    route_entries: Vec<RouteIr>,
    #[serde(default)]
    sg_rules: Vec<SgRuleIr>,
    #[serde(default)]
    ip_groups: Vec<IpGroupIr>,
    #[serde(default)]
    network_policies: Vec<NetworkPolicyIr>,
    #[serde(default)]
    qos_policies: Vec<QosPolicyIr>,
    health_checks: Vec<CompiledHealthCheckView>,
    backend_sets: Vec<CompiledBackendSetView>,
    services: Vec<CompiledServiceView>,
    #[serde(default)]
    service_programs: Vec<ServiceProgramIr>,
    domain_summaries: Vec<CompileDomainSummary>,
    warnings: Vec<String>,
    degraded_reasons: Vec<String>,
    compiled_at: String,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReconcileAction {
    domain: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReconcilePlan {
    generation: String,
    previous_generation: Option<String>,
    compiled_at: String,
    full_reconcile: bool,
    changed_kinds: Vec<String>,
    actions: Vec<ReconcileAction>,
    warnings: Vec<String>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttachBindingPlan {
    hook_family: String,
    scope: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttachPlan {
    generation: String,
    compiled_at: String,
    required_hooks: Vec<String>,
    bindings: Vec<AttachBindingPlan>,
    required_qdisc: Vec<String>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MapPlanEntry {
    map_family: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MapPlan {
    generation: String,
    compiled_at: String,
    entries: Vec<MapPlanEntry>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimePlan {
    generation: String,
    compiled_at: String,
    attach_plan: AttachPlan,
    map_plan: MapPlan,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryAttach {
    domain: String,
    hook_family: String,
    scope: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryMapEntry {
    domain: String,
    map_family: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryDomainSummary {
    domain: String,
    compiled_objects: usize,
    failed_objects: usize,
    attach_operations: usize,
    map_operations: usize,
    status: String,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventory {
    generation: String,
    previous_generation: Option<String>,
    compiled_at: String,
    observed_at: String,
    compiler_version: String,
    required_hooks: Vec<String>,
    required_qdisc: Vec<String>,
    attach_inventory: Vec<RuntimeInventoryAttach>,
    map_inventory: Vec<RuntimeInventoryMapEntry>,
    domain_inventory: Vec<RuntimeInventoryDomainSummary>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryAttachDelta {
    domain: String,
    hook_family: String,
    scope: String,
    operation: String,
    previous_object_count: usize,
    current_object_count: usize,
    change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryMapDelta {
    domain: String,
    map_family: String,
    operation: String,
    previous_object_count: usize,
    current_object_count: usize,
    change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryDomainDelta {
    domain: String,
    previous_attach_operations: usize,
    current_attach_operations: usize,
    previous_map_operations: usize,
    current_map_operations: usize,
    previous_compiled_objects: usize,
    current_compiled_objects: usize,
    change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryDiff {
    generation: String,
    previous_generation: Option<String>,
    observed_at: String,
    changed_domains: Vec<String>,
    attach_deltas: Vec<RuntimeInventoryAttachDelta>,
    map_deltas: Vec<RuntimeInventoryMapDelta>,
    domain_deltas: Vec<RuntimeInventoryDomainDelta>,
    has_cleanup: bool,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeDomainIntent {
    domain: String,
    desired_action: String,
    reason: String,
    full_reconcile: bool,
    requires_cleanup: bool,
    changed: bool,
    attach_operations: usize,
    map_operations: usize,
    compiled_objects: usize,
    failed_objects: usize,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceRuntimeIntentSummary {
    service_count: usize,
    frontend_listener_count: usize,
    #[serde(default)]
    frontend_runtime_entry_count: usize,
    #[serde(default)]
    socket_lb_frontend_count: usize,
    #[serde(default)]
    packet_lb_frontend_count: usize,
    backend_member_count: usize,
    #[serde(default)]
    backend_runtime_entry_count: usize,
    forwarding_projection_count: usize,
    node_local_service_count: usize,
    cross_node_service_count: usize,
    #[serde(default)]
    cross_node_native_service_count: usize,
    #[serde(default)]
    cross_node_overlay_service_count: usize,
    #[serde(default)]
    cross_node_hybrid_service_count: usize,
    #[serde(default)]
    revnat_reservation_count: usize,
    #[serde(default)]
    affinity_reservation_count: usize,
    #[serde(default)]
    maglev_reservation_count: usize,
    desired_action: String,
    requires_cleanup: bool,
    changed: bool,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeIntent {
    generation: String,
    previous_generation: Option<String>,
    compiled_at: String,
    observed_at: String,
    changed_domains: Vec<String>,
    intents: Vec<RuntimeDomainIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service_intent: Option<ServiceRuntimeIntentSummary>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeExecutionDomainSummary {
    domain: String,
    planned_action: String,
    execution_status: String,
    requires_cleanup: bool,
    changed: bool,
    input_objects: usize,
    compiled_objects: usize,
    failed_objects: usize,
    warnings: Vec<String>,
    degraded_reasons: Vec<String>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceRuntimeExecutionSummary {
    service_count: usize,
    frontend_listener_count: usize,
    #[serde(default)]
    frontend_runtime_entry_count: usize,
    #[serde(default)]
    socket_lb_frontend_count: usize,
    #[serde(default)]
    packet_lb_frontend_count: usize,
    backend_member_count: usize,
    #[serde(default)]
    backend_runtime_entry_count: usize,
    forwarding_projection_count: usize,
    node_local_service_count: usize,
    cross_node_service_count: usize,
    #[serde(default)]
    cross_node_native_service_count: usize,
    #[serde(default)]
    cross_node_overlay_service_count: usize,
    #[serde(default)]
    cross_node_hybrid_service_count: usize,
    #[serde(default)]
    revnat_reservation_count: usize,
    #[serde(default)]
    affinity_reservation_count: usize,
    #[serde(default)]
    maglev_reservation_count: usize,
    execution_status: String,
    planned_action: String,
    warnings: Vec<String>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeExecutionSummary {
    generation: String,
    previous_generation: Option<String>,
    compiled_at: String,
    observed_at: String,
    changed_domains: Vec<String>,
    domain_summaries: Vec<RuntimeExecutionDomainSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service_execution: Option<ServiceRuntimeExecutionSummary>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketBackendCandidate {
    backend_id: String,
    target_type: String,
    resolved_ip_hint: Option<String>,
    port: u16,
    weight: u16,
    locality: String,
    forwarding_scope: String,
    health_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketBackendChoiceShape {
    total_candidates: usize,
    local_candidates: usize,
    remote_candidates: usize,
    total_weight: u32,
    local_weight: u32,
    remote_weight: u32,
    handoff_type: Option<String>,
    candidates: Vec<SocketBackendCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketSelectionPlanEntry {
    service_id: String,
    service_name: Option<String>,
    vip: String,
    protocol: String,
    service_port: u16,
    target_port: Option<u16>,
    lb_policy: String,
    session_affinity: Option<String>,
    normalized_lb_strategy: String,
    normalized_affinity_strategy: String,
    forwarding_mode: String,
    local_backend_count: usize,
    remote_backend_count: usize,
    handoff_required: bool,
    backend_choice: SocketBackendChoiceShape,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketSelectionPlanSummary {
    listener_count: usize,
    node_local_listener_count: usize,
    cross_node_handoff_listener_count: usize,
    random_listener_count: usize,
    maglev_listener_count: usize,
    deferred_hash_listener_count: usize,
    unsupported_policy_listener_count: usize,
    affinity_listener_count: usize,
    client_ip_affinity_listener_count: usize,
    deferred_affinity_listener_count: usize,
    unsupported_affinity_listener_count: usize,
    total_backend_candidates: usize,
    total_local_candidates: usize,
    total_remote_candidates: usize,
    listeners_with_no_backends: usize,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketSelectionPlan {
    generation: String,
    previous_generation: Option<String>,
    compiled_at: String,
    observed_at: String,
    entries: Vec<SocketSelectionPlanEntry>,
    summary: SocketSelectionPlanSummary,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CompileOutcome {
    compiled_state: CompiledNodeState,
    reconcile_plan: ReconcilePlan,
    runtime_plan: RuntimePlan,
    runtime_inventory: RuntimeInventory,
    runtime_inventory_diff: RuntimeInventoryDiff,
    runtime_intent: RuntimeIntent,
    runtime_execution_summary: RuntimeExecutionSummary,
    socket_selection_plan: SocketSelectionPlan,
    apply_report: ApplyStatusReport,
}

#[derive(Debug)]
pub(crate) struct CompilerContext<'a> {
    node_id: &'a str,
    desired: &'a DesiredStateEnvelope,
    capability: &'a NodeCapability,
    previous_compiled_state: Option<&'a CompiledNodeState>,
    previous_runtime_inventory: Option<&'a RuntimeInventory>,
}
