use super::*;

impl ControlPlane {
    // ── Mirror ──

    pub async fn list_mirror(
        &self,
        instance: &str,
    ) -> Result<Vec<MirrorRuleInfo>, ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let state = inst.read().await;
        Ok(state.state.mirror_rules.clone())
    }

    pub async fn add_mirror(
        &self,
        instance: &str,
        src_group: &str,
        dst_group: &str,
        proto: u8,
        direction: u8,
        target_iface: &str,
    ) -> Result<(), ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let mut state = inst.write().await;
        Self::check_xdp_ready(&state.pin_path)?;

        let src_id = self.resolve_group_id(&state.state, src_group)?;
        let dst_id = self.resolve_group_id(&state.state, dst_group)?;

        let target_ifindex = aria_core::mirror_ops::resolve_ifindex(target_iface)
            .map_err(|e| ControlPlaneError::ValidationError(e))?;

        let is_global = src_id == 0 && dst_id == 0 && proto == 0;

        if is_global {
            if let Err(e) = aria_core::mirror_ops::add_global_mirror(
                direction,
                target_ifindex,
                state.map_runtime(),
                state.state.mirror_enabled,
            ) {
                return Err(ControlPlaneError::KernelError(e));
            }
        } else {
            if let Err(e) = aria_core::mirror_ops::add_mirror_rule(
                src_id,
                dst_id,
                proto,
                direction,
                target_ifindex,
                state.map_runtime(),
                state.state.mirror_enabled,
            ) {
                return Err(ControlPlaneError::KernelError(e));
            }
        }

        // Update in-memory state
        if is_global {
            state
                .state
                .mirror_rules
                .retain(|r| !(r.is_global && r.direction == direction));
        } else {
            state.state.mirror_rules.retain(|r| {
                !(r.src_group_id == src_id
                    && r.dst_group_id == dst_id
                    && r.proto == proto
                    && r.direction == direction
                    && !r.is_global)
            });
        }
        state.state.mirror_rules.push(MirrorRuleInfo {
            src_group_name: src_group.to_string(),
            src_group_id: src_id,
            dst_group_name: dst_group.to_string(),
            dst_group_id: dst_id,
            proto,
            direction,
            target_iface: target_iface.to_string(),
            target_ifindex,
            is_global,
        });

        state
            .wal_append(&WalEntry::AddMirror {
                src_group_name: src_group.to_string(),
                src_group_id: src_id,
                dst_group_name: dst_group.to_string(),
                dst_group_id: dst_id,
                proto,
                direction,
                target_iface: target_iface.to_string(),
                target_ifindex,
                is_global,
            })
            .await;
        Ok(())
    }

    pub async fn delete_mirror(
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

        let is_global = src_id == 0 && dst_id == 0 && proto == 0;

        let target_directions = Self::requested_directions(direction);
        let matching_rules: Vec<MirrorRuleInfo> = target_directions
            .iter()
            .filter_map(|dir| {
                state
                    .state
                    .mirror_rules
                    .iter()
                    .find(|r| {
                        if is_global {
                            r.is_global && r.direction == *dir
                        } else {
                            !r.is_global
                                && r.src_group_id == src_id
                                && r.dst_group_id == dst_id
                                && r.proto == proto
                                && r.direction == *dir
                        }
                    })
                    .cloned()
            })
            .collect();
        if matching_rules.is_empty() {
            return Err(ControlPlaneError::PolicyNotFound(
                "Mirror rule not found".to_string(),
            ));
        }

        let mut deleted_rules: Vec<MirrorRuleInfo> = Vec::new();
        for rule in &matching_rules {
            let result = if rule.is_global {
                aria_core::mirror_ops::delete_global_mirror(
                    rule.direction,
                    state.map_runtime(),
                    state.state.mirror_enabled,
                )
            } else {
                aria_core::mirror_ops::delete_mirror_rule(
                    rule.src_group_id,
                    rule.dst_group_id,
                    rule.proto,
                    rule.direction,
                    state.map_runtime(),
                    state.state.mirror_enabled,
                )
            };
            if let Err(e) = result {
                let rollback = Self::rollback_mirror_deletes(
                    state.map_runtime(),
                    &deleted_rules,
                    state.state.mirror_enabled,
                );
                let error = match rollback {
                    Ok(()) => e,
                    Err(rollback_err) => format!("{}; rollback failed: {}", e, rollback_err),
                };
                return Err(ControlPlaneError::KernelError(error));
            }
            deleted_rules.push(rule.clone());
        }

        for rule in &matching_rules {
            let clear_stats_result = if rule.is_global {
                aria_core::mirror_ops::clear_global_mirror_stats(
                    rule.direction,
                    state.map_runtime(),
                )
            } else {
                aria_core::mirror_ops::clear_mirror_rule_stats(
                    rule.src_group_id,
                    rule.dst_group_id,
                    rule.proto,
                    rule.direction,
                    state.map_runtime(),
                )
            };
            if let Err(e) = clear_stats_result {
                warn!(
                    instance,
                    src_group_id = rule.src_group_id,
                    dst_group_id = rule.dst_group_id,
                    proto = rule.proto,
                    direction = rule.direction,
                    is_global = rule.is_global,
                    error = %e,
                    "failed to clear mirror stats after delete"
                );
            }

            if rule.is_global {
                state
                    .state
                    .mirror_rules
                    .retain(|r| !(r.is_global && r.direction == rule.direction));
            } else {
                state.state.mirror_rules.retain(|r| {
                    !(r.src_group_id == rule.src_group_id
                        && r.dst_group_id == rule.dst_group_id
                        && r.proto == rule.proto
                        && r.direction == rule.direction
                        && !r.is_global)
                });
            }

            state
                .wal_append(&WalEntry::DeleteMirror {
                    src_group_id: rule.src_group_id,
                    dst_group_id: rule.dst_group_id,
                    proto: rule.proto,
                    direction: rule.direction,
                    is_global: rule.is_global,
                })
                .await;
        }
        Ok(())
    }

    pub async fn get_mirror_stats(
        &self,
        instance: &str,
    ) -> Result<
        (
            Vec<aria_core::monitoring::MirrorStatsEntry>,
            HashMap<String, GroupInfo>,
        ),
        ControlPlaneError,
    > {
        let inst = self.get_instance(instance).await?;
        let state = inst.read().await;
        let stats = aria_core::monitoring::get_mirror_stats(state.map_runtime())
            .map_err(|e| ControlPlaneError::KernelError(e))?;
        Ok((stats, state.state.groups.clone()))
    }
}
