use aria_api::{
    ApplyStatusReport, BackendSetResource, DesiredStateEnvelope, DesiredStatePublishRecord,
    HealthCheckResource, IpGroupResource, MirrorPolicyResource, NetworkPolicyResource,
    NetworkResource, NodeCapability, NodeConfigResource, NodeHealthReport, NodeInfo,
    NodeRegisterRequest, PortResource, QosPolicyResource, RouteTableResource,
    SecurityGroupResource, ServiceChainResource, ServiceResource, SouthboundNodeStatusResponse,
    SouthboundSyncStatus, TenantResource,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;

use super::resource_store::{PersistedControllerState, unix_timestamp_string};
use super::{InMemoryControllerStore, StoreError};

impl InMemoryControllerStore {
    pub(crate) fn current_generation_inner(&self) -> String {
        self.generation.load(Ordering::Relaxed).to_string()
    }

    pub(crate) fn bump_generation_inner(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn desired_state_object_counts(
        tenants: &[TenantResource],
        networks: &[NetworkResource],
        ports: &[PortResource],
        security_groups: &[SecurityGroupResource],
        ip_groups: &[IpGroupResource],
        network_policies: &[NetworkPolicyResource],
        qos_policies: &[QosPolicyResource],
        mirror_policies: &[MirrorPolicyResource],
        service_chains: &[ServiceChainResource],
        route_tables: &[RouteTableResource],
        health_checks: &[HealthCheckResource],
        backend_sets: &[BackendSetResource],
        services: &[ServiceResource],
        node_configs: &[NodeConfigResource],
        deletes: usize,
    ) -> BTreeMap<String, usize> {
        BTreeMap::from([
            ("tenants".to_string(), tenants.len()),
            ("networks".to_string(), networks.len()),
            ("ports".to_string(), ports.len()),
            ("security_groups".to_string(), security_groups.len()),
            ("ip_groups".to_string(), ip_groups.len()),
            ("network_policies".to_string(), network_policies.len()),
            ("qos_policies".to_string(), qos_policies.len()),
            ("mirror_policies".to_string(), mirror_policies.len()),
            ("service_chains".to_string(), service_chains.len()),
            ("route_tables".to_string(), route_tables.len()),
            ("health_checks".to_string(), health_checks.len()),
            ("backend_sets".to_string(), backend_sets.len()),
            ("services".to_string(), services.len()),
            ("node_configs".to_string(), node_configs.len()),
            ("deletes".to_string(), deletes),
        ])
    }

    pub(crate) async fn published_desired_state(
        &self,
        node_id: &str,
        generation: &str,
        full_sync: bool,
        object_counts: &BTreeMap<String, usize>,
    ) -> (DesiredStatePublishRecord, bool) {
        let mut publishes = self.southbound_publishes.write().await;
        if let Some(existing) = publishes.get(node_id) {
            if existing.generation == generation
                && existing.full_sync == full_sync
                && existing.object_counts == *object_counts
            {
                return (existing.clone(), false);
            }
        }

        let record = DesiredStatePublishRecord {
            generation: generation.to_string(),
            issued_at: unix_timestamp_string(),
            full_sync,
            object_counts: object_counts.clone(),
        };
        publishes.insert(node_id.to_string(), record.clone());
        (record, true)
    }

    pub(crate) fn derive_sync_status(
        desired_generation: &str,
        last_applied_generation: Option<&str>,
        last_desired_state: Option<&DesiredStatePublishRecord>,
        last_apply_status: Option<&ApplyStatusReport>,
        last_health: Option<&NodeHealthReport>,
    ) -> SouthboundSyncStatus {
        let mut reasons = Vec::new();

        if last_desired_state.is_none() {
            reasons.push("desired_state_not_published".to_string());
            return SouthboundSyncStatus {
                state: "pending".to_string(),
                reconcile_required: true,
                reasons,
            };
        }

        let Some(applied_generation) = last_applied_generation else {
            reasons.push("desired_generation_not_applied".to_string());
            return SouthboundSyncStatus {
                state: "pending".to_string(),
                reconcile_required: true,
                reasons,
            };
        };

        if applied_generation != desired_generation {
            reasons.push(format!(
                "desired_generation_mismatch:{desired_generation}!={applied_generation}"
            ));
            return SouthboundSyncStatus {
                state: "out_of_sync".to_string(),
                reconcile_required: true,
                reasons,
            };
        }

        if let Some(apply_status) = last_apply_status {
            if apply_status.status == "failed" {
                reasons.push("apply_reported_failed".to_string());
                reasons.extend(
                    apply_status
                        .failed_objects
                        .iter()
                        .map(|failure| format!("failed:{}:{}", failure.resource_kind, failure.id)),
                );
                return SouthboundSyncStatus {
                    state: "failed".to_string(),
                    reconcile_required: true,
                    reasons,
                };
            }

            if apply_status.status == "partial" {
                reasons.push("apply_reported_partial".to_string());
            }

            reasons.extend(
                apply_status
                    .degraded_reasons
                    .iter()
                    .map(|reason| format!("degraded:{reason}")),
            );
        }

        if let Some(health) = last_health {
            if !health.datapath_ready {
                reasons.push("datapath_not_ready".to_string());
            }
            if let Some(error) = &health.last_error {
                reasons.push(format!("node_error:{error}"));
            }
        }

        if reasons.is_empty() {
            SouthboundSyncStatus {
                state: "in_sync".to_string(),
                reconcile_required: false,
                reasons,
            }
        } else {
            SouthboundSyncStatus {
                state: "degraded".to_string(),
                reconcile_required: true,
                reasons,
            }
        }
    }

    pub(crate) fn pending_object_counts(
        desired_generation: &str,
        last_desired_state: Option<&DesiredStatePublishRecord>,
        last_apply_status: Option<&ApplyStatusReport>,
    ) -> BTreeMap<String, usize> {
        let Some(last_desired_state) =
            last_desired_state.filter(|state| state.generation == desired_generation)
        else {
            return BTreeMap::new();
        };

        let Some(last_apply_status) = last_apply_status
            .filter(|report| report.generation == desired_generation && report.status == "applied")
        else {
            return last_desired_state
                .object_counts
                .iter()
                .filter_map(|(kind, count)| (*count > 0).then(|| (kind.clone(), *count)))
                .collect();
        };

        let applied_counts = &last_apply_status.compiled_objects;
        last_desired_state
            .object_counts
            .iter()
            .filter_map(|(kind, desired_count)| {
                let applied = applied_counts.get(kind).copied().unwrap_or(0);
                let pending = desired_count.saturating_sub(applied);
                (pending > 0).then(|| (kind.clone(), pending))
            })
            .collect()
    }

    pub(crate) fn change_summary(
        desired_generation: &str,
        last_desired_state: Option<&DesiredStatePublishRecord>,
        pending_object_counts: &BTreeMap<String, usize>,
    ) -> (Vec<String>, bool) {
        let changed_kinds = pending_object_counts
            .keys()
            .filter(|kind| kind.as_str() != "deletes")
            .cloned()
            .collect::<Vec<_>>();
        let has_deletes = last_desired_state
            .filter(|state| state.generation == desired_generation)
            .and_then(|state| state.object_counts.get("deletes"))
            .copied()
            .unwrap_or(0)
            > 0;
        (changed_kinds, has_deletes)
    }

    pub(crate) fn southbound_status_from_parts(
        &self,
        node_id: &str,
        last_applied_generation: Option<String>,
        last_seen_at: Option<String>,
        last_desired_state: Option<DesiredStatePublishRecord>,
        registration: Option<NodeRegisterRequest>,
        last_apply_status: Option<ApplyStatusReport>,
        last_health: Option<NodeHealthReport>,
    ) -> SouthboundNodeStatusResponse {
        let desired_generation = self.current_generation_inner();
        let sync_status = Self::derive_sync_status(
            &desired_generation,
            last_applied_generation.as_deref(),
            last_desired_state.as_ref(),
            last_apply_status.as_ref(),
            last_health.as_ref(),
        );
        let pending_object_counts = Self::pending_object_counts(
            &desired_generation,
            last_desired_state.as_ref(),
            last_apply_status.as_ref(),
        );
        let (changed_kinds, has_deletes) = Self::change_summary(
            &desired_generation,
            last_desired_state.as_ref(),
            &pending_object_counts,
        );

        SouthboundNodeStatusResponse {
            node_id: node_id.to_string(),
            desired_generation,
            last_applied_generation,
            last_seen_at,
            pending_object_counts,
            changed_kinds,
            has_deletes,
            last_desired_state,
            sync_status,
            registration,
            last_apply_status,
            last_health,
        }
    }

    pub(crate) async fn snapshot_state(&self) -> PersistedControllerState {
        PersistedControllerState {
            tenants: self.tenants.snapshot().await,
            nodes: self.nodes.snapshot().await,
            networks: self.networks.snapshot().await,
            ports: self.ports.snapshot().await,
            security_groups: self.security_groups.snapshot().await,
            route_tables: self.route_tables.snapshot().await,
            ip_groups: self.ip_groups.snapshot().await,
            network_policies: self.network_policies.snapshot().await,
            qos_policies: self.qos_policies.snapshot().await,
            mirror_policies: self.mirror_policies.snapshot().await,
            service_chains: self.service_chains.snapshot().await,
            health_checks: self.health_checks.snapshot().await,
            backend_sets: self.backend_sets.snapshot().await,
            services: self.services.snapshot().await,
            node_configs: self.node_configs.snapshot().await,
            generation: self.generation.load(Ordering::Relaxed),
            southbound_publishes: self.southbound_publishes.read().await.clone(),
        }
    }

    pub(crate) async fn restore_state(&self, snapshot: PersistedControllerState) {
        self.tenants.restore(snapshot.tenants).await;
        self.nodes.restore(snapshot.nodes).await;
        self.networks.restore(snapshot.networks).await;
        self.ports.restore(snapshot.ports).await;
        self.security_groups.restore(snapshot.security_groups).await;
        self.route_tables.restore(snapshot.route_tables).await;
        self.ip_groups.restore(snapshot.ip_groups).await;
        self.network_policies
            .restore(snapshot.network_policies)
            .await;
        self.qos_policies.restore(snapshot.qos_policies).await;
        self.mirror_policies.restore(snapshot.mirror_policies).await;
        self.service_chains.restore(snapshot.service_chains).await;
        self.health_checks.restore(snapshot.health_checks).await;
        self.backend_sets.restore(snapshot.backend_sets).await;
        self.services.restore(snapshot.services).await;
        self.node_configs.restore(snapshot.node_configs).await;
        self.generation
            .store(snapshot.generation, Ordering::Relaxed);
        *self.southbound_publishes.write().await = snapshot.southbound_publishes;
    }

    pub(crate) async fn record_registration_inner(
        &self,
        node_id: &str,
        info: NodeInfo,
        capability: NodeCapability,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.nodes
            .get(node_id)
            .await
            .ok_or_else(|| StoreError::NotFound {
                resource: "node",
                id: node_id.to_string(),
            })?;

        let now = unix_timestamp_string();
        let registration = NodeRegisterRequest { info, capability };
        let (last_applied_generation, last_seen_at, last_apply_status, last_health) = {
            let mut states = self.southbound_nodes.write().await;
            let entry =
                states
                    .entry(node_id.to_string())
                    .or_insert_with(|| super::SouthboundNodeRuntime {
                        registration: None,
                        last_apply_status: None,
                        last_health: None,
                        last_applied_generation: None,
                        last_seen_at: now.clone(),
                    });
            entry.registration = Some(registration.clone());
            entry.last_seen_at = now;
            (
                entry.last_applied_generation.clone(),
                entry.last_seen_at.clone(),
                entry.last_apply_status.clone(),
                entry.last_health.clone(),
            )
        };
        let last_desired_state = self.southbound_publishes.read().await.get(node_id).cloned();

        Ok(self.southbound_status_from_parts(
            node_id,
            last_applied_generation,
            Some(last_seen_at),
            last_desired_state,
            Some(registration),
            last_apply_status,
            last_health,
        ))
    }

    pub(crate) async fn record_apply_status_inner(
        &self,
        node_id: &str,
        report: ApplyStatusReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.nodes
            .get(node_id)
            .await
            .ok_or_else(|| StoreError::NotFound {
                resource: "node",
                id: node_id.to_string(),
            })?;

        let now = unix_timestamp_string();
        let (last_applied_generation, last_seen_at, registration, last_apply_status, last_health) = {
            let mut states = self.southbound_nodes.write().await;
            let entry =
                states
                    .entry(node_id.to_string())
                    .or_insert_with(|| super::SouthboundNodeRuntime {
                        registration: None,
                        last_apply_status: None,
                        last_health: None,
                        last_applied_generation: None,
                        last_seen_at: now.clone(),
                    });
            entry.last_applied_generation = Some(report.generation.clone());
            entry.last_apply_status = Some(report);
            entry.last_seen_at = now;
            (
                entry.last_applied_generation.clone(),
                entry.last_seen_at.clone(),
                entry.registration.clone(),
                entry.last_apply_status.clone(),
                entry.last_health.clone(),
            )
        };
        let last_desired_state = self.southbound_publishes.read().await.get(node_id).cloned();

        Ok(self.southbound_status_from_parts(
            node_id,
            last_applied_generation,
            Some(last_seen_at),
            last_desired_state,
            registration,
            last_apply_status,
            last_health,
        ))
    }

    pub(crate) async fn record_health_inner(
        &self,
        node_id: &str,
        report: NodeHealthReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.nodes
            .get(node_id)
            .await
            .ok_or_else(|| StoreError::NotFound {
                resource: "node",
                id: node_id.to_string(),
            })?;

        let now = unix_timestamp_string();
        let (last_applied_generation, last_seen_at, registration, last_apply_status, last_health) = {
            let mut states = self.southbound_nodes.write().await;
            let entry =
                states
                    .entry(node_id.to_string())
                    .or_insert_with(|| super::SouthboundNodeRuntime {
                        registration: None,
                        last_apply_status: None,
                        last_health: None,
                        last_applied_generation: None,
                        last_seen_at: now.clone(),
                    });
            entry.last_health = Some(report);
            entry.last_seen_at = now;
            (
                entry.last_applied_generation.clone(),
                entry.last_seen_at.clone(),
                entry.registration.clone(),
                entry.last_apply_status.clone(),
                entry.last_health.clone(),
            )
        };
        let last_desired_state = self.southbound_publishes.read().await.get(node_id).cloned();

        Ok(self.southbound_status_from_parts(
            node_id,
            last_applied_generation,
            Some(last_seen_at),
            last_desired_state,
            registration,
            last_apply_status,
            last_health,
        ))
    }

    pub(crate) async fn southbound_status_inner(
        &self,
        node_id: &str,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.nodes
            .get(node_id)
            .await
            .ok_or_else(|| StoreError::NotFound {
                resource: "node",
                id: node_id.to_string(),
            })?;

        let last_desired_state = self.southbound_publishes.read().await.get(node_id).cloned();
        let states = self.southbound_nodes.read().await;
        if let Some(entry) = states.get(node_id) {
            Ok(self.southbound_status_from_parts(
                node_id,
                entry.last_applied_generation.clone(),
                Some(entry.last_seen_at.clone()),
                last_desired_state,
                entry.registration.clone(),
                entry.last_apply_status.clone(),
                entry.last_health.clone(),
            ))
        } else {
            Ok(self.southbound_status_from_parts(
                node_id,
                None,
                None,
                last_desired_state,
                None,
                None,
                None,
            ))
        }
    }

    pub(crate) async fn clear_southbound_runtime_inner(&self, node_id: &str) {
        self.southbound_nodes.write().await.remove(node_id);
    }

    pub(crate) async fn clear_southbound_publish_inner(&self, node_id: &str) {
        self.southbound_publishes.write().await.remove(node_id);
    }

    pub(crate) async fn desired_state_for_node_inner(
        &self,
        node_id: &str,
    ) -> Result<(DesiredStateEnvelope, bool), StoreError> {
        self.nodes
            .get(node_id)
            .await
            .ok_or_else(|| StoreError::NotFound {
                resource: "node",
                id: node_id.to_string(),
            })?;

        let ports = self
            .ports
            .list()
            .await
            .into_iter()
            .filter(|port| port.spec.node_id.as_deref() == Some(node_id))
            .collect::<Vec<_>>();

        let mut tenant_ids = BTreeSet::new();
        let mut network_ids = BTreeSet::new();
        let mut security_group_ids = BTreeSet::new();

        for port in &ports {
            tenant_ids.insert(port.spec.tenant_id.clone());
            network_ids.insert(port.spec.network_id.clone());
            security_group_ids.extend(port.spec.security_group_ids.iter().cloned());
        }

        let networks = self
            .networks
            .list()
            .await
            .into_iter()
            .filter(|network| network_ids.contains(&network.metadata.id))
            .collect::<Vec<_>>();

        for network in &networks {
            tenant_ids.insert(network.spec.tenant_id.clone());
        }

        let security_groups = self
            .security_groups
            .list()
            .await
            .into_iter()
            .filter(|sg| security_group_ids.contains(&sg.metadata.id))
            .collect::<Vec<_>>();

        let route_tables = self
            .route_tables
            .list()
            .await
            .into_iter()
            .filter(|route_table| network_ids.contains(&route_table.spec.network_id))
            .collect::<Vec<_>>();

        let ip_groups = self
            .ip_groups
            .list()
            .await
            .into_iter()
            .filter(|ip_group| network_ids.contains(&ip_group.spec.network_id))
            .collect::<Vec<_>>();

        let network_policies = self
            .network_policies
            .list()
            .await
            .into_iter()
            .filter(|np| network_ids.contains(&np.spec.network_id))
            .collect::<Vec<_>>();

        let qos_policies = self
            .qos_policies
            .list()
            .await
            .into_iter()
            .filter(|qp| network_ids.contains(&qp.spec.network_id))
            .collect::<Vec<_>>();

        let mirror_policies = self
            .mirror_policies
            .list()
            .await
            .into_iter()
            .filter(|mp| network_ids.contains(&mp.spec.network_id))
            .collect::<Vec<_>>();

        let service_chains = self
            .service_chains
            .list()
            .await
            .into_iter()
            .filter(|sc| network_ids.contains(&sc.spec.network_id))
            .collect::<Vec<_>>();

        let backend_sets = self
            .backend_sets
            .list()
            .await
            .into_iter()
            .filter(|backend_set| network_ids.contains(&backend_set.spec.network_id))
            .collect::<Vec<_>>();

        let services = self
            .services
            .list()
            .await
            .into_iter()
            .filter(|service| network_ids.contains(&service.spec.network_id))
            .collect::<Vec<_>>();

        let health_check_ids = backend_sets
            .iter()
            .filter_map(|backend_set| backend_set.spec.health_check_id.clone())
            .collect::<BTreeSet<_>>();

        let health_checks = self
            .health_checks
            .list()
            .await
            .into_iter()
            .filter(|health_check| {
                health_check_ids.contains(&health_check.metadata.id)
                    || health_check
                        .spec
                        .network_id
                        .as_deref()
                        .is_some_and(|network_id| network_ids.contains(network_id))
            })
            .collect::<Vec<_>>();

        let node_configs = self
            .node_configs
            .list()
            .await
            .into_iter()
            .filter(|nc| nc.spec.node_id == node_id)
            .collect::<Vec<_>>();

        let tenants = self
            .tenants
            .list()
            .await
            .into_iter()
            .filter(|tenant| tenant_ids.contains(&tenant.metadata.id))
            .collect::<Vec<_>>();

        let object_counts = Self::desired_state_object_counts(
            &tenants,
            &networks,
            &ports,
            &security_groups,
            &ip_groups,
            &network_policies,
            &qos_policies,
            &mirror_policies,
            &service_chains,
            &route_tables,
            &health_checks,
            &backend_sets,
            &services,
            &node_configs,
            0,
        );
        let generation = self.current_generation_inner();
        let (publish, changed) = self
            .published_desired_state(node_id, &generation, true, &object_counts)
            .await;

        Ok((
            DesiredStateEnvelope {
                generation: publish.generation.clone(),
                full_sync: publish.full_sync,
                issued_at: publish.issued_at.clone(),
                node_id: node_id.to_string(),
                object_counts: publish.object_counts.clone(),
                tenants,
                networks,
                ports,
                security_groups,
                ip_groups,
                network_policies,
                route_tables,
                qos_policies,
                mirror_policies,
                service_chains,
                health_checks,
                backend_sets,
                services,
                node_configs,
                deletes: Vec::new(),
            },
            changed,
        ))
    }
}
