use aria_api::{
    ApplyStatusReport, DesiredStateEnvelope, DesiredStatePublishRecord, NetworkResource,
    NodeCapability, NodeHealthReport, NodeInfo, NodeRegisterRequest, NodeResource, PortResource,
    ResourceMetadata, RouteTableResource, SecurityGroupResource, SouthboundNodeStatusResponse,
    SouthboundSyncStatus, TenantResource,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs,
    sync::{Mutex, RwLock},
};

pub type SharedStore = Arc<dyn ControllerStore>;

pub trait StoredResource: Clone + Send + Sync + 'static {
    fn metadata(&self) -> &ResourceMetadata;
    fn metadata_mut(&mut self) -> &mut ResourceMetadata;
}

macro_rules! impl_stored_resource {
    ($ty:ty) => {
        impl StoredResource for $ty {
            fn metadata(&self) -> &ResourceMetadata {
                &self.metadata
            }

            fn metadata_mut(&mut self) -> &mut ResourceMetadata {
                &mut self.metadata
            }
        }
    };
}

impl_stored_resource!(TenantResource);
impl_stored_resource!(NodeResource);
impl_stored_resource!(NetworkResource);
impl_stored_resource!(PortResource);
impl_stored_resource!(SecurityGroupResource);
impl_stored_resource!(RouteTableResource);

