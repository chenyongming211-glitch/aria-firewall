use aria_api::{
    BackendSetResource, HealthCheckResource, IpGroupResource, NetworkPolicyResource,
    NetworkResource, PortResource, QosPolicyResource, RouteTableResource,
    SecurityGroupResource, ServiceResource, TenantResource, NodeResource,
};
use std::collections::BTreeSet;

use super::resource_store::ResourceStore;
use super::{InMemoryControllerStore, StoreError};

impl InMemoryControllerStore {
    pub(crate) async fn ensure_tenant_exists_inner(
        &self,
        tenant_id: &str,
        resource: &'static str,
        field: &'static str,
    ) -> Result<TenantResource, StoreError> {
        self.tenants
            .get(tenant_id)
            .await
            .ok_or(StoreError::InvalidReference {
                resource,
                field,
                value: tenant_id.to_string(),
                referenced_resource: "tenant",
            })
    }

    pub(crate) async fn ensure_network_exists_inner(
        &self,
        network_id: &str,
        resource: &'static str,
        field: &'static str,
    ) -> Result<NetworkResource, StoreError> {
        self.networks
            .get(network_id)
            .await
            .ok_or(StoreError::InvalidReference {
                resource,
                field,
                value: network_id.to_string(),
                referenced_resource: "network",
            })
    }

    pub(crate) async fn ensure_node_exists_inner(
        &self,
        node_id: &str,
        resource: &'static str,
        field: &'static str,
    ) -> Result<NodeResource, StoreError> {
        self.nodes
            .get(node_id)
            .await
            .ok_or(StoreError::InvalidReference {
                resource,
                field,
                value: node_id.to_string(),
                referenced_resource: "node",
            })
    }

    pub(crate) async fn ensure_security_group_exists_inner(
        &self,
        security_group_id: &str,
        resource: &'static str,
        field: &'static str,
    ) -> Result<SecurityGroupResource, StoreError> {
        self.security_groups
            .get(security_group_id)
            .await
            .ok_or(StoreError::InvalidReference {
                resource,
                field,
                value: security_group_id.to_string(),
                referenced_resource: "security_group",
            })
    }

    pub(crate) async fn ensure_health_check_exists_inner(
        &self,
        health_check_id: &str,
        resource: &'static str,
        field: &'static str,
    ) -> Result<HealthCheckResource, StoreError> {
        self.health_checks
            .get(health_check_id)
            .await
            .ok_or(StoreError::InvalidReference {
                resource,
                field,
                value: health_check_id.to_string(),
                referenced_resource: "health_check",
            })
    }

    pub(crate) async fn ensure_backend_set_exists_inner(
        &self,
        backend_set_id: &str,
        resource: &'static str,
        field: &'static str,
    ) -> Result<BackendSetResource, StoreError> {
        self.backend_sets
            .get(backend_set_id)
            .await
            .ok_or(StoreError::InvalidReference {
                resource,
                field,
                value: backend_set_id.to_string(),
                referenced_resource: "backend_set",
            })
    }

    pub(crate) async fn ensure_port_exists_inner(
        &self,
        port_id: &str,
        resource: &'static str,
        field: &'static str,
    ) -> Result<PortResource, StoreError> {
        self.ports
            .get(port_id)
            .await
            .ok_or(StoreError::InvalidReference {
                resource,
                field,
                value: port_id.to_string(),
                referenced_resource: "port",
            })
    }

    pub(crate) async fn ensure_ip_group_exists_inner(
        &self,
        ip_group_id: &str,
        resource: &'static str,
        field: &'static str,
    ) -> Result<IpGroupResource, StoreError> {
        self.ip_groups
            .get(ip_group_id)
            .await
            .ok_or(StoreError::InvalidReference {
                resource,
                field,
                value: ip_group_id.to_string(),
                referenced_resource: "ip_group",
            })
    }

