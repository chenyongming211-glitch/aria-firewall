use aria_api::{
    ApplyStatusReport, BackendSetResource, DesiredStateEnvelope, HealthCheckResource,
    IpGroupResource, MirrorPolicyResource, NetworkPolicyResource, NetworkResource, NodeCapability,
    NodeHealthReport, NodeInfo, NodeResource, PortResource, QosPolicyResource, RouteTableResource,
    SecurityGroupResource, ServiceResource, SouthboundNodeStatusResponse, TenantResource,
};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, RwLock};

use super::resource_store::ResourceStore;
use super::{ControllerStore, SouthboundNodeRuntime, StoredResource, StoreError};

pub struct InMemoryControllerStore {
    pub tenants: ResourceStore<TenantResource>,
    pub nodes: ResourceStore<NodeResource>,
    pub networks: ResourceStore<NetworkResource>,
    pub ports: ResourceStore<PortResource>,
    pub security_groups: ResourceStore<SecurityGroupResource>,
    pub route_tables: ResourceStore<RouteTableResource>,
    pub ip_groups: ResourceStore<IpGroupResource>,
    pub network_policies: ResourceStore<NetworkPolicyResource>,
    pub qos_policies: ResourceStore<QosPolicyResource>,
    pub mirror_policies: ResourceStore<MirrorPolicyResource>,
    pub health_checks: ResourceStore<HealthCheckResource>,
    pub backend_sets: ResourceStore<BackendSetResource>,
    pub services: ResourceStore<ServiceResource>,
    pub(crate) generation: AtomicU64,
    // High-frequency southbound runtime stays in memory so heartbeat/status
    // updates do not rewrite the controller snapshot on every report.
    pub(crate) southbound_nodes: RwLock<BTreeMap<String, SouthboundNodeRuntime>>,
    pub(crate) southbound_publishes: RwLock<BTreeMap<String, aria_api::DesiredStatePublishRecord>>,
    pub(crate) mutation_lock: Mutex<()>,
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
            ip_groups: ResourceStore::new("ip_group", "ipg"),
            network_policies: ResourceStore::new("network_policy", "npol"),
            qos_policies: ResourceStore::new("qos_policy", "qos"),
            mirror_policies: ResourceStore::new("mirror_policy", "mpol"),
            health_checks: ResourceStore::new("health_check", "hc"),
            backend_sets: ResourceStore::new("backend_set", "bset"),
            services: ResourceStore::new("service", "svc"),
            generation: AtomicU64::new(0),
            southbound_nodes: RwLock::new(BTreeMap::new()),
            southbound_publishes: RwLock::new(BTreeMap::new()),
            mutation_lock: Mutex::new(()),
        }
    }

    pub(crate) async fn resource_counts_inner(&self) -> BTreeMap<String, usize> {
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
        counts.insert("ip_groups".to_string(), self.ip_groups.count().await);
        counts.insert(
            "network_policies".to_string(),
            self.network_policies.count().await,
        );
        counts.insert("qos_policies".to_string(), self.qos_policies.count().await);
        counts.insert(
            "mirror_policies".to_string(),
            self.mirror_policies.count().await,
        );
        counts.insert(
            "health_checks".to_string(),
            self.health_checks.count().await,
        );
        counts.insert("backend_sets".to_string(), self.backend_sets.count().await);
        counts.insert("services".to_string(), self.services.count().await);
        counts
    }

    pub(crate) async fn list_resource<T>(&self, store: &ResourceStore<T>) -> Vec<T>
    where
        T: StoredResource,
    {
        store.list().await
    }

    pub(crate) async fn get_resource<T>(&self, store: &ResourceStore<T>, id: &str) -> Option<T>
    where
        T: StoredResource,
    {
        store.get(id).await
    }

    pub(crate) async fn create_resource<T>(
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

    pub(crate) async fn update_resource<T>(
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

    pub(crate) async fn delete_resource<T>(&self, store: &ResourceStore<T>, id: &str) -> Result<T, StoreError>
    where
        T: StoredResource,
    {
        let deleted = store.delete(id).await?;
        self.bump_generation_inner();
        Ok(deleted)
    }

    pub(crate) async fn run_mutation<T, Fut>(&self, fut: Fut) -> Result<T, StoreError>
    where
        Fut: std::future::Future<Output = Result<T, StoreError>>,
    {
        let _guard = self.mutation_lock.lock().await;
        fut.await
    }
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

#[async_trait]
impl ControllerStore for InMemoryControllerStore {
    async fn resource_counts(&self) -> BTreeMap<String, usize> {
        self.resource_counts_inner().await
    }

    fn current_generation(&self) -> String {
        self.current_generation_inner()
    }

    async fn list_tenants(&self) -> Vec<TenantResource> {
        self.list_resource(&self.tenants).await
    }

    async fn get_tenant(&self, id: &str) -> Option<TenantResource> {
        self.get_resource(&self.tenants, id).await
    }

    async fn create_tenant(&self, resource: TenantResource) -> Result<TenantResource, StoreError> {
        self.run_mutation(async { self.create_resource(&self.tenants, resource).await })
            .await
    }

    async fn update_tenant(
        &self,
        id: &str,
        resource: TenantResource,
    ) -> Result<TenantResource, StoreError> {
        self.run_mutation(async {
            self.update_resource(&self.tenants, id, resource).await
        })
        .await
    }

    async fn delete_tenant(&self, id: &str) -> Result<TenantResource, StoreError> {
        self.run_mutation(async {
            self.ensure_tenant_delete_allowed_inner(id).await?;
            self.delete_resource(&self.tenants, id).await
        })
        .await
    }

    async fn list_networks(&self) -> Vec<NetworkResource> {
        self.list_resource(&self.networks).await
    }

    async fn get_network(&self, id: &str) -> Option<NetworkResource> {
        self.get_resource(&self.networks, id).await
    }

    async fn create_network(
        &self,
        resource: NetworkResource,
    ) -> Result<NetworkResource, StoreError> {
        self.run_mutation(async {
            self.validate_network_resource_inner(&resource).await?;
            self.create_resource(&self.networks, resource).await
        })
        .await
    }

    async fn update_network(
        &self,
        id: &str,
        resource: NetworkResource,
    ) -> Result<NetworkResource, StoreError> {
        self.run_mutation(async {
            let existing = self
                .networks
                .get(id)
                .await
                .ok_or_else(|| StoreError::NotFound {
                    resource: "network",
                    id: id.to_string(),
                })?;
            self.validate_network_resource_inner(&resource).await?;
            if existing.spec.tenant_id != resource.spec.tenant_id {
                self.ensure_network_tenant_change_allowed_inner(id).await?;
            }
            self.update_resource(&self.networks, id, resource).await
        })
        .await
    }

    async fn delete_network(&self, id: &str) -> Result<NetworkResource, StoreError> {
        self.run_mutation(async {
            self.ensure_network_delete_allowed_inner(id).await?;
            self.delete_resource(&self.networks, id).await
        })
        .await
    }

    async fn list_ports(&self) -> Vec<PortResource> {
        self.list_resource(&self.ports).await
    }

    async fn get_port(&self, id: &str) -> Option<PortResource> {
        self.get_resource(&self.ports, id).await
    }

    async fn create_port(&self, resource: PortResource) -> Result<PortResource, StoreError> {
        self.run_mutation(async {
            self.validate_port_resource_inner(&resource).await?;
            self.create_resource(&self.ports, resource).await
        })
        .await
    }

    async fn update_port(
        &self,
        id: &str,
        resource: PortResource,
    ) -> Result<PortResource, StoreError> {
        self.run_mutation(async {
            self
                .ports
                .get(id)
                .await
                .ok_or_else(|| StoreError::NotFound {
                    resource: "port",
                    id: id.to_string(),
                })?;
            self.validate_port_resource_inner(&resource).await?;
            self.update_resource(&self.ports, id, resource).await
        })
        .await
    }

    async fn delete_port(&self, id: &str) -> Result<PortResource, StoreError> {
        self.run_mutation(async { self.delete_resource(&self.ports, id).await })
            .await
    }

    async fn list_security_groups(&self) -> Vec<SecurityGroupResource> {
        self.list_resource(&self.security_groups).await
    }

    async fn get_security_group(&self, id: &str) -> Option<SecurityGroupResource> {
        self.get_resource(&self.security_groups, id).await
    }

    async fn create_security_group(
        &self,
        resource: SecurityGroupResource,
    ) -> Result<SecurityGroupResource, StoreError> {
        self.run_mutation(async {
            self
                .validate_security_group_resource_inner(&resource)
                .await?;
            self
                .create_resource(&self.security_groups, resource)
                .await
        })
        .await
    }

    async fn update_security_group(
        &self,
        id: &str,
        resource: SecurityGroupResource,
    ) -> Result<SecurityGroupResource, StoreError> {
        self.run_mutation(async {
            let existing =
                self
                    .security_groups
                    .get(id)
                    .await
                    .ok_or_else(|| StoreError::NotFound {
                        resource: "security_group",
                        id: id.to_string(),
                    })?;
            self
                .validate_security_group_resource_inner(&resource)
                .await?;
            if existing.spec.tenant_id != resource.spec.tenant_id {
                self.ensure_security_group_delete_allowed_inner(id).await?;
            }
            self
                .update_resource(&self.security_groups, id, resource)
                .await
        })
        .await
    }

    async fn delete_security_group(&self, id: &str) -> Result<SecurityGroupResource, StoreError> {
        self.run_mutation(async {
            self.ensure_security_group_delete_allowed_inner(id).await?;
            self.delete_resource(&self.security_groups, id).await
        })
        .await
    }

    async fn list_route_tables(&self) -> Vec<RouteTableResource> {
        self.list_resource(&self.route_tables).await
    }

    async fn get_route_table(&self, id: &str) -> Option<RouteTableResource> {
        self.get_resource(&self.route_tables, id).await
    }

    async fn create_route_table(
        &self,
        resource: RouteTableResource,
    ) -> Result<RouteTableResource, StoreError> {
        self.run_mutation(async {
            self.validate_route_table_resource_inner(&resource).await?;
            self.create_resource(&self.route_tables, resource).await
        })
        .await
    }

    async fn update_route_table(
        &self,
        id: &str,
        resource: RouteTableResource,
    ) -> Result<RouteTableResource, StoreError> {
        self.run_mutation(async {
            self
                .route_tables
                .get(id)
                .await
                .ok_or_else(|| StoreError::NotFound {
                    resource: "route_table",
                    id: id.to_string(),
                })?;
            self.validate_route_table_resource_inner(&resource).await?;
            self
                .update_resource(&self.route_tables, id, resource)
                .await
        })
        .await
    }

    async fn delete_route_table(&self, id: &str) -> Result<RouteTableResource, StoreError> {
        self.run_mutation(async { self.delete_resource(&self.route_tables, id).await })
            .await
    }

    async fn list_ip_groups(&self) -> Vec<IpGroupResource> {
        self.list_resource(&self.ip_groups).await
    }

    async fn get_ip_group(&self, id: &str) -> Option<IpGroupResource> {
        self.get_resource(&self.ip_groups, id).await
    }

    async fn create_ip_group(
        &self,
        resource: IpGroupResource,
    ) -> Result<IpGroupResource, StoreError> {
        self.run_mutation(async {
            self.validate_ip_group_resource_inner(&resource).await?;
            self.create_resource(&self.ip_groups, resource).await
        })
        .await
    }

    async fn update_ip_group(
        &self,
        id: &str,
        resource: IpGroupResource,
    ) -> Result<IpGroupResource, StoreError> {
        self.run_mutation(async {
            let existing = self
                .ip_groups
                .get(id)
                .await
                .ok_or_else(|| StoreError::NotFound {
                    resource: "ip_group",
                    id: id.to_string(),
                })?;
            self.validate_ip_group_resource_inner(&resource).await?;
            if existing.spec.tenant_id != resource.spec.tenant_id
                || existing.spec.network_id != resource.spec.network_id
            {
                self.ensure_ip_group_delete_allowed_inner(id).await?;
            }
            self.update_resource(&self.ip_groups, id, resource).await
        })
        .await
    }

    async fn delete_ip_group(&self, id: &str) -> Result<IpGroupResource, StoreError> {
        self.run_mutation(async {
            self.ensure_ip_group_delete_allowed_inner(id).await?;
            self.delete_resource(&self.ip_groups, id).await
        })
        .await
    }

    async fn list_network_policies(&self) -> Vec<NetworkPolicyResource> {
        self.list_resource(&self.network_policies).await
    }

    async fn get_network_policy(&self, id: &str) -> Option<NetworkPolicyResource> {
        self.get_resource(&self.network_policies, id).await
    }

    async fn create_network_policy(
        &self,
        resource: NetworkPolicyResource,
    ) -> Result<NetworkPolicyResource, StoreError> {
        self.run_mutation(async {
            self
                .validate_network_policy_resource_inner(&resource)
                .await?;
            self
                .create_resource(&self.network_policies, resource)
                .await
        })
        .await
    }

    async fn update_network_policy(
        &self,
        id: &str,
        resource: NetworkPolicyResource,
    ) -> Result<NetworkPolicyResource, StoreError> {
        self.run_mutation(async {
            self
                .network_policies
                .get(id)
                .await
                .ok_or_else(|| StoreError::NotFound {
                    resource: "network_policy",
                    id: id.to_string(),
                })?;
            self
                .validate_network_policy_resource_inner(&resource)
                .await?;
            self
                .update_resource(&self.network_policies, id, resource)
                .await
        })
        .await
    }

    async fn delete_network_policy(&self, id: &str) -> Result<NetworkPolicyResource, StoreError> {
        self.run_mutation(async {
            self.delete_resource(&self.network_policies, id).await
        })
        .await
    }

    // --- QosPolicy (Phase 3.6) ---
    async fn list_qos_policies(&self) -> Vec<QosPolicyResource> {
        self.list_resource(&self.qos_policies).await
    }

    async fn get_qos_policy(&self, id: &str) -> Option<QosPolicyResource> {
        self.get_resource(&self.qos_policies, id).await
    }

    async fn create_qos_policy(
        &self,
        resource: QosPolicyResource,
    ) -> Result<QosPolicyResource, StoreError> {
        self.run_mutation(async {
            self.validate_qos_policy_resource_inner(&resource).await?;
            self.create_resource(&self.qos_policies, resource).await
        })
        .await
    }

    async fn update_qos_policy(
        &self,
        id: &str,
        resource: QosPolicyResource,
    ) -> Result<QosPolicyResource, StoreError> {
        self.run_mutation(async {
            self
                .qos_policies
                .get(id)
                .await
                .ok_or_else(|| StoreError::NotFound {
                    resource: "qos_policy",
                    id: id.to_string(),
                })?;
            self.validate_qos_policy_resource_inner(&resource).await?;
            self
                .update_resource(&self.qos_policies, id, resource)
                .await
        })
        .await
    }

    async fn delete_qos_policy(&self, id: &str) -> Result<QosPolicyResource, StoreError> {
        self.run_mutation(async { self.delete_resource(&self.qos_policies, id).await })
            .await
    }

    // --- MirrorPolicy (Phase 3.7) ---
    async fn list_mirror_policies(&self) -> Vec<MirrorPolicyResource> {
        self.list_resource(&self.mirror_policies).await
    }

    async fn get_mirror_policy(&self, id: &str) -> Option<MirrorPolicyResource> {
        self.get_resource(&self.mirror_policies, id).await
    }

    async fn create_mirror_policy(
        &self,
        resource: MirrorPolicyResource,
    ) -> Result<MirrorPolicyResource, StoreError> {
        self.run_mutation(async {
            self.validate_mirror_policy_resource_inner(&resource).await?;
            self.create_resource(&self.mirror_policies, resource).await
        })
        .await
    }

    async fn update_mirror_policy(
        &self,
        id: &str,
        resource: MirrorPolicyResource,
    ) -> Result<MirrorPolicyResource, StoreError> {
        self.run_mutation(async {
            self
                .mirror_policies
                .get(id)
                .await
                .ok_or_else(|| StoreError::NotFound {
                    resource: "mirror_policy",
                    id: id.to_string(),
                })?;
            self.validate_mirror_policy_resource_inner(&resource).await?;
            self
                .update_resource(&self.mirror_policies, id, resource)
                .await
        })
        .await
    }

    async fn delete_mirror_policy(&self, id: &str) -> Result<MirrorPolicyResource, StoreError> {
        self.run_mutation(async {
            self.ensure_mirror_policy_delete_allowed_inner(id).await?;
            self.delete_resource(&self.mirror_policies, id).await
        })
        .await
    }

    async fn list_health_checks(&self) -> Vec<HealthCheckResource> {
        self.list_resource(&self.health_checks).await
    }

    async fn get_health_check(&self, id: &str) -> Option<HealthCheckResource> {
        self.get_resource(&self.health_checks, id).await
    }

    async fn create_health_check(
        &self,
        resource: HealthCheckResource,
    ) -> Result<HealthCheckResource, StoreError> {
        self.run_mutation(async {
            self
                .validate_health_check_resource_inner(&resource)
                .await?;
            self.create_resource(&self.health_checks, resource).await
        })
        .await
    }

    async fn update_health_check(
        &self,
        id: &str,
        resource: HealthCheckResource,
    ) -> Result<HealthCheckResource, StoreError> {
        self.run_mutation(async {
            let existing =
                self
                    .health_checks
                    .get(id)
                    .await
                    .ok_or_else(|| StoreError::NotFound {
                        resource: "health_check",
                        id: id.to_string(),
                    })?;
            self
                .validate_health_check_resource_inner(&resource)
                .await?;
            if existing.spec.tenant_id != resource.spec.tenant_id
                || existing.spec.network_id != resource.spec.network_id
            {
                self.ensure_health_check_delete_allowed_inner(id).await?;
            }
            self
                .update_resource(&self.health_checks, id, resource)
                .await
        })
        .await
    }

    async fn delete_health_check(&self, id: &str) -> Result<HealthCheckResource, StoreError> {
        self.run_mutation(async {
            self.ensure_health_check_delete_allowed_inner(id).await?;
            self.delete_resource(&self.health_checks, id).await
        })
        .await
    }

    async fn list_backend_sets(&self) -> Vec<BackendSetResource> {
        self.list_resource(&self.backend_sets).await
    }

    async fn get_backend_set(&self, id: &str) -> Option<BackendSetResource> {
        self.get_resource(&self.backend_sets, id).await
    }

    async fn create_backend_set(
        &self,
        resource: BackendSetResource,
    ) -> Result<BackendSetResource, StoreError> {
        self.run_mutation(async {
            self.validate_backend_set_resource_inner(&resource).await?;
            self.create_resource(&self.backend_sets, resource).await
        })
        .await
    }

    async fn update_backend_set(
        &self,
        id: &str,
        resource: BackendSetResource,
    ) -> Result<BackendSetResource, StoreError> {
        self.run_mutation(async {
            let existing =
                self
                    .backend_sets
                    .get(id)
                    .await
                    .ok_or_else(|| StoreError::NotFound {
                        resource: "backend_set",
                        id: id.to_string(),
                    })?;
            self.validate_backend_set_resource_inner(&resource).await?;
            if existing.spec.tenant_id != resource.spec.tenant_id
                || existing.spec.network_id != resource.spec.network_id
            {
                self.ensure_backend_set_delete_allowed_inner(id).await?;
            }
            self
                .update_resource(&self.backend_sets, id, resource)
                .await
        })
        .await
    }

    async fn delete_backend_set(&self, id: &str) -> Result<BackendSetResource, StoreError> {
        self.run_mutation(async {
            self.ensure_backend_set_delete_allowed_inner(id).await?;
            self.delete_resource(&self.backend_sets, id).await
        })
        .await
    }

    async fn list_services(&self) -> Vec<ServiceResource> {
        self.list_resource(&self.services).await
    }

    async fn get_service(&self, id: &str) -> Option<ServiceResource> {
        self.get_resource(&self.services, id).await
    }

    async fn create_service(
        &self,
        resource: ServiceResource,
    ) -> Result<ServiceResource, StoreError> {
        self.run_mutation(async {
            self.validate_service_resource_inner(&resource).await?;
            self.create_resource(&self.services, resource).await
        })
        .await
    }

    async fn update_service(
        &self,
        id: &str,
        resource: ServiceResource,
    ) -> Result<ServiceResource, StoreError> {
        self.run_mutation(async {
            self
                .services
                .get(id)
                .await
                .ok_or_else(|| StoreError::NotFound {
                    resource: "service",
                    id: id.to_string(),
                })?;
            self.validate_service_resource_inner(&resource).await?;
            self.update_resource(&self.services, id, resource).await
        })
        .await
    }

    async fn delete_service(&self, id: &str) -> Result<ServiceResource, StoreError> {
        self.run_mutation(async { self.delete_resource(&self.services, id).await })
            .await
    }

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
        self.run_mutation(async {
            self.ensure_node_delete_allowed_inner(id).await?;
            let deleted = self.nodes.delete(id).await?;
            self.clear_southbound_runtime_inner(id).await;
            self.clear_southbound_publish_inner(id).await;
            self.bump_generation_inner();
            Ok(deleted)
        })
        .await
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