#[derive(Debug, Clone)]
pub enum StoreError {
    AlreadyExists { resource: &'static str, id: String },
    NotFound { resource: &'static str, id: String },
    Internal(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists { resource, id } => {
                write!(f, "{resource} '{id}' already exists")
            }
            Self::NotFound { resource, id } => write!(f, "{resource} '{id}' was not found"),
            Self::Internal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StoreError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SouthboundNodeRuntime {
    pub registration: Option<NodeRegisterRequest>,
    pub last_apply_status: Option<ApplyStatusReport>,
    pub last_health: Option<NodeHealthReport>,
    pub last_applied_generation: Option<String>,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedResourceStore<T> {
    counter: u64,
    items: BTreeMap<String, T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedControllerState {
    tenants: PersistedResourceStore<TenantResource>,
    nodes: PersistedResourceStore<NodeResource>,
    networks: PersistedResourceStore<NetworkResource>,
    ports: PersistedResourceStore<PortResource>,
    security_groups: PersistedResourceStore<SecurityGroupResource>,
    route_tables: PersistedResourceStore<RouteTableResource>,
    generation: u64,
    southbound_nodes: BTreeMap<String, SouthboundNodeRuntime>,
    #[serde(default)]
    southbound_publishes: BTreeMap<String, DesiredStatePublishRecord>,
}

#[async_trait]
pub trait ControllerStore: Send + Sync {
    async fn resource_counts(&self) -> BTreeMap<String, usize>;
    fn current_generation(&self) -> String;

    async fn list_tenants(&self) -> Vec<TenantResource>;
    async fn get_tenant(&self, id: &str) -> Option<TenantResource>;
    async fn create_tenant(&self, resource: TenantResource) -> Result<TenantResource, StoreError>;
    async fn update_tenant(
        &self,
        id: &str,
        resource: TenantResource,
    ) -> Result<TenantResource, StoreError>;
    async fn delete_tenant(&self, id: &str) -> Result<TenantResource, StoreError>;

    async fn list_nodes(&self) -> Vec<NodeResource>;
    async fn get_node(&self, id: &str) -> Option<NodeResource>;
    async fn create_node(&self, resource: NodeResource) -> Result<NodeResource, StoreError>;
    async fn update_node(
        &self,
        id: &str,
        resource: NodeResource,
    ) -> Result<NodeResource, StoreError>;
    async fn delete_node(&self, id: &str) -> Result<NodeResource, StoreError>;

    async fn list_networks(&self) -> Vec<NetworkResource>;
    async fn get_network(&self, id: &str) -> Option<NetworkResource>;
    async fn create_network(
        &self,
        resource: NetworkResource,
    ) -> Result<NetworkResource, StoreError>;
    async fn update_network(
        &self,
        id: &str,
        resource: NetworkResource,
    ) -> Result<NetworkResource, StoreError>;
    async fn delete_network(&self, id: &str) -> Result<NetworkResource, StoreError>;

    async fn list_ports(&self) -> Vec<PortResource>;
    async fn get_port(&self, id: &str) -> Option<PortResource>;
    async fn create_port(&self, resource: PortResource) -> Result<PortResource, StoreError>;
    async fn update_port(
        &self,
        id: &str,
        resource: PortResource,
    ) -> Result<PortResource, StoreError>;
    async fn delete_port(&self, id: &str) -> Result<PortResource, StoreError>;

    async fn list_security_groups(&self) -> Vec<SecurityGroupResource>;
    async fn get_security_group(&self, id: &str) -> Option<SecurityGroupResource>;
    async fn create_security_group(
        &self,
        resource: SecurityGroupResource,
    ) -> Result<SecurityGroupResource, StoreError>;
    async fn update_security_group(
        &self,
        id: &str,
        resource: SecurityGroupResource,
    ) -> Result<SecurityGroupResource, StoreError>;
    async fn delete_security_group(&self, id: &str) -> Result<SecurityGroupResource, StoreError>;

    async fn list_route_tables(&self) -> Vec<RouteTableResource>;
    async fn get_route_table(&self, id: &str) -> Option<RouteTableResource>;
    async fn create_route_table(
        &self,
        resource: RouteTableResource,
    ) -> Result<RouteTableResource, StoreError>;
    async fn update_route_table(
        &self,
        id: &str,
        resource: RouteTableResource,
    ) -> Result<RouteTableResource, StoreError>;
    async fn delete_route_table(&self, id: &str) -> Result<RouteTableResource, StoreError>;

    async fn record_registration(
        &self,
        node_id: &str,
        info: NodeInfo,
        capability: NodeCapability,
    ) -> Result<SouthboundNodeStatusResponse, StoreError>;
    async fn record_apply_status(
        &self,
        node_id: &str,
        report: ApplyStatusReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError>;
    async fn record_health(
        &self,
        node_id: &str,
        report: NodeHealthReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError>;
    async fn southbound_status(
        &self,
        node_id: &str,
    ) -> Result<SouthboundNodeStatusResponse, StoreError>;
    async fn clear_southbound_runtime(&self, node_id: &str) -> Result<(), StoreError>;
    async fn desired_state_for_node(
        &self,
        node_id: &str,
    ) -> Result<DesiredStateEnvelope, StoreError>;
}

pub struct ResourceStore<T> {
    resource: &'static str,
    prefix: &'static str,
    counter: AtomicU64,
    items: RwLock<BTreeMap<String, T>>,
}

impl<T> ResourceStore<T>
where
    T: StoredResource,
{
    pub fn new(resource: &'static str, prefix: &'static str) -> Self {
        Self {
            resource,
            prefix,
            counter: AtomicU64::new(0),
            items: RwLock::new(BTreeMap::new()),
        }
    }

    pub async fn list(&self) -> Vec<T> {
        self.items.read().await.values().cloned().collect()
    }

    pub async fn count(&self) -> usize {
        self.items.read().await.len()
    }

    pub async fn get(&self, id: &str) -> Option<T> {
        self.items.read().await.get(id).cloned()
    }

    pub async fn create(&self, mut resource: T) -> Result<T, StoreError> {
        let now = unix_timestamp_string();
        let requested_id = resource.metadata().id.clone();
        let id = if requested_id.is_empty() {
            let next = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
            format!("{}-{:04}", self.prefix, next)
        } else {
            requested_id
        };

        let mut items = self.items.write().await;
        if items.contains_key(&id) {
            return Err(StoreError::AlreadyExists {
                resource: self.resource,
                id,
            });
        }

        let metadata = resource.metadata_mut();
        metadata.id = id.clone();
        metadata.resource_version = "1".to_string();
        metadata.created_at = now.clone();
        metadata.updated_at = now;

        items.insert(id, resource.clone());
        Ok(resource)
    }

    pub async fn replace(&self, id: &str, mut resource: T) -> Result<T, StoreError> {
        let mut items = self.items.write().await;
        let Some(existing) = items.get(id).cloned() else {
            return Err(StoreError::NotFound {
                resource: self.resource,
                id: id.to_string(),
            });
        };

        let metadata = resource.metadata_mut();
        metadata.id = id.to_string();
        metadata.created_at = existing.metadata().created_at.clone();
        metadata.updated_at = unix_timestamp_string();
        let next_version = existing
            .metadata()
            .resource_version
            .parse::<u64>()
            .unwrap_or(0)
            + 1;
        metadata.resource_version = next_version.to_string();

        items.insert(id.to_string(), resource.clone());
        Ok(resource)
    }

    pub async fn delete(&self, id: &str) -> Result<T, StoreError> {
        self.items
            .write()
            .await
            .remove(id)
            .ok_or_else(|| StoreError::NotFound {
                resource: self.resource,
                id: id.to_string(),
            })
    }

    async fn snapshot(&self) -> PersistedResourceStore<T> {
        PersistedResourceStore {
            counter: self.counter.load(Ordering::Relaxed),
            items: self.items.read().await.clone(),
        }
    }

    async fn restore(&self, snapshot: PersistedResourceStore<T>) {
        self.counter.store(snapshot.counter, Ordering::Relaxed);
        *self.items.write().await = snapshot.items;
    }
}

pub struct InMemoryControllerStore {
    pub tenants: ResourceStore<TenantResource>,
    pub nodes: ResourceStore<NodeResource>,
    pub networks: ResourceStore<NetworkResource>,
    pub ports: ResourceStore<PortResource>,
    pub security_groups: ResourceStore<SecurityGroupResource>,
    pub route_tables: ResourceStore<RouteTableResource>,
    generation: AtomicU64,
    southbound_nodes: RwLock<BTreeMap<String, SouthboundNodeRuntime>>,
    southbound_publishes: RwLock<BTreeMap<String, DesiredStatePublishRecord>>,
}

impl InMemoryControllerStore {
    pub fn new() -> Self {
        Self {
            tenants: ResourceStore::new("tenant", "tenant"),
            nodes: ResourceStore::new("node", "node"),
            networks: ResourceStore::new("network", "network"),
            ports: ResourceStore::new("port", "port"),
            security_groups: ResourceStore::new("security_group", "sg"),
            route_tables: ResourceStore::new("route_table", "rt"),
            generation: AtomicU64::new(0),
            southbound_nodes: RwLock::new(BTreeMap::new()),
            southbound_publishes: RwLock::new(BTreeMap::new()),
        }
    }

    async fn resource_counts_inner(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        counts.insert("tenants".to_string(), self.tenants.count().await);
        counts.insert("nodes".to_string(), self.nodes.count().await);
        counts.insert("networks".to_string(), self.networks.count().await);
        counts.insert("ports".to_string(), self.ports.count().await);
        counts.insert(
            "security_groups".to_string(),
            self.security_groups.count().await,
        );
        counts.insert("route_tables".to_string(), self.route_tables.count().await);
        counts
    }

    async fn list_resource<T>(&self, store: &ResourceStore<T>) -> Vec<T>
    where
        T: StoredResource,
    {
        store.list().await
    }

    async fn get_resource<T>(&self, store: &ResourceStore<T>, id: &str) -> Option<T>
    where
        T: StoredResource,
    {
        store.get(id).await
    }

    async fn create_resource<T>(
        &self,
        store: &ResourceStore<T>,
        resource: T,
    ) -> Result<T, StoreError>
    where
        T: StoredResource,
    {
        let created = store.create(resource).await?;
        self.bump_generation_inner();
        Ok(created)
    }

    async fn update_resource<T>(
        &self,
        store: &ResourceStore<T>,
        id: &str,
        resource: T,
    ) -> Result<T, StoreError>
    where
        T: StoredResource,
    {
        let updated = store.replace(id, resource).await?;
        self.bump_generation_inner();
        Ok(updated)
    }

    async fn delete_resource<T>(&self, store: &ResourceStore<T>, id: &str) -> Result<T, StoreError>
    where
        T: StoredResource,
    {
        let deleted = store.delete(id).await?;
        self.bump_generation_inner();
        Ok(deleted)
    }

    fn current_generation_inner(&self) -> String {
        self.generation.load(Ordering::Relaxed).to_string()
    }

    fn bump_generation_inner(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn desired_state_object_counts(
        tenants: &[TenantResource],
        networks: &[NetworkResource],
        ports: &[PortResource],
        security_groups: &[SecurityGroupResource],
        route_tables: &[RouteTableResource],
        deletes: usize,
    ) -> BTreeMap<String, usize> {
        BTreeMap::from([
            ("tenants".to_string(), tenants.len()),
            ("networks".to_string(), networks.len()),
            ("ports".to_string(), ports.len()),
            ("security_groups".to_string(), security_groups.len()),
            ("route_tables".to_string(), route_tables.len()),
            ("deletes".to_string(), deletes),
        ])
    }

    async fn published_desired_state(
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

    fn derive_sync_status(
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

    fn southbound_status_from_parts(
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

        SouthboundNodeStatusResponse {
            node_id: node_id.to_string(),
            desired_generation,
            last_applied_generation,
            last_seen_at,
            last_desired_state,
            sync_status,
            registration,
            last_apply_status,
            last_health,
        }
    }

    async fn snapshot_state(&self) -> PersistedControllerState {
        PersistedControllerState {
            tenants: self.tenants.snapshot().await,
            nodes: self.nodes.snapshot().await,
            networks: self.networks.snapshot().await,
            ports: self.ports.snapshot().await,
            security_groups: self.security_groups.snapshot().await,
            route_tables: self.route_tables.snapshot().await,
            generation: self.generation.load(Ordering::Relaxed),
            southbound_nodes: self.southbound_nodes.read().await.clone(),
            southbound_publishes: self.southbound_publishes.read().await.clone(),
        }
    }

    async fn restore_state(&self, snapshot: PersistedControllerState) {
        self.tenants.restore(snapshot.tenants).await;
        self.nodes.restore(snapshot.nodes).await;
        self.networks.restore(snapshot.networks).await;
        self.ports.restore(snapshot.ports).await;
        self.security_groups.restore(snapshot.security_groups).await;
        self.route_tables.restore(snapshot.route_tables).await;
        self.generation
            .store(snapshot.generation, Ordering::Relaxed);
        *self.southbound_nodes.write().await = snapshot.southbound_nodes;
        *self.southbound_publishes.write().await = snapshot.southbound_publishes;
    }

    async fn record_registration_inner(
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
                    .or_insert_with(|| SouthboundNodeRuntime {
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

    async fn record_apply_status_inner(
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
                    .or_insert_with(|| SouthboundNodeRuntime {
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

    async fn record_health_inner(
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
                    .or_insert_with(|| SouthboundNodeRuntime {
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

    async fn southbound_status_inner(
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

    async fn clear_southbound_runtime_inner(&self, node_id: &str) {
        self.southbound_nodes.write().await.remove(node_id);
    }

    async fn clear_southbound_publish_inner(&self, node_id: &str) {
        self.southbound_publishes.write().await.remove(node_id);
    }

    async fn desired_state_for_node_inner(
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
            &route_tables,
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
                route_tables,
                deletes: Vec::new(),
            },
            changed,
        ))
    }
}

pub struct FileBackedControllerStore {
    inner: InMemoryControllerStore,
    snapshot_path: PathBuf,
    mutation_lock: Mutex<()>,
}

impl FileBackedControllerStore {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let store = Self {
            inner: InMemoryControllerStore::new(),
            snapshot_path: path.into(),
            mutation_lock: Mutex::new(()),
        };
        store.load_snapshot().await?;
        Ok(store)
    }

    async fn load_snapshot(&self) -> Result<(), StoreError> {
        match fs::read(&self.snapshot_path).await {
            Ok(bytes) => {
                let snapshot =
                    serde_json::from_slice::<PersistedControllerState>(&bytes).map_err(|err| {
                        StoreError::Internal(format!(
                            "failed to decode controller snapshot {}: {err}",
                            self.snapshot_path.display()
                        ))
                    })?;
                self.inner.restore_state(snapshot).await;
                Ok(())
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(StoreError::Internal(format!(
                "failed to read controller snapshot {}: {err}",
                self.snapshot_path.display()
            ))),
        }
    }

    async fn persist_snapshot_locked(
        &self,
        snapshot: &PersistedControllerState,
    ) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(snapshot).map_err(|err| {
            StoreError::Internal(format!(
                "failed to encode controller snapshot {}: {err}",
                self.snapshot_path.display()
            ))
        })?;

        if let Some(parent) = self.snapshot_path.parent() {
            fs::create_dir_all(parent).await.map_err(|err| {
                StoreError::Internal(format!(
                    "failed to create controller state directory {}: {err}",
                    parent.display()
                ))
            })?;
        }

        let tmp_path = temp_snapshot_path(&self.snapshot_path);
        fs::write(&tmp_path, bytes).await.map_err(|err| {
            StoreError::Internal(format!(
                "failed to write controller snapshot {}: {err}",
                tmp_path.display()
            ))
        })?;
        fs::rename(&tmp_path, &self.snapshot_path)
            .await
            .map_err(|err| {
                StoreError::Internal(format!(
                    "failed to replace controller snapshot {}: {err}",
                    self.snapshot_path.display()
                ))
            })?;
        Ok(())
    }

    async fn run_persisted<T, F, Fut>(&self, op: F) -> Result<T, StoreError>
    where
        F: FnOnce(&InMemoryControllerStore) -> Fut,
        Fut: std::future::Future<Output = Result<T, StoreError>>,
    {
        let _guard = self.mutation_lock.lock().await;
        let before = self.inner.snapshot_state().await;
        let value = op(&self.inner).await?;
        let after = self.inner.snapshot_state().await;

        if let Err(err) = self.persist_snapshot_locked(&after).await {
            self.inner.restore_state(before).await;
            return Err(err);
        }

        Ok(value)
    }
}

fn temp_snapshot_path(path: &Path) -> PathBuf {
    let mut tmp = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|ext| format!("{ext}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    tmp.set_extension(extension);
    tmp
}

macro_rules! impl_resource_methods {
    ($list:ident, $get:ident, $create:ident, $update:ident, $delete:ident, $ty:ty, $field:ident) => {
        async fn $list(&self) -> Vec<$ty> {
            self.list_resource(&self.$field).await
        }

        async fn $get(&self, id: &str) -> Option<$ty> {
            self.get_resource(&self.$field, id).await
        }

        async fn $create(&self, resource: $ty) -> Result<$ty, StoreError> {
            self.create_resource(&self.$field, resource).await
        }

        async fn $update(&self, id: &str, resource: $ty) -> Result<$ty, StoreError> {
            self.update_resource(&self.$field, id, resource).await
        }

        async fn $delete(&self, id: &str) -> Result<$ty, StoreError> {
            self.delete_resource(&self.$field, id).await
        }
    };
}

macro_rules! impl_file_backed_resource_methods {
    ($list:ident, $get:ident, $create:ident, $update:ident, $delete:ident, $ty:ty) => {
        async fn $list(&self) -> Vec<$ty> {
            self.inner.$list().await
        }

        async fn $get(&self, id: &str) -> Option<$ty> {
            self.inner.$get(id).await
        }

        async fn $create(&self, resource: $ty) -> Result<$ty, StoreError> {
            self.run_persisted(|inner| inner.$create(resource)).await
        }

        async fn $update(&self, id: &str, resource: $ty) -> Result<$ty, StoreError> {
            self.run_persisted(|inner| inner.$update(id, resource))
                .await
        }

        async fn $delete(&self, id: &str) -> Result<$ty, StoreError> {
            self.run_persisted(|inner| inner.$delete(id)).await
        }
    };
}

#[async_trait]
impl ControllerStore for InMemoryControllerStore {
    async fn resource_counts(&self) -> BTreeMap<String, usize> {
        self.resource_counts_inner().await
    }

    fn current_generation(&self) -> String {
        self.current_generation_inner()
    }

    impl_resource_methods!(
        list_tenants,
        get_tenant,
        create_tenant,
        update_tenant,
        delete_tenant,
        TenantResource,
        tenants
    );
    impl_resource_methods!(
        list_networks,
        get_network,
        create_network,
        update_network,
        delete_network,
        NetworkResource,
        networks
    );
    impl_resource_methods!(
        list_ports,
        get_port,
        create_port,
        update_port,
        delete_port,
        PortResource,
        ports
    );
    impl_resource_methods!(
        list_security_groups,
        get_security_group,
        create_security_group,
        update_security_group,
        delete_security_group,
        SecurityGroupResource,
        security_groups
    );
    impl_resource_methods!(
        list_route_tables,
        get_route_table,
        create_route_table,
        update_route_table,
        delete_route_table,
        RouteTableResource,
        route_tables
    );

    // `node` keeps a hand-written delete path because removing a node must
    // also purge any cached southbound runtime state keyed by the same ID.
    async fn list_nodes(&self) -> Vec<NodeResource> {
        self.list_resource(&self.nodes).await
    }

    async fn get_node(&self, id: &str) -> Option<NodeResource> {
        self.get_resource(&self.nodes, id).await
    }

    async fn create_node(&self, resource: NodeResource) -> Result<NodeResource, StoreError> {
        self.create_resource(&self.nodes, resource).await
    }

    async fn update_node(
        &self,
        id: &str,
        resource: NodeResource,
    ) -> Result<NodeResource, StoreError> {
        self.update_resource(&self.nodes, id, resource).await
    }

    async fn delete_node(&self, id: &str) -> Result<NodeResource, StoreError> {
        let deleted = self.nodes.delete(id).await?;
        self.clear_southbound_runtime_inner(id).await;
        self.clear_southbound_publish_inner(id).await;
        self.bump_generation_inner();
        Ok(deleted)
    }

    async fn record_registration(
        &self,
        node_id: &str,
        info: NodeInfo,
        capability: NodeCapability,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.record_registration_inner(node_id, info, capability)
            .await
    }

    async fn record_apply_status(
        &self,
        node_id: &str,
        report: ApplyStatusReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.record_apply_status_inner(node_id, report).await
    }

    async fn record_health(
        &self,
        node_id: &str,
        report: NodeHealthReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.record_health_inner(node_id, report).await
    }

    async fn southbound_status(
        &self,
        node_id: &str,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.southbound_status_inner(node_id).await
    }

    async fn clear_southbound_runtime(&self, node_id: &str) -> Result<(), StoreError> {
        self.clear_southbound_runtime_inner(node_id).await;
        Ok(())
    }

    async fn desired_state_for_node(
        &self,
        node_id: &str,
    ) -> Result<DesiredStateEnvelope, StoreError> {
        let (envelope, _) = self.desired_state_for_node_inner(node_id).await?;
        Ok(envelope)
    }
}

#[async_trait]
impl ControllerStore for FileBackedControllerStore {
    async fn resource_counts(&self) -> BTreeMap<String, usize> {
        self.inner.resource_counts().await
    }

    fn current_generation(&self) -> String {
        self.inner.current_generation()
    }

    impl_file_backed_resource_methods!(
        list_tenants,
        get_tenant,
        create_tenant,
        update_tenant,
        delete_tenant,
        TenantResource
    );
    impl_file_backed_resource_methods!(
        list_nodes,
        get_node,
        create_node,
        update_node,
        delete_node,
        NodeResource
    );
    impl_file_backed_resource_methods!(
        list_networks,
        get_network,
        create_network,
        update_network,
        delete_network,
        NetworkResource
    );
    impl_file_backed_resource_methods!(
        list_ports,
        get_port,
        create_port,
        update_port,
        delete_port,
        PortResource
    );
    impl_file_backed_resource_methods!(
        list_security_groups,
        get_security_group,
        create_security_group,
        update_security_group,
        delete_security_group,
        SecurityGroupResource
    );
    impl_file_backed_resource_methods!(
        list_route_tables,
        get_route_table,
        create_route_table,
        update_route_table,
        delete_route_table,
        RouteTableResource
    );

    async fn record_registration(
        &self,
        node_id: &str,
        info: NodeInfo,
        capability: NodeCapability,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.run_persisted(|inner| inner.record_registration(node_id, info, capability))
            .await
    }

    async fn record_apply_status(
        &self,
        node_id: &str,
        report: ApplyStatusReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.run_persisted(|inner| inner.record_apply_status(node_id, report))
            .await
    }

    async fn record_health(
        &self,
        node_id: &str,
        report: NodeHealthReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.run_persisted(|inner| inner.record_health(node_id, report))
            .await
    }

    async fn southbound_status(
        &self,
        node_id: &str,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.inner.southbound_status(node_id).await
    }

    async fn clear_southbound_runtime(&self, node_id: &str) -> Result<(), StoreError> {
        self.run_persisted(|inner| async move {
            inner.clear_southbound_runtime(node_id).await?;
            Ok(())
        })
        .await
    }

    async fn desired_state_for_node(
        &self,
        node_id: &str,
    ) -> Result<DesiredStateEnvelope, StoreError> {
        let _guard = self.mutation_lock.lock().await;
        let before = self.inner.snapshot_state().await;
        let (envelope, changed) = self.inner.desired_state_for_node_inner(node_id).await?;

        if changed {
            let after = self.inner.snapshot_state().await;
            if let Err(err) = self.persist_snapshot_locked(&after).await {
                self.inner.restore_state(before).await;
                return Err(err);
            }
        }

        Ok(envelope)
    }
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
