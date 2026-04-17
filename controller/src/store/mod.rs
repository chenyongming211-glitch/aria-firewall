use aria_api::{
    ApplyStatusReport, BackendSetResource, DesiredStateEnvelope, DesiredStatePublishRecord,
    HealthCheckResource, IpGroupResource, MirrorPolicyResource, NetworkPolicyResource,
    NetworkResource, NodeCapability, NodeHealthReport, NodeInfo, NodeRegisterRequest, NodeResource,
    PortResource, QosPolicyResource, ResourceMetadata, RouteTableResource, SecurityGroupResource,
    ServiceResource, SouthboundNodeStatusResponse, SouthboundSyncStatus, TenantResource,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod file_backed;
pub mod in_memory;
pub mod protection;
pub mod resource_store;
pub mod southbound;
pub mod validation;

// ---- Re-exports ----
pub use file_backed::FileBackedControllerStore;
pub use in_memory::InMemoryControllerStore;
pub use resource_store::ResourceStore;

pub type SharedStore = Arc<dyn ControllerStore>;

use std::sync::Arc;

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
impl_stored_resource!(IpGroupResource);
impl_stored_resource!(NetworkPolicyResource);
impl_stored_resource!(QosPolicyResource);
impl_stored_resource!(MirrorPolicyResource);
impl_stored_resource!(HealthCheckResource);
impl_stored_resource!(BackendSetResource);
impl_stored_resource!(ServiceResource);

#[derive(Debug, Clone)]
pub enum StoreError {
    AlreadyExists {
        resource: &'static str,
        id: String,
    },
    BadRequest(String),
    DependencyConflict {
        resource: &'static str,
        id: String,
        dependent_resource: &'static str,
        dependent_id: String,
    },
    InvalidReference {
        resource: &'static str,
        field: &'static str,
        value: String,
        referenced_resource: &'static str,
    },
    NotFound {
        resource: &'static str,
        id: String,
    },
    Internal(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists { resource, id } => {
                write!(f, "{resource} '{id}' already exists")
            }
            Self::BadRequest(message) => f.write_str(message),
            Self::DependencyConflict {
                resource,
                id,
                dependent_resource,
                dependent_id,
            } => write!(
                f,
                "{resource} '{id}' is still referenced by {dependent_resource} '{dependent_id}'"
            ),
            Self::InvalidReference {
                resource,
                field,
                value,
                referenced_resource,
            } => write!(
                f,
                "{resource} field '{field}' references missing {referenced_resource} '{value}'"
            ),
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

    async fn list_ip_groups(&self) -> Vec<IpGroupResource>;
    async fn get_ip_group(&self, id: &str) -> Option<IpGroupResource>;
    async fn create_ip_group(
        &self,
        resource: IpGroupResource,
    ) -> Result<IpGroupResource, StoreError>;
    async fn update_ip_group(
        &self,
        id: &str,
        resource: IpGroupResource,
    ) -> Result<IpGroupResource, StoreError>;
    async fn delete_ip_group(&self, id: &str) -> Result<IpGroupResource, StoreError>;

    async fn list_network_policies(&self) -> Vec<NetworkPolicyResource>;
    async fn get_network_policy(&self, id: &str) -> Option<NetworkPolicyResource>;
    async fn create_network_policy(
        &self,
        resource: NetworkPolicyResource,
    ) -> Result<NetworkPolicyResource, StoreError>;
    async fn update_network_policy(
        &self,
        id: &str,
        resource: NetworkPolicyResource,
    ) -> Result<NetworkPolicyResource, StoreError>;
    async fn delete_network_policy(&self, id: &str) -> Result<NetworkPolicyResource, StoreError>;

    // --- QosPolicy (Phase 3.6) ---
    async fn list_qos_policies(&self) -> Vec<QosPolicyResource>;
    async fn get_qos_policy(&self, id: &str) -> Option<QosPolicyResource>;
    async fn create_qos_policy(
        &self,
        resource: QosPolicyResource,
    ) -> Result<QosPolicyResource, StoreError>;
    async fn update_qos_policy(
        &self,
        id: &str,
        resource: QosPolicyResource,
    ) -> Result<QosPolicyResource, StoreError>;
    async fn delete_qos_policy(&self, id: &str) -> Result<QosPolicyResource, StoreError>;

    // --- MirrorPolicy (Phase 3.7) ---
    async fn list_mirror_policies(&self) -> Vec<MirrorPolicyResource>;
    async fn get_mirror_policy(&self, id: &str) -> Option<MirrorPolicyResource>;
    async fn create_mirror_policy(
        &self,
        resource: MirrorPolicyResource,
    ) -> Result<MirrorPolicyResource, StoreError>;
    async fn update_mirror_policy(
        &self,
        id: &str,
        resource: MirrorPolicyResource,
    ) -> Result<MirrorPolicyResource, StoreError>;
    async fn delete_mirror_policy(&self, id: &str) -> Result<MirrorPolicyResource, StoreError>;

    async fn list_health_checks(&self) -> Vec<HealthCheckResource>;
    async fn get_health_check(&self, id: &str) -> Option<HealthCheckResource>;
    async fn create_health_check(
        &self,
        resource: HealthCheckResource,
    ) -> Result<HealthCheckResource, StoreError>;
    async fn update_health_check(
        &self,
        id: &str,
        resource: HealthCheckResource,
    ) -> Result<HealthCheckResource, StoreError>;
    async fn delete_health_check(&self, id: &str) -> Result<HealthCheckResource, StoreError>;

    async fn list_backend_sets(&self) -> Vec<BackendSetResource>;
    async fn get_backend_set(&self, id: &str) -> Option<BackendSetResource>;
    async fn create_backend_set(
        &self,
        resource: BackendSetResource,
    ) -> Result<BackendSetResource, StoreError>;
    async fn update_backend_set(
        &self,
        id: &str,
        resource: BackendSetResource,
    ) -> Result<BackendSetResource, StoreError>;
    async fn delete_backend_set(&self, id: &str) -> Result<BackendSetResource, StoreError>;

    async fn list_services(&self) -> Vec<ServiceResource>;
    async fn get_service(&self, id: &str) -> Option<ServiceResource>;
    async fn create_service(
        &self,
        resource: ServiceResource,
    ) -> Result<ServiceResource, StoreError>;
    async fn update_service(
        &self,
        id: &str,
        resource: ServiceResource,
    ) -> Result<ServiceResource, StoreError>;
    async fn delete_service(&self, id: &str) -> Result<ServiceResource, StoreError>;

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
