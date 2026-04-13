use super::*;

impl FirewallState {
    /// Add or update a group. Returns the group ID.
    pub fn add_group(&mut self, name: &str, cidr: &str) -> Result<u32, String> {
        if name == "any" {
            return Err("Group name 'any' is reserved and cannot be used".to_string());
        }
        if let Some(existing) = self.groups.get_mut(name) {
            if !existing.cidrs.contains(&cidr.to_string()) {
                existing.cidrs.push(cidr.to_string());
            }
            Ok(existing.id)
        } else {
            let id = self.next_group_id;
            self.next_group_id += 1;
            self.groups.insert(
                name.to_string(),
                GroupInfo {
                    id,
                    name: name.to_string(),
                    cidrs: vec![cidr.to_string()],
                },
            );
            Ok(id)
        }
    }

    /// Rollback a group add: remove the CIDR, and if the group is now empty, remove it and undo next_group_id.
    pub fn rollback_add_group(&mut self, name: &str, cidr: &str, was_new_group: bool) {
        if let Some(g) = self.groups.get_mut(name) {
            g.cidrs.retain(|c| c != cidr);
            if g.cidrs.is_empty() {
                self.groups.remove(name);
                if was_new_group {
                    self.next_group_id -= 1;
                }
            }
        }
    }

    /// Add or update a rule in-memory. Returns AddRuleResult.
    pub fn apply_add_rule(
        &mut self,
        src_group_id: u32,
        dst_group_id: u32,
        proto: u8,
        action: u8,
        ports: Option<&str>,
        direction: u8,
    ) -> Result<AddRuleResult, String> {
        let mut result = AddRuleResult {
            bitmap_idx: None,
            is_new_port_set: false,
            old_port_set_released: None,
        };

        let stored_ports = ports.map(|p| {
            let trimmed = p.trim();
            if trimmed.eq_ignore_ascii_case("all") {
                "all".to_string()
            } else {
                trimmed.to_string()
            }
        });

        let (bitmap_idx, is_new) = if let Some(p) = stored_ports
            .as_deref()
            .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case("all"))
        {
            let normalized = normalize_ports(p, action)?;

            if let Some(existing_ps) = self.port_sets.get_mut(&normalized) {
                existing_ps.ref_count += 1;
                (Some(existing_ps.bitmap_idx), false)
            } else {
                let idx = if let Some(recycled) = self.free_bitmap_indices.pop() {
                    recycled
                } else {
                    if self.next_bitmap_idx >= self.max_port_policies {
                        return Err(format!(
                            "Port set limit ({}) reached. Unique port combinations: {}",
                            self.max_port_policies,
                            self.port_sets.len()
                        ));
                    }
                    let idx = self.next_bitmap_idx;
                    self.next_bitmap_idx += 1;
                    idx
                };
                self.port_sets.insert(
                    normalized.clone(),
                    PortSetInfo {
                        bitmap_idx: idx,
                        ports_normalized: normalized,
                        ref_count: 1,
                    },
                );
                (Some(idx), true)
            }
        } else {
            (None, false)
        };

        // 检测重复规则：相同 (src_group_id, dst_group_id, proto, direction) → 更新
        if let Some(existing) = self.rules.iter_mut().find(|r| {
            r.src_group_id == src_group_id
                && r.dst_group_id == dst_group_id
                && r.proto == proto
                && r.direction == direction
        }) {
            // 旧规则有 bitmap → 减引用计数
            if let Some(old_idx) = existing.bitmap_idx {
                if bitmap_idx != Some(old_idx) {
                    let old_ports_normalized = self
                        .port_sets
                        .iter()
                        .find(|(_, ps)| ps.bitmap_idx == old_idx)
                        .map(|(_, ps)| ps.ports_normalized.clone());

                    release_port_set(&mut self.port_sets, &mut self.free_bitmap_indices, old_idx);

                    if self.free_bitmap_indices.contains(&old_idx) {
                        if let Some(ports_norm) = old_ports_normalized {
                            result.old_port_set_released = Some((old_idx, ports_norm));
                        }
                    }
                } else {
                    // 新旧相同 bitmap_idx，撤销上面多加的 ref_count
                    if let Some(key) = self
                        .port_sets
                        .iter()
                        .find(|(_, ps)| ps.bitmap_idx == old_idx)
                        .map(|(k, _)| k.clone())
                    {
                        if let Some(ps) = self.port_sets.get_mut(&key) {
                            ps.ref_count -= 1;
                        }
                    }
                }
            }
            existing.action = action;
            existing.ports = stored_ports.clone();
            existing.bitmap_idx = bitmap_idx;
        } else {
            self.rules.push(RuleInfo {
                name: None,
                src_group_id,
                dst_group_id,
                proto,
                action,
                ports: stored_ports,
                bitmap_idx,
                direction,
            });
        }

