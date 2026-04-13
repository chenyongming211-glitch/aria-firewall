use super::*;

impl ControlPlane {
    // ── Policies ──

    pub async fn list_policies(
        &self,
        instance: &str,
    ) -> Result<(Vec<RuleInfo>, HashMap<String, GroupInfo>), ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let state = inst.read().await;
        Ok((state.state.rules.clone(), state.state.groups.clone()))
    }

    pub async fn add_policy(
        &self,
        instance: &str,
        src_group: &str,
        dst_group: &str,
        proto: u8,
        action: u8,
        direction: u8,
        ports: Option<&str>,
    ) -> Result<(), ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let mut state = inst.write().await;
        Self::check_xdp_ready(&state.pin_path)?;

        let src_id = self.resolve_group_id(&state.state, src_group)?;
        let dst_id = self.resolve_group_id(&state.state, dst_group)?;
        Self::validate_policy_ports(proto, ports)?;

        // Snapshot state for rollback (clone the parts that apply_add_rule mutates)
        let snapshot_rules = state.state.rules.clone();
        let snapshot_port_sets = state.state.port_sets.clone();
        let snapshot_free_indices = state.state.free_bitmap_indices.clone();
        let snapshot_next_bitmap_idx = state.state.next_bitmap_idx;

        // Operate directly on in-memory state (no StateManager disk round-trip)
        let add_result = state
            .state
            .apply_add_rule(src_id, dst_id, proto, action, ports, direction)
            .map_err(|e| ControlPlaneError::ValidationError(e))?;

        // Write to kernel
        if let Err(e) = aria_core::ebpf_ops::add_policy(
            src_id,
            dst_id,
            proto,
            action,
            ports,
            add_result.bitmap_idx,
            add_result.is_new_port_set,
            direction,
            state.map_runtime(),
            &self.ebpf_path,
        ) {
            if add_result.is_new_port_set {
                if let (Some(idx), Some(ports_str)) = (add_result.bitmap_idx, ports) {
                    if let Err(cleanup_err) = aria_core::ebpf_ops::delete_port_set(
                        idx,
                        ports_str,
                        state.map_runtime(),
                        &self.ebpf_path,
                    ) {
                        warn!(error = %cleanup_err, "failed to clean new port bitmap after add_policy error");
                    }
                }
            }
            // Rollback: restore snapshotted state
            state.state.rules = snapshot_rules;
            state.state.port_sets = snapshot_port_sets;
            state.state.free_bitmap_indices = snapshot_free_indices;
            state.state.next_bitmap_idx = snapshot_next_bitmap_idx;
            return Err(ControlPlaneError::KernelError(e));
        }

        // Clean up old port set if replaced
        if let Some((old_idx, ref ports_normalized)) = add_result.old_port_set_released {
            if let Err(e) = aria_core::ebpf_ops::delete_port_set(
                old_idx,
                ports_normalized,
                state.map_runtime(),
                &self.ebpf_path,
            ) {
                warn!(error = %e, "failed to clean old port bitmap");
            }
        }

        state
            .wal_append(&WalEntry::AddRule {
                src_id,
                dst_id,
                proto,
                action,
                ports: ports.map(|s| s.to_string()),
                direction,
            })
            .await;
        Ok(())
    }

    pub async fn delete_policy(
        &self,
        instance: &str,
        src_group: &str,
        dst_group: &str,
        proto: u8,
        direction: u8,
    ) -> Result<(), ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let mut state = inst.write().await;
        Self::check_xdp_ready(&state.pin_path)?;

        let src_id = self.resolve_group_id(&state.state, src_group)?;
        let dst_id = self.resolve_group_id(&state.state, dst_group)?;

        let target_directions = Self::requested_directions(direction);
        let matching_rules: Vec<RuleInfo> = target_directions
            .iter()
            .filter_map(|dir| {
                state
                    .state
                    .rules
                    .iter()
                    .find(|r| {
                        r.src_group_id == src_id
                            && r.dst_group_id == dst_id
                            && r.proto == proto
                            && r.direction == *dir
                    })
                    .cloned()
            })
            .collect();
        if matching_rules.is_empty() {
            return Err(ControlPlaneError::PolicyNotFound(format!(
                "Policy not found: src={}, dst={}, proto={}, direction={}",
                src_group, dst_group, proto, direction
            )));
        }

        let mut deleted_rules: Vec<RuleInfo> = Vec::new();
        for rule in &matching_rules {
            if let Err(e) = aria_core::ebpf_ops::delete_policy(
                rule.src_group_id,
                rule.dst_group_id,
                rule.proto,
                rule.direction,
                state.map_runtime(),
                &self.ebpf_path,
            ) {
                let rollback = Self::rollback_policy_deletes(
                    state.map_runtime(),
                    &self.ebpf_path,
                    &deleted_rules,
                );
                let error = match rollback {
                    Ok(()) => e,
                    Err(rollback_err) => format!("{}; rollback failed: {}", e, rollback_err),
                };
                return Err(ControlPlaneError::KernelError(error));
            }
            deleted_rules.push(rule.clone());
        }

        let mut released_port_sets: Vec<(u32, String)> = Vec::new();
        for rule in &matching_rules {
            let remove_result = state
                .state
                .apply_remove_rule(
                    rule.src_group_id,
                    rule.dst_group_id,
                    rule.proto,
                    rule.direction,
                )
                .map_err(|e| ControlPlaneError::PolicyNotFound(e))?;

            if let (Some(idx), Some(ports_normalized)) =
                (remove_result.bitmap_idx, remove_result.port_set_released)
            {
                released_port_sets.push((idx, ports_normalized));
            }

            state
                .wal_append(&WalEntry::RemoveRule {
                    src_id: rule.src_group_id,
                    dst_id: rule.dst_group_id,
                    proto: rule.proto,
                    direction: rule.direction,
                })
                .await;
        }

        for (idx, ports_normalized) in released_port_sets {
            if let Err(e) = aria_core::ebpf_ops::delete_port_set(
                idx,
                &ports_normalized,
                state.map_runtime(),
                &self.ebpf_path,
            ) {
                warn!(error = %e, bitmap_idx = idx, "failed to clean port bitmap");
            }
        }

        // Clear stale RULE_STATS entries so deleted rules no longer appear in API responses.
        for rule in &matching_rules {
            if let Err(e) = aria_core::monitoring::clear_rule_stats_for_policy(
                state.map_runtime(),
                rule.src_group_id,
                rule.dst_group_id,
                rule.proto,
                rule.direction,
            ) {
                warn!(error = %e, "failed to clear rule stats after policy delete");
            }
        }
        Ok(())
    }
}
