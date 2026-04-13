use super::*;

impl ControlPlane {
    async fn cleanup_failed_managed_registration(
        name: &str,
        pin_path: &str,
        tap_id: u32,
        ifindex: u32,
        wal: WalClient,
        preserve_existing_runtime: bool,
        iface_ctx_synced: bool,
        tap_config_written: bool,
    ) {
        if !preserve_existing_runtime && iface_ctx_synced {
            if let Err(e) = aria_core::ebpf_ops::clear_iface_ctx(pin_path, ifindex) {
                warn!(instance = %name, tap_id, ifindex, error = %e, "failed to clear iface context after register failure");
            }
        }
        if !preserve_existing_runtime
            && tap_config_written
            && tap_id != aria_core::common::TAP_ID_UNASSIGNED
        {
            let runtime = TapMapRuntime::new(pin_path, tap_id);
            if let Err(e) = aria_core::ebpf_ops::delete_tap_config(runtime) {
                warn!(instance = %name, tap_id, error = %e, "failed to clear tap runtime config after register failure");
            }
        }
        if !preserve_existing_runtime && tap_id != aria_core::common::TAP_ID_UNASSIGNED {
            let runtime = TapMapRuntime::new(pin_path, tap_id);
            if let Err(e) = aria_core::ebpf_ops::scrub_managed_runtime_state(runtime) {
                warn!(instance = %name, tap_id, error = %e, "failed to scrub tap runtime state after register failure");
            }
        }
        wal.shutdown().await;
    }

    /// Prepare tap-scoped runtime state before any interface link goes live.
    pub async fn prepare_managed_registration(
        &self,
        name: &str,
        pin_state: &RuntimePinState,
    ) -> Result<PreparedManagedInstance, String> {
        let pin_path = self.managed_pin_path();
        let state_path = format!("{}/{}", self.base_state_path, name);
        let ifindex = Self::resolve_ifindex(name)?;
        let global_ssl_enabled = match self.read_ssl_global_config().await {
            Ok(enabled) => Some(enabled),
            Err(e) => {
                warn!(instance = %name, error = %e, "failed to read global SSL config during register");
                None
            }
        };

        // If already registered, compact before replacing
        let replacing_existing = {
            let instances = self.instances.read().await;
            if let Some(existing) = instances.get(name) {
                let mut st = existing.write().await;
                st.do_compact().await;
                true
            } else {
                false
            }
        };

        let mut state = aria_core::wal::load_with_wal(&state_path);
        let tap_id_assigned = self.ensure_managed_tap_id(name, &mut state).await?;
        if tap_id_assigned {
            let state_manager = aria_core::state::StateManager::new(&state_path);
            state_manager
                .set_tap_id(state.tap_id)
                .map_err(|e| format!("failed to persist tap_id for {}: {}", name, e))?;
            info!(instance = %name, tap_id = state.tap_id, "prepared managed tap state");
        }
        let ssl_changed = global_ssl_enabled
            .map(|enabled| state.ssl_enabled != enabled)
            .unwrap_or(false);
        if let Some(enabled) = global_ssl_enabled {
            if ssl_changed {
                state.ssl_enabled = enabled;
            }
        }

        let wal = match WalClient::open(&state_path) {
            Ok(w) => w,
            Err(e) => {
                return Err(format!("failed to open WAL for {}: {}", name, e));
            }
        };

        // Compact on startup if WAL had replayed entries
        if wal.entry_count() > 0 || ssl_changed || tap_id_assigned {
            match serde_json::to_string_pretty(&state) {
                Ok(json) => {
                    if let Err(e) = wal.compact(json).await {
                        error!(instance = %name, error = %e, "failed to compact WAL on register");
                    }
                }
                Err(e) => {
                    error!(instance = %name, error = %e, "failed to serialize state on register");
                }
            }
        }

        let tap_id = state.tap_id;
        let preserve_existing_runtime = replacing_existing || pin_state.preexisting_xdp_link;
        let mut iface_ctx_synced = false;
        let mut tap_config_written = false;

        if pin_state.preexisting_xdp_link {
            if let Err(e) =
                self.validate_preexisting_live_runtime(name, &pin_path, tap_id, ifindex, &state)
            {
                wal.shutdown().await;
                return Err(e);
            }
        } else if !replacing_existing {
            if let Err(e) = aria_core::ebpf_ops::scrub_managed_runtime_state(TapMapRuntime::new(
                &pin_path, tap_id,
            )) {
                wal.shutdown().await;
                return Err(format!("failed to scrub stale tap runtime state: {}", e));
            }
        } else {
            info!(instance = %name, tap_id, "skipping pre-replay scrub while replacing existing registered instance");
        }

        if !pin_state.preexisting_xdp_link {
            if let Err(e) = aria_core::ebpf_ops::update_runtime_config(
                TapMapRuntime::new(&pin_path, tap_id),
                Some(state.conntrack_enabled),
                Some(state.monitoring_enabled),
                Some(state.acl_enabled),
                Some(state.qos_enabled && !state.qos_rules.is_empty()),
                Some(state.mirror_enabled && !state.mirror_rules.is_empty()),
                Some(state.tcprt_enabled),
                None,
                Some(state.lb_enabled),
            ) {
                Self::cleanup_failed_managed_registration(
                    name,
                    &pin_path,
                    tap_id,
                    ifindex,
                    wal,
                    preserve_existing_runtime,
                    iface_ctx_synced,
                    tap_config_written,
                )
                .await;
                return Err(e);
            }
            tap_config_written = tap_id != aria_core::common::TAP_ID_UNASSIGNED;

            if let Err(e) = aria_core::ebpf_ops::replay_state_to_pinned_maps(&pin_path, &state_path)
            {
                Self::cleanup_failed_managed_registration(
                    name,
                    &pin_path,
                    tap_id,
                    ifindex,
                    wal,
                    preserve_existing_runtime,
                    iface_ctx_synced,
                    tap_config_written,
                )
                .await;
                return Err(e);
            }

            if let Err(e) =
                aria_core::ebpf_ops::sync_iface_ctx(TapMapRuntime::new(&pin_path, tap_id), ifindex)
            {
                Self::cleanup_failed_managed_registration(
                    name,
                    &pin_path,
                    tap_id,
                    ifindex,
                    wal,
                    preserve_existing_runtime,
                    iface_ctx_synced,
                    tap_config_written,
                )
                .await;
                return Err(e);
            }
            iface_ctx_synced = true;
        }

        Ok(PreparedManagedInstance {
            name: name.to_string(),
            state,
            tap_id,
            ifindex,
            pin_path,
            state_path,
            wal,
            desired_ssl_enabled: if pin_state.preexisting_xdp_link {
                None
            } else {
                global_ssl_enabled
            },
            preserve_existing_runtime,
            iface_ctx_synced,
            tap_config_written,
        })
    }

