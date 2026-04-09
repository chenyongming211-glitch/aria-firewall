use aria_api::{
    ApplyStatusReport, DesiredStateEnvelope, NetworkResource, NodeCapability, NodeHealthReport,
    NodeInfo, NodeRegisterRequest, NodeResource, PortResource, ResourceMetadata,
    RouteTableResource, SecurityGroupResource, SouthboundNodeStatusResponse, TenantResource,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;

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
}

#[derive(Debug, Clone)]
pub struct SouthboundNodeRuntime {
    pub registration: Option<NodeRegisterRequest>,
    pub last_apply_status: Option<ApplyStatusReport>,
    pub last_health: Option<NodeHealthReport>,
    pub last_applied_generation: Option<String>,
    pub last_seen_at: String,
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
}

pub struct PlatformStore {
    pub tenants: ResourceStore<TenantResource>,
    pub nodes: ResourceStore<NodeResource>,
    pub networks: ResourceStore<NetworkResource>,
    pub ports: ResourceStore<PortResource>,
    pub security_groups: ResourceStore<SecurityGroupResource>,
    pub route_tables: ResourceStore<RouteTableResource>,
    generation: AtomicU64,
    southbound_nodes: RwLock<BTreeMap<String, SouthboundNodeRuntime>>,
}

impl PlatformStore {
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
        }
    }

    pub async fn resource_counts(&self) -> BTreeMap<String, usize> {
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

    pub fn current_generation(&self) -> String {
        self.generation.load(Ordering::Relaxed).to_string()
    }

    pub fn bump_generation(&self) -> String {
        (self.generation.fetch_add(1, Ordering::Relaxed) + 1).to_string()
    }

    pub async fn record_registration(
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
        let mut states = self.southbound_nodes.write().await;
        let entry = states
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

        Ok(SouthboundNodeStatusResponse {
            node_id: node_id.to_string(),
            desired_generation: self.current_generation(),
            last_applied_generation: entry.last_applied_generation.clone(),
            last_seen_at: entry.last_seen_at.clone(),
            registration: Some(registration),
            last_apply_status: entry.last_apply_status.clone(),
            last_health: entry.last_health.clone(),
        })
    }

    pub async fn record_apply_status(
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
        let mut states = self.southbound_nodes.write().await;
        let entry = states
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

        Ok(SouthboundNodeStatusResponse {
            node_id: node_id.to_string(),
            desired_generation: self.current_generation(),
            last_applied_generation: entry.last_applied_generation.clone(),
            last_seen_at: entry.last_seen_at.clone(),
            registration: entry.registration.clone(),
            last_apply_status: entry.last_apply_status.clone(),
            last_health: entry.last_health.clone(),
        })
    }

    pub async fn record_health(
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
        let mut states = self.southbound_nodes.write().await;
        let entry = states
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

        Ok(SouthboundNodeStatusResponse {
            node_id: node_id.to_string(),
            desired_generation: self.current_generation(),
            last_applied_generation: entry.last_applied_generation.clone(),
            last_seen_at: entry.last_seen_at.clone(),
            registration: entry.registration.clone(),
            last_apply_status: entry.last_apply_status.clone(),
            last_health: entry.last_health.clone(),
        })
    }

    pub async fn southbound_status(
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

        let states = self.southbound_nodes.read().await;
        if let Some(entry) = states.get(node_id) {
            Ok(SouthboundNodeStatusResponse {
                node_id: node_id.to_string(),
                desired_generation: self.current_generation(),
                last_applied_generation: entry.last_applied_generation.clone(),
                last_seen_at: entry.last_seen_at.clone(),
                registration: entry.registration.clone(),
                last_apply_status: entry.last_apply_status.clone(),
                last_health: entry.last_health.clone(),
            })
        } else {
            Ok(SouthboundNodeStatusResponse {
                node_id: node_id.to_string(),
                desired_generation: self.current_generation(),
                last_applied_generation: None,
                last_seen_at: unix_timestamp_string(),
                registration: None,
                last_apply_status: None,
                last_health: None,
            })
        }
    }

    pub async fn clear_southbound_runtime(&self, node_id: &str) {
        self.southbound_nodes.write().await.remove(node_id);
    }

    pub async fn desired_state_for_node(
        &self,
        node_id: &str,
    ) -> Result<DesiredStateEnvelope, StoreError> {
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

        Ok(DesiredStateEnvelope {
            generation: self.current_generation(),
            full_sync: true,
            issued_at: unix_timestamp_string(),
            node_id: node_id.to_string(),
            tenants,
            networks,
            ports,
            security_groups,
            route_tables,
            deletes: Vec::new(),
        })
    }
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