        result.bitmap_idx = bitmap_idx;
        result.is_new_port_set = is_new;
        Ok(result)
    }

    /// Remove a rule in-memory. Returns RemoveRuleResult.
    pub fn apply_remove_rule(
        &mut self,
        src_group_id: u32,
        dst_group_id: u32,
        proto: u8,
        direction: u8,
    ) -> Result<RemoveRuleResult, String> {
        let mut result = RemoveRuleResult {
            bitmap_idx: None,
            port_set_released: None,
        };

        if let Some(pos) = self.rules.iter().position(|r| {
            r.src_group_id == src_group_id
                && r.dst_group_id == dst_group_id
                && r.proto == proto
                && r.direction == direction
        }) {
            let rule = self.rules.remove(pos);
            if let Some(idx) = rule.bitmap_idx {
                let ports_normalized = self
                    .port_sets
                    .iter()
                    .find(|(_, ps)| ps.bitmap_idx == idx)
                    .map(|(_, ps)| ps.ports_normalized.clone());

                release_port_set(&mut self.port_sets, &mut self.free_bitmap_indices, idx);

                result.bitmap_idx = Some(idx);
                if self.free_bitmap_indices.contains(&idx) {
                    result.port_set_released = ports_normalized;
                }
            }
        } else {
            return Err(format!(
                "Policy not found: src_id={}, dst_id={}, proto={}, direction={}",
                src_group_id, dst_group_id, proto, direction
            ));
        }

        Ok(result)
    }
}

impl StateManager {
    pub fn new(state_path: &str) -> Self {
        let state_file = PathBuf::from(format!("{}/state.json", state_path));
        if let Some(parent) = state_file.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        Self { state_file }
    }

    /// 获取操作级锁，覆盖 state + kernel maps 的整个操作
    pub fn acquire_ops_lock(&self) -> Result<LockFile, String> {
        let lock_path = self.state_file.with_file_name("ops.lock");
        let mut lock =
            LockFile::open(&lock_path).map_err(|e| format!("Failed to open ops lock: {}", e))?;
        lock.lock()
            .map_err(|e| format!("Failed to acquire ops lock: {}", e))?;
        Ok(lock)
    }

    fn with_state<F>(&self, mut f: F) -> Result<(), String>
    where
        F: FnMut(&mut FirewallState) -> Result<(), String>,
    {
        let lock_path = self.state_file.with_extension("lock");
        let mut lock =
            LockFile::open(&lock_path).map_err(|e| format!("Failed to open lock file: {}", e))?;
        lock.lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;

        let mut state = if self.state_file.exists() {
            let mut file = File::open(&self.state_file)
                .map_err(|e| format!("Failed to open state file: {}", e))?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(|e| format!("Failed to read state file: {}", e))?;
            if contents.is_empty() {
                warn!(path = %self.state_file.display(), "state file is empty; starting with default state");
                FirewallState::default()
            } else {
                serde_json::from_str(&contents)
                    .map_err(|e| format!("Failed to parse state file: {}", e))?
            }
        } else {
            info!(path = %self.state_file.display(), "state file does not exist; creating new state");
            FirewallState::default()
        };

        f(&mut state)?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.state_file)
            .map_err(|e| format!("Failed to open state file for writing: {}", e))?;