    pub(crate) async fn validate_network_resource_inner(
        &self,
        resource: &NetworkResource,
    ) -> Result<(), StoreError> {
        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "network", "tenant_id")
            .await?;
        if !matches!(
            resource.spec.route_mode.as_str(),
            "native" | "overlay" | "hybrid"
        ) {
            return Err(StoreError::BadRequest(format!(
                "network route_mode '{}' must be one of: native, overlay, hybrid",
                resource.spec.route_mode
            )));
        }
        Ok(())
    }

    pub(crate) async fn validate_security_group_resource_inner(
        &self,
        resource: &SecurityGroupResource,
    ) -> Result<(), StoreError> {
        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "security_group", "tenant_id")
            .await?;
        Ok(())
    }

    pub(crate) async fn validate_route_table_resource_inner(
        &self,
        resource: &RouteTableResource,
    ) -> Result<(), StoreError> {
        self.ensure_network_exists_inner(&resource.spec.network_id, "route_table", "network_id")
            .await?;
        Ok(())
    }

    pub(crate) async fn validate_ip_group_resource_inner(
        &self,
        resource: &IpGroupResource,
    ) -> Result<(), StoreError> {
        if resource.spec.cidrs.is_empty() {
            return Err(StoreError::BadRequest(
                "ip_group must define at least one CIDR entry".to_string(),
            ));
        }
        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "ip_group", "tenant_id")
            .await?;
        let network = self
            .ensure_network_exists_inner(&resource.spec.network_id, "ip_group", "network_id")
            .await?;
        if network.spec.tenant_id != resource.spec.tenant_id {
            return Err(StoreError::InvalidReference {
                resource: "ip_group",
                field: "network_id",
                value: resource.spec.network_id.clone(),
                referenced_resource: "network",
            });
        }
        // Validate CIDR strings parse correctly.
        for cidr in &resource.spec.cidrs {
            if cidr.parse::<std::net::IpNet>().is_err() {
                return Err(StoreError::BadRequest(format!(
                    "invalid CIDR '{}'",
                    cidr
                )));
            }
        }
        // Enforce name uniqueness within the same network.
        for existing in self.ip_groups.list().await {
            if existing.spec.network_id == resource.spec.network_id
                && existing.spec.name == resource.spec.name
                && existing.metadata.id != resource.metadata.id
            {
                return Err(StoreError::AlreadyExists {
                    resource: "ip_group",
                    id: format!(
                        "name '{}' in network '{}'",
                        resource.spec.name, resource.spec.network_id
                    ),
                });
            }
        }
        Ok(())
    }

    pub(crate) async fn validate_network_policy_resource_inner(
        &self,
        resource: &NetworkPolicyResource,
    ) -> Result<(), StoreError> {
        if resource.spec.rules.is_empty() {
            return Err(StoreError::BadRequest(
                "network_policy must define at least one rule".to_string(),
            ));
        }

        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "network_policy", "tenant_id")
            .await?;
        let network = self
            .ensure_network_exists_inner(&resource.spec.network_id, "network_policy", "network_id")
            .await?;
        if network.spec.tenant_id != resource.spec.tenant_id {
            return Err(StoreError::InvalidReference {
                resource: "network_policy",
                field: "network_id",
                value: resource.spec.network_id.clone(),
                referenced_resource: "network",
            });
        }
        // Collect IpGroup IDs in the same network for cross-reference.
        let ip_group_ids_in_network: BTreeSet<&str> = self
            .ip_groups
            .list()
            .await
            .iter()
            .filter(|ig| ig.spec.network_id == resource.spec.network_id)
            .map(|ig| ig.metadata.id.as_str())
            .collect();
        for (i, rule) in resource.spec.rules.iter().enumerate() {
            if !ip_group_ids_in_network.contains(rule.src_ip_group_id.as_str()) {
                return Err(StoreError::InvalidReference {
                    resource: "network_policy",
                    field: format!("rules[{}].src_ip_group_id", i),
                    value: rule.src_ip_group_id.clone(),
                    referenced_resource: "ip_group",
                });
            }
            if !ip_group_ids_in_network.contains(rule.dst_ip_group_id.as_str()) {
                return Err(StoreError::InvalidReference {
                    resource: "network_policy",
                    field: format!("rules[{}].dst_ip_group_id", i),
                    value: rule.dst_ip_group_id.clone(),
                    referenced_resource: "ip_group",
                });
            }
            if rule.proto != 0 && rule.proto != 1 && rule.proto != 6 && rule.proto != 17 {
                return Err(StoreError::BadRequest(format!(
                    "rules[{}].proto must be 0(wildcard), 1(ICMP), 6(TCP) or 17(UDP), got {}",
                    i, rule.proto
                )));
            }
            if rule.direction > 1 {
                return Err(StoreError::BadRequest(format!(
                    "rules[{}].direction must be 0(ingress) or 1(egress), got {}",
                    i, rule.direction
                )));
            }
            if rule.action > 1 {
                return Err(StoreError::BadRequest(format!(
                    "rules[{}].action must be 0(allow) or 1(deny), got {}",
                    i, rule.action
                )));
            }
        }
        // Enforce name uniqueness within the same network.
        for existing in self.network_policies.list().await {
            if existing.spec.network_id == resource.spec.network_id
                && existing.spec.name == resource.spec.name
                && existing.metadata.id != resource.metadata.id
            {
                return Err(StoreError::AlreadyExists {
                    resource: "network_policy",
                    id: format!(
                        "name '{}' in network '{}'",
                        resource.spec.name, resource.spec.network_id
                    ),
                });
            }
        }        Ok(())
    }

    pub(crate) async fn validate_qos_policy_resource_inner(
        &self,
        resource: &QosPolicyResource,
    ) -> Result<(), StoreError> {
        if resource.spec.rules.is_empty() {
            return Err(StoreError::BadRequest(
                "qos_policy must define at least one rule".to_string(),
            ));
        }
        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "qos_policy", "tenant_id")
            .await?;
        let network = self
            .ensure_network_exists_inner(&resource.spec.network_id, "qos_policy", "network_id")
            .await?;
        if network.spec.tenant_id != resource.spec.tenant_id {
            return Err(StoreError::InvalidReference {
                resource: "qos_policy",
                field: "network_id",
                value: resource.spec.network_id.clone(),
                referenced_resource: "network",
            });
        }
        // Collect IpGroup IDs in the same network for cross-reference.
        let ip_group_ids_in_network: BTreeSet<&str> = self
            .ip_groups
            .list()
            .await
            .iter()
            .filter(|ig| ig.spec.network_id == resource.spec.network_id)
            .map(|ig| ig.metadata.id.as_str())
            .collect();
        for (i, rule) in resource.spec.rules.iter().enumerate() {
            if !ip_group_ids_in_network.contains(rule.ip_group_id.as_str()) {
                return Err(StoreError::InvalidReference {
                    resource: "qos_policy",
                    field: format!("rules[{}].ip_group_id", i),
                    value: rule.ip_group_id.clone(),
                    referenced_resource: "ip_group",
                });
            }
            if rule.direction > 1 {
                return Err(StoreError::BadRequest(format!(
                    "rules[{}].direction must be 0(ingress) or 1(egress), got {}",
                    i, rule.direction
                )));
            }
            if rule.rate_bps == 0 {
                return Err(StoreError::BadRequest(format!(
                    "rules[{}].rate_bps must be > 0",
                    i
                )));
            }
            if rule.mode > 1 {
                return Err(StoreError::BadRequest(format!(
                    "rules[{}].mode must be 0(policing) or 1(shaping), got {}",
                    i, rule.mode
                )));
            }
        }
        // Enforce name uniqueness within the same network.
        for existing in self.qos_policies.list().await {
            if existing.spec.network_id == resource.spec.network_id
                && existing.spec.name == resource.spec.name
                && existing.metadata.id != resource.metadata.id
            {
                return Err(StoreError::AlreadyExists {
                    resource: "qos_policy",
                    id: format!(
                        "name '{}' in network '{}'",
                        resource.spec.name, resource.spec.network_id
                    ),
                });
            }
        }
        Ok(())
    }

    pub(crate) async fn validate_health_check_resource_inner(
        &self,
        resource: &HealthCheckResource,
    ) -> Result<(), StoreError> {
        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "health_check", "tenant_id")
            .await?;

        if let Some(network_id) = resource.spec.network_id.as_deref() {
            let network = self
                .ensure_network_exists_inner(network_id, "health_check", "network_id")
                .await?;
            if network.spec.tenant_id != resource.spec.tenant_id {
                return Err(StoreError::BadRequest(format!(
                    "health_check tenant_id '{}' must match network '{}' tenant '{}'",
                    resource.spec.tenant_id, network.metadata.id, network.spec.tenant_id
                )));
            }
        }

        Ok(())
    }

    pub(crate) async fn validate_backend_set_resource_inner(
        &self,
        resource: &BackendSetResource,
    ) -> Result<(), StoreError> {
        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "backend_set", "tenant_id")
            .await?;
        let network = self
            .ensure_network_exists_inner(&resource.spec.network_id, "backend_set", "network_id")
            .await?;
        if network.spec.tenant_id != resource.spec.tenant_id {
            return Err(StoreError::BadRequest(format!(
                "backend_set tenant_id '{}' must match network '{}' tenant '{}'",
                resource.spec.tenant_id, network.metadata.id, network.spec.tenant_id
            )));
        }

        if let Some(health_check_id) = resource.spec.health_check_id.as_deref() {
            let health_check = self
                .ensure_health_check_exists_inner(health_check_id, "backend_set", "health_check_id")
                .await?;
            if health_check.spec.tenant_id != resource.spec.tenant_id {
                return Err(StoreError::BadRequest(format!(
                    "backend_set health_check '{}' belongs to tenant '{}' but backend_set belongs to tenant '{}'",
                    health_check.metadata.id, health_check.spec.tenant_id, resource.spec.tenant_id
                )));
            }

            if let Some(health_check_network_id) = health_check.spec.network_id.as_deref() {
                if health_check_network_id != resource.spec.network_id {
                    return Err(StoreError::BadRequest(format!(
                        "backend_set health_check '{}' is scoped to network '{}' but backend_set targets network '{}'",
                        health_check.metadata.id, health_check_network_id, resource.spec.network_id
                    )));
                }
            }
        }

        for backend in &resource.spec.backends {
            if backend.target_type == "port_ref" {
                let target_ref = backend.target_ref.as_deref().ok_or_else(|| {
                    StoreError::BadRequest(format!(
                        "backend_set backend '{}' with target_type 'port_ref' must set target_ref",
                        backend.id
                    ))
                })?;
                let port = self
                    .ensure_port_exists_inner(target_ref, "backend_set", "backends.target_ref")
                    .await?;
                if port.spec.tenant_id != resource.spec.tenant_id {
                    return Err(StoreError::BadRequest(format!(
                        "backend_set backend '{}' references port '{}' in tenant '{}' but backend_set belongs to tenant '{}'",
                        backend.id, port.metadata.id, port.spec.tenant_id, resource.spec.tenant_id
                    )));
                }
                if port.spec.network_id != resource.spec.network_id {
                    return Err(StoreError::BadRequest(format!(
                        "backend_set backend '{}' references port '{}' in network '{}' but backend_set targets network '{}'",
                        backend.id, port.metadata.id, port.spec.network_id, resource.spec.network_id
                    )));
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn validate_service_resource_inner(
        &self,
        resource: &ServiceResource,
    ) -> Result<(), StoreError> {
        if resource.spec.ports.is_empty() {
            return Err(StoreError::BadRequest(
                "service must define at least one listener port".to_string(),
            ));
        }

        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "service", "tenant_id")
            .await?;
        let network = self
            .ensure_network_exists_inner(&resource.spec.network_id, "service", "network_id")
            .await?;
        if network.spec.tenant_id != resource.spec.tenant_id {
            return Err(StoreError::BadRequest(format!(
                "service tenant_id '{}' must match network '{}' tenant '{}'",
                resource.spec.tenant_id, network.metadata.id, network.spec.tenant_id
            )));
        }

        if let Some(backend_set_id) = resource.spec.backend_set_id.as_deref() {
            let backend_set = self
                .ensure_backend_set_exists_inner(backend_set_id, "service", "backend_set_id")
                .await?;
            if backend_set.spec.tenant_id != resource.spec.tenant_id {
                return Err(StoreError::BadRequest(format!(
                    "service backend_set '{}' belongs to tenant '{}' but service belongs to tenant '{}'",
                    backend_set.metadata.id, backend_set.spec.tenant_id, resource.spec.tenant_id
                )));
            }
            if backend_set.spec.network_id != resource.spec.network_id {
                return Err(StoreError::BadRequest(format!(
                    "service backend_set '{}' targets network '{}' but service targets network '{}'",
                    backend_set.metadata.id, backend_set.spec.network_id, resource.spec.network_id
                )));
            }
        }

        Ok(())
    }

    pub(crate) async fn validate_port_resource_inner(
        &self,
        resource: &PortResource,
    ) -> Result<(), StoreError> {
        self.ensure_tenant_exists_inner(&resource.spec.tenant_id, "port", "tenant_id")
            .await?;
        let network = self
            .ensure_network_exists_inner(&resource.spec.network_id, "port", "network_id")
            .await?;
        if network.spec.tenant_id != resource.spec.tenant_id {
            return Err(StoreError::BadRequest(format!(
                "port tenant_id '{}' must match network '{}' tenant '{}'",
                resource.spec.tenant_id, network.metadata.id, network.spec.tenant_id
            )));
        }

        if let Some(node_id) = resource.spec.node_id.as_deref() {
            self.ensure_node_exists_inner(node_id, "port", "node_id")
                .await?;
        }

        for security_group_id in &resource.spec.security_group_ids {
            let security_group = self
                .ensure_security_group_exists_inner(security_group_id, "port", "security_group_ids")
                .await?;
            if security_group.spec.tenant_id != resource.spec.tenant_id {
                return Err(StoreError::BadRequest(format!(
                    "port security_group '{}' belongs to tenant '{}' but port belongs to tenant '{}'",
                    security_group.metadata.id, security_group.spec.tenant_id, resource.spec.tenant_id
                )));
            }
        }

        Ok(())
    }
}
