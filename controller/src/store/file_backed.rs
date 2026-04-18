use aria_api::{
    ApplyStatusReport, BackendSetResource, DesiredStateEnvelope, HealthCheckResource,
    IpGroupResource, MirrorPolicyResource, NetworkPolicyResource, NetworkResource, NodeCapability,
    NodeHealthReport, NodeInfo, NodeResource, PortResource, QosPolicyResource, RouteTableResource,
    SecurityGroupResource, ServiceResource, SouthboundNodeStatusResponse, TenantResource,
};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::{fs, sync::Mutex};

use super::in_memory::InMemoryControllerStore;
use super::resource_store::PersistedControllerState;
use super::{ControllerStore, StoreError};

pub struct FileBackedControllerStore {
    pub(crate) inner: InMemoryControllerStore,
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

    pub(crate) async fn load_snapshot(&self) -> Result<(), StoreError> {
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
impl ControllerStore for FileBackedControllerStore {
    async fn resource_counts(&self) -> BTreeMap<String, usize> {
        self.inner.resource_counts().await
    }

    fn current_generation(&self) -> String {
        self.inner.current_generation()
    }

    async fn list_tenants(&self) -> Vec<TenantResource> {
        self.inner.list_tenants().await
    }

    async fn get_tenant(&self, id: &str) -> Option<TenantResource> {
        self.inner.get_tenant(id).await
    }

    async fn create_tenant(&self, resource: TenantResource) -> Result<TenantResource, StoreError> {
        self.run_persisted(|inner| inner.create_tenant(resource))
            .await
    }

    async fn update_tenant(
        &self,
        id: &str,
        resource: TenantResource,
    ) -> Result<TenantResource, StoreError> {
        self.run_persisted(|inner| inner.update_tenant(id, resource))
            .await
    }

    async fn delete_tenant(&self, id: &str) -> Result<TenantResource, StoreError> {
        self.run_persisted(|inner| inner.delete_tenant(id)).await
    }

    async fn list_nodes(&self) -> Vec<NodeResource> {
        self.inner.list_nodes().await
    }

    async fn get_node(&self, id: &str) -> Option<NodeResource> {
        self.inner.get_node(id).await
    }

    async fn create_node(&self, resource: NodeResource) -> Result<NodeResource, StoreError> {
        self.run_persisted(|inner| inner.create_node(resource))
            .await
    }

    async fn update_node(
        &self,
        id: &str,
        resource: NodeResource,
    ) -> Result<NodeResource, StoreError> {
        self.run_persisted(|inner| inner.update_node(id, resource))
            .await
    }

    async fn delete_node(&self, id: &str) -> Result<NodeResource, StoreError> {
        self.run_persisted(|inner| inner.delete_node(id)).await
    }

    async fn list_networks(&self) -> Vec<NetworkResource> {
        self.inner.list_networks().await
    }

    async fn get_network(&self, id: &str) -> Option<NetworkResource> {
        self.inner.get_network(id).await
    }

    async fn create_network(
        &self,
        resource: NetworkResource,
    ) -> Result<NetworkResource, StoreError> {
        self.run_persisted(|inner| inner.create_network(resource))
            .await
    }

    async fn update_network(
        &self,
        id: &str,
        resource: NetworkResource,
    ) -> Result<NetworkResource, StoreError> {
        self.run_persisted(|inner| inner.update_network(id, resource))
            .await
    }

    async fn delete_network(&self, id: &str) -> Result<NetworkResource, StoreError> {
        self.run_persisted(|inner| inner.delete_network(id)).await
    }

    async fn list_ports(&self) -> Vec<PortResource> {
        self.inner.list_ports().await
    }

    async fn get_port(&self, id: &str) -> Option<PortResource> {
        self.inner.get_port(id).await
    }

    async fn create_port(&self, resource: PortResource) -> Result<PortResource, StoreError> {
        self.run_persisted(|inner| inner.create_port(resource))
            .await
    }

    async fn update_port(
        &self,
        id: &str,
        resource: PortResource,
    ) -> Result<PortResource, StoreError> {
        self.run_persisted(|inner| inner.update_port(id, resource))
            .await
    }

    async fn delete_port(&self, id: &str) -> Result<PortResource, StoreError> {
        self.run_persisted(|inner| inner.delete_port(id)).await
    }

    async fn list_security_groups(&self) -> Vec<SecurityGroupResource> {
        self.inner.list_security_groups().await
    }

    async fn get_security_group(&self, id: &str) -> Option<SecurityGroupResource> {
        self.inner.get_security_group(id).await
    }

    async fn create_security_group(
        &self,
        resource: SecurityGroupResource,
    ) -> Result<SecurityGroupResource, StoreError> {
        self.run_persisted(|inner| inner.create_security_group(resource))
            .await
    }

    async fn update_security_group(
        &self,
        id: &str,
        resource: SecurityGroupResource,
    ) -> Result<SecurityGroupResource, StoreError> {
        self.run_persisted(|inner| inner.update_security_group(id, resource))
            .await
    }

    async fn delete_security_group(&self, id: &str) -> Result<SecurityGroupResource, StoreError> {
        self.run_persisted(|inner| inner.delete_security_group(id))
            .await
    }

    async fn list_route_tables(&self) -> Vec<RouteTableResource> {
        self.inner.list_route_tables().await
    }

    async fn get_route_table(&self, id: &str) -> Option<RouteTableResource> {
        self.inner.get_route_table(id).await
    }

    async fn create_route_table(
        &self,
        resource: RouteTableResource,
    ) -> Result<RouteTableResource, StoreError> {
        self.run_persisted(|inner| inner.create_route_table(resource))
            .await
    }

    async fn update_route_table(
        &self,
        id: &str,
        resource: RouteTableResource,
    ) -> Result<RouteTableResource, StoreError> {
        self.run_persisted(|inner| inner.update_route_table(id, resource))
            .await
    }

    async fn delete_route_table(&self, id: &str) -> Result<RouteTableResource, StoreError> {
        self.run_persisted(|inner| inner.delete_route_table(id))
            .await
    }

    async fn list_ip_groups(&self) -> Vec<IpGroupResource> {
        self.inner.list_ip_groups().await
    }

    async fn get_ip_group(&self, id: &str) -> Option<IpGroupResource> {
        self.inner.get_ip_group(id).await
    }

    async fn create_ip_group(
        &self,
        resource: IpGroupResource,
    ) -> Result<IpGroupResource, StoreError> {
        self.run_persisted(|inner| inner.create_ip_group(resource))
            .await
    }

    async fn update_ip_group(
        &self,
        id: &str,
        resource: IpGroupResource,
    ) -> Result<IpGroupResource, StoreError> {
        self.run_persisted(|inner| inner.update_ip_group(id, resource))
            .await
    }

    async fn delete_ip_group(&self, id: &str) -> Result<IpGroupResource, StoreError> {
        self.run_persisted(|inner| inner.delete_ip_group(id)).await
    }

    async fn list_network_policies(&self) -> Vec<NetworkPolicyResource> {
        self.inner.list_network_policies().await
    }

    async fn get_network_policy(&self, id: &str) -> Option<NetworkPolicyResource> {
        self.inner.get_network_policy(id).await
    }

    async fn create_network_policy(
        &self,
        resource: NetworkPolicyResource,
    ) -> Result<NetworkPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.create_network_policy(resource))
            .await
    }

