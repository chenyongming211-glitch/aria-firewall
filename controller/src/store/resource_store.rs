use aria_api::{
    BackendSetResource, HealthCheckResource, IpGroupResource, MirrorPolicyResource,
    NetworkPolicyResource, NetworkResource, NodeResource, PortResource, QosPolicyResource,
    ResourceMetadata, RouteTableResource, SecurityGroupResource, ServiceResource,
    TenantResource, DesiredStatePublishRecord,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;

use super::{StoredResource, StoreError};

// ---- Persisted types ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedResourceStore<T> {
    pub(crate) counter: u64,
    pub(crate) items: BTreeMap<String, T>,
}

impl<T> Default for PersistedResourceStore<T> {
    fn default() -> Self {
        Self {
            counter: 0,
            items: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedControllerState {
    pub(crate) tenants: PersistedResourceStore<TenantResource>,
    pub(crate) nodes: PersistedResourceStore<NodeResource>,
    pub(crate) networks: PersistedResourceStore<NetworkResource>,
    pub(crate) ports: PersistedResourceStore<PortResource>,
    pub(crate) security_groups: PersistedResourceStore<SecurityGroupResource>,
    pub(crate) route_tables: PersistedResourceStore<RouteTableResource>,
    #[serde(default)]
    pub(crate) ip_groups: PersistedResourceStore<IpGroupResource>,
    #[serde(default)]
    pub(crate) network_policies: PersistedResourceStore<NetworkPolicyResource>,
    #[serde(default)]
    pub(crate) qos_policies: PersistedResourceStore<QosPolicyResource>,
    #[serde(default)]
    pub(crate) mirror_policies: PersistedResourceStore<MirrorPolicyResource>,
    pub(crate) health_checks: PersistedResourceStore<HealthCheckResource>,
    pub(crate) backend_sets: PersistedResourceStore<BackendSetResource>,
    pub(crate) services: PersistedResourceStore<ServiceResource>,
    pub(crate) generation: u64,
    #[serde(default)]
    pub(crate) southbound_publishes: BTreeMap<String, DesiredStatePublishRecord>,
}

impl Default for PersistedControllerState {
    fn default() -> Self {
        Self {
            tenants: PersistedResourceStore::default(),
            nodes: PersistedResourceStore::default(),
            networks: PersistedResourceStore::default(),
            ports: PersistedResourceStore::default(),
            security_groups: PersistedResourceStore::default(),
            route_tables: PersistedResourceStore::default(),
            ip_groups: PersistedResourceStore::default(),
            network_policies: PersistedResourceStore::default(),
            qos_policies: PersistedResourceStore::default(),
            mirror_policies: PersistedResourceStore::default(),
            health_checks: PersistedResourceStore::default(),
            backend_sets: PersistedResourceStore::default(),
            services: PersistedResourceStore::default(),
            generation: 0,
            southbound_publishes: BTreeMap::new(),
        }
    }
}

// ---- ResourceStore ----

pub struct ResourceStore<T> {
    pub(crate) resource: &'static str,
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

    pub(crate) async fn snapshot(&self) -> PersistedResourceStore<T> {
        PersistedResourceStore {
            counter: self.counter.load(Ordering::Relaxed),
            items: self.items.read().await.clone(),
        }
    }

    pub(crate) async fn restore(&self, snapshot: PersistedResourceStore<T>) {
        self.counter.store(snapshot.counter, Ordering::Relaxed);
        *self.items.write().await = snapshot.items;
    }
}

pub(crate) fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
