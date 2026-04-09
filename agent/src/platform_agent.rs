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

#[derive(Debug, Clone)]
struct CompileOutcome {
    compiled_state: CompiledNodeState,
    reconcile_plan: ReconcilePlan,
    runtime_plan: RuntimePlan,
    runtime_inventory: RuntimeInventory,
    apply_report: ApplyStatusReport,
}

#[derive(Debug)]
struct CompilerContext<'a> {
    node_id: &'a str,
    desired: &'a DesiredStateEnvelope,
    capability: &'a NodeCapability,
    previous_compiled_state: Option<&'a CompiledNodeState>,
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
    if !context.desired.deletes.is_empty() {
        compiled_objects.insert("deletes".to_string(), context.desired.deletes.len());
    }

    let mut degraded_reasons = vec!["shadow_apply_only".to_string()];
    if !failed_objects.is_empty() {
        degraded_reasons.push("object_validation_failed".to_string());
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
        domain_summaries,
        warnings: warnings.clone(),
        degraded_reasons: degraded_reasons.clone(),
        compiled_at: compiled_at.clone(),
        shadow_apply_only: true,
    };
    let domain_statuses = compiled_state
        .domain_summaries
        .iter()
        .map(|summary| ApplyDomainStatus {
            domain: summary.domain.clone(),
            input_objects: summary.input_objects,
            compiled_objects: summary.compiled_objects,
            failed_objects: summary.failed_objects,
            status: summary.status.clone(),
            shadow_apply_only: summary.shadow_apply_only,
        })
        .collect();
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

    let ports_removed = previous_port_ids.difference(&next_port_ids).count();
    let route_tables_removed = previous_route_table_ids
        .difference(&next_route_table_ids)
        .count();

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

    let mut actions = Vec::new();
    if full_reconcile {
        actions.push(ReconcileAction {
            domain: "core".to_string(),
            operation: "full_shadow_reconcile".to_string(),
            object_count: next_state.port_bindings.len() + next_state.route_tables.len(),
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

    let ports_removed = previous_port_ids.difference(&next_port_ids).count();
    let route_tables_removed = previous_route_table_ids
        .difference(&next_route_table_ids)
        .count();

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
            status: if summary.failed_objects == 0 {
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
