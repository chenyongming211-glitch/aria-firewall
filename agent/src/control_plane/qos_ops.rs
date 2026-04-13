use super::*;

impl ControlPlane {
    // ── QoS ──

    pub async fn list_qos(&self, instance: &str) -> Result<Vec<QosRuleInfo>, ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let state = inst.read().await;
        Ok(state.state.qos_rules.clone())
    }

    pub async fn add_qos(
        &self,
        instance: &str,
        group_name: &str,
        direction: u8,
        rate_bps: u64,
        burst_bytes: u64,
        priority: u8,
        mode: u8,
    ) -> Result<(), ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let mut state = inst.write().await;
        Self::check_xdp_ready(&state.pin_path)?;

        let group_id = if group_name == "default" || group_name == "any" {
            0
        } else {
            state
                .state
                .groups
                .get(group_name)
                .map(|g| g.id)
                .ok_or_else(|| ControlPlaneError::GroupNotFound(group_name.to_string()))?
        };

        let fq_state = if mode == 1 {
            let iface = Self::runtime_iface_name(instance, &state)?;
            match aria_core::ebpf_ops::ensure_fq_qdisc(&iface) {
                Ok(aria_core::ebpf_ops::FqQdiscState::InstalledNow) => {
                    Self::mark_owned_fq_qdisc(&state, &iface)?;
                    Some(aria_core::ebpf_ops::FqQdiscState::InstalledNow)
                }
                Ok(aria_core::ebpf_ops::FqQdiscState::AlreadyPresent) => {
                    Some(aria_core::ebpf_ops::FqQdiscState::AlreadyPresent)
                }
                Err(e) => {
                    return Err(ControlPlaneError::KernelError(format!(
                        "[{}] failed to prepare FQ qdisc for QoS shaping: {}",
                        iface, e
                    )));
                }
            }
        } else {
            None
        };

        // Write to kernel
        if let Err(e) = aria_core::qos_ops::add_qos_rule(
            group_id,
            direction,
            rate_bps,
            burst_bytes,
            priority,
            mode,
            state.map_runtime(),
            state.state.qos_enabled,
        ) {
            if matches!(
                fq_state,
                Some(aria_core::ebpf_ops::FqQdiscState::InstalledNow)
            ) {
                Self::rollback_installed_fq_qdisc(instance, &state);
            }
            return Err(ControlPlaneError::KernelError(e));
        }

        // Update in-memory state
        state
            .state
            .qos_rules
            .retain(|r| !(r.group_id == group_id && r.direction == direction));
        state.state.qos_rules.push(QosRuleInfo {
            group_name: group_name.to_string(),
            group_id,
            direction,
            rate_bps,
            burst_bytes,
            priority,
            mode,
        });

        state
            .wal_append(&WalEntry::AddQos {
                group_name: group_name.to_string(),
                group_id,
                direction,
                rate_bps,
                burst_bytes,
                priority,
                mode,
            })
            .await;
        Ok(())
    }

    pub async fn delete_qos(
        &self,
        instance: &str,
        group_name: &str,
        direction: u8,
    ) -> Result<(), ControlPlaneError> {
        let inst = self.get_instance(instance).await?;
        let mut state = inst.write().await;
        Self::check_xdp_ready(&state.pin_path)?;

        let group_id = if group_name == "default" || group_name == "any" {
            0
        } else {
            state
                .state
                .groups
                .get(group_name)
                .map(|g| g.id)
                .ok_or_else(|| ControlPlaneError::GroupNotFound(group_name.to_string()))?
        };

        let target_directions = Self::requested_directions(direction);
        let matching_rules: Vec<QosRuleInfo> = target_directions
            .iter()
            .filter_map(|dir| {
                state
                    .state
                    .qos_rules
                    .iter()
                    .find(|r| r.group_id == group_id && r.direction == *dir)
                    .cloned()
            })
            .collect();
        if matching_rules.is_empty() {
            return Err(ControlPlaneError::PolicyNotFound(format!(
                "QoS rule not found: group={}, direction={}",
                group_name, direction
            )));
        }

        let mut deleted_rules: Vec<QosRuleInfo> = Vec::new();
        for rule in &matching_rules {
            if let Err(e) = aria_core::qos_ops::delete_qos_rule(
                rule.group_id,
                rule.direction,
                state.map_runtime(),
                state.state.qos_enabled,
            ) {
                let rollback = Self::rollback_qos_deletes(
                    state.map_runtime(),
                    &deleted_rules,
                    state.state.qos_enabled,
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
            state
                .state
                .qos_rules
                .retain(|r| !(r.group_id == rule.group_id && r.direction == rule.direction));
            state
                .wal_append(&WalEntry::DeleteQos {
                    group_id: rule.group_id,
                    direction: rule.direction,
                })
                .await;

            // Clear stale QOS_STATS entries so deleted rules no longer appear in API responses.
            if let Err(e) = aria_core::monitoring::clear_qos_stats_for_rule(
                state.map_runtime(),
                rule.group_id,
                rule.direction,
            ) {
                warn!(error = %e, group_id = rule.group_id, direction = rule.direction,
                    "failed to clear qos stats after qos rule delete");
            }
        }

        // If no shaping rules remain, clean up the owned fq qdisc.
        let has_shaping = state.state.qos_rules.iter().any(|r| r.mode == 1);
        if !has_shaping {
            let marker_path = Self::fq_qdisc_marker_path(&state);
            if marker_path.exists() {
                if let Ok(iface) = Self::runtime_iface_name(instance, &state) {
                    if let Err(e) = aria_core::ebpf_ops::cleanup_root_qdisc(&iface) {
                        warn!(instance = %instance, iface = %iface, error = %e,
                            "failed to remove owned fq qdisc after last shaping rule deleted");
                    }
                }
                if let Err(e) = fs::remove_file(&marker_path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        warn!(instance = %instance, path = %marker_path.display(), error = %e,
                            "failed to remove fq qdisc ownership marker");
                    }
                }
            }
        }

        Ok(())
    }
}
