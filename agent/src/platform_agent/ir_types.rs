use aria_api::{ApplyStatusReport, DesiredStateEnvelope, NodeCapability};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct DesiredStateCacheEntry {
    pub(crate) cached_at: String,
    pub(crate) envelope: DesiredStateEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledPortBinding {
    pub(crate) port_id: String,
    pub(crate) tenant_id: String,
    pub(crate) network_id: String,
    pub(crate) segment_id: Option<String>,
    pub(crate) security_group_ids: Vec<String>,
    pub(crate) fixed_ips: Vec<String>,
    pub(crate) allowed_address_pairs: Vec<String>,
    pub(crate) mac_address: String,
    pub(crate) anti_spoof_enabled: bool,
    pub(crate) admin_state_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AntiSpoofIr {
    pub(crate) address: [u8; 16],
    pub(crate) flags: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PortIdentityIr {
    pub(crate) port_id: String,
    pub(crate) network_id: String,
    pub(crate) tap_id: u32,
    pub(crate) ifindex: u32,
    pub(crate) sg_program_id: u32,
    pub(crate) tenant_local_id: u32,
    pub(crate) network_local_id: u32,
    pub(crate) segment_local_id: u32,
    pub(crate) mac: [u8; 6],
    pub(crate) primary_ipv4: u32,
    pub(crate) primary_ipv6: [u8; 16],
    pub(crate) anti_spoof_enabled: bool,
    pub(crate) anti_spoof_entries: Vec<AntiSpoofIr>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledRouteTableView {
    pub(crate) route_table_id: String,
    pub(crate) network_id: String,
    pub(crate) route_count: usize,
    pub(crate) default_route: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RouteIr {
    pub(crate) route_table_id: String,
    pub(crate) port_id: String,
    pub(crate) tap_id: u32,
    pub(crate) destination: [u8; 16],
    pub(crate) prefix_len: u8,
    pub(crate) is_ipv6: bool,
    pub(crate) next_hop_type: u8,
    pub(crate) next_hop_ref: String,
    pub(crate) next_hop_ip: [u8; 16],
    pub(crate) egress_ifindex: u32,
    pub(crate) route_id: u16,
    pub(crate) priority: u8,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IpGroupIr {
    pub(crate) ip_group_id: String,
    pub(crate) numeric_id: u32,
    pub(crate) network_id: String,
    pub(crate) cidrs: Vec<IpGroupCidrIr>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IpGroupCidrIr {
    pub(crate) cidr: String,
    pub(crate) is_ipv6: bool,
    pub(crate) address: [u8; 16],
    pub(crate) prefix_len: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NetworkPolicyIr {
    pub(crate) policy_id: String,
    pub(crate) network_id: String,
    pub(crate) rules: Vec<NetworkPolicyRuleIr>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NetworkPolicyRuleIr {
    pub(crate) src_numeric_id: u32,
    pub(crate) dst_numeric_id: u32,
    pub(crate) proto: u8,
    pub(crate) direction: u8,
    pub(crate) action: u8,
    pub(crate) ports: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QosPolicyIr {
    pub(crate) policy_id: String,
    pub(crate) network_id: String,
    pub(crate) rules: Vec<QosPolicyRuleIr>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QosPolicyRuleIr {
    pub(crate) ip_group_numeric_id: u32,
    pub(crate) direction: u8,
    pub(crate) rate_bps: u64,
    pub(crate) burst_bytes: u64,
    pub(crate) priority: u8,
    pub(crate) mode: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SgRuleIr {
    pub(crate) port_id: String,
    pub(crate) tap_id: u32,
    pub(crate) sg_program_id: u32,
    pub(crate) source_security_group_id: String,
    pub(crate) direction: u8,
    pub(crate) proto: u8,
    pub(crate) remote_prefix: [u8; 16],
    pub(crate) prefix_len: u8,
    pub(crate) action: u8,
    pub(crate) priority: u8,
    pub(crate) port_start: u16,
    pub(crate) port_end: u16,
    pub(crate) rule_id: u16,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledHealthCheckView {
    pub(crate) health_check_id: String,
    pub(crate) tenant_id: String,
    pub(crate) network_id: Option<String>,
    pub(crate) protocol: String,
    pub(crate) target_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledBackendSetView {
    pub(crate) backend_set_id: String,
    pub(crate) tenant_id: String,
    pub(crate) network_id: String,
    pub(crate) health_check_id: Option<String>,
    pub(crate) policy: String,
    pub(crate) backend_count: usize,
    pub(crate) local_backend_count: usize,
    pub(crate) remote_backend_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledServiceView {
    pub(crate) service_id: String,
    pub(crate) tenant_id: String,
    pub(crate) network_id: String,
    pub(crate) backend_set_id: Option<String>,
    pub(crate) vip: String,
    pub(crate) protocol: String,
    pub(crate) port_count: usize,
    pub(crate) exposure_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HealthCheckIr {
    pub(crate) health_check_id: String,
    pub(crate) tenant_id: String,
    pub(crate) network_id: Option<String>,
    pub(crate) probe_protocol: String,
    pub(crate) interval_seconds: u32,
    pub(crate) timeout_seconds: u32,
    pub(crate) healthy_threshold: u32,
    pub(crate) unhealthy_threshold: u32,
    pub(crate) target_port: Option<u16>,
    pub(crate) has_request_template: bool,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BackendMemberIr {
    pub(crate) backend_id: String,
    pub(crate) target_type: String,
    pub(crate) target_ref: Option<String>,
    pub(crate) resolved_ip_hint: Option<String>,
    pub(crate) service_port: u16,
    pub(crate) weight: u16,
    pub(crate) admin_state: String,
    pub(crate) node_id: Option<String>,
    pub(crate) declared_locality: Option<String>,
    pub(crate) resolved_locality: String,
    pub(crate) forwarding_scope: String,
    pub(crate) resolution: String,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BackendSetIr {
    pub(crate) backend_set_id: String,
    pub(crate) tenant_id: String,
    pub(crate) network_id: String,
    pub(crate) selection_policy: String,
    pub(crate) health_check_id: Option<String>,
    pub(crate) local_backend_count: usize,
    pub(crate) remote_backend_count: usize,
    pub(crate) backends: Vec<BackendMemberIr>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceFrontendPortIr {
    pub(crate) name: Option<String>,
    pub(crate) service_port: u16,
    pub(crate) target_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceFrontendIr {
    pub(crate) service_id: String,
    pub(crate) tenant_id: String,
    pub(crate) network_id: String,
    #[serde(default = "default_service_route_mode")]
    pub(crate) route_mode: String,
    pub(crate) vip: String,
    pub(crate) protocol: String,
    pub(crate) lb_policy: String,
    pub(crate) session_affinity: Option<String>,
    pub(crate) exposure_type: String,
    #[serde(default = "default_service_forwarding_mode")]
    pub(crate) forwarding_mode: String,
    pub(crate) listener_ports: Vec<ServiceFrontendPortIr>,
    pub(crate) node_local_forwarding: bool,
    pub(crate) cross_node_forwarding: bool,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceProgramIr {
    pub(crate) service_id: String,
    pub(crate) backend_set_id: Option<String>,
    pub(crate) health_check_id: Option<String>,
    pub(crate) frontend: ServiceFrontendIr,
    pub(crate) backend_set: Option<BackendSetIr>,
    pub(crate) health_check: Option<HealthCheckIr>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompileDomainSummary {
    pub(crate) domain: String,
    pub(crate) input_objects: usize,
    pub(crate) compiled_objects: usize,
    pub(crate) failed_objects: usize,
    pub(crate) status: String,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompiledNodeState {
    pub(crate) generation: String,
    pub(crate) compiler_version: String,
    pub(crate) node_id: String,
    pub(crate) capability_profile: String,
    pub(crate) full_sync: bool,
    pub(crate) issued_at: String,
    pub(crate) tenant_ids: Vec<String>,
    pub(crate) network_ids: Vec<String>,
    pub(crate) security_group_ids: Vec<String>,
    pub(crate) port_bindings: Vec<CompiledPortBinding>,
    #[serde(default)]
    pub(crate) port_identities: Vec<PortIdentityIr>,
    pub(crate) route_tables: Vec<CompiledRouteTableView>,
    #[serde(default)]
    pub(crate) route_entries: Vec<RouteIr>,
    #[serde(default)]
    pub(crate) sg_rules: Vec<SgRuleIr>,
    #[serde(default)]
    pub(crate) ip_groups: Vec<IpGroupIr>,
    #[serde(default)]
    pub(crate) network_policies: Vec<NetworkPolicyIr>,
    #[serde(default)]
    pub(crate) qos_policies: Vec<QosPolicyIr>,
    pub(crate) health_checks: Vec<CompiledHealthCheckView>,
    pub(crate) backend_sets: Vec<CompiledBackendSetView>,
    pub(crate) services: Vec<CompiledServiceView>,
    #[serde(default)]
    pub(crate) service_programs: Vec<ServiceProgramIr>,
    pub(crate) domain_summaries: Vec<CompileDomainSummary>,
    pub(crate) warnings: Vec<String>,
    pub(crate) degraded_reasons: Vec<String>,
    pub(crate) compiled_at: String,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReconcileAction {
    pub(crate) domain: String,
    pub(crate) operation: String,
    pub(crate) object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReconcilePlan {
    pub(crate) generation: String,
    pub(crate) previous_generation: Option<String>,
    pub(crate) compiled_at: String,
    pub(crate) full_reconcile: bool,
    pub(crate) changed_kinds: Vec<String>,
    pub(crate) actions: Vec<ReconcileAction>,
    pub(crate) warnings: Vec<String>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttachBindingPlan {
    pub(crate) hook_family: String,
    pub(crate) scope: String,
    pub(crate) operation: String,
    pub(crate) object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttachPlan {
    pub(crate) generation: String,
    pub(crate) compiled_at: String,
    pub(crate) required_hooks: Vec<String>,
    pub(crate) bindings: Vec<AttachBindingPlan>,
    pub(crate) required_qdisc: Vec<String>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MapPlanEntry {
    pub(crate) map_family: String,
    pub(crate) operation: String,
    pub(crate) object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MapPlan {
    pub(crate) generation: String,
    pub(crate) compiled_at: String,
    pub(crate) entries: Vec<MapPlanEntry>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimePlan {
    pub(crate) generation: String,
    pub(crate) compiled_at: String,
    pub(crate) attach_plan: AttachPlan,
    pub(crate) map_plan: MapPlan,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryAttach {
    pub(crate) domain: String,
    pub(crate) hook_family: String,
    pub(crate) scope: String,
    pub(crate) operation: String,
    pub(crate) object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryMapEntry {
    pub(crate) domain: String,
    pub(crate) map_family: String,
    pub(crate) operation: String,
    pub(crate) object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryDomainSummary {
    pub(crate) domain: String,
    pub(crate) compiled_objects: usize,
    pub(crate) failed_objects: usize,
    pub(crate) attach_operations: usize,
    pub(crate) map_operations: usize,
    pub(crate) status: String,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventory {
    pub(crate) generation: String,
    pub(crate) previous_generation: Option<String>,
    pub(crate) compiled_at: String,
    pub(crate) observed_at: String,
    pub(crate) compiler_version: String,
    pub(crate) required_hooks: Vec<String>,
    pub(crate) required_qdisc: Vec<String>,
    pub(crate) attach_inventory: Vec<RuntimeInventoryAttach>,
    pub(crate) map_inventory: Vec<RuntimeInventoryMapEntry>,
    pub(crate) domain_inventory: Vec<RuntimeInventoryDomainSummary>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryAttachDelta {
    pub(crate) domain: String,
    pub(crate) hook_family: String,
    pub(crate) scope: String,
    pub(crate) operation: String,
    pub(crate) previous_object_count: usize,
    pub(crate) current_object_count: usize,
    pub(crate) change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryMapDelta {
    pub(crate) domain: String,
    pub(crate) map_family: String,
    pub(crate) operation: String,
    pub(crate) previous_object_count: usize,
    pub(crate) current_object_count: usize,
    pub(crate) change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryDomainDelta {
    pub(crate) domain: String,
    pub(crate) previous_attach_operations: usize,
    pub(crate) current_attach_operations: usize,
    pub(crate) previous_map_operations: usize,
    pub(crate) current_map_operations: usize,
    pub(crate) previous_compiled_objects: usize,
    pub(crate) current_compiled_objects: usize,
    pub(crate) change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeInventoryDiff {
    pub(crate) generation: String,
    pub(crate) previous_generation: Option<String>,
    pub(crate) observed_at: String,
    pub(crate) changed_domains: Vec<String>,
    pub(crate) attach_deltas: Vec<RuntimeInventoryAttachDelta>,
    pub(crate) map_deltas: Vec<RuntimeInventoryMapDelta>,
    pub(crate) domain_deltas: Vec<RuntimeInventoryDomainDelta>,
    pub(crate) has_cleanup: bool,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeDomainIntent {
    pub(crate) domain: String,
    pub(crate) desired_action: String,
    pub(crate) reason: String,
    pub(crate) full_reconcile: bool,
    pub(crate) requires_cleanup: bool,
    pub(crate) changed: bool,
    pub(crate) attach_operations: usize,
    pub(crate) map_operations: usize,
    pub(crate) compiled_objects: usize,
    pub(crate) failed_objects: usize,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceRuntimeIntentSummary {
    pub(crate) service_count: usize,
    pub(crate) frontend_listener_count: usize,
    #[serde(default)]
    pub(crate) frontend_runtime_entry_count: usize,
    #[serde(default)]
    pub(crate) socket_lb_frontend_count: usize,
    #[serde(default)]
    pub(crate) packet_lb_frontend_count: usize,
    pub(crate) backend_member_count: usize,
    #[serde(default)]
    pub(crate) backend_runtime_entry_count: usize,
    pub(crate) forwarding_projection_count: usize,
    pub(crate) node_local_service_count: usize,
    pub(crate) cross_node_service_count: usize,
    #[serde(default)]
    pub(crate) cross_node_native_service_count: usize,
    #[serde(default)]
    pub(crate) cross_node_overlay_service_count: usize,
    #[serde(default)]
    pub(crate) cross_node_hybrid_service_count: usize,
    #[serde(default)]
    pub(crate) revnat_reservation_count: usize,
    #[serde(default)]
    pub(crate) affinity_reservation_count: usize,
    #[serde(default)]
    pub(crate) maglev_reservation_count: usize,
    pub(crate) desired_action: String,
    pub(crate) requires_cleanup: bool,
    pub(crate) changed: bool,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeIntent {
    pub(crate) generation: String,
    pub(crate) previous_generation: Option<String>,
    pub(crate) compiled_at: String,
    pub(crate) observed_at: String,
    pub(crate) changed_domains: Vec<String>,
    pub(crate) intents: Vec<RuntimeDomainIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) service_intent: Option<ServiceRuntimeIntentSummary>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeExecutionDomainSummary {
    pub(crate) domain: String,
    pub(crate) planned_action: String,
    pub(crate) execution_status: String,
    pub(crate) requires_cleanup: bool,
    pub(crate) changed: bool,
    pub(crate) input_objects: usize,
    pub(crate) compiled_objects: usize,
    pub(crate) failed_objects: usize,
    pub(crate) warnings: Vec<String>,
    pub(crate) degraded_reasons: Vec<String>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceRuntimeExecutionSummary {
    pub(crate) service_count: usize,
    pub(crate) frontend_listener_count: usize,
    #[serde(default)]
    pub(crate) frontend_runtime_entry_count: usize,
    #[serde(default)]
    pub(crate) socket_lb_frontend_count: usize,
    #[serde(default)]
    pub(crate) packet_lb_frontend_count: usize,
    pub(crate) backend_member_count: usize,
    #[serde(default)]
    pub(crate) backend_runtime_entry_count: usize,
    pub(crate) forwarding_projection_count: usize,
    pub(crate) node_local_service_count: usize,
    pub(crate) cross_node_service_count: usize,
    #[serde(default)]
    pub(crate) cross_node_native_service_count: usize,
    #[serde(default)]
    pub(crate) cross_node_overlay_service_count: usize,
    #[serde(default)]
    pub(crate) cross_node_hybrid_service_count: usize,
    #[serde(default)]
    pub(crate) revnat_reservation_count: usize,
    #[serde(default)]
    pub(crate) affinity_reservation_count: usize,
    #[serde(default)]
    pub(crate) maglev_reservation_count: usize,
    pub(crate) execution_status: String,
    pub(crate) planned_action: String,
    pub(crate) warnings: Vec<String>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeExecutionSummary {
    pub(crate) generation: String,
    pub(crate) previous_generation: Option<String>,
    pub(crate) compiled_at: String,
    pub(crate) observed_at: String,
    pub(crate) changed_domains: Vec<String>,
    pub(crate) domain_summaries: Vec<RuntimeExecutionDomainSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) service_execution: Option<ServiceRuntimeExecutionSummary>,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketBackendCandidate {
    pub(crate) backend_id: String,
    pub(crate) target_type: String,
    pub(crate) resolved_ip_hint: Option<String>,
    pub(crate) port: u16,
    pub(crate) weight: u16,
    pub(crate) locality: String,
    pub(crate) forwarding_scope: String,
    pub(crate) health_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketBackendChoiceShape {
    pub(crate) total_candidates: usize,
    pub(crate) local_candidates: usize,
    pub(crate) remote_candidates: usize,
    pub(crate) total_weight: u32,
    pub(crate) local_weight: u32,
    pub(crate) remote_weight: u32,
    pub(crate) handoff_type: Option<String>,
    pub(crate) candidates: Vec<SocketBackendCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketSelectionPlanEntry {
    pub(crate) service_id: String,
    pub(crate) service_name: Option<String>,
    pub(crate) vip: String,
    pub(crate) protocol: String,
    pub(crate) service_port: u16,
    pub(crate) target_port: Option<u16>,
    pub(crate) lb_policy: String,
    pub(crate) session_affinity: Option<String>,
    pub(crate) normalized_lb_strategy: String,
    pub(crate) normalized_affinity_strategy: String,
    pub(crate) forwarding_mode: String,
    pub(crate) local_backend_count: usize,
    pub(crate) remote_backend_count: usize,
    pub(crate) handoff_required: bool,
    pub(crate) backend_choice: SocketBackendChoiceShape,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketSelectionPlanSummary {
    pub(crate) listener_count: usize,
    pub(crate) node_local_listener_count: usize,
    pub(crate) cross_node_handoff_listener_count: usize,
    pub(crate) random_listener_count: usize,
    pub(crate) maglev_listener_count: usize,
    pub(crate) deferred_hash_listener_count: usize,
    pub(crate) unsupported_policy_listener_count: usize,
    pub(crate) affinity_listener_count: usize,
    pub(crate) client_ip_affinity_listener_count: usize,
    pub(crate) deferred_affinity_listener_count: usize,
    pub(crate) unsupported_affinity_listener_count: usize,
    pub(crate) total_backend_candidates: usize,
    pub(crate) total_local_candidates: usize,
    pub(crate) total_remote_candidates: usize,
    pub(crate) listeners_with_no_backends: usize,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SocketSelectionPlan {
    pub(crate) generation: String,
    pub(crate) previous_generation: Option<String>,
    pub(crate) compiled_at: String,
    pub(crate) observed_at: String,
    pub(crate) entries: Vec<SocketSelectionPlanEntry>,
    pub(crate) summary: SocketSelectionPlanSummary,
    pub(crate) shadow_apply_only: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CompileOutcome {
    pub(crate) compiled_state: CompiledNodeState,
    pub(crate) reconcile_plan: ReconcilePlan,
    pub(crate) runtime_plan: RuntimePlan,
    pub(crate) runtime_inventory: RuntimeInventory,
    pub(crate) runtime_inventory_diff: RuntimeInventoryDiff,
    pub(crate) runtime_intent: RuntimeIntent,
    pub(crate) runtime_execution_summary: RuntimeExecutionSummary,
    pub(crate) socket_selection_plan: SocketSelectionPlan,
    pub(crate) apply_report: ApplyStatusReport,
}

#[derive(Debug)]
pub(crate) struct CompilerContext<'a> {
    pub(crate) node_id: &'a str,
    pub(crate) desired: &'a DesiredStateEnvelope,
    pub(crate) capability: &'a NodeCapability,
    pub(crate) previous_compiled_state: Option<&'a CompiledNodeState>,
    pub(crate) previous_runtime_inventory: Option<&'a RuntimeInventory>,
}