    async fn update_network_policy(
        &self,
        id: &str,
        resource: NetworkPolicyResource,
    ) -> Result<NetworkPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.update_network_policy(id, resource))
            .await
    }

    async fn delete_network_policy(&self, id: &str) -> Result<NetworkPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.delete_network_policy(id))
            .await
    }

    async fn list_health_checks(&self) -> Vec<HealthCheckResource> {
        self.inner.list_health_checks().await
    }

    async fn get_health_check(&self, id: &str) -> Option<HealthCheckResource> {
        self.inner.get_health_check(id).await
    }

    async fn create_health_check(
        &self,
        resource: HealthCheckResource,
    ) -> Result<HealthCheckResource, StoreError> {
        self.run_persisted(|inner| inner.create_health_check(resource))
            .await
    }

    async fn update_health_check(
        &self,
        id: &str,
        resource: HealthCheckResource,
    ) -> Result<HealthCheckResource, StoreError> {
        self.run_persisted(|inner| inner.update_health_check(id, resource))
            .await
    }

    async fn delete_health_check(&self, id: &str) -> Result<HealthCheckResource, StoreError> {
        self.run_persisted(|inner| inner.delete_health_check(id))
            .await
    }

    async fn list_backend_sets(&self) -> Vec<BackendSetResource> {
        self.inner.list_backend_sets().await
    }

    async fn get_backend_set(&self, id: &str) -> Option<BackendSetResource> {
        self.inner.get_backend_set(id).await
    }

    async fn create_backend_set(
        &self,
        resource: BackendSetResource,
    ) -> Result<BackendSetResource, StoreError> {
        self.run_persisted(|inner| inner.create_backend_set(resource))
            .await
    }

    async fn update_backend_set(
        &self,
        id: &str,
        resource: BackendSetResource,
    ) -> Result<BackendSetResource, StoreError> {
        self.run_persisted(|inner| inner.update_backend_set(id, resource))
            .await
    }

    async fn delete_backend_set(&self, id: &str) -> Result<BackendSetResource, StoreError> {
        self.run_persisted(|inner| inner.delete_backend_set(id))
            .await
    }

    // --- Service ---
    async fn list_services(&self) -> Vec<ServiceResource> {
        self.inner.list_services().await
    }

    async fn get_service(&self, id: &str) -> Option<ServiceResource> {
        self.inner.get_service(id).await
    }

    async fn create_service(
        &self,
        resource: ServiceResource,
    ) -> Result<ServiceResource, StoreError> {
        self.run_persisted(|inner| inner.create_service(resource))
            .await
    }

    async fn update_service(
        &self,
        id: &str,
        resource: ServiceResource,
    ) -> Result<ServiceResource, StoreError> {
        self.run_persisted(|inner| inner.update_service(id, resource))
            .await
    }

    async fn delete_service(&self, id: &str) -> Result<ServiceResource, StoreError> {
        self.run_persisted(|inner| inner.delete_service(id)).await
    }

    // --- QosPolicy (Phase 3.6) ---
    async fn list_qos_policies(&self) -> Vec<QosPolicyResource> {
        self.inner.list_qos_policies().await
    }

    async fn get_qos_policy(&self, id: &str) -> Option<QosPolicyResource> {
        self.inner.get_qos_policy(id).await
    }

    async fn create_qos_policy(
        &self,
        resource: QosPolicyResource,
    ) -> Result<QosPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.create_qos_policy(resource))
            .await
    }

    async fn update_qos_policy(
        &self,
        id: &str,
        resource: QosPolicyResource,
    ) -> Result<QosPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.update_qos_policy(id, resource))
            .await
    }

    async fn delete_qos_policy(&self, id: &str) -> Result<QosPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.delete_qos_policy(id)).await
    }

    // --- MirrorPolicy (Phase 3.7) ---
    async fn list_mirror_policies(&self) -> Vec<MirrorPolicyResource> {
        self.inner.list_mirror_policies().await
    }

    async fn get_mirror_policy(&self, id: &str) -> Option<MirrorPolicyResource> {
        self.inner.get_mirror_policy(id).await
    }

    async fn create_mirror_policy(
        &self,
        resource: MirrorPolicyResource,
    ) -> Result<MirrorPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.create_mirror_policy(resource))
            .await
    }

    async fn update_mirror_policy(
        &self,
        id: &str,
        resource: MirrorPolicyResource,
    ) -> Result<MirrorPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.update_mirror_policy(id, resource))
            .await
    }

    async fn delete_mirror_policy(&self, id: &str) -> Result<MirrorPolicyResource, StoreError> {
        self.run_persisted(|inner| inner.delete_mirror_policy(id))
            .await
    }

    async fn record_registration(
        &self,
        node_id: &str,
        info: NodeInfo,
        capability: NodeCapability,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.inner
            .record_registration(node_id, info, capability)
            .await
    }

    async fn record_apply_status(
        &self,
        node_id: &str,
        report: ApplyStatusReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.inner.record_apply_status(node_id, report).await
    }

    async fn record_health(
        &self,
        node_id: &str,
        report: NodeHealthReport,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.inner.record_health(node_id, report).await
    }

    async fn southbound_status(
        &self,
        node_id: &str,
    ) -> Result<SouthboundNodeStatusResponse, StoreError> {
        self.inner.southbound_status(node_id).await
    }

    async fn clear_southbound_runtime(&self, node_id: &str) -> Result<(), StoreError> {
        self.inner.clear_southbound_runtime(node_id).await
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
