use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use aria_api::{
    ApplyStatusReport, DesiredStateEnvelope, HeartbeatResponse, NodeAddress, NodeCapability,
    NodeHealthReport, NodeInfo, NodeRegisterRequest, NodeRegisterResponse,
};
use tokio::task::JoinHandle;
use tokio::time;
use tracing::{info, warn};

mod compiler;
mod helpers;
mod ir_builders;
mod ir_types;
mod planner;
mod runtime_intent;
mod runtime_materialize;
mod socket_plan;
mod southbound_client;
mod state_store;

use compiler::{build_node_capability, compile_desired_state};
use helpers::{hostname, unix_timestamp_string};
use ir_types::{CompiledNodeState, CompileOutcome, CompilerContext, DesiredStateCacheEntry};
use runtime_materialize::{materialize_phase3_maps, materialize_service_maps};
use southbound_client::SouthboundClient;
use state_store::LocalPlatformStateStore;

#[derive(Clone, Debug)]
pub struct PlatformAgentConfig {
    pub controller_url: String,
    pub node_id: String,
    pub management_address: Option<String>,
    pub labels: BTreeMap<String, String>,
    pub poll_interval: Duration,
    pub register_interval: Duration,
    pub state_dir: PathBuf,
    pub pin_path: String,
    pub trace_backend: String,
    pub kernel_version: Option<String>,
    pub max_port_policies: u32,
}

