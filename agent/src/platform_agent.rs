use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aria_api::{
    ApplyDomainStatus, ApplyObjectFailure, ApplyStatusReport, ApplyStatusResponse,
    DesiredStateEnvelope, HeartbeatResponse, NodeAddress, NodeCapability, NodeHealthReport,
    NodeInfo, NodeRegisterRequest, NodeRegisterResponse, PlatformApiError,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{fs, task::JoinHandle, time};
use tracing::{debug, info, warn};

#[derive(Clone, Debug)]
pub struct PlatformAgentConfig {
    pub controller_url: String,
    pub node_id: String,
    pub management_address: Option<String>,
    pub labels: BTreeMap<String, String>,
    pub poll_interval: Duration,
    pub register_interval: Duration,
    pub state_dir: PathBuf,
    pub trace_backend: String,
    pub kernel_version: Option<String>,
    pub max_port_policies: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesiredStateCacheEntry {
    cached_at: String,
    envelope: DesiredStateEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompiledPortBinding {
    port_id: String,
    tenant_id: String,
    network_id: String,
    security_group_ids: Vec<String>,
    fixed_ips: Vec<String>,
    allowed_address_pairs: Vec<String>,
    mac_address: String,
    anti_spoof_enabled: bool,
    admin_state_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompiledRouteTableView {
    route_table_id: String,
    network_id: String,
    route_count: usize,
    default_route: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompiledHealthCheckView {
    health_check_id: String,
    tenant_id: String,
    network_id: Option<String>,
    protocol: String,
    target_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompiledBackendSetView {
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
struct CompiledServiceView {
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
struct HealthCheckIr {
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
struct BackendMemberIr {
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
struct BackendSetIr {
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
struct ServiceFrontendPortIr {
    name: Option<String>,
    service_port: u16,
    target_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceFrontendIr {
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
struct ServiceProgramIr {
    service_id: String,
    backend_set_id: Option<String>,
    health_check_id: Option<String>,
    frontend: ServiceFrontendIr,
    backend_set: Option<BackendSetIr>,
    health_check: Option<HealthCheckIr>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompileDomainSummary {
    domain: String,
    input_objects: usize,
    compiled_objects: usize,
    failed_objects: usize,
    status: String,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompiledNodeState {
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
    route_tables: Vec<CompiledRouteTableView>,
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
struct ReconcileAction {
    domain: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReconcilePlan {
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
struct AttachBindingPlan {
    hook_family: String,
    scope: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttachPlan {
    generation: String,
    compiled_at: String,
    required_hooks: Vec<String>,
    bindings: Vec<AttachBindingPlan>,
    required_qdisc: Vec<String>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapPlanEntry {
    map_family: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapPlan {
    generation: String,
    compiled_at: String,
    entries: Vec<MapPlanEntry>,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimePlan {
    generation: String,
    compiled_at: String,
    attach_plan: AttachPlan,
    map_plan: MapPlan,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeInventoryAttach {
    domain: String,
    hook_family: String,
    scope: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeInventoryMapEntry {
    domain: String,
    map_family: String,
    operation: String,
    object_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeInventoryDomainSummary {
    domain: String,
    compiled_objects: usize,
    failed_objects: usize,
    attach_operations: usize,
    map_operations: usize,
    status: String,
    shadow_apply_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeInventory {
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
struct RuntimeInventoryAttachDelta {
    domain: String,
    hook_family: String,
    scope: String,
    operation: String,
    previous_object_count: usize,
    current_object_count: usize,
    change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeInventoryMapDelta {
    domain: String,
    map_family: String,
    operation: String,
    previous_object_count: usize,
    current_object_count: usize,
    change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeInventoryDomainDelta {
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
struct RuntimeInventoryDiff {
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
struct RuntimeDomainIntent {
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
struct ServiceRuntimeIntentSummary {
    service_count: usize,
    frontend_listener_count: usize,
    backend_member_count: usize,
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
struct RuntimeIntent {
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
struct RuntimeExecutionDomainSummary {
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
struct ServiceRuntimeExecutionSummary {
    service_count: usize,
    frontend_listener_count: usize,
    backend_member_count: usize,
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
struct RuntimeExecutionSummary {
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

#[derive(Debug, Clone)]
struct CompileOutcome {
    compiled_state: CompiledNodeState,
    reconcile_plan: ReconcilePlan,
    runtime_plan: RuntimePlan,
    runtime_inventory: RuntimeInventory,
    runtime_inventory_diff: RuntimeInventoryDiff,
    runtime_intent: RuntimeIntent,
    runtime_execution_summary: RuntimeExecutionSummary,
    apply_report: ApplyStatusReport,
}

#[derive(Debug)]
struct CompilerContext<'a> {
    node_id: &'a str,
    desired: &'a DesiredStateEnvelope,
    capability: &'a NodeCapability,
    previous_compiled_state: Option<&'a CompiledNodeState>,
    previous_runtime_inventory: Option<&'a RuntimeInventory>,
}

struct SouthboundClient {
    base_url: String,
    client: reqwest::Client,
}

struct LocalPlatformStateStore {
    root: PathBuf,
}

struct PlatformAgent {
    config: PlatformAgentConfig,
    client: SouthboundClient,
    state_store: LocalPlatformStateStore,
    start_time: Instant,
    capability: NodeCapability,
}

pub fn start(config: PlatformAgentConfig) -> JoinHandle<()> {
    tokio::spawn(async move {
        let agent = PlatformAgent::new(config);
        agent.run().await;
    })
}

impl PlatformAgent {
    fn new(config: PlatformAgentConfig) -> Self {
        let client = SouthboundClient::new(&config.controller_url);
        let state_store = LocalPlatformStateStore::new(config.state_dir.clone());
        let capability = build_node_capability(&config);
        Self {
            config,
            client,
            state_store,
            start_time: Instant::now(),
            capability,
        }
    }

    async fn run(self) {
        let mut interval = time::interval(self.config.poll_interval);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        interval.tick().await;

        let mut last_register_at: Option<Instant> = None;
        let mut last_register_response: Option<NodeRegisterResponse> = None;
        let mut desired_cache = self.state_store.load_desired_state().await;
        let mut compiled_state = self.state_store.load_compiled_state().await;
        let mut reconcile_plan = self.state_store.load_reconcile_plan().await;
        let mut runtime_plan = self.state_store.load_runtime_plan().await;
        let mut runtime_inventory = self.state_store.load_runtime_inventory().await;
        let mut runtime_inventory_diff = self.state_store.load_runtime_inventory_diff().await;
        let mut runtime_intent = self.state_store.load_runtime_intent().await;
        let mut runtime_execution_summary = self.state_store.load_runtime_execution_summary().await;
        loop {
            interval.tick().await;

            let needs_register = last_register_at
                .map(|registered_at| registered_at.elapsed() >= self.config.register_interval)
                .unwrap_or(true);
            if needs_register {
                match self.register().await {
                    Ok(response) => {
                        info!(
                            node_id = %response.node_id,
                            desired_generation = %response.desired_generation,
                            full_sync_required = response.full_sync_required,
                            "southbound node registration refreshed"
                        );
                        last_register_response = Some(response);
                        last_register_at = Some(Instant::now());
                    }
                    Err(error) => {
                        warn!(error = %error, "southbound node registration failed");
                        continue;
                    }
                }
            }

            let Some(register_response) = last_register_response.as_ref() else {
                continue;
            };

            let desired_state = match self
                .client
                .desired_state(&self.config.node_id, &register_response.desired_state_url)
                .await
            {
                Ok(envelope) => envelope,
                Err(error) => {
                    warn!(error = %error, "failed to fetch desired-state envelope");
                    let last_reconcile_at = compiled_state
                        .as_ref()
                        .map(|state| state.compiled_at.clone());
                    let attached_ports = compiled_state
                        .as_ref()
                        .map(|state| state.port_bindings.len())
                        .unwrap_or(0);
                    if let Err(heartbeat_error) = self
                        .send_heartbeat(attached_ports, last_reconcile_at, Some(error))
                        .await
                    {
                        warn!(error = %heartbeat_error, "failed to report degraded heartbeat");
                    }
                    continue;
                }
            };

            let desired_generation = desired_state.generation.clone();
            let needs_compile = desired_cache
                .as_ref()
                .map(|cache| cache.envelope.generation.as_str())
                != Some(desired_generation.as_str())
                || compiled_state
                    .as_ref()
                    .map(|state| state.generation.as_str())
                    != Some(desired_generation.as_str())
                || reconcile_plan.as_ref().map(|plan| plan.generation.as_str())
                    != Some(desired_generation.as_str())
                || runtime_plan.as_ref().map(|plan| plan.generation.as_str())
                    != Some(desired_generation.as_str())
                || runtime_inventory
                    .as_ref()
                    .map(|inventory| inventory.generation.as_str())
                    != Some(desired_generation.as_str())
                || runtime_inventory_diff
                    .as_ref()
                    .map(|diff| diff.generation.as_str())
                    != Some(desired_generation.as_str())
                || runtime_intent
                    .as_ref()
                    .map(|intent| intent.generation.as_str())
                    != Some(desired_generation.as_str())
                || runtime_execution_summary
                    .as_ref()
                    .map(|summary| summary.generation.as_str())
                    != Some(desired_generation.as_str());

            let mut last_reconcile_at = compiled_state
                .as_ref()
                .map(|state| state.compiled_at.clone());
            let mut attached_ports = compiled_state
                .as_ref()
                .map(|state| state.port_bindings.len())
                .unwrap_or(0);
            let mut heartbeat_error: Option<String> = None;

            if needs_compile {
                let cache_entry = DesiredStateCacheEntry {
                    cached_at: unix_timestamp_string(),
                    envelope: desired_state.clone(),
                };
                if let Err(error) = self.state_store.save_desired_state(&cache_entry).await {
                    warn!(error = %error, "failed to persist desired-state cache");
                    heartbeat_error = Some(error);
                } else {
                    desired_cache = Some(cache_entry);
                }

                let outcome = compile_desired_state(CompilerContext {
                    node_id: &self.config.node_id,
                    desired: &desired_state,
                    capability: &self.capability,
                    previous_compiled_state: compiled_state.as_ref(),
                    previous_runtime_inventory: runtime_inventory.as_ref(),
                });
                attached_ports = outcome.compiled_state.port_bindings.len();
                last_reconcile_at = Some(outcome.compiled_state.compiled_at.clone());

                if let Err(error) = self
                    .state_store
                    .save_compiled_state(&outcome.compiled_state)
                    .await
                {
                    warn!(error = %error, "failed to persist compiled node state");
                    heartbeat_error = Some(error);
                } else {
                    compiled_state = Some(outcome.compiled_state.clone());
                }

                if let Err(error) = self
                    .state_store
                    .save_reconcile_plan(&outcome.reconcile_plan)
                    .await
                {
                    warn!(error = %error, "failed to persist reconcile plan");
                    heartbeat_error = Some(error);
                } else {
                    reconcile_plan = Some(outcome.reconcile_plan.clone());
                    info!(
                        generation = %outcome.reconcile_plan.generation,
                        full_reconcile = outcome.reconcile_plan.full_reconcile,
                        actions = outcome.reconcile_plan.actions.len(),
                        changed_kinds = outcome.reconcile_plan.changed_kinds.len(),
                        "persisted shadow reconcile plan"
                    );
                }

                if let Err(error) = self
                    .state_store
                    .save_runtime_plan(&outcome.runtime_plan)
                    .await
                {
                    warn!(error = %error, "failed to persist runtime plan");
                    heartbeat_error = Some(error);
                } else {
                    runtime_plan = Some(outcome.runtime_plan.clone());
                    info!(
                        generation = %outcome.runtime_plan.generation,
                        attach_bindings = outcome.runtime_plan.attach_plan.bindings.len(),
                        map_entries = outcome.runtime_plan.map_plan.entries.len(),
                        "persisted shadow runtime plan"
                    );
                }

                if let Err(error) = self
                    .state_store
                    .save_runtime_inventory(&outcome.runtime_inventory)
                    .await
                {
                    warn!(error = %error, "failed to persist runtime inventory");
                    heartbeat_error = Some(error);
                } else {
                    runtime_inventory = Some(outcome.runtime_inventory.clone());
                    info!(
                        generation = %outcome.runtime_inventory.generation,
                        attach_inventory = outcome.runtime_inventory.attach_inventory.len(),
                        map_inventory = outcome.runtime_inventory.map_inventory.len(),
                        domain_inventory = outcome.runtime_inventory.domain_inventory.len(),
                        "persisted shadow runtime inventory"
                    );
                }

                if let Err(error) = self
                    .state_store
                    .save_runtime_inventory_diff(&outcome.runtime_inventory_diff)
                    .await
                {
                    warn!(error = %error, "failed to persist runtime inventory diff");
                    heartbeat_error = Some(error);
                } else {
                    runtime_inventory_diff = Some(outcome.runtime_inventory_diff.clone());
                    info!(
                        generation = %outcome.runtime_inventory_diff.generation,
                        changed_domains = outcome.runtime_inventory_diff.changed_domains.len(),
                        attach_deltas = outcome.runtime_inventory_diff.attach_deltas.len(),
                        map_deltas = outcome.runtime_inventory_diff.map_deltas.len(),
                        "persisted shadow runtime inventory diff"
                    );
                }

                if let Err(error) = self
                    .state_store
                    .save_runtime_intent(&outcome.runtime_intent)
                    .await
                {
                    warn!(error = %error, "failed to persist runtime intent");
                    heartbeat_error = Some(error);
                } else {
                    runtime_intent = Some(outcome.runtime_intent.clone());
                    info!(
                        generation = %outcome.runtime_intent.generation,
                        changed_domains = outcome.runtime_intent.changed_domains.len(),
                        intents = outcome.runtime_intent.intents.len(),
                        "persisted shadow runtime intent"
                    );
                }

                if let Err(error) = self
                    .state_store
                    .save_runtime_execution_summary(&outcome.runtime_execution_summary)
                    .await
                {
                    warn!(error = %error, "failed to persist runtime execution summary");
                    heartbeat_error = Some(error);
                } else {
                    runtime_execution_summary = Some(outcome.runtime_execution_summary.clone());
                    info!(
                        generation = %outcome.runtime_execution_summary.generation,
                        changed_domains = outcome.runtime_execution_summary.changed_domains.len(),
                        domain_summaries = outcome.runtime_execution_summary.domain_summaries.len(),
                        "persisted shadow runtime execution summary"
                    );
                }

                if let Err(error) = self
                    .client
                    .report_apply_status(&self.config.node_id, &outcome.apply_report)
                    .await
                {
                    warn!(error = %error, "failed to report apply status");
                    heartbeat_error = Some(error);
                } else {
                    info!(
                        generation = %outcome.apply_report.generation,
                        status = %outcome.apply_report.status,
                        warnings = outcome.apply_report.warnings.len(),
                        failed_objects = outcome.apply_report.failed_objects.len(),
                        "reported southbound compile/apply status"
                    );
                }
            }

            if !needs_compile {
                last_reconcile_at = reconcile_plan
                    .as_ref()
                    .map(|plan| plan.compiled_at.clone())
                    .or_else(|| runtime_plan.as_ref().map(|plan| plan.compiled_at.clone()));
            }

            if let Err(error) = self
                .send_heartbeat(attached_ports, last_reconcile_at, heartbeat_error.clone())
                .await
            {
                warn!(error = %error, "failed to report southbound heartbeat");
            }
        }
    }

    async fn register(&self) -> Result<NodeRegisterResponse, String> {
        let request = NodeRegisterRequest {
            info: NodeInfo {
                node_id: self.config.node_id.clone(),
                hostname: hostname(),
                agent_version: env!("CARGO_PKG_VERSION").to_string(),
                kernel_version: self
                    .config
                    .kernel_version
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                addresses: self
                    .config
                    .management_address
                    .as_ref()
                    .map(|value| {
                        vec![NodeAddress {
                            kind: "management".to_string(),
                            value: value.clone(),
                        }]
                    })
                    .unwrap_or_default(),
                labels: self.config.labels.clone(),
            },
            capability: self.capability.clone(),
        };
        self.client
            .register_node(&self.config.node_id, &request)
            .await
    }

    async fn send_heartbeat(
        &self,
        attached_ports: usize,
        last_reconcile_at: Option<String>,
        last_error: Option<String>,
    ) -> Result<HeartbeatResponse, String> {
        let report = NodeHealthReport {
            agent_uptime: self.start_time.elapsed().as_secs(),
            datapath_ready: last_error.is_none(),
            attached_ports,
            event_queue_depth: 0,
            wal_health: "ok".to_string(),
            last_reconcile_at,
            last_error,
        };
        self.client.heartbeat(&self.config.node_id, &report).await
    }
}

impl SouthboundClient {
    fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    async fn register_node(
        &self,
        node_id: &str,
        request: &NodeRegisterRequest,
    ) -> Result<NodeRegisterResponse, String> {
        let response = self
            .client
            .post(self.url(&format!("/api/v1/southbound/nodes/{node_id}/register")))
            .json(request)
            .send()
            .await
            .map_err(connection_error)?;
        self.parse_response(response).await
    }

    async fn desired_state(
        &self,
        node_id: &str,
        desired_state_url: &str,
    ) -> Result<DesiredStateEnvelope, String> {
        let response = self
            .client
            .get(self.resolve_url(
                desired_state_url,
                &format!("/api/v1/southbound/nodes/{node_id}/desired-state"),
            ))
            .send()
            .await
            .map_err(connection_error)?;
        self.parse_response(response).await
    }

    async fn report_apply_status(
        &self,
        node_id: &str,
        report: &ApplyStatusReport,
    ) -> Result<ApplyStatusResponse, String> {
        let response = self
            .client
            .post(self.url(&format!("/api/v1/southbound/nodes/{node_id}/apply-status")))
            .json(report)
            .send()
            .await
            .map_err(connection_error)?;
        self.parse_response(response).await
    }

    async fn heartbeat(
        &self,
        node_id: &str,
        report: &NodeHealthReport,
    ) -> Result<HeartbeatResponse, String> {
        let response = self
            .client
            .post(self.url(&format!("/api/v1/southbound/nodes/{node_id}/heartbeat")))
            .json(report)
            .send()
            .await
            .map_err(connection_error)?;
        self.parse_response(response).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn resolve_url(&self, desired_state_url: &str, fallback_path: &str) -> String {
        if desired_state_url.starts_with("http://") || desired_state_url.starts_with("https://") {
            desired_state_url.to_string()
        } else if desired_state_url.trim().is_empty() {
            self.url(fallback_path)
        } else {
            self.url(desired_state_url)
        }
    }

    async fn parse_response<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, String> {
        let status = response.status();
        if status.is_success() {
            return response
                .json::<T>()
                .await
                .map_err(|error| format!("failed to decode southbound response: {error}"));
        }

        let message = parse_platform_error(response).await.unwrap_or_else(|| {
            format!("southbound request failed with status {}", status.as_u16())
        });
        Err(message)
    }
}

impl LocalPlatformStateStore {
    fn new(base_state_dir: PathBuf) -> Self {
        Self {
            root: base_state_dir.join("platform-agent"),
        }
    }

    async fn load_desired_state(&self) -> Option<DesiredStateCacheEntry> {
        self.load_json(self.desired_state_path()).await
    }

    async fn save_desired_state(&self, state: &DesiredStateCacheEntry) -> Result<(), String> {
        self.save_json(self.desired_state_path(), state).await
    }

    async fn load_compiled_state(&self) -> Option<CompiledNodeState> {
        self.load_json(self.compiled_state_path()).await
    }

    async fn save_compiled_state(&self, state: &CompiledNodeState) -> Result<(), String> {
        self.save_json(self.compiled_state_path(), state).await
    }

    async fn load_reconcile_plan(&self) -> Option<ReconcilePlan> {
        self.load_json(self.reconcile_plan_path()).await
    }

    async fn save_reconcile_plan(&self, plan: &ReconcilePlan) -> Result<(), String> {
        self.save_json(self.reconcile_plan_path(), plan).await
    }

    async fn load_runtime_plan(&self) -> Option<RuntimePlan> {
        self.load_json(self.runtime_plan_path()).await
    }

    async fn save_runtime_plan(&self, plan: &RuntimePlan) -> Result<(), String> {
        self.save_json(self.runtime_plan_path(), plan).await
    }

    async fn load_runtime_inventory(&self) -> Option<RuntimeInventory> {
        self.load_json(self.runtime_inventory_path()).await
    }

    async fn save_runtime_inventory(&self, inventory: &RuntimeInventory) -> Result<(), String> {
        self.save_json(self.runtime_inventory_path(), inventory)
            .await
    }

    async fn load_runtime_inventory_diff(&self) -> Option<RuntimeInventoryDiff> {
        self.load_json(self.runtime_inventory_diff_path()).await
    }

    async fn save_runtime_inventory_diff(&self, diff: &RuntimeInventoryDiff) -> Result<(), String> {
        self.save_json(self.runtime_inventory_diff_path(), diff)
            .await
    }

    async fn load_runtime_intent(&self) -> Option<RuntimeIntent> {
        self.load_json(self.runtime_intent_path()).await
    }

    async fn save_runtime_intent(&self, intent: &RuntimeIntent) -> Result<(), String> {
        self.save_json(self.runtime_intent_path(), intent).await
    }

    async fn load_runtime_execution_summary(&self) -> Option<RuntimeExecutionSummary> {
        self.load_json(self.runtime_execution_summary_path()).await
    }

    async fn save_runtime_execution_summary(
        &self,
        summary: &RuntimeExecutionSummary,
    ) -> Result<(), String> {
        self.save_json(self.runtime_execution_summary_path(), summary)
            .await
    }

    fn desired_state_path(&self) -> PathBuf {
        self.root.join("desired-state-cache.json")
    }

    fn compiled_state_path(&self) -> PathBuf {
        self.root.join("compiled-node-state.json")
    }

    fn reconcile_plan_path(&self) -> PathBuf {
        self.root.join("reconcile-plan.json")
    }

    fn runtime_plan_path(&self) -> PathBuf {
        self.root.join("runtime-plan.json")
    }

    fn runtime_inventory_path(&self) -> PathBuf {
        self.root.join("runtime-inventory.json")
    }

    fn runtime_inventory_diff_path(&self) -> PathBuf {
        self.root.join("runtime-inventory-diff.json")
    }

    fn runtime_intent_path(&self) -> PathBuf {
        self.root.join("runtime-intent.json")
    }

    fn runtime_execution_summary_path(&self) -> PathBuf {
        self.root.join("runtime-execution-summary.json")
    }

    async fn load_json<T>(&self, path: PathBuf) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let contents = fs::read_to_string(&path).await.ok()?;
        match serde_json::from_str::<T>(&contents) {
            Ok(value) => Some(value),
            Err(error) => {
                warn!(path = %path.display(), error = %error, "failed to decode local platform-agent state");
                None
            }
        }
    }

    async fn save_json<T>(&self, path: PathBuf, value: &T) -> Result<(), String>
    where
        T: Serialize,
    {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|error| {
                format!(
                    "failed to create platform-agent state directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
            format!(
                "failed to encode platform-agent state {}: {error}",
                path.display()
            )
        })?;
        let tmp_path = temp_path(&path);
        fs::write(&tmp_path, bytes).await.map_err(|error| {
            format!(
                "failed to write platform-agent state {}: {error}",
                tmp_path.display()
            )
        })?;
        fs::rename(&tmp_path, &path).await.map_err(|error| {
            format!(
                "failed to replace platform-agent state {}: {error}",
                path.display()
            )
        })?;
        Ok(())
    }
}

fn build_node_capability(config: &PlatformAgentConfig) -> NodeCapability {
    let mut supported_hooks = vec!["xdp".to_string(), "tc".to_string()];
    supported_hooks.sort();
    supported_hooks.dedup();

    let mut limits = BTreeMap::new();
    limits.insert(
        "max_port_policies".to_string(),
        config.max_port_policies as u64,
    );

    NodeCapability {
        supported_hooks,
        supports_xdp: true,
        supports_tc: true,
        supports_socket_lb: false,
        supports_trace_ringbuf: config.trace_backend == "ringbuf",
        supports_nat: false,
        supports_lb: false,
        supports_encap: false,
        supports_qos_shaping: true,
        limits,
        observability_profile: Some(if config.trace_backend == "ringbuf" {
            "full".to_string()
        } else {
            "standard".to_string()
        }),
    }
}

fn compile_desired_state(context: CompilerContext<'_>) -> CompileOutcome {
    let tenant_ids = context
        .desired
        .tenants
        .iter()
        .map(|tenant| tenant.metadata.id.clone())
        .collect::<BTreeSet<_>>();
    let network_by_id = context
        .desired
        .networks
        .iter()
        .map(|network| (network.metadata.id.clone(), network))
        .collect::<BTreeMap<_, _>>();
    let security_group_by_id = context
        .desired
        .security_groups
        .iter()
        .map(|security_group| (security_group.metadata.id.clone(), security_group))
        .collect::<BTreeMap<_, _>>();
    let health_check_by_id = context
        .desired
        .health_checks
        .iter()
        .map(|health_check| (health_check.metadata.id.clone(), health_check))
        .collect::<BTreeMap<_, _>>();
    let port_by_id = context
        .desired
        .ports
        .iter()
        .map(|port| (port.metadata.id.clone(), port))
        .collect::<BTreeMap<_, _>>();

    let mut failed_objects = Vec::new();
    let mut warnings = Vec::new();
    let mut port_bindings = Vec::new();

    for port in &context.desired.ports {
        if let Some(bound_node_id) = port.spec.node_id.as_deref() {
            if bound_node_id != context.node_id {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "port".to_string(),
                    id: port.metadata.id.clone(),
                    reason: format!(
                        "port bound to node '{}' instead of '{}'",
                        bound_node_id, context.node_id
                    ),
                });
                continue;
            }
        } else {
            warnings.push(format!(
                "port '{}' has no explicit node binding; treating it as node-local shadow state",
                port.metadata.id
            ));
        }

        if !tenant_ids.is_empty() && !tenant_ids.contains(&port.spec.tenant_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "port".to_string(),
                id: port.metadata.id.clone(),
                reason: format!(
                    "missing tenant '{}' in desired envelope",
                    port.spec.tenant_id
                ),
            });
            continue;
        }

        if !network_by_id.contains_key(&port.spec.network_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "port".to_string(),
                id: port.metadata.id.clone(),
                reason: format!(
                    "missing network '{}' in desired envelope",
                    port.spec.network_id
                ),
            });
            continue;
        }

        let mut missing_security_group = None;
        for security_group_id in &port.spec.security_group_ids {
            if !security_group_by_id.contains_key(security_group_id) {
                missing_security_group = Some(security_group_id.clone());
                break;
            }
        }
        if let Some(security_group_id) = missing_security_group {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "port".to_string(),
                id: port.metadata.id.clone(),
                reason: format!(
                    "missing security group '{}' in desired envelope",
                    security_group_id
                ),
            });
            continue;
        }

        port_bindings.push(CompiledPortBinding {
            port_id: port.metadata.id.clone(),
            tenant_id: port.spec.tenant_id.clone(),
            network_id: port.spec.network_id.clone(),
            security_group_ids: port.spec.security_group_ids.clone(),
            fixed_ips: port.spec.fixed_ips.clone(),
            allowed_address_pairs: port.spec.allowed_address_pairs.clone(),
            mac_address: port.spec.mac_address.clone(),
            anti_spoof_enabled: port.spec.anti_spoof_enabled,
            admin_state_up: port.spec.admin_state_up,
        });
    }

    let mut compiled_route_tables = Vec::new();
    for route_table in &context.desired.route_tables {
        if !network_by_id.contains_key(&route_table.spec.network_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "route_table".to_string(),
                id: route_table.metadata.id.clone(),
                reason: format!(
                    "missing network '{}' in desired envelope",
                    route_table.spec.network_id
                ),
            });
            continue;
        }

        compiled_route_tables.push(CompiledRouteTableView {
            route_table_id: route_table.metadata.id.clone(),
            network_id: route_table.spec.network_id.clone(),
            route_count: route_table.spec.routes.len(),
            default_route: route_table.spec.default_route.clone(),
        });
    }

    let mut compiled_health_checks = Vec::new();
    for health_check in &context.desired.health_checks {
        if !tenant_ids.is_empty() && !tenant_ids.contains(&health_check.spec.tenant_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "health_check".to_string(),
                id: health_check.metadata.id.clone(),
                reason: format!(
                    "missing tenant '{}' in desired envelope",
                    health_check.spec.tenant_id
                ),
            });
            continue;
        }

        if let Some(network_id) = health_check.spec.network_id.as_deref() {
            if !network_by_id.contains_key(network_id) {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "health_check".to_string(),
                    id: health_check.metadata.id.clone(),
                    reason: format!("missing network '{}' in desired envelope", network_id),
                });
                continue;
            }
        }

        compiled_health_checks.push(CompiledHealthCheckView {
            health_check_id: health_check.metadata.id.clone(),
            tenant_id: health_check.spec.tenant_id.clone(),
            network_id: health_check.spec.network_id.clone(),
            protocol: health_check.spec.protocol.clone(),
            target_port: health_check.spec.target_port,
        });
    }

    let compiled_health_check_ids = compiled_health_checks
        .iter()
        .map(|health_check| health_check.health_check_id.clone())
        .collect::<BTreeSet<_>>();

    let mut compiled_backend_sets = Vec::new();
    for backend_set in &context.desired.backend_sets {
        if !tenant_ids.is_empty() && !tenant_ids.contains(&backend_set.spec.tenant_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "backend_set".to_string(),
                id: backend_set.metadata.id.clone(),
                reason: format!(
                    "missing tenant '{}' in desired envelope",
                    backend_set.spec.tenant_id
                ),
            });
            continue;
        }

        if !network_by_id.contains_key(&backend_set.spec.network_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "backend_set".to_string(),
                id: backend_set.metadata.id.clone(),
                reason: format!(
                    "missing network '{}' in desired envelope",
                    backend_set.spec.network_id
                ),
            });
            continue;
        }

        if let Some(health_check_id) = backend_set.spec.health_check_id.as_deref() {
            if !compiled_health_check_ids.contains(health_check_id)
                && !health_check_by_id.contains_key(health_check_id)
            {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "backend_set".to_string(),
                    id: backend_set.metadata.id.clone(),
                    reason: format!(
                        "missing health_check '{}' in desired envelope",
                        health_check_id
                    ),
                });
                continue;
            }
        }

        let mut local_backend_count = 0usize;
        let mut remote_backend_count = 0usize;
        for backend in &backend_set.spec.backends {
            if let Some(target_ref) = backend.target_ref.as_deref() {
                if let Some(port) = port_by_id.get(target_ref) {
                    let bound_node = port.spec.node_id.as_deref().unwrap_or(context.node_id);
                    if bound_node == context.node_id {
                        local_backend_count += 1;
                    } else {
                        remote_backend_count += 1;
                    }
                } else if backend.node_id.as_deref() == Some(context.node_id)
                    || backend.locality.as_deref() == Some("local")
                {
                    failed_objects.push(ApplyObjectFailure {
                        resource_kind: "backend_set".to_string(),
                        id: backend_set.metadata.id.clone(),
                        reason: format!(
                            "backend '{}' references local port '{}' that is missing from desired envelope",
                            backend.id, target_ref
                        ),
                    });
                    local_backend_count = 0;
                    remote_backend_count = 0;
                    break;
                } else {
                    warnings.push(format!(
                        "backend_set '{}' backend '{}' references remote port '{}' outside node-local desired envelope; treating as remote shadow backend",
                        backend_set.metadata.id, backend.id, target_ref
                    ));
                    remote_backend_count += 1;
                }
            } else if backend.locality.as_deref() == Some("local") {
                local_backend_count += 1;
            } else {
                remote_backend_count += 1;
            }
        }

        if failed_objects.iter().any(|failure| {
            failure.resource_kind == "backend_set" && failure.id == backend_set.metadata.id
        }) {
            continue;
        }

        compiled_backend_sets.push(CompiledBackendSetView {
            backend_set_id: backend_set.metadata.id.clone(),
            tenant_id: backend_set.spec.tenant_id.clone(),
            network_id: backend_set.spec.network_id.clone(),
            health_check_id: backend_set.spec.health_check_id.clone(),
            policy: backend_set.spec.policy.clone(),
            backend_count: backend_set.spec.backends.len(),
            local_backend_count,
            remote_backend_count,
        });
    }

    let compiled_backend_set_ids = compiled_backend_sets
        .iter()
        .map(|backend_set| backend_set.backend_set_id.clone())
        .collect::<BTreeSet<_>>();

    let mut compiled_services = Vec::new();
    for service in &context.desired.services {
        if !tenant_ids.is_empty() && !tenant_ids.contains(&service.spec.tenant_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "service".to_string(),
                id: service.metadata.id.clone(),
                reason: format!(
                    "missing tenant '{}' in desired envelope",
                    service.spec.tenant_id
                ),
            });
            continue;
        }

        if !network_by_id.contains_key(&service.spec.network_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "service".to_string(),
                id: service.metadata.id.clone(),
                reason: format!(
                    "missing network '{}' in desired envelope",
                    service.spec.network_id
                ),
            });
            continue;
        }

        if let Some(backend_set_id) = service.spec.backend_set_id.as_deref() {
            if !compiled_backend_set_ids.contains(backend_set_id) {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "service".to_string(),
                    id: service.metadata.id.clone(),
                    reason: format!(
                        "missing backend_set '{}' in desired envelope",
                        backend_set_id
                    ),
                });
                continue;
            }
        }

        compiled_services.push(CompiledServiceView {
            service_id: service.metadata.id.clone(),
            tenant_id: service.spec.tenant_id.clone(),
            network_id: service.spec.network_id.clone(),
            backend_set_id: service.spec.backend_set_id.clone(),
            vip: service.spec.vip.clone(),
            protocol: service.spec.protocol.clone(),
            port_count: service.spec.ports.len(),
            exposure_type: service.spec.exposure_type.clone(),
        });
    }

    let service_programs = build_service_programs(
        context.desired,
        &compiled_health_checks,
        &compiled_backend_sets,
        &compiled_services,
        context.node_id,
    );
    let overlay_encap_unsupported = !context.capability.supports_encap
        && service_programs
            .iter()
            .any(|program| program.frontend.forwarding_mode == "cross_node_overlay");
    if overlay_encap_unsupported {
        warnings.push(
            "overlay cross-node service forwarding requested but node capability supports_encap=false; keeping shadow plan but marking services degraded"
                .to_string(),
        );
    }

    if context.desired.deletes.is_empty() {
        debug!(
            generation = %context.desired.generation,
            "southbound desired-state contains no explicit delete refs"
        );
    }

    let mut compiled_objects = BTreeMap::new();
    compiled_objects.insert("tenants".to_string(), context.desired.tenants.len());
    compiled_objects.insert("networks".to_string(), context.desired.networks.len());
    compiled_objects.insert("ports".to_string(), port_bindings.len());
    compiled_objects.insert(
        "security_groups".to_string(),
        context.desired.security_groups.len(),
    );
    compiled_objects.insert("route_tables".to_string(), compiled_route_tables.len());
    compiled_objects.insert("health_checks".to_string(), compiled_health_checks.len());
    compiled_objects.insert("backend_sets".to_string(), compiled_backend_sets.len());
    compiled_objects.insert("services".to_string(), compiled_services.len());
    if !context.desired.deletes.is_empty() {
        compiled_objects.insert("deletes".to_string(), context.desired.deletes.len());
    }

    let mut degraded_reasons = vec!["shadow_apply_only".to_string()];
    if !failed_objects.is_empty() {
        degraded_reasons.push("object_validation_failed".to_string());
    }
    if overlay_encap_unsupported {
        degraded_reasons.push("overlay_encap_unsupported".to_string());
    }
    degraded_reasons.sort();
    degraded_reasons.dedup();

    let port_failure_count = failed_objects
        .iter()
        .filter(|failure| failure.resource_kind == "port")
        .count();
    let route_failure_count = failed_objects
        .iter()
        .filter(|failure| failure.resource_kind == "route_table")
        .count();
    let service_failure_count = failed_objects
        .iter()
        .filter(|failure| {
            matches!(
                failure.resource_kind.as_str(),
                "health_check" | "backend_set" | "service"
            )
        })
        .count();

    let domain_summaries = vec![
        CompileDomainSummary {
            domain: "identity".to_string(),
            input_objects: context.desired.tenants.len() + context.desired.networks.len(),
            compiled_objects: tenant_ids.len() + network_by_id.len(),
            failed_objects: 0,
            status: "shadow_ready".to_string(),
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "ports".to_string(),
            input_objects: context.desired.ports.len(),
            compiled_objects: port_bindings.len(),
            failed_objects: port_failure_count,
            status: if port_failure_count == 0 {
                "shadow_ready".to_string()
            } else {
                "shadow_degraded".to_string()
            },
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "security".to_string(),
            input_objects: context.desired.security_groups.len(),
            compiled_objects: security_group_by_id.len(),
            failed_objects: 0,
            status: "shadow_ready".to_string(),
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "routes".to_string(),
            input_objects: context.desired.route_tables.len(),
            compiled_objects: compiled_route_tables.len(),
            failed_objects: route_failure_count,
            status: if route_failure_count == 0 {
                "shadow_ready".to_string()
            } else {
                "shadow_degraded".to_string()
            },
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "services".to_string(),
            input_objects: context.desired.health_checks.len()
                + context.desired.backend_sets.len()
                + context.desired.services.len(),
            compiled_objects: compiled_health_checks.len()
                + compiled_backend_sets.len()
                + compiled_services.len(),
            failed_objects: service_failure_count,
            status: if service_failure_count == 0 && !overlay_encap_unsupported {
                "shadow_ready".to_string()
            } else {
                "shadow_degraded".to_string()
            },
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "nat".to_string(),
            input_objects: 0,
            compiled_objects: 0,
            failed_objects: 0,
            status: "shadow_reserved".to_string(),
            shadow_apply_only: true,
        },
    ];

    let compiled_at = unix_timestamp_string();
    let compiled_state = CompiledNodeState {
        generation: context.desired.generation.clone(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        node_id: context.node_id.to_string(),
        capability_profile: capability_profile(context.capability),
        full_sync: context.desired.full_sync,
        issued_at: context.desired.issued_at.clone(),
        tenant_ids: tenant_ids.into_iter().collect(),
        network_ids: network_by_id.keys().cloned().collect(),
        security_group_ids: security_group_by_id.keys().cloned().collect(),
        port_bindings,
        route_tables: compiled_route_tables,
        health_checks: compiled_health_checks,
        backend_sets: compiled_backend_sets,
        services: compiled_services,
        service_programs,
        domain_summaries,
        warnings: warnings.clone(),
        degraded_reasons: degraded_reasons.clone(),
        compiled_at: compiled_at.clone(),
        shadow_apply_only: true,
    };
    let reconcile_plan = build_reconcile_plan(
        context.previous_compiled_state,
        &compiled_state,
        warnings.clone(),
    );
    let runtime_plan = build_runtime_plan(
        context.previous_compiled_state,
        &compiled_state,
        context.capability,
    );
    let runtime_inventory = build_runtime_inventory(
        context.previous_compiled_state,
        &compiled_state,
        &runtime_plan,
    );
    let runtime_inventory_diff =
        build_runtime_inventory_diff(context.previous_runtime_inventory, &runtime_inventory);
    let runtime_intent = build_runtime_intent(
        &compiled_state,
        &reconcile_plan,
        &runtime_inventory,
        &runtime_inventory_diff,
    );
    let runtime_execution_summary =
        build_runtime_execution_summary(&compiled_state, &reconcile_plan, &runtime_intent);
    let domain_statuses = runtime_execution_summary
        .domain_summaries
        .iter()
        .map(|summary| ApplyDomainStatus {
            domain: summary.domain.clone(),
            input_objects: summary.input_objects,
            compiled_objects: summary.compiled_objects,
            failed_objects: summary.failed_objects,
            status: summary.execution_status.clone(),
            shadow_apply_only: summary.shadow_apply_only,
        })
        .collect();

    let status = if failed_objects.is_empty() {
        "partial".to_string()
    } else if compiled_objects.values().copied().sum::<usize>() > failed_objects.len() {
        "partial".to_string()
    } else {
        "failed".to_string()
    };

    CompileOutcome {
        compiled_state,
        reconcile_plan,
        runtime_plan,
        runtime_inventory,
        runtime_inventory_diff,
        runtime_intent,
        runtime_execution_summary,
        apply_report: ApplyStatusReport {
            generation: context.desired.generation.clone(),
            status,
            applied_at: compiled_at,
            compiled_objects,
            domain_statuses,
            failed_objects,
            warnings,
            degraded_reasons,
        },
    }
}

fn build_service_programs(
    desired: &DesiredStateEnvelope,
    compiled_health_checks: &[CompiledHealthCheckView],
    compiled_backend_sets: &[CompiledBackendSetView],
    compiled_services: &[CompiledServiceView],
    node_id: &str,
) -> Vec<ServiceProgramIr> {
    let network_by_id = desired
        .networks
        .iter()
        .map(|network| (network.metadata.id.as_str(), network))
        .collect::<BTreeMap<_, _>>();
    let health_check_by_id = desired
        .health_checks
        .iter()
        .map(|health_check| (health_check.metadata.id.as_str(), health_check))
        .collect::<BTreeMap<_, _>>();
    let backend_set_by_id = desired
        .backend_sets
        .iter()
        .map(|backend_set| (backend_set.metadata.id.as_str(), backend_set))
        .collect::<BTreeMap<_, _>>();
    let service_by_id = desired
        .services
        .iter()
        .map(|service| (service.metadata.id.as_str(), service))
        .collect::<BTreeMap<_, _>>();
    let port_by_id = desired
        .ports
        .iter()
        .map(|port| (port.metadata.id.as_str(), port))
        .collect::<BTreeMap<_, _>>();
    let compiled_backend_set_by_id = compiled_backend_sets
        .iter()
        .map(|backend_set| (backend_set.backend_set_id.as_str(), backend_set))
        .collect::<BTreeMap<_, _>>();
    let compiled_health_check_ids = compiled_health_checks
        .iter()
        .map(|health_check| health_check.health_check_id.as_str())
        .collect::<BTreeSet<_>>();

    compiled_services
        .iter()
        .filter_map(|compiled_service| {
            let service = service_by_id.get(compiled_service.service_id.as_str())?;
            let backend_set_ir =
                compiled_service
                    .backend_set_id
                    .as_deref()
                    .and_then(|backend_set_id| {
                        let backend_set = backend_set_by_id.get(backend_set_id)?;
                        let compiled_backend_set =
                            compiled_backend_set_by_id.get(backend_set_id)?;

                        let backends = backend_set
                            .spec
                            .backends
                            .iter()
                            .map(|backend| build_backend_member_ir(backend, &port_by_id, node_id))
                            .collect::<Vec<_>>();

                        Some(BackendSetIr {
                            backend_set_id: backend_set.metadata.id.clone(),
                            tenant_id: backend_set.spec.tenant_id.clone(),
                            network_id: backend_set.spec.network_id.clone(),
                            selection_policy: backend_set.spec.policy.clone(),
                            health_check_id: backend_set.spec.health_check_id.clone(),
                            local_backend_count: compiled_backend_set.local_backend_count,
                            remote_backend_count: compiled_backend_set.remote_backend_count,
                            backends,
                            shadow_apply_only: true,
                        })
                    });

            let health_check_ir = backend_set_ir
                .as_ref()
                .and_then(|backend_set| backend_set.health_check_id.as_deref())
                .and_then(|health_check_id| {
                    let health_check = health_check_by_id.get(health_check_id)?;
                    if !compiled_health_check_ids.contains(health_check_id) {
                        return None;
                    }
                    Some(HealthCheckIr {
                        health_check_id: health_check.metadata.id.clone(),
                        tenant_id: health_check.spec.tenant_id.clone(),
                        network_id: health_check.spec.network_id.clone(),
                        probe_protocol: health_check.spec.protocol.clone(),
                        interval_seconds: health_check.spec.interval_seconds,
                        timeout_seconds: health_check.spec.timeout_seconds,
                        healthy_threshold: health_check.spec.healthy_threshold,
                        unhealthy_threshold: health_check.spec.unhealthy_threshold,
                        target_port: health_check.spec.target_port,
                        // Keep the shadow IR bounded; the full probe payload can stay in desired state.
                        has_request_template: health_check.spec.request_template.is_some(),
                        shadow_apply_only: true,
                    })
                });

            let node_local_forwarding = backend_set_ir
                .as_ref()
                .map(|backend_set| backend_set.local_backend_count > 0)
                .unwrap_or(false);
            let cross_node_forwarding = backend_set_ir
                .as_ref()
                .map(|backend_set| backend_set.remote_backend_count > 0)
                .unwrap_or(false);
            let route_mode = network_by_id
                .get(compiled_service.network_id.as_str())
                .map(|network| network.spec.route_mode.clone())
                .unwrap_or_else(default_service_route_mode);
            let forwarding_mode =
                derive_service_forwarding_mode(&route_mode, cross_node_forwarding);

            Some(ServiceProgramIr {
                service_id: compiled_service.service_id.clone(),
                backend_set_id: compiled_service.backend_set_id.clone(),
                health_check_id: health_check_ir
                    .as_ref()
                    .map(|health_check| health_check.health_check_id.clone()),
                frontend: ServiceFrontendIr {
                    service_id: compiled_service.service_id.clone(),
                    tenant_id: compiled_service.tenant_id.clone(),
                    network_id: compiled_service.network_id.clone(),
                    route_mode,
                    vip: compiled_service.vip.clone(),
                    protocol: compiled_service.protocol.clone(),
                    lb_policy: service.spec.lb_policy.clone(),
                    session_affinity: service.spec.session_affinity.clone(),
                    exposure_type: compiled_service.exposure_type.clone(),
                    forwarding_mode,
                    listener_ports: service
                        .spec
                        .ports
                        .iter()
                        .map(|port| ServiceFrontendPortIr {
                            name: port.name.clone(),
                            service_port: port.port,
                            target_port: port.target_port,
                        })
                        .collect(),
                    node_local_forwarding,
                    cross_node_forwarding,
                    shadow_apply_only: true,
                },
                backend_set: backend_set_ir,
                health_check: health_check_ir,
                shadow_apply_only: true,
            })
        })
        .collect()
}

fn build_backend_member_ir(
    backend: &aria_api::BackendTargetSpec,
    port_by_id: &BTreeMap<&str, &aria_api::PortResource>,
    node_id: &str,
) -> BackendMemberIr {
    let mut resolved_ip_hint = backend.ip.clone();
    let mut resolved_locality = backend
        .locality
        .clone()
        .unwrap_or_else(|| "remote".to_string());
    let mut resolution = if backend.target_ref.is_some() {
        "shadow_remote_reference".to_string()
    } else if backend.ip.is_some() {
        "direct_ip".to_string()
    } else {
        "literal_target".to_string()
    };
    let mut resolved_node_id = backend.node_id.clone();

    if let Some(target_ref) = backend.target_ref.as_deref() {
        if let Some(port) = port_by_id.get(target_ref) {
            let bound_node_id = port.spec.node_id.as_deref().unwrap_or(node_id);
            resolved_locality = if bound_node_id == node_id {
                "local".to_string()
            } else {
                "remote".to_string()
            };
            resolved_node_id = port
                .spec
                .node_id
                .clone()
                .or_else(|| backend.node_id.clone());
            resolved_ip_hint = resolved_ip_hint
                .or_else(|| port.spec.fixed_ips.first().map(|ip| strip_cidr_suffix(ip)));
            resolution = "resolved_from_port_ref".to_string();
        }
    } else if backend.node_id.as_deref() == Some(node_id) {
        resolved_locality = "local".to_string();
    }

    let forwarding_scope = if resolved_locality == "local" {
        "node_local".to_string()
    } else {
        "cross_node".to_string()
    };

    BackendMemberIr {
        backend_id: backend.id.clone(),
        target_type: backend.target_type.clone(),
        target_ref: backend.target_ref.clone(),
        resolved_ip_hint,
        service_port: backend.port,
        weight: backend.weight,
        admin_state: backend
            .admin_state
            .clone()
            .unwrap_or_else(|| "enabled".to_string()),
        node_id: resolved_node_id,
        declared_locality: backend.locality.clone(),
        resolved_locality,
        forwarding_scope,
        resolution,
        shadow_apply_only: true,
    }
}

fn strip_cidr_suffix(value: &str) -> String {
    value.split('/').next().unwrap_or(value).to_string()
}

fn default_service_route_mode() -> String {
    "native".to_string()
}

fn default_service_forwarding_mode() -> String {
    "node_local_only".to_string()
}

fn derive_service_forwarding_mode(route_mode: &str, cross_node_forwarding: bool) -> String {
    if !cross_node_forwarding {
        return default_service_forwarding_mode();
    }

    match route_mode {
        "overlay" => "cross_node_overlay".to_string(),
        "hybrid" => "cross_node_hybrid".to_string(),
        _ => "cross_node_native".to_string(),
    }
}

fn build_reconcile_plan(
    previous_state: Option<&CompiledNodeState>,
    next_state: &CompiledNodeState,
    warnings: Vec<String>,
) -> ReconcilePlan {
    let previous_port_ids = previous_state
        .map(|state| {
            state
                .port_bindings
                .iter()
                .map(|binding| binding.port_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_port_ids = next_state
        .port_bindings
        .iter()
        .map(|binding| binding.port_id.as_str())
        .collect::<BTreeSet<_>>();

    let previous_route_table_ids = previous_state
        .map(|state| {
            state
                .route_tables
                .iter()
                .map(|route_table| route_table.route_table_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_route_table_ids = next_state
        .route_tables
        .iter()
        .map(|route_table| route_table.route_table_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_health_check_ids = previous_state
        .map(|state| {
            state
                .health_checks
                .iter()
                .map(|health_check| health_check.health_check_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_health_check_ids = next_state
        .health_checks
        .iter()
        .map(|health_check| health_check.health_check_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_backend_set_ids = previous_state
        .map(|state| {
            state
                .backend_sets
                .iter()
                .map(|backend_set| backend_set.backend_set_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_backend_set_ids = next_state
        .backend_sets
        .iter()
        .map(|backend_set| backend_set.backend_set_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_service_ids = previous_state
        .map(|state| {
            state
                .services
                .iter()
                .map(|service| service.service_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_service_ids = next_state
        .services
        .iter()
        .map(|service| service.service_id.as_str())
        .collect::<BTreeSet<_>>();

    let ports_removed = previous_port_ids.difference(&next_port_ids).count();
    let route_tables_removed = previous_route_table_ids
        .difference(&next_route_table_ids)
        .count();
    let health_checks_removed = previous_health_check_ids
        .difference(&next_health_check_ids)
        .count();
    let backend_sets_removed = previous_backend_set_ids
        .difference(&next_backend_set_ids)
        .count();
    let services_removed = previous_service_ids.difference(&next_service_ids).count();

    let previous_generation = previous_state.map(|state| state.generation.clone());
    let full_reconcile = previous_state
        .map(|state| {
            state.generation != next_state.generation
                || state.capability_profile != next_state.capability_profile
                || state.full_sync
                || next_state.full_sync
        })
        .unwrap_or(true);

    let mut changed_kinds = Vec::new();
    if previous_state.is_none()
        || next_state.port_bindings.len()
            != previous_state
                .map(|state| state.port_bindings.len())
                .unwrap_or(0)
        || ports_removed > 0
    {
        changed_kinds.push("ports".to_string());
    }
    if previous_state.is_none()
        || next_state.route_tables.len()
            != previous_state
                .map(|state| state.route_tables.len())
                .unwrap_or(0)
        || route_tables_removed > 0
    {
        changed_kinds.push("route_tables".to_string());
    }
    if previous_state.is_none()
        || next_state.security_group_ids
            != previous_state
                .map(|state| state.security_group_ids.clone())
                .unwrap_or_default()
    {
        changed_kinds.push("security_groups".to_string());
    }
    if previous_state.is_none()
        || next_state.network_ids
            != previous_state
                .map(|state| state.network_ids.clone())
                .unwrap_or_default()
    {
        changed_kinds.push("networks".to_string());
    }
    if previous_state.is_none()
        || next_state.tenant_ids
            != previous_state
                .map(|state| state.tenant_ids.clone())
                .unwrap_or_default()
    {
        changed_kinds.push("tenants".to_string());
    }
    if previous_state.is_none()
        || next_state.health_checks.len()
            != previous_state
                .map(|state| state.health_checks.len())
                .unwrap_or(0)
        || health_checks_removed > 0
    {
        changed_kinds.push("health_checks".to_string());
    }
    if previous_state.is_none()
        || next_state.backend_sets.len()
            != previous_state
                .map(|state| state.backend_sets.len())
                .unwrap_or(0)
        || backend_sets_removed > 0
    {
        changed_kinds.push("backend_sets".to_string());
    }
    if previous_state.is_none()
        || next_state.services.len()
            != previous_state
                .map(|state| state.services.len())
                .unwrap_or(0)
        || services_removed > 0
    {
        changed_kinds.push("services".to_string());
    }

    let mut actions = Vec::new();
    if full_reconcile {
        actions.push(ReconcileAction {
            domain: "core".to_string(),
            operation: "full_shadow_reconcile".to_string(),
            object_count: next_state.port_bindings.len()
                + next_state.route_tables.len()
                + next_state.health_checks.len()
                + next_state.backend_sets.len()
                + next_state.services.len(),
        });
    }
    if !next_state.port_bindings.is_empty() {
        actions.push(ReconcileAction {
            domain: "ports".to_string(),
            operation: "refresh_shadow_bindings".to_string(),
            object_count: next_state.port_bindings.len(),
        });
    }
    if ports_removed > 0 {
        actions.push(ReconcileAction {
            domain: "ports".to_string(),
            operation: "cleanup_shadow_bindings".to_string(),
            object_count: ports_removed,
        });
    }
    if !next_state.route_tables.is_empty() {
        actions.push(ReconcileAction {
            domain: "routes".to_string(),
            operation: "refresh_shadow_routes".to_string(),
            object_count: next_state.route_tables.len(),
        });
    }
    if route_tables_removed > 0 {
        actions.push(ReconcileAction {
            domain: "routes".to_string(),
            operation: "cleanup_shadow_routes".to_string(),
            object_count: route_tables_removed,
        });
    }
    if !next_state.security_group_ids.is_empty() {
        actions.push(ReconcileAction {
            domain: "security".to_string(),
            operation: "refresh_shadow_security".to_string(),
            object_count: next_state.security_group_ids.len(),
        });
    }
    let service_shadow_count =
        next_state.health_checks.len() + next_state.backend_sets.len() + next_state.services.len();
    if service_shadow_count > 0 {
        actions.push(ReconcileAction {
            domain: "services".to_string(),
            operation: "refresh_shadow_services".to_string(),
            object_count: service_shadow_count,
        });
    }
    let services_cleanup = health_checks_removed + backend_sets_removed + services_removed;
    if services_cleanup > 0 {
        actions.push(ReconcileAction {
            domain: "services".to_string(),
            operation: "cleanup_shadow_services".to_string(),
            object_count: services_cleanup,
        });
    }

    ReconcilePlan {
        generation: next_state.generation.clone(),
        previous_generation,
        compiled_at: next_state.compiled_at.clone(),
        full_reconcile,
        changed_kinds,
        actions,
        warnings,
        shadow_apply_only: true,
    }
}

fn build_runtime_plan(
    previous_state: Option<&CompiledNodeState>,
    next_state: &CompiledNodeState,
    capability: &NodeCapability,
) -> RuntimePlan {
    let previous_port_ids = previous_state
        .map(|state| {
            state
                .port_bindings
                .iter()
                .map(|binding| binding.port_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_port_ids = next_state
        .port_bindings
        .iter()
        .map(|binding| binding.port_id.as_str())
        .collect::<BTreeSet<_>>();

    let previous_route_table_ids = previous_state
        .map(|state| {
            state
                .route_tables
                .iter()
                .map(|route_table| route_table.route_table_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_route_table_ids = next_state
        .route_tables
        .iter()
        .map(|route_table| route_table.route_table_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_health_check_ids = previous_state
        .map(|state| {
            state
                .health_checks
                .iter()
                .map(|health_check| health_check.health_check_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_health_check_ids = next_state
        .health_checks
        .iter()
        .map(|health_check| health_check.health_check_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_backend_set_ids = previous_state
        .map(|state| {
            state
                .backend_sets
                .iter()
                .map(|backend_set| backend_set.backend_set_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_backend_set_ids = next_state
        .backend_sets
        .iter()
        .map(|backend_set| backend_set.backend_set_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_service_ids = previous_state
        .map(|state| {
            state
                .services
                .iter()
                .map(|service| service.service_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_service_ids = next_state
        .services
        .iter()
        .map(|service| service.service_id.as_str())
        .collect::<BTreeSet<_>>();

    let ports_removed = previous_port_ids.difference(&next_port_ids).count();
    let route_tables_removed = previous_route_table_ids
        .difference(&next_route_table_ids)
        .count();
    let health_checks_removed = previous_health_check_ids
        .difference(&next_health_check_ids)
        .count();
    let backend_sets_removed = previous_backend_set_ids
        .difference(&next_backend_set_ids)
        .count();
    let services_removed = previous_service_ids.difference(&next_service_ids).count();
    let previous_service_listener_count = previous_state
        .map(total_service_listener_ports)
        .unwrap_or_default();
    let next_service_listener_count = total_service_listener_ports(next_state);
    let previous_backend_member_count = previous_state
        .map(total_service_backend_members)
        .unwrap_or_default();
    let next_backend_member_count = total_service_backend_members(next_state);
    let previous_forwarding_projection_count = previous_state
        .map(total_service_forwarding_projections)
        .unwrap_or_default();
    let next_forwarding_projection_count = total_service_forwarding_projections(next_state);
    let previous_service_revnat_count = previous_state
        .map(total_service_revnat_entries)
        .unwrap_or_default();
    let next_service_revnat_count = total_service_revnat_entries(next_state);
    let previous_service_affinity_count = previous_state
        .map(total_affinity_service_programs)
        .unwrap_or_default();
    let next_service_affinity_count = total_affinity_service_programs(next_state);
    let previous_service_maglev_count = previous_state
        .map(total_maglev_service_programs)
        .unwrap_or_default();
    let next_service_maglev_count = total_maglev_service_programs(next_state);

    let mut bindings = Vec::new();
    if capability.supports_tc && !next_state.port_bindings.is_empty() {
        bindings.push(AttachBindingPlan {
            hook_family: "tc_ingress".to_string(),
            scope: "port-bindings".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.port_bindings.len(),
        });
        bindings.push(AttachBindingPlan {
            hook_family: "tc_egress".to_string(),
            scope: "port-bindings".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.port_bindings.len(),
        });
    }
    if capability.supports_xdp && !next_state.port_bindings.is_empty() {
        bindings.push(AttachBindingPlan {
            hook_family: "xdp".to_string(),
            scope: "anti-spoof-fastpath".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.port_bindings.len(),
        });
    }
    if capability.supports_tc && !next_state.route_tables.is_empty() {
        bindings.push(AttachBindingPlan {
            hook_family: "tc_egress".to_string(),
            scope: "route-tables".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.route_tables.len(),
        });
    }
    if capability.supports_tc && ports_removed > 0 {
        bindings.push(AttachBindingPlan {
            hook_family: "tc".to_string(),
            scope: "port-bindings".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: ports_removed,
        });
    }
    if capability.supports_tc && route_tables_removed > 0 {
        bindings.push(AttachBindingPlan {
            hook_family: "tc".to_string(),
            scope: "route-tables".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: route_tables_removed,
        });
    }

    let mut required_hooks = bindings
        .iter()
        .map(|binding| binding.hook_family.clone())
        .collect::<Vec<_>>();
    required_hooks.sort();
    required_hooks.dedup();

    let mut required_qdisc = Vec::new();
    if bindings
        .iter()
        .any(|binding| binding.hook_family.starts_with("tc"))
    {
        required_qdisc.push("clsact".to_string());
    }

    let attach_plan = AttachPlan {
        generation: next_state.generation.clone(),
        compiled_at: next_state.compiled_at.clone(),
        required_hooks,
        bindings,
        required_qdisc,
        shadow_apply_only: true,
    };

    let mut entries = Vec::new();
    if !next_state.tenant_ids.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "tenant_index".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.tenant_ids.len(),
        });
    }
    if !next_state.network_ids.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "network_index".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.network_ids.len(),
        });
    }
    if !next_state.security_group_ids.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "security_program".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.security_group_ids.len(),
        });
    }
    if !next_state.port_bindings.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "port_bindings".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.port_bindings.len(),
        });
    }
    if ports_removed > 0 {
        entries.push(MapPlanEntry {
            map_family: "port_bindings".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: ports_removed,
        });
    }
    if !next_state.route_tables.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "route_program".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.route_tables.len(),
        });
    }
    if route_tables_removed > 0 {
        entries.push(MapPlanEntry {
            map_family: "route_program".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: route_tables_removed,
        });
    }
    if !next_state.health_checks.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "health_check_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.health_checks.len(),
        });
    }
    if !next_state.backend_sets.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "backend_set_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.backend_sets.len(),
        });
    }
    if !next_state.services.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "service_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.services.len(),
        });
    }
    if next_service_listener_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_frontend_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_listener_count,
        });
    }
    if next_backend_member_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "backend_member_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_backend_member_count,
        });
    }
    if next_forwarding_projection_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_forwarding_projection".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_forwarding_projection_count,
        });
    }
    if next_service_revnat_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_revnat_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_revnat_count,
        });
    }
    if next_service_affinity_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_affinity_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_affinity_count,
        });
    }
    if next_service_maglev_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_maglev_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_maglev_count,
        });
    }
    if health_checks_removed > 0 {
        entries.push(MapPlanEntry {
            map_family: "health_check_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: health_checks_removed,
        });
    }
    if backend_sets_removed > 0 {
        entries.push(MapPlanEntry {
            map_family: "backend_set_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: backend_sets_removed,
        });
    }
    if services_removed > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: services_removed,
        });
    }
    if previous_service_listener_count > next_service_listener_count {
        entries.push(MapPlanEntry {
            map_family: "service_frontend_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_listener_count - next_service_listener_count,
        });
    }
    if previous_backend_member_count > next_backend_member_count {
        entries.push(MapPlanEntry {
            map_family: "backend_member_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_backend_member_count - next_backend_member_count,
        });
    }
    if previous_forwarding_projection_count > next_forwarding_projection_count {
        entries.push(MapPlanEntry {
            map_family: "service_forwarding_projection".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_forwarding_projection_count - next_forwarding_projection_count,
        });
    }
    if previous_service_revnat_count > next_service_revnat_count {
        entries.push(MapPlanEntry {
            map_family: "service_revnat_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_revnat_count - next_service_revnat_count,
        });
    }
    if previous_service_affinity_count > next_service_affinity_count {
        entries.push(MapPlanEntry {
            map_family: "service_affinity_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_affinity_count - next_service_affinity_count,
        });
    }
    if previous_service_maglev_count > next_service_maglev_count {
        entries.push(MapPlanEntry {
            map_family: "service_maglev_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_maglev_count - next_service_maglev_count,
        });
    }

    let map_plan = MapPlan {
        generation: next_state.generation.clone(),
        compiled_at: next_state.compiled_at.clone(),
        entries,
        shadow_apply_only: true,
    };

    RuntimePlan {
        generation: next_state.generation.clone(),
        compiled_at: next_state.compiled_at.clone(),
        attach_plan,
        map_plan,
        shadow_apply_only: true,
    }
}

fn build_runtime_inventory(
    previous_state: Option<&CompiledNodeState>,
    compiled_state: &CompiledNodeState,
    runtime_plan: &RuntimePlan,
) -> RuntimeInventory {
    let attach_inventory = runtime_plan
        .attach_plan
        .bindings
        .iter()
        .map(|binding| RuntimeInventoryAttach {
            domain: inventory_domain_from_scope(&binding.scope),
            hook_family: binding.hook_family.clone(),
            scope: binding.scope.clone(),
            operation: binding.operation.clone(),
            object_count: binding.object_count,
        })
        .collect::<Vec<_>>();

    let map_inventory = runtime_plan
        .map_plan
        .entries
        .iter()
        .map(|entry| RuntimeInventoryMapEntry {
            domain: inventory_domain_from_map_family(&entry.map_family),
            map_family: entry.map_family.clone(),
            operation: entry.operation.clone(),
            object_count: entry.object_count,
        })
        .collect::<Vec<_>>();

    let mut attach_counts = BTreeMap::new();
    for entry in &attach_inventory {
        *attach_counts.entry(entry.domain.clone()).or_insert(0usize) += 1;
    }

    let mut map_counts = BTreeMap::new();
    for entry in &map_inventory {
        *map_counts.entry(entry.domain.clone()).or_insert(0usize) += 1;
    }

    let domain_inventory = compiled_state
        .domain_summaries
        .iter()
        .map(|summary| RuntimeInventoryDomainSummary {
            domain: summary.domain.clone(),
            compiled_objects: summary.compiled_objects,
            failed_objects: summary.failed_objects,
            attach_operations: attach_counts.get(&summary.domain).copied().unwrap_or(0),
            map_operations: map_counts.get(&summary.domain).copied().unwrap_or(0),
            status: if summary.status == "shadow_reserved" {
                "shadow_inventory_reserved".to_string()
            } else if summary.failed_objects == 0 {
                "shadow_inventory_ready".to_string()
            } else {
                "shadow_inventory_degraded".to_string()
            },
            shadow_apply_only: true,
        })
        .collect::<Vec<_>>();

    RuntimeInventory {
        generation: runtime_plan.generation.clone(),
        previous_generation: previous_state
            .map(|state| state.generation.clone())
            .filter(|generation| generation != &runtime_plan.generation),
        compiled_at: runtime_plan.compiled_at.clone(),
        observed_at: unix_timestamp_string(),
        compiler_version: compiled_state.compiler_version.clone(),
        required_hooks: runtime_plan.attach_plan.required_hooks.clone(),
        required_qdisc: runtime_plan.attach_plan.required_qdisc.clone(),
        attach_inventory,
        map_inventory,
        domain_inventory,
        shadow_apply_only: true,
    }
}

fn build_runtime_inventory_diff(
    previous_inventory: Option<&RuntimeInventory>,
    current_inventory: &RuntimeInventory,
) -> RuntimeInventoryDiff {
    let mut attach_deltas = Vec::new();
    let mut changed_domains = BTreeSet::new();
    let mut has_cleanup = false;

    let previous_attach = previous_inventory
        .map(|inventory| {
            inventory
                .attach_inventory
                .iter()
                .map(|entry| {
                    (
                        (
                            entry.domain.clone(),
                            entry.hook_family.clone(),
                            entry.scope.clone(),
                            entry.operation.clone(),
                        ),
                        entry.object_count,
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let current_attach = current_inventory
        .attach_inventory
        .iter()
        .map(|entry| {
            (
                (
                    entry.domain.clone(),
                    entry.hook_family.clone(),
                    entry.scope.clone(),
                    entry.operation.clone(),
                ),
                entry.object_count,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let attach_keys = previous_attach
        .keys()
        .cloned()
        .chain(current_attach.keys().cloned())
        .collect::<BTreeSet<_>>();
    for (domain, hook_family, scope, operation) in attach_keys {
        let previous_object_count = previous_attach
            .get(&(
                domain.clone(),
                hook_family.clone(),
                scope.clone(),
                operation.clone(),
            ))
            .copied()
            .unwrap_or(0);
        let current_object_count = current_attach
            .get(&(
                domain.clone(),
                hook_family.clone(),
                scope.clone(),
                operation.clone(),
            ))
            .copied()
            .unwrap_or(0);
        if previous_object_count == current_object_count {
            continue;
        }
        let change_type = if previous_object_count == 0 {
            "added".to_string()
        } else if current_object_count == 0 {
            "removed".to_string()
        } else {
            "updated".to_string()
        };
        if operation.contains("cleanup") || change_type == "removed" {
            has_cleanup = true;
        }
        changed_domains.insert(domain.clone());
        attach_deltas.push(RuntimeInventoryAttachDelta {
            domain,
            hook_family,
            scope,
            operation,
            previous_object_count,
            current_object_count,
            change_type,
        });
    }

    let previous_maps = previous_inventory
        .map(|inventory| {
            inventory
                .map_inventory
                .iter()
                .map(|entry| {
                    (
                        (
                            entry.domain.clone(),
                            entry.map_family.clone(),
                            entry.operation.clone(),
                        ),
                        entry.object_count,
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let current_maps = current_inventory
        .map_inventory
        .iter()
        .map(|entry| {
            (
                (
                    entry.domain.clone(),
                    entry.map_family.clone(),
                    entry.operation.clone(),
                ),
                entry.object_count,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let map_keys = previous_maps
        .keys()
        .cloned()
        .chain(current_maps.keys().cloned())
        .collect::<BTreeSet<_>>();
    let mut map_deltas = Vec::new();
    for (domain, map_family, operation) in map_keys {
        let previous_object_count = previous_maps
            .get(&(domain.clone(), map_family.clone(), operation.clone()))
            .copied()
            .unwrap_or(0);
        let current_object_count = current_maps
            .get(&(domain.clone(), map_family.clone(), operation.clone()))
            .copied()
            .unwrap_or(0);
        if previous_object_count == current_object_count {
            continue;
        }
        let change_type = if previous_object_count == 0 {
            "added".to_string()
        } else if current_object_count == 0 {
            "removed".to_string()
        } else {
            "updated".to_string()
        };
        if operation.contains("cleanup") || change_type == "removed" {
            has_cleanup = true;
        }
        changed_domains.insert(domain.clone());
        map_deltas.push(RuntimeInventoryMapDelta {
            domain,
            map_family,
            operation,
            previous_object_count,
            current_object_count,
            change_type,
        });
    }

    let previous_domains = previous_inventory
        .map(|inventory| {
            inventory
                .domain_inventory
                .iter()
                .map(|domain| (domain.domain.clone(), domain))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let current_domains = current_inventory
        .domain_inventory
        .iter()
        .map(|domain| (domain.domain.clone(), domain))
        .collect::<BTreeMap<_, _>>();
    let domain_keys = previous_domains
        .keys()
        .cloned()
        .chain(current_domains.keys().cloned())
        .collect::<BTreeSet<_>>();
    let mut domain_deltas = Vec::new();
    for domain in domain_keys {
        let previous = previous_domains.get(&domain);
        let current = current_domains.get(&domain);
        let previous_attach_operations = previous
            .map(|summary| summary.attach_operations)
            .unwrap_or(0);
        let current_attach_operations = current
            .map(|summary| summary.attach_operations)
            .unwrap_or(0);
        let previous_map_operations = previous.map(|summary| summary.map_operations).unwrap_or(0);
        let current_map_operations = current.map(|summary| summary.map_operations).unwrap_or(0);
        let previous_compiled_objects = previous
            .map(|summary| summary.compiled_objects)
            .unwrap_or(0);
        let current_compiled_objects = current.map(|summary| summary.compiled_objects).unwrap_or(0);

        if previous_attach_operations == current_attach_operations
            && previous_map_operations == current_map_operations
            && previous_compiled_objects == current_compiled_objects
        {
            continue;
        }

        let change_type = if previous.is_none() {
            "added".to_string()
        } else if current.is_none() {
            "removed".to_string()
        } else {
            "updated".to_string()
        };
        if change_type == "removed" {
            has_cleanup = true;
        }
        changed_domains.insert(domain.clone());
        domain_deltas.push(RuntimeInventoryDomainDelta {
            domain,
            previous_attach_operations,
            current_attach_operations,
            previous_map_operations,
            current_map_operations,
            previous_compiled_objects,
            current_compiled_objects,
            change_type,
        });
    }

    RuntimeInventoryDiff {
        generation: current_inventory.generation.clone(),
        previous_generation: previous_inventory
            .map(|inventory| inventory.generation.clone())
            .filter(|generation| generation != &current_inventory.generation),
        observed_at: unix_timestamp_string(),
        changed_domains: changed_domains.into_iter().collect(),
        attach_deltas,
        map_deltas,
        domain_deltas,
        has_cleanup,
        shadow_apply_only: true,
    }
}

fn build_runtime_intent(
    compiled_state: &CompiledNodeState,
    reconcile_plan: &ReconcilePlan,
    runtime_inventory: &RuntimeInventory,
    runtime_inventory_diff: &RuntimeInventoryDiff,
) -> RuntimeIntent {
    let changed_domains = runtime_inventory_diff
        .changed_domains
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let cleanup_domains = runtime_inventory_diff
        .attach_deltas
        .iter()
        .filter(|delta| delta.change_type == "removed" || delta.operation.contains("cleanup"))
        .map(|delta| delta.domain.clone())
        .chain(
            runtime_inventory_diff
                .map_deltas
                .iter()
                .filter(|delta| {
                    delta.change_type == "removed" || delta.operation.contains("cleanup")
                })
                .map(|delta| delta.domain.clone()),
        )
        .collect::<BTreeSet<_>>();

    let reconcile_domains = reconcile_plan
        .actions
        .iter()
        .map(|action| action.domain.as_str())
        .collect::<BTreeSet<_>>();

    let intents = runtime_inventory
        .domain_inventory
        .iter()
        .map(|domain_summary| {
            let changed = changed_domains.contains(&domain_summary.domain)
                || reconcile_domains.contains(domain_summary.domain.as_str())
                || (reconcile_plan.full_reconcile && reconcile_domains.contains("core"));
            let requires_cleanup = cleanup_domains.contains(&domain_summary.domain);
            let reserved_nat_domain = domain_summary.domain == "nat"
                && domain_summary.compiled_objects == 0
                && domain_summary.attach_operations == 0
                && domain_summary.map_operations == 0
                && domain_summary.failed_objects == 0;
            let full_reconcile = !reserved_nat_domain
                && reconcile_plan.full_reconcile
                && (changed || reconcile_domains.contains("core"));
            let desired_action = if reserved_nat_domain {
                "reserve_shadow".to_string()
            } else if full_reconcile {
                "full_shadow_reconcile".to_string()
            } else if requires_cleanup {
                "cleanup_shadow".to_string()
            } else if changed {
                "refresh_shadow".to_string()
            } else {
                "maintain_shadow".to_string()
            };
            let reason = if reserved_nat_domain {
                "reserved_for_snat_dnat_floating_ip".to_string()
            } else if full_reconcile {
                "generation_or_capability_shift".to_string()
            } else if requires_cleanup {
                "runtime_cleanup_required".to_string()
            } else if changed {
                "runtime_inventory_delta".to_string()
            } else {
                "shadow_inventory_stable".to_string()
            };

            RuntimeDomainIntent {
                domain: domain_summary.domain.clone(),
                desired_action,
                reason,
                full_reconcile,
                requires_cleanup,
                changed,
                attach_operations: domain_summary.attach_operations,
                map_operations: domain_summary.map_operations,
                compiled_objects: domain_summary.compiled_objects,
                failed_objects: domain_summary.failed_objects,
                shadow_apply_only: true,
            }
        })
        .collect::<Vec<_>>();

    let service_intent = intents
        .iter()
        .find(|intent| intent.domain == "services")
        .map(|intent| ServiceRuntimeIntentSummary {
            service_count: compiled_state.service_programs.len(),
            frontend_listener_count: total_service_listener_ports(compiled_state),
            backend_member_count: total_service_backend_members(compiled_state),
            forwarding_projection_count: total_service_forwarding_projections(compiled_state),
            node_local_service_count: total_node_local_service_programs(compiled_state),
            cross_node_service_count: total_cross_node_service_programs(compiled_state),
            cross_node_native_service_count: total_cross_node_native_service_programs(
                compiled_state,
            ),
            cross_node_overlay_service_count: total_cross_node_overlay_service_programs(
                compiled_state,
            ),
            cross_node_hybrid_service_count: total_cross_node_hybrid_service_programs(
                compiled_state,
            ),
            revnat_reservation_count: total_service_revnat_entries(compiled_state),
            affinity_reservation_count: total_affinity_service_programs(compiled_state),
            maglev_reservation_count: total_maglev_service_programs(compiled_state),
            desired_action: intent.desired_action.clone(),
            requires_cleanup: intent.requires_cleanup,
            changed: intent.changed,
            shadow_apply_only: true,
        });

    RuntimeIntent {
        generation: runtime_inventory.generation.clone(),
        previous_generation: runtime_inventory
            .previous_generation
            .clone()
            .or_else(|| runtime_inventory_diff.previous_generation.clone()),
        compiled_at: runtime_inventory.compiled_at.clone(),
        observed_at: unix_timestamp_string(),
        changed_domains: changed_domains.into_iter().collect(),
        intents,
        service_intent,
        shadow_apply_only: true,
    }
}

fn build_runtime_execution_summary(
    compiled_state: &CompiledNodeState,
    reconcile_plan: &ReconcilePlan,
    runtime_intent: &RuntimeIntent,
) -> RuntimeExecutionSummary {
    let compile_summary_by_domain = compiled_state
        .domain_summaries
        .iter()
        .map(|summary| (summary.domain.as_str(), summary))
        .collect::<BTreeMap<_, _>>();

    let reconcile_domains = reconcile_plan
        .actions
        .iter()
        .map(|action| action.domain.as_str())
        .collect::<BTreeSet<_>>();

    let domain_summaries = runtime_intent
        .intents
        .iter()
        .map(|intent| {
            let compile_summary = compile_summary_by_domain.get(intent.domain.as_str()).copied();
            let input_objects = compile_summary.map(|summary| summary.input_objects).unwrap_or(0);
            let failed_objects = compile_summary
                .map(|summary| summary.failed_objects)
                .unwrap_or(intent.failed_objects);
            let overlay_encap_unsupported =
                intent.domain == "services"
                    && compiled_state
                        .degraded_reasons
                        .iter()
                        .any(|reason| reason == "overlay_encap_unsupported");
            let execution_status = if intent.desired_action == "reserve_shadow" {
                "shadow_reserved".to_string()
            } else if overlay_encap_unsupported {
                "shadow_execute_degraded".to_string()
            } else if failed_objects > 0 {
                "shadow_execute_degraded".to_string()
            } else if intent.requires_cleanup {
                "shadow_execute_cleanup".to_string()
            } else if intent.changed || reconcile_domains.contains(intent.domain.as_str()) {
                "shadow_execute_planned".to_string()
            } else {
                "shadow_execute_stable".to_string()
            };

            let mut degraded_reasons = Vec::new();
            if failed_objects > 0 {
                degraded_reasons.push("compile_or_inventory_failures_present".to_string());
            }
            if overlay_encap_unsupported {
                degraded_reasons.push("overlay_encap_unsupported".to_string());
            }
            if intent.requires_cleanup {
                degraded_reasons.push("cleanup_required".to_string());
            }
            if reconcile_plan.full_reconcile && intent.full_reconcile {
                degraded_reasons.push("full_reconcile_required".to_string());
            }

            let mut warnings = Vec::new();
            if intent.domain == "services" && intent.changed && intent.compiled_objects > 0 {
                warnings.push(
                    "services shadow execution planned; node-local and cross-node LB datapath not materialized yet"
                        .to_string(),
                );
            }
            if intent.domain == "nat" && intent.desired_action == "reserve_shadow" {
                warnings.push(
                    "nat shadow reservation present; snat/dnat/floating-ip datapath not materialized yet"
                        .to_string(),
                );
            }

            RuntimeExecutionDomainSummary {
                domain: intent.domain.clone(),
                planned_action: intent.desired_action.clone(),
                execution_status,
                requires_cleanup: intent.requires_cleanup,
                changed: intent.changed,
                input_objects,
                compiled_objects: intent.compiled_objects,
                failed_objects,
                warnings,
                degraded_reasons,
                shadow_apply_only: intent.shadow_apply_only,
            }
        })
        .collect::<Vec<_>>();

    let service_execution = runtime_intent.service_intent.as_ref().map(|service_intent| {
        let execution_status = domain_summaries
            .iter()
            .find(|summary| summary.domain == "services")
            .map(|summary| summary.execution_status.clone())
            .unwrap_or_else(|| "shadow_execute_stable".to_string());

        let mut warnings = Vec::new();
        if service_intent.node_local_service_count > 0 {
            warnings.push(
                "node-local service forwarding is still shadow planned; lb datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.cross_node_service_count > 0 {
            warnings.push(
                "cross-node service forwarding is still shadow planned; handoff datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.cross_node_native_service_count > 0 {
            warnings.push(
                "native cross-node service forwarding is still shadow planned; route handoff datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.cross_node_overlay_service_count > 0 {
            if compiled_state
                .degraded_reasons
                .iter()
                .any(|reason| reason == "overlay_encap_unsupported")
            {
                warnings.push(
                    "overlay cross-node service forwarding requested but node capability supports_encap=false; shadow compiler marked services degraded"
                        .to_string(),
                );
            } else {
                warnings.push(
                    "overlay cross-node service forwarding is still shadow planned; vxlan/geneve handoff datapath not materialized yet"
                        .to_string(),
                );
            }
        }
        if service_intent.cross_node_hybrid_service_count > 0 {
            warnings.push(
                "hybrid cross-node service forwarding is still shadow planned; overlay/native policy handoff not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.revnat_reservation_count > 0 {
            warnings.push(
                "service revnat runtime family is still shadow reserved; packet return-path datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.affinity_reservation_count > 0 {
            warnings.push(
                "service affinity runtime family is still shadow reserved; session stickiness datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.maglev_reservation_count > 0 {
            warnings.push(
                "service maglev runtime family is still shadow reserved; consistent-hash datapath not materialized yet"
                    .to_string(),
            );
        }

        ServiceRuntimeExecutionSummary {
            service_count: service_intent.service_count,
            frontend_listener_count: service_intent.frontend_listener_count,
            backend_member_count: service_intent.backend_member_count,
            forwarding_projection_count: service_intent.forwarding_projection_count,
            node_local_service_count: service_intent.node_local_service_count,
            cross_node_service_count: service_intent.cross_node_service_count,
            cross_node_native_service_count: service_intent.cross_node_native_service_count,
            cross_node_overlay_service_count: service_intent
                .cross_node_overlay_service_count,
            cross_node_hybrid_service_count: service_intent.cross_node_hybrid_service_count,
            revnat_reservation_count: service_intent.revnat_reservation_count,
            affinity_reservation_count: service_intent.affinity_reservation_count,
            maglev_reservation_count: service_intent.maglev_reservation_count,
            execution_status,
            planned_action: service_intent.desired_action.clone(),
            warnings,
            shadow_apply_only: true,
        }
    });

    RuntimeExecutionSummary {
        generation: runtime_intent.generation.clone(),
        previous_generation: runtime_intent.previous_generation.clone(),
        compiled_at: runtime_intent.compiled_at.clone(),
        observed_at: unix_timestamp_string(),
        changed_domains: runtime_intent.changed_domains.clone(),
        domain_summaries,
        service_execution,
        shadow_apply_only: true,
    }
}

fn total_service_listener_ports(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

fn total_service_backend_members(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .map(|program| {
            program
                .backend_set
                .as_ref()
                .map(|backend_set| backend_set.backends.len())
                .unwrap_or(0)
        })
        .sum()
}

fn total_service_forwarding_projections(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .map(|program| {
            usize::from(program.frontend.node_local_forwarding)
                + usize::from(program.frontend.cross_node_forwarding)
        })
        .sum()
}

fn total_node_local_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.node_local_forwarding)
        .count()
}

fn total_cross_node_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.cross_node_forwarding)
        .count()
}

fn total_cross_node_native_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.forwarding_mode == "cross_node_native")
        .count()
}

fn total_cross_node_overlay_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.forwarding_mode == "cross_node_overlay")
        .count()
}

fn total_cross_node_hybrid_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.forwarding_mode == "cross_node_hybrid")
        .count()
}

fn total_service_revnat_entries(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

fn total_affinity_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| {
            program
                .frontend
                .session_affinity
                .as_deref()
                .map(|value| value != "none")
                .unwrap_or(false)
        })
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

fn total_maglev_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.lb_policy == "maglev")
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

fn inventory_domain_from_scope(scope: &str) -> String {
    match scope {
        "port-bindings" | "anti-spoof-fastpath" => "ports".to_string(),
        "route-tables" => "routes".to_string(),
        _ => "runtime".to_string(),
    }
}

fn inventory_domain_from_map_family(map_family: &str) -> String {
    match map_family {
        "tenant_index" | "network_index" => "identity".to_string(),
        "security_program" => "security".to_string(),
        "port_bindings" => "ports".to_string(),
        "route_program" => "routes".to_string(),
        "health_check_catalog"
        | "backend_set_catalog"
        | "service_catalog"
        | "service_frontend_catalog"
        | "backend_member_catalog"
        | "service_forwarding_projection"
        | "service_revnat_map"
        | "service_affinity_map"
        | "service_maglev_map" => "services".to_string(),
        "nat_program" => "nat".to_string(),
        _ => "runtime".to_string(),
    }
}

fn capability_profile(capability: &NodeCapability) -> String {
    let mut hooks = capability.supported_hooks.clone();
    hooks.sort();
    let hooks = hooks.join("+");
    let trace = if capability.supports_trace_ringbuf {
        "ringbuf"
    } else {
        "legacy"
    };
    format!("{hooks}:trace={trace}:nat={}", capability.supports_nat)
}

async fn parse_platform_error(response: reqwest::Response) -> Option<String> {
    let status = response.status();
    let body = response.text().await.ok()?;
    if let Ok(error) = serde_json::from_str::<PlatformApiError>(&body) {
        return Some(format!("{}: {}", status.as_u16(), error.message));
    }
    if body.trim().is_empty() {
        None
    } else {
        Some(format!("{}: {}", status.as_u16(), body.trim()))
    }
}

fn connection_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "southbound request timed out".to_string()
    } else if error.is_connect() {
        format!("failed to connect to controller: {error}")
    } else if let Some(status) = error.status() {
        format!("southbound request failed with status {}", status.as_u16())
    } else {
        format!("southbound request failed: {error}")
    }
}

fn hostname() -> String {
    for path in ["/proc/sys/kernel/hostname", "/etc/hostname"] {
        if let Ok(raw) = std::fs::read_to_string(path) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string())
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn temp_path(path: &Path) -> PathBuf {
    let mut tmp = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|ext| format!("{ext}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    tmp.set_extension(extension);
    tmp
}
