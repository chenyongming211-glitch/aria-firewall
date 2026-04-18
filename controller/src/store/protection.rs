use super::{InMemoryControllerStore, StoreError};

impl InMemoryControllerStore {
    pub(crate) async fn ensure_tenant_delete_allowed_inner(&self, tenant_id: &str) -> Result<(), StoreError> {
        if let Some(network) = self
            .networks
            .list()
            .await
            .into_iter()
            .find(|network| network.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "network",
                dependent_id: network.metadata.id,
            });
        }

        if let Some(port) = self
            .ports
            .list()
            .await
            .into_iter()
            .find(|port| port.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "port",
                dependent_id: port.metadata.id,
            });
        }

        if let Some(security_group) = self
            .security_groups
            .list()
            .await
            .into_iter()
            .find(|security_group| security_group.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "security_group",
                dependent_id: security_group.metadata.id,
            });
        }

        if let Some(health_check) = self
            .health_checks
            .list()
            .await
            .into_iter()
            .find(|health_check| health_check.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "health_check",
                dependent_id: health_check.metadata.id,
            });
        }

        if let Some(backend_set) = self
            .backend_sets
            .list()
            .await
            .into_iter()
            .find(|backend_set| backend_set.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "backend_set",
                dependent_id: backend_set.metadata.id,
            });
        }

        if let Some(service) = self
            .services
            .list()
            .await
            .into_iter()
            .find(|service| service.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "service",
                dependent_id: service.metadata.id,
            });
        }

        if let Some(ip_group) = self
            .ip_groups
            .list()
            .await
            .into_iter()
            .find(|ip_group| ip_group.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "ip_group",
                dependent_id: ip_group.metadata.id,
            });
        }

        if let Some(network_policy) = self
            .network_policies
            .list()
            .await
            .into_iter()
            .find(|network_policy| network_policy.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "network_policy",
                dependent_id: network_policy.metadata.id,
            });
        }

        if let Some(qos_policy) = self
            .qos_policies
            .list()
            .await
            .into_iter()
            .find(|qos_policy| qos_policy.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "qos_policy",
                dependent_id: qos_policy.metadata.id,
            });
        }

        if let Some(mirror_policy) = self
            .mirror_policies
            .list()
            .await
            .into_iter()
            .find(|mirror_policy| mirror_policy.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "mirror_policy",
                dependent_id: mirror_policy.metadata.id,
            });
        }

        if let Some(service_chain) = self
            .service_chains
            .list()
            .await
            .into_iter()
            .find(|service_chain| service_chain.spec.tenant_id == tenant_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "tenant",
                id: tenant_id.to_string(),
                dependent_resource: "service_chain",
                dependent_id: service_chain.metadata.id,
            });
        }

        Ok(())
    }

    pub(crate) async fn ensure_node_delete_allowed_inner(&self, node_id: &str) -> Result<(), StoreError> {
        if let Some(port) = self
            .ports
            .list()
            .await
            .into_iter()
            .find(|port| port.spec.node_id.as_deref() == Some(node_id))
        {
            return Err(StoreError::DependencyConflict {
                resource: "node",
                id: node_id.to_string(),
                dependent_resource: "port",
                dependent_id: port.metadata.id,
            });
        }

        Ok(())
    }

    pub(crate) async fn ensure_network_delete_allowed_inner(
        &self,
        network_id: &str,
    ) -> Result<(), StoreError> {
        if let Some(port) = self
            .ports
            .list()
            .await
            .into_iter()
            .find(|port| port.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "port",
                dependent_id: port.metadata.id,
            });
        }

        if let Some(route_table) = self
            .route_tables
            .list()
            .await
            .into_iter()
            .find(|route_table| route_table.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "route_table",
                dependent_id: route_table.metadata.id,
            });
        }

        if let Some(health_check) = self
            .health_checks
            .list()
            .await
            .into_iter()
            .find(|health_check| health_check.spec.network_id.as_deref() == Some(network_id))
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "health_check",
                dependent_id: health_check.metadata.id,
            });
        }

        if let Some(backend_set) = self
            .backend_sets
            .list()
            .await
            .into_iter()
            .find(|backend_set| backend_set.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "backend_set",
                dependent_id: backend_set.metadata.id,
            });
        }

        if let Some(service) = self
            .services
            .list()
            .await
            .into_iter()
            .find(|service| service.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "service",
                dependent_id: service.metadata.id,
            });
        }

        if let Some(ip_group) = self
            .ip_groups
            .list()
            .await
            .into_iter()
            .find(|ip_group| ip_group.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "ip_group",
                dependent_id: ip_group.metadata.id,
            });
        }

        if let Some(network_policy) = self
            .network_policies
            .list()
            .await
            .into_iter()
            .find(|network_policy| network_policy.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "network_policy",
                dependent_id: network_policy.metadata.id,
            });
        }

        if let Some(qos_policy) = self
            .qos_policies
            .list()
            .await
            .into_iter()
            .find(|qos_policy| qos_policy.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "qos_policy",
                dependent_id: qos_policy.metadata.id,
            });
        }

        if let Some(mirror_policy) = self
            .mirror_policies
            .list()
            .await
            .into_iter()
            .find(|mirror_policy| mirror_policy.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "mirror_policy",
                dependent_id: mirror_policy.metadata.id,
            });
        }

        if let Some(service_chain) = self
            .service_chains
            .list()
            .await
            .into_iter()
            .find(|service_chain| service_chain.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "service_chain",
                dependent_id: service_chain.metadata.id,
            });
        }

        Ok(())
    }

    pub(crate) async fn ensure_network_tenant_change_allowed_inner(
        &self,
        network_id: &str,
    ) -> Result<(), StoreError> {
        if let Some(port) = self
            .ports
            .list()
            .await
            .into_iter()
            .find(|port| port.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "port",
                dependent_id: port.metadata.id,
            });
        }

        if let Some(health_check) = self
            .health_checks
            .list()
            .await
            .into_iter()
            .find(|health_check| health_check.spec.network_id.as_deref() == Some(network_id))
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "health_check",
                dependent_id: health_check.metadata.id,
            });
        }

        if let Some(backend_set) = self
            .backend_sets
            .list()
            .await
            .into_iter()
            .find(|backend_set| backend_set.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "backend_set",
                dependent_id: backend_set.metadata.id,
            });
        }

        if let Some(service) = self
            .services
            .list()
            .await
            .into_iter()
            .find(|service| service.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "service",
                dependent_id: service.metadata.id,
            });
        }

        if let Some(ip_group) = self
            .ip_groups
            .list()
            .await
            .into_iter()
            .find(|ip_group| ip_group.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "ip_group",
                dependent_id: ip_group.metadata.id,
            });
        }

        if let Some(network_policy) = self
            .network_policies
            .list()
            .await
            .into_iter()
            .find(|network_policy| network_policy.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "network_policy",
                dependent_id: network_policy.metadata.id,
            });
        }

        if let Some(qos_policy) = self
            .qos_policies
            .list()
            .await
            .into_iter()
            .find(|qos_policy| qos_policy.spec.network_id == network_id)
        {
            return Err(StoreError::DependencyConflict {
                resource: "network",
                id: network_id.to_string(),
                dependent_resource: "qos_policy",
                dependent_id: qos_policy.metadata.id,
            });
        }

        Ok(())
    }

    pub(crate) async fn ensure_ip_group_delete_allowed_inner(
        &self,
        ip_group_id: &str,
    ) -> Result<(), StoreError> {
        if let Some(network_policy) =
            self.network_policies
                .list()
                .await
                .into_iter()
                .find(|network_policy| {
                    network_policy.spec.rules.iter().any(|rule| {
                        rule.src_ip_group_id == ip_group_id
                            || rule.dst_ip_group_id == ip_group_id
                    })
                })
        {
            return Err(StoreError::DependencyConflict {
                resource: "ip_group",
                id: ip_group_id.to_string(),
                dependent_resource: "network_policy",
                dependent_id: network_policy.metadata.id,
            });
        }

        // Also check QosPolicy references
        if let Some(qos_policy) = self
            .qos_policies
            .list()
            .await
            .into_iter()
            .find(|qp| {
                qp.spec.rules.iter().any(|rule| rule.ip_group_id == ip_group_id)
            })
        {
            return Err(StoreError::DependencyConflict {
                resource: "ip_group",
                id: ip_group_id.to_string(),
                dependent_resource: "qos_policy",
                dependent_id: qos_policy.metadata.id,
            });
        }

        // Also check MirrorPolicy references
        if let Some(mirror_policy) = self
            .mirror_policies
            .list()
            .await
            .into_iter()
            .find(|mp| {
                mp.spec.rules.iter().any(|rule| {
                    rule.src_ip_group_id == ip_group_id
                        || rule.dst_ip_group_id == ip_group_id
                })
            })
        {
            return Err(StoreError::DependencyConflict {
                resource: "ip_group",
                id: ip_group_id.to_string(),
                dependent_resource: "mirror_policy",
                dependent_id: mirror_policy.metadata.id,
            });
        }

        Ok(())
    }

    pub(crate) async fn ensure_security_group_delete_allowed_inner(
        &self,
        security_group_id: &str,
    ) -> Result<(), StoreError> {
        if let Some(port) = self.ports.list().await.into_iter().find(|port| {
            port.spec
                .security_group_ids
                .iter()
                .any(|id| id == security_group_id)
        }) {
            return Err(StoreError::DependencyConflict {
                resource: "security_group",
                id: security_group_id.to_string(),
                dependent_resource: "port",
                dependent_id: port.metadata.id,
            });
        }

        Ok(())
    }

    pub(crate) async fn ensure_health_check_delete_allowed_inner(
        &self,
        health_check_id: &str,
    ) -> Result<(), StoreError> {
        if let Some(backend_set) = self
            .backend_sets
            .list()
            .await
            .into_iter()
            .find(|backend_set| {
                backend_set.spec.health_check_id.as_deref() == Some(health_check_id)
            })
        {
            return Err(StoreError::DependencyConflict {
                resource: "health_check",
                id: health_check_id.to_string(),
                dependent_resource: "backend_set",
                dependent_id: backend_set.metadata.id,
            });
        }

        Ok(())
    }

    pub(crate) async fn ensure_backend_set_delete_allowed_inner(
        &self,
        backend_set_id: &str,
    ) -> Result<(), StoreError> {
        if let Some(service) = self
            .services
            .list()
            .await
            .into_iter()
            .find(|service| service.spec.backend_set_id.as_deref() == Some(backend_set_id))
        {
            return Err(StoreError::DependencyConflict {
                resource: "backend_set",
                id: backend_set_id.to_string(),
                dependent_resource: "service",
                dependent_id: service.metadata.id,
            });
        }

        Ok(())
    }

    pub(crate) async fn ensure_mirror_policy_delete_allowed_inner(
        &self,
        _mirror_policy_id: &str,
    ) -> Result<(), StoreError> {
        // No resources currently depend on mirror_policy.
        Ok(())
    }

    pub(crate) async fn ensure_service_chain_delete_allowed_inner(
        &self,
        _service_chain_id: &str,
    ) -> Result<(), StoreError> {
        // No resources currently depend on service_chain.
        Ok(())
    }
}