    pub async fn publish_managed_instance(&self, prepared: PreparedManagedInstance) {
        let PreparedManagedInstance {
            name,
            state,
            tap_id,
            ifindex,
            pin_path,
            state_path,
            wal,
            desired_ssl_enabled,
            ..
        } = prepared;

        let instance = Arc::new(tokio::sync::RwLock::new(InstanceState {
            state,
            tap_id,
            ifindex: Some(ifindex),
            pin_path,
            state_path,
            wal,
            ssl_sync_pending: false,
            last_ssl_sync_error: None,
        }));

        let mut instances = self.instances.write().await;
        instances.insert(name.to_string(), instance.clone());
        drop(instances);

        let trace_pin_path = {
            let state = instance.read().await;
            state.pin_path.clone()
        };
        if let Err(e) = self.trace_manager.register_tap(&trace_pin_path, tap_id).await {
            warn!(
                instance = %name,
                tap_id,
                error = %e,
                "failed to register trace runtime for managed instance"
            );
        }

        if let Some(enabled) = desired_ssl_enabled {
            let _ = self
                .reconcile_instance_ssl_state(&name, &instance, enabled)
                .await;
        }

        if let Err(e) = self
            .kernel_drop_manager
            .sync_managed_iface(&name, ifindex, tap_id)
            .await
        {
            warn!(
                instance = %name,
                ifindex,
                tap_id,
                error = %e,
                "failed to register managed interface with kernel drop manager"
            );
        }

        info!(instance = %name, tap_id, ifindex, "registered instance");
    }

    pub async fn abort_managed_registration(&self, prepared: PreparedManagedInstance) {
        Self::cleanup_failed_managed_registration(
            &prepared.name,
            &prepared.pin_path,
            prepared.tap_id,
            prepared.ifindex,
            prepared.wal,
            prepared.preserve_existing_runtime,
            prepared.iface_ctx_synced,
            prepared.tap_config_written,
        )
        .await;
    }

