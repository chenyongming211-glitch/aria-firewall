use aria_api::{
    NetworkResource, NodeResource, PortResource, ResourceMetadata, RouteTableResource,
    SecurityGroupResource, TenantResource,
};
use std::{
    collections::BTreeMap,
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
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
