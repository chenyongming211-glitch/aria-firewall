use super::*;

impl ControlPlane {
    // ── Groups ──

    pub async fn list_groups(&self, instance: &str) -> Result<Vec<GroupInfo>, ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let state = inst.read().await;
        Ok(state.state.groups.values().cloned().collect())
    }

    pub async fn add_group(
        &self,
        instance: &str,
        name: &str,
        cidr: &str,
    ) -> Result<u32, ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let mut state = inst.write().await;
        Self::check_xdp_ready(&state.pin_path)?;

        // Check if this is a new group (for rollback)
        let was_new_group = !state.state.groups.contains_key(name);

        // Modify in-memory state
        let id = state
            .state
            .add_group(name, cidr)
            .map_err(|e| ControlPlaneError::ValidationError(e))?;

        // Write to kernel maps
        if let Err(e) =
            aria_core::ebpf_ops::add_network("src", cidr, id, state.map_runtime(), &self.ebpf_path)
        {
            state.state.rollback_add_group(name, cidr, was_new_group);
            return Err(ControlPlaneError::KernelError(format!("src: {}", e)));
        }
        if let Err(e) =
            aria_core::ebpf_ops::add_network("dst", cidr, id, state.map_runtime(), &self.ebpf_path)
        {
            let _ = aria_core::ebpf_ops::delete_network(
                "src",
                cidr,
                id,
                state.map_runtime(),
                &self.ebpf_path,
            );
            state.state.rollback_add_group(name, cidr, was_new_group);
            return Err(ControlPlaneError::KernelError(format!("dst: {}", e)));
        }

        state
            .wal_append(&WalEntry::AddGroup {
                name: name.to_string(),
                cidr: cidr.to_string(),
            })
            .await;
        Ok(id)
    }

    pub async fn delete_group(&self, instance: &str, name: &str) -> Result<(), ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let mut state = inst.write().await;
        Self::check_xdp_ready(&state.pin_path)?;

        let group = state
            .state
            .groups
            .get(name)
            .ok_or_else(|| ControlPlaneError::GroupNotFound(name.to_string()))?
            .clone();

        // Check if group is referenced by any rule
        for rule in &state.state.rules {
            if rule.src_group_id == group.id || rule.dst_group_id == group.id {
                return Err(ControlPlaneError::GroupInUse(format!(
                    "Group '{}' is referenced by a policy",
                    name
                )));
            }
        }

        // Also check QoS rules
        for qos in &state.state.qos_rules {
            if qos.group_id == group.id {
                return Err(ControlPlaneError::GroupInUse(format!(
                    "Group '{}' is referenced by a QoS rule",
                    name
                )));
            }
        }

        // Also check mirror rules
        for mirror in &state.state.mirror_rules {
            if mirror.src_group_id == group.id || mirror.dst_group_id == group.id {
                return Err(ControlPlaneError::GroupInUse(format!(
                    "Group '{}' is referenced by a mirror rule",
                    name
                )));
            }
        }

        // Delete from kernel
        let mut errors = Vec::new();
        let mut deleted_networks: Vec<(&'static str, String)> = Vec::new();
        for cidr in &group.cidrs {
            match aria_core::ebpf_ops::delete_network(
                "src",
                cidr,
                group.id,
                state.map_runtime(),
                &self.ebpf_path,
            ) {
                Ok(()) => deleted_networks.push(("src", cidr.clone())),
                Err(e) => errors.push(format!("src {}: {}", cidr, e)),
            }
            match aria_core::ebpf_ops::delete_network(
                "dst",
                cidr,
                group.id,
                state.map_runtime(),
                &self.ebpf_path,
            ) {
                Ok(()) => deleted_networks.push(("dst", cidr.clone())),
                Err(e) => errors.push(format!("dst {}: {}", cidr, e)),
            }
        }
        if !errors.is_empty() {
            let rollback = Self::rollback_group_deletes(
                state.map_runtime(),
                &self.ebpf_path,
                group.id,
                &deleted_networks,
            );
            let error = match rollback {
                Ok(()) => errors.join("; "),
                Err(rollback_err) => {
                    format!("{}; rollback failed: {}", errors.join("; "), rollback_err)
                }
            };
            return Err(ControlPlaneError::KernelError(error));
        }

        state.state.groups.remove(name);
        state
            .wal_append(&WalEntry::DeleteGroup {
                name: name.to_string(),
            })
            .await;

        // Clear stale GROUP_STATS entries so the deleted group no longer appears in API responses.
        if let Err(e) = aria_core::monitoring::clear_group_stats_for_id(
            state.map_runtime(),
            group.id,
        ) {
            warn!(error = %e, group_id = group.id, "failed to clear group stats after group delete");
        }
        Ok(())
    }

    // ── Groups with Stats (Aggregation) ──

    pub async fn list_groups_with_stats(
        &self,
        instance: &str,
    ) -> Result<(Vec<GroupInfo>, Vec<aria_core::monitoring::GroupStatsEntry>), ControlPlaneError>
    {
        // Get groups configuration
        let inst = self.get_instance(instance).await?;
        let state = inst.read().await;
        let groups: Vec<_> = state.state.groups.values().cloned().collect();
        let stats = aria_core::monitoring::get_group_stats(state.map_runtime())
            .map_err(|e| ControlPlaneError::KernelError(e))?;

        Ok((groups, stats))
    }
}