pub(crate) struct PlatformAgent {
    pub(crate) config: PlatformAgentConfig,
    pub(crate) client: SouthboundClient,
    pub(crate) state_store: LocalPlatformStateStore,
    pub(crate) start_time: Instant,
    pub(crate) capability: NodeCapability,
    pub(crate) health_executor: crate::health_check::HealthCheckExecutor,
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
            health_executor: crate::health_check::HealthCheckExecutor::new(),
        }
    }

    async fn run(mut self) {
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
        let mut socket_selection_plan = self.state_store.load_socket_selection_plan().await;
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
                        .map(attached_port_count)
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
                    != Some(desired_generation.as_str())
                || socket_selection_plan
                    .as_ref()
                    .map(|plan| plan.generation.as_str())
                    != Some(desired_generation.as_str());

            let mut last_reconcile_at = compiled_state
                .as_ref()
                .map(|state| state.compiled_at.clone());
            let mut attached_ports = compiled_state
                .as_ref()
                .map(attached_port_count)
                .unwrap_or(0);
            let mut heartbeat_error: Option<String> = None;

            if needs_compile {
                let previous_compiled_state = compiled_state.clone();
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

                let mut outcome = compile_desired_state(CompilerContext {
                    node_id: &self.config.node_id,
                    desired: &desired_state,
                    capability: &self.capability,
                    previous_compiled_state: previous_compiled_state.as_ref(),
                    previous_runtime_inventory: runtime_inventory.as_ref(),
                });
                attached_ports = attached_port_count(&outcome.compiled_state);
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
                    .state_store
                    .save_socket_selection_plan(&outcome.socket_selection_plan)
                    .await
                {
                    warn!(error = %error, "failed to persist socket selection plan");
                    heartbeat_error = Some(error);
                } else {
                    socket_selection_plan = Some(outcome.socket_selection_plan.clone());
                    info!(
                        generation = %outcome.socket_selection_plan.generation,
                        listeners = outcome.socket_selection_plan.summary.listener_count,
                        cross_node_handoffs = outcome.socket_selection_plan
                            .summary
                            .cross_node_handoff_listener_count,
                        "persisted shadow socket selection plan"
                    );
                }

                // Materialize phase-3 single-node IaaS maps first so service
                // datapath keeps building on resolved port/route/security state.
                match materialize_phase3_maps(
                    &self.config.pin_path,
                    previous_compiled_state.as_ref(),
                    &outcome.compiled_state,
                ) {
                    Ok(result) => {
                        info!(
                            port_identities = result.port_identities_written,
                            anti_spoof_entries = result.anti_spoof_entries_written,
                            sg_rules = result.sg_rules_written,
                            route_v4 = result.route_v4_written,
                            route_v6 = result.route_v6_written,
                            qos_rules = result.qos_entries_written,
                            "materialized phase-3 iaas maps into eBPF datapath"
                        );
                        for domain in &mut outcome.runtime_execution_summary.domain_summaries {
                            if matches!(
                                domain.domain.as_str(),
                                "identity" | "ports" | "security" | "routes" | "qos"
                            ) {
                                domain.execution_status = "applied".to_string();
                                domain.shadow_apply_only = false;
                            }
                        }
                        for ds in &mut outcome.apply_report.domain_statuses {
                            if matches!(
                                ds.domain.as_str(),
                                "identity" | "ports" | "security" | "routes" | "qos"
                            ) {
                                ds.status = "applied".to_string();
                                ds.shadow_apply_only = false;
                            }
                        }
                    }
                    Err(error) => {
                        warn!(error = %error, "failed to materialize phase-3 iaas maps");
                        heartbeat_error = Some(error.clone());
                        for domain in &mut outcome.runtime_execution_summary.domain_summaries {
                            if matches!(
                                domain.domain.as_str(),
                                "identity" | "ports" | "security" | "routes" | "qos"
                            ) {
                                domain.execution_status = "failed".to_string();
                                domain.shadow_apply_only = false;
                                domain
                                    .warnings
                                    .push(format!("materialize failed: {}", error));
                            }
                        }
                        for ds in &mut outcome.apply_report.domain_statuses {
                            if matches!(
                                ds.domain.as_str(),
                                "identity" | "ports" | "security" | "routes" | "qos"
                            ) {
                                ds.status = "failed".to_string();
                                ds.shadow_apply_only = false;
                            }
                        }
                    }
                }

                // Materialize service maps into pinned eBPF maps.
                let current_service_tap_ids: Vec<u32> = outcome
                    .compiled_state
                    .port_identities
                    .iter()
                    .map(|p| p.tap_id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                let previous_had_services = previous_compiled_state
                    .as_ref()
                    .map(|state| !state.service_programs.is_empty())
                    .unwrap_or(false);
                let should_materialize_services =
                    !outcome.compiled_state.service_programs.is_empty() || previous_had_services;

                if should_materialize_services {
                    let lb_enabled = !outcome.compiled_state.service_programs.is_empty();
                    let mut failed_taps = Vec::new();
                    let mut successful_taps = 0usize;
                    let mut total_frontends = 0usize;
                    let mut total_backends = 0usize;
                    let mut total_revnats = 0usize;

                    for tap_id in &current_service_tap_ids {
                        match materialize_service_maps(
                            &self.config.pin_path,
                            *tap_id,
                            &outcome.compiled_state.service_programs,
                            &self.health_executor,
                        ) {
                            Ok((frontends, backends, revnats)) => {
                                let runtime = aria_core::common::TapMapRuntime::new(
                                    &self.config.pin_path,
                                    *tap_id,
                                );
                                match aria_core::ebpf_ops::update_runtime_config(
                                    runtime,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    Some(lb_enabled),
                                ) {
                                    Ok(()) => {
                                        successful_taps += 1;
                                        total_frontends += frontends;
                                        total_backends += backends;
                                        total_revnats += revnats;
                                    }
                                    Err(error) => {
                                        warn!(error = %error, tap_id, "failed to update lb_enabled after service materialization");
                                        failed_taps.push(*tap_id);
                                    }
                                }
                            }
                            Err(error) => {
                                warn!(error = %error, tap_id, "failed to materialize service maps for tap");
                                failed_taps.push(*tap_id);
                            }
                        }
                    }

                    let service_status = if failed_taps.is_empty() {
                        "applied"
                    } else if successful_taps > 0 {
                        "partial"
                    } else {
                        "failed"
                    };

                    if successful_taps > 0 {
                        info!(
                            total_frontends,
                            total_backends,
                            total_revnats,
                            taps = current_service_tap_ids.len(),
                            "materialized service maps into eBPF datapath"
                        );
                    }

                    for domain in &mut outcome.runtime_execution_summary.domain_summaries {
                        if domain.domain == "services" {
                            domain.execution_status = service_status.to_string();
                            domain.shadow_apply_only = false;
                            domain
                                .warnings
                                .retain(|w| !w.contains("not materialized yet"));
                            if !failed_taps.is_empty() {
                                domain.warnings.push(format!(
                                    "service materialization failed on {} tap(s): {:?}",
                                    failed_taps.len(),
                                    failed_taps
                                ));
                            }
                        }
                    }
                    for ds in &mut outcome.apply_report.domain_statuses {
                        if ds.domain == "services" {
                            ds.status = service_status.to_string();
                            ds.shadow_apply_only = false;
                        }
                    }
                }

                if let Err(error) = self
                    .state_store
                    .save_runtime_execution_summary(&outcome.runtime_execution_summary)
                    .await
                {
                    warn!(error = %error, "failed to persist materialized runtime execution summary");
                    heartbeat_error = Some(error);
                } else {
                    runtime_execution_summary = Some(outcome.runtime_execution_summary.clone());
                }

                // Run health check probes for services with health checks.
                {
                    let probe_targets: Vec<(
                        crate::health_check::BackendTarget,
                        crate::health_check::ProbeConfig,
                    )> = outcome
                        .compiled_state
                        .service_programs
                        .iter()
                        .filter_map(|program| {
                            let hc = program.health_check.as_ref()?;
                            let bs = program.backend_set.as_ref()?;
                            Some(
                                bs.backends
                                    .iter()
                                    .filter(|b| {
                                        b.admin_state != "disabled"
                                            && b.resolved_locality == "local"
                                    })
                                    .filter_map(|b| {
                                        let ip = b.resolved_ip_hint.as_deref()?;
                                        Some((
                                            crate::health_check::BackendTarget {
                                                service_id: program.service_id.clone(),
                                                backend_id: b.backend_id.clone(),
                                                address: ip.to_string(),
                                                port: b.service_port,
                                            },
                                            crate::health_check::ProbeConfig {
                                                protocol: hc.probe_protocol.clone(),
                                                interval: std::time::Duration::from_secs(
                                                    hc.interval_seconds as u64,
                                                ),
                                                timeout: std::time::Duration::from_secs(
                                                    hc.timeout_seconds as u64,
                                                ),
                                                healthy_threshold: hc.healthy_threshold,
                                                unhealthy_threshold: hc.unhealthy_threshold,
                                                target_port: hc.target_port,
                                            },
                                        ))
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .flatten()
                        .collect();

                    if !probe_targets.is_empty() {
                        let changed = self.health_executor.probe_round(&probe_targets).await;
                        if !changed.is_empty() {
                            info!(
                                changed_backends = changed.len(),
                                "health check state changed, re-materializing service maps"
                            );
                            let hc_tap_ids: Vec<u32> = outcome
                                .compiled_state
                                .port_identities
                                .iter()
                                .map(|p| p.tap_id)
                                .collect::<std::collections::BTreeSet<_>>()
                                .into_iter()
                                .collect();
                            let hc_tap_ids = if hc_tap_ids.is_empty() {
                                vec![1u32]
                            } else {
                                hc_tap_ids
                            };
                            for tap_id in &hc_tap_ids {
                                if let Err(e) = materialize_service_maps(
                                    &self.config.pin_path,
                                    *tap_id,
                                    &outcome.compiled_state.service_programs,
                                    &self.health_executor,
                                ) {
                                    warn!(error = %e, tap_id, "failed to re-materialize after health change for tap");
                                }
                            }
                        }
                    }
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