    /// Register the "system" instance (standalone mode)
    pub async fn register_system_instance(
        &self,
        pin_path: &str,
        state_path: &str,
    ) -> Result<(), String> {
        let global_ssl_enabled = match self.read_ssl_global_config().await {
            Ok(enabled) => Some(enabled),
            Err(e) => {
                warn!(error = %e, "failed to read global SSL config during system register");
                None
            }
        };
        let mut state = aria_core::wal::load_with_wal(state_path);
        let tap_id_reset = if state.tap_id != aria_core::common::TAP_ID_UNASSIGNED {
            state.tap_id = aria_core::common::TAP_ID_UNASSIGNED;
            true
        } else {
            false
        };
        let ssl_changed = global_ssl_enabled
            .map(|enabled| state.ssl_enabled != enabled)
            .unwrap_or(false);
        if let Some(enabled) = global_ssl_enabled {
            if ssl_changed {
                state.ssl_enabled = enabled;
            }
        }
        let ifindex = match state.attached_iface.as_deref() {
            Some(iface) => match Self::resolve_ifindex(iface) {
                Ok(ifindex) => Some(ifindex),
                Err(e) => {
                    warn!(
                        instance = "system",
                        iface = %iface,
                        error = %e,
                        "failed to resolve system interface ifindex for kernel drop manager"
                    );
                    None
                }
            },
            None => None,
        };

        let wal = match WalClient::open(state_path) {
            Ok(w) => w,
            Err(e) => {
                return Err(format!("failed to open WAL for system: {}", e));
            }
        };

        // Compact on startup if WAL had replayed entries
        if wal.entry_count() > 0 || ssl_changed || tap_id_reset {
            match serde_json::to_string_pretty(&state) {
                Ok(json) => {
                    if let Err(e) = wal.compact(json).await {
                        error!(instance = "system", error = %e, "failed to compact WAL on system register");
                    }
                }
                Err(e) => {
                    error!(instance = "system", error = %e, "failed to serialize state on system register");
                }
            }
        }

        let tap_id = state.tap_id;
        let runtime = TapMapRuntime::new(pin_path, tap_id);
        aria_core::ebpf_ops::update_runtime_config(
            runtime,
            Some(state.conntrack_enabled),
            Some(state.monitoring_enabled),
            Some(state.acl_enabled),
            Some(state.qos_enabled && !state.qos_rules.is_empty()),
            Some(state.mirror_enabled && !state.mirror_rules.is_empty()),
            Some(state.tcprt_enabled),
            None,
            Some(state.lb_enabled),
        )?;
        let instance = Arc::new(tokio::sync::RwLock::new(InstanceState {
            state,
            tap_id,
            ifindex,
            pin_path: pin_path.to_string(),
            state_path: state_path.to_string(),
            wal,
            ssl_sync_pending: false,
            last_ssl_sync_error: None,
        }));

        let mut instances = self.instances.write().await;
        instances.insert("system".to_string(), instance.clone());
        drop(instances);

        if let Err(e) = self.trace_manager.register_tap(pin_path, tap_id).await {
            warn!(
                instance = "system",
                tap_id,
                error = %e,
                "failed to register trace runtime for system instance"
            );
        }

        if let Some(enabled) = global_ssl_enabled {
            let _ = self
                .reconcile_instance_ssl_state("system", &instance, enabled)
                .await;
        }

        if let Some(ifindex) = ifindex {
            if let Err(e) = self
                .kernel_drop_manager
                .sync_managed_iface("system", ifindex, tap_id)
                .await
            {
                warn!(
                    instance = "system",
                    ifindex,
                    tap_id,
                    error = %e,
                    "failed to register system interface with kernel drop manager"
                );
            }
        }

        info!(instance = "system", tap_id, ifindex = ?ifindex, "registered system instance");
        Ok(())
    }

    /// Unregister an instance (called when TapRegistry detaches)
    pub async fn unregister_instance(&self, name: &str) {
        // Compact before removing
        self.compact_instance(name).await;
        let mut instances = self.instances.write().await;
        if let Some(inst) = instances.remove(name) {
            let mut state = inst.write().await;
            let tap_id = state.tap_id;
            let ifindex = state.ifindex;
            if let Some(ifindex) = ifindex {
                if let Err(e) = self.kernel_drop_manager.remove_managed_iface(ifindex).await {
                    warn!(
                        instance = %name,
                        ifindex,
                        error = %e,
                        "failed to remove managed interface from kernel drop manager"
                    );
                }
            }
            if tap_id != aria_core::common::TAP_ID_UNASSIGNED {
                self.trace_manager.unregister_tap(&state.pin_path, tap_id).await;
                if let Err(e) =
                    aria_core::ebpf_ops::scrub_managed_runtime_state(state.map_runtime())
                {
                    warn!(
                        instance = %name,
                        tap_id,
                        ifindex = ?ifindex,
                        error = %e,
                        "failed to scrub managed runtime state during unregister"
                    );
                }
            } else if name == "system" {
                self.trace_manager.unregister_tap(&state.pin_path, tap_id).await;
            } else if name != "system" {
                if let Some(ifindex) = ifindex {
                    if let Err(e) = aria_core::ebpf_ops::clear_iface_ctx(&state.pin_path, ifindex) {
                        warn!(instance = %name, tap_id, ifindex, error = %e, "failed to clear iface context");
                    }
                }
            }
            state.shutdown_wal().await;
            info!(instance = %name, tap_id, ifindex = ?ifindex, "unregistered instance");
            return;
        }
        info!(instance = %name, "unregistered instance");
    }

    /// List all registered instance names
    pub async fn list_instances(&self) -> Vec<String> {
        let instances = self.instances.read().await;
        let mut names: Vec<String> = instances.keys().cloned().collect();
        names.sort();
        names
    }
}