        let contents = serde_json::to_string_pretty(&state)
            .map_err(|e| format!("Failed to serialize state: {}", e))?;

        file.write_all(contents.as_bytes())
            .map_err(|e| format!("Failed to write state file: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync state file: {}", e))?;

        Ok(())
    }

    pub fn add_group(&self, name: &str, cidr: &str) -> Result<u32, String> {
        if name == "any" {
            return Err("Group name 'any' is reserved and cannot be used".to_string());
        }

        let mut id = 0;
        self.with_state(|state| {
            if let Some(existing) = state.groups.get_mut(name) {
                if !existing.cidrs.contains(&cidr.to_string()) {
                    existing.cidrs.push(cidr.to_string());
                }
                id = existing.id;
            } else {
                id = state.next_group_id;
                state.next_group_id += 1;
                let group = GroupInfo {
                    id,
                    name: name.to_string(),
                    cidrs: vec![cidr.to_string()],
                };
                state.groups.insert(name.to_string(), group);
            }
            Ok(())
        })?;
        Ok(id)
    }

    pub fn remove_cidr_from_group(&self, name: &str, cidr: &str) -> Result<(), String> {
        self.with_state(|state| {
            if let Some(group) = state.groups.get_mut(name) {
                group.cidrs.retain(|c| c != cidr);
                if group.cidrs.is_empty() {
                    state.groups.remove(name);
                }
            }
            Ok(())
        })
    }

    pub fn delete_group(&self, name: &str) -> Result<(), String> {
        self.with_state(|state| {
            state.groups.remove(name);
            Ok(())
        })
    }

    pub fn set_max_port_policies(&self, max: u32) -> Result<(), String> {
        self.with_state(|state| {
            state.max_port_policies = max;
            Ok(())
        })
    }

    pub fn set_attached_iface(&self, iface: &str) -> Result<(), String> {
        self.with_state(|state| {
            state.attached_iface = Some(iface.to_string());
            Ok(())
        })
    }

    pub fn clear_attached_iface(&self) -> Result<(), String> {
        self.with_state(|state| {
            state.attached_iface = None;
            Ok(())
        })
    }

    pub fn get_attached_iface(&self) -> Result<Option<String>, String> {
        let state = self._load_readonly()?;
        Ok(state.attached_iface)
    }

    pub fn set_tap_id(&self, tap_id: u32) -> Result<(), String> {
        self.with_state(|state| {
            state.tap_id = tap_id;
            Ok(())
        })
    }

    pub fn get_tap_id(&self) -> Result<u32, String> {
        let state = self._load_readonly()?;
        Ok(state.tap_id)
    }

    pub fn add_rule(
        &self,
        src_group_id: u32,
        dst_group_id: u32,
        proto: u8,
        action: u8,
        ports: Option<&str>,
        direction: u8,
    ) -> Result<AddRuleResult, String> {
        let mut result = AddRuleResult {
            bitmap_idx: None,
            is_new_port_set: false,
            old_port_set_released: None,
        };
        self.with_state(|state| {
            result = state.apply_add_rule(
                src_group_id,
                dst_group_id,
                proto,
                action,
                ports,
                direction,
            )?;
            Ok(())
        })?;
        Ok(result)
    }

    pub fn remove_rule(
        &self,
        src_group_id: u32,
        dst_group_id: u32,
        proto: u8,
        direction: u8,
    ) -> Result<RemoveRuleResult, String> {
        let mut result = RemoveRuleResult {
            bitmap_idx: None,
            port_set_released: None,
        };
        self.with_state(|state| {
            result = state.apply_remove_rule(src_group_id, dst_group_id, proto, direction)?;
            Ok(())
        })?;
        Ok(result)
    }

    pub fn get_group(&self, name: &str) -> Result<Option<GroupInfo>, String> {
        let state = self._load_readonly()?;
        Ok(state.groups.get(name).cloned())
    }

    pub fn list_groups(&self) -> Result<Vec<GroupInfo>, String> {
        let state = self._load_readonly()?;
        Ok(state.groups.values().cloned().collect())
    }

    pub fn list_rules(&self) -> Result<Vec<RuleInfo>, String> {
        let state = self._load_readonly()?;
        Ok(state.rules.clone())
    }

    #[allow(dead_code)]
    pub fn get_group_by_id(&self, id: u32) -> Result<Option<GroupInfo>, String> {
        let state = self._load_readonly()?;
        Ok(state.groups.values().find(|g| g.id == id).cloned())
    }

    fn _load_readonly(&self) -> Result<FirewallState, String> {
        let lock_path = self.state_file.with_extension("lock");
        let mut lock =
            LockFile::open(&lock_path).map_err(|e| format!("Failed to open lock file: {}", e))?;
        lock.lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;

        let state = if self.state_file.exists() {
            let mut file = File::open(&self.state_file)
                .map_err(|e| format!("Failed to open state file: {}", e))?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(|e| format!("Failed to read state file: {}", e))?;
            if contents.is_empty() {
                FirewallState::default()
            } else {
                serde_json::from_str(&contents)
                    .map_err(|e| format!("Failed to parse state file: {}", e))?
            }
        } else {
            FirewallState::default()
        };

        Ok(state)
    }

    // --- QoS state management ---

    pub fn add_qos_rule(
        &self,
        group_name: &str,
        group_id: u32,
        direction: u8,
        rate_bps: u64,
        burst_bytes: u64,
        priority: u8,
        mode: u8,
    ) -> Result<(), String> {
        self.with_state(|state| {
            // Remove existing rule with same group+direction
            state
                .qos_rules
                .retain(|r| !(r.group_id == group_id && r.direction == direction));
            state.qos_rules.push(QosRuleInfo {
                group_name: group_name.to_string(),
                group_id,
                direction,
                rate_bps,
                burst_bytes,
                priority,
                mode,
            });
            Ok(())
        })
    }

    pub fn remove_qos_rule(&self, group_id: u32, direction: u8) -> Result<(), String> {
        self.with_state(|state| {
            let before = state.qos_rules.len();
            state
                .qos_rules
                .retain(|r| !(r.group_id == group_id && r.direction == direction));
            if state.qos_rules.len() == before {
                return Err(format!(
                    "QoS rule not found: group_id={}, direction={}",
                    group_id, direction
                ));
            }
            Ok(())
        })
    }

    pub fn list_qos_rules(&self) -> Result<Vec<QosRuleInfo>, String> {
        let state = self._load_readonly()?;
        Ok(state.qos_rules.clone())
    }

    pub fn set_conntrack_enabled(&self, enabled: bool) -> Result<(), String> {
        self.with_state(|state| {
            state.conntrack_enabled = enabled;
            Ok(())
        })
    }

    pub fn set_monitoring_enabled(&self, enabled: bool) -> Result<(), String> {
        self.with_state(|state| {
            state.monitoring_enabled = enabled;
            Ok(())
        })
    }

    pub fn get_config(&self) -> Result<(bool, bool, bool, bool, bool), String> {
        let state = self._load_readonly()?;
        Ok((
            state.conntrack_enabled,
            state.monitoring_enabled,
            state.acl_enabled,
            state.qos_enabled,
            state.mirror_enabled,
        ))
    }

    pub fn set_acl_enabled(&self, enabled: bool) -> Result<(), String> {
        self.with_state(|state| {
            state.acl_enabled = enabled;
            Ok(())
        })
    }

    pub fn set_qos_enabled(&self, enabled: bool) -> Result<(), String> {
        self.with_state(|state| {
            state.qos_enabled = enabled;
            Ok(())
        })
    }

    // --- Mirror state management ---

    pub fn set_mirror_enabled(&self, enabled: bool) -> Result<(), String> {
        self.with_state(|state| {
            state.mirror_enabled = enabled;
            Ok(())
        })
    }
}
