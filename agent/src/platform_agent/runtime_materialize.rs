use aria_core::common::TapMapRuntime;

use super::ir_types::*;

pub(crate) struct Phase3MaterializeResult {
    pub(crate) port_identities_written: usize,
    pub(crate) anti_spoof_entries_written: usize,
    pub(crate) sg_rules_written: usize,
    pub(crate) route_v4_written: usize,
    pub(crate) route_v6_written: usize,
    pub(crate) ip_group_entries_written: usize,
    pub(crate) policy_entries_written: usize,
    pub(crate) qos_entries_written: usize,
}

pub(crate) fn clear_phase3_state_for_port(
    pin_path: &str,
    tap_id: u32,
    ifindex: Option<u32>,
) -> Result<(), String> {
    use aria_core::common::TapMapRuntime;
    use aria_core::ebpf_ops::{clear_iface_ctx, delete_tap_config};
    use aria_core::port_ops::{clear_anti_spoof_entries, delete_port_identity};
    use aria_core::route_ops::clear_route_entries_for_tap;
    use aria_core::sg_ops::clear_sg_rules;
    use aria_core::svc_ops::clear_service_maps_for_tap;

    if let Some(ifindex) = ifindex {
        let _ = clear_iface_ctx(pin_path, ifindex);
    }
    let runtime = TapMapRuntime::new(pin_path, tap_id);
    let _ = delete_tap_config(runtime);
    delete_port_identity(pin_path, tap_id)?;
    clear_anti_spoof_entries(pin_path, tap_id)?;
    clear_sg_rules(pin_path, tap_id, Some(tap_id))?;
    clear_route_entries_for_tap(pin_path, tap_id)?;
    clear_service_maps_for_tap(pin_path, tap_id)?;
    Ok(())
}

pub(crate) fn materialize_phase3_maps(
    pin_path: &str,
    previous_state: Option<&CompiledNodeState>,
    compiled_state: &CompiledNodeState,
) -> Result<Phase3MaterializeResult, String> {
    use aria_core::common::{
        PortIdentityValue, RouteValue, SgRuleKey, SgRuleValue, TapConfig, TapMapRuntime,
        PORT_FLAG_ANTI_SPOOF, PORT_FLAG_HAS_ALLOWED_PAIRS,
    };
    use aria_core::ebpf_ops::{
        clear_iface_ctx, read_runtime_config, sync_iface_ctx, write_tap_config,
    };
    use aria_core::port_ops::{
        clear_anti_spoof_entries, write_anti_spoof_entries, write_port_identity, AntiSpoofEntry,
    };
    use aria_core::route_ops::{clear_route_entries_for_tap, write_route_v4, write_route_v6};
    use aria_core::sg_ops::{clear_sg_rules, write_sg_rule};

    let previous_ports = previous_state
        .map(|state| {
            state
                .port_identities
                .iter()
                .map(|port| (port.port_id.as_str(), port))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let current_tap_ids = compiled_state
        .port_identities
        .iter()
        .map(|port| port.tap_id)
        .collect::<BTreeSet<_>>();
    let previous_tap_ids = previous_state
        .map(|state| {
            state
                .port_identities
                .iter()
                .map(|port| port.tap_id)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    for current_port in &compiled_state.port_identities {
        if let Some(previous_port) = previous_ports.get(current_port.port_id.as_str()) {
            if previous_port.ifindex != current_port.ifindex {
                let _ = clear_iface_ctx(pin_path, previous_port.ifindex);
            }
        }
    }

    for removed_tap_id in previous_tap_ids.difference(&current_tap_ids) {
        let previous_ifindex = previous_state.and_then(|state| {
            state
                .port_identities
                .iter()
                .find(|port| port.tap_id == *removed_tap_id)
                .map(|port| port.ifindex)
        });
        clear_phase3_state_for_port(pin_path, *removed_tap_id, previous_ifindex)?;
    }

    let mut anti_spoof_entries_written = 0usize;
    for port in &compiled_state.port_identities {
        let runtime = TapMapRuntime::new(pin_path, port.tap_id);
        sync_iface_ctx(runtime, port.ifindex)?;
        let existing_runtime = read_runtime_config(runtime).ok();
        write_tap_config(
            runtime,
            TapConfig {
                conntrack_enabled: 1,
                monitoring_enabled: 1,
                acl_enabled: 1,
                qos_enabled: 0,
                mirror_enabled: 0,
                tcprt_enabled: 1,
                lb_enabled: existing_runtime.map(|cfg| cfg.lb_enabled).unwrap_or(0),
                pad: [0; 1],
            },
        )?;

        let mut flags = 0u16;
        if port.anti_spoof_enabled {
            flags |= PORT_FLAG_ANTI_SPOOF;
        }
        if port
            .anti_spoof_entries
            .iter()
            .any(|entry| entry.flags & 0x02 != 0)
        {
            flags |= PORT_FLAG_HAS_ALLOWED_PAIRS;
        }

        write_port_identity(
            pin_path,
            port.tap_id,
            PortIdentityValue {
                mac: port.mac,
                flags,
                network_id: port.network_local_id,
                segment_id: port.segment_local_id,
                tenant_id: port.tenant_local_id,
                primary_ipv4: port.primary_ipv4,
                primary_ipv6: port.primary_ipv6,
                sg_id: port.sg_program_id,
                ip_count: port.anti_spoof_entries.len().min(u16::MAX as usize) as u16,
                pad: [0; 2],
            },
        )?;

        clear_anti_spoof_entries(pin_path, port.tap_id)?;
        anti_spoof_entries_written += write_anti_spoof_entries(
            pin_path,
            port.tap_id,
            &port
                .anti_spoof_entries
                .iter()
                .map(|entry| AntiSpoofEntry {
                    address: entry.address,
                    flags: entry.flags,
                })
                .collect::<Vec<_>>(),
        )?;
    }

    for tap_id in &current_tap_ids {
        clear_sg_rules(pin_path, *tap_id, Some(*tap_id))?;
    }
    let mut sg_rules_written = 0usize;
    for rule in &compiled_state.sg_rules {
        write_sg_rule(
            pin_path,
            &SgRuleKey {
                tap_id: rule.tap_id,
                sg_id: rule.sg_program_id,
                direction: rule.direction,
                proto: rule.proto,
                pad: [0; 2],
                remote_prefix: rule.remote_prefix,
                prefix_len: rule.prefix_len,
                pad2: [0; 3],
            },
            &SgRuleValue {
                action: rule.action,
                priority: rule.priority,
                port_start: rule.port_start,
                port_end: rule.port_end,
                rule_id: rule.rule_id,
            },
        )?;
        sg_rules_written += 1;
    }

    for tap_id in &current_tap_ids {
        clear_route_entries_for_tap(pin_path, *tap_id)?;
    }
    let mut route_v4_written = 0usize;
    let mut route_v6_written = 0usize;
    for route in &compiled_state.route_entries {
        let value = RouteValue {
            next_hop_ip: route.next_hop_ip,
            egress_ifindex: route.egress_ifindex,
            route_id: route.route_id,
            next_hop_type: route.next_hop_type,
            priority: route.priority,
            flags: 0,
            pad: [0; 3],
        };
        if route.is_ipv6 {
            write_route_v6(
                pin_path,
                route.tap_id,
                route.destination,
                route.prefix_len,
                value,
            )?;
            route_v6_written += 1;
        } else {
            write_route_v4(
                pin_path,
                route.tap_id,
                [
                    route.destination[12],
                    route.destination[13],
                    route.destination[14],
                    route.destination[15],
                ],
                route.prefix_len,
                value,
            )?;
            route_v4_written += 1;
        }
    }

    // --- NetworkPolicy POLICY_TABLE materialization ---
    // Build a map from network_id to tap_ids (shared by IpGroup and NetworkPolicy).
    let network_tap_ids: BTreeMap<&str, Vec<u32>> =
        compiled_state
            .port_identities
            .iter()
            .fold(BTreeMap::new(), |mut acc, port| {
                acc.entry(&port.network_id).or_default().push(port.tap_id);
                acc
            });
    // Clear previous Controller-written policy entries.
    if let Some(prev) = previous_state {
        for prev_np in &prev.network_policies {
            for prev_rule in &prev_np.rules {
                for tap_id in &current_tap_ids {
                    let runtime = TapMapRuntime::new(pin_path, *tap_id);
                    let _ = aria_core::ebpf_ops::delete_policy(
                        prev_rule.src_numeric_id,
                        prev_rule.dst_numeric_id,
                        prev_rule.proto,
                        prev_rule.direction,
                        runtime,
                        "",
                    );
                }
            }
        }
    }
    let mut policy_entries_written = 0usize;
    for np in &compiled_state.network_policies {
        let tap_ids = match network_tap_ids.get(np.network_id.as_str()) {
            Some(ids) => ids,
            None => continue,
        };
        for rule in &np.rules {
            for tap_id in tap_ids {
                let runtime = TapMapRuntime::new(pin_path, *tap_id);
                aria_core::ebpf_ops::add_policy(
                    rule.src_numeric_id,
                    rule.dst_numeric_id,
                    rule.proto,
                    rule.action,
                    rule.ports.as_deref(),
                    None,  // bitmap_idx: not managed per-rule for Controller path
                    false, // is_new_port_set: no bitmap to write without idx
                    rule.direction,
                    runtime,
                    "",
                )?;
                policy_entries_written += 1;
            }
        }
    }

    // --- IpGroup LPM Trie materialization ---
    // Clear previous Controller-written LPM entries for IpGroups.
    if let Some(prev) = previous_state {
        for prev_group in &prev.ip_groups {
            for cidr_ir in &prev_group.cidrs {
                for tap_id in &current_tap_ids {
                    let runtime = TapMapRuntime::new(pin_path, *tap_id);
                    let _ = aria_core::ebpf_ops::delete_network(
                        "src",
                        &cidr_ir.cidr,
                        prev_group.numeric_id,
                        runtime,
                        "",
                    );
                    let runtime = TapMapRuntime::new(pin_path, *tap_id);
                    let _ = aria_core::ebpf_ops::delete_network(
                        "dst",
                        &cidr_ir.cidr,
                        prev_group.numeric_id,
                        runtime,
                        "",
                    );
                }
            }
        }
    }
    let mut ip_group_entries_written = 0usize;
    for group in &compiled_state.ip_groups {
        let tap_ids = match network_tap_ids.get(group.network_id.as_str()) {
            Some(ids) => ids,
            None => continue,
        };
        for cidr_ir in &group.cidrs {
            for tap_id in tap_ids {
                let runtime = TapMapRuntime::new(pin_path, *tap_id);
                aria_core::ebpf_ops::add_network(
                    "src",
                    &cidr_ir.cidr,
                    group.numeric_id,
                    runtime,
                    "",
                )?;
                let runtime = TapMapRuntime::new(pin_path, *tap_id);
                aria_core::ebpf_ops::add_network(
                    "dst",
                    &cidr_ir.cidr,
                    group.numeric_id,
                    runtime,
                    "",
                )?;
                ip_group_entries_written += 2; // src + dst
            }
        }
    }

    // --- QosPolicy QOS_CONFIG / QOS_TOKEN_BUCKET materialization ---
    // Clear previous Controller-written QoS entries.
    if let Some(prev) = previous_state {
        for prev_qp in &prev.qos_policies {
            for prev_rule in &prev_qp.rules {
                for tap_id in &current_tap_ids {
                    let runtime = TapMapRuntime::new(pin_path, *tap_id);
                    let _ = aria_core::qos_ops::delete_qos_rule(
                        prev_rule.ip_group_numeric_id,
                        prev_rule.direction,
                        runtime,
                        false, // user_qos_enabled — sync happens after full write
                    );
                }
            }
        }
    }
    let mut qos_entries_written = 0usize;
    let mut has_any_qos_rule = false;
    for qp in &compiled_state.qos_policies {
        let tap_ids = match network_tap_ids.get(qp.network_id.as_str()) {
            Some(ids) => ids,
            None => continue,
        };
        for rule in &qp.rules {
            has_any_qos_rule = true;
            for tap_id in tap_ids {
                let runtime = TapMapRuntime::new(pin_path, *tap_id);
                aria_core::qos_ops::add_qos_rule(
                    rule.ip_group_numeric_id,
                    rule.direction,
                    rule.rate_bps,
                    rule.burst_bytes,
                    rule.priority,
                    rule.mode,
                    runtime,
                    true, // user_qos_enabled — Controller-managed QoS is always enabled
                )?;
                qos_entries_written += 1;
            }
        }
    }
    // Update qos_enabled flag on each tap that now has (or no longer has) QoS rules.
    for tap_id in &current_tap_ids {
        let runtime = TapMapRuntime::new(pin_path, *tap_id);
        let _ = aria_core::ebpf_ops::update_runtime_config(
            runtime,
            None,
            None,
            None,
            Some(has_any_qos_rule),
            None,
            None,
            None,
            None,
        );
    }

    Ok(Phase3MaterializeResult {
        port_identities_written: compiled_state.port_identities.len(),
        anti_spoof_entries_written,
        sg_rules_written,
        route_v4_written,
        route_v6_written,
        ip_group_entries_written,
        policy_entries_written,
        qos_entries_written,
    })
}

/// Materialize compiled service state into pinned eBPF maps.
/// Returns (frontends_written, backends_written, revnats_written) on success.
pub(crate) fn materialize_service_maps(
    pin_path: &str,
    tap_id: u32,
    service_programs: &[ServiceProgramIr],
    health_executor: &crate::health_check::HealthCheckExecutor,
) -> Result<(usize, usize, usize), String> {
    use std::collections::btree_map::Entry;

    use aria_core::common::{
        SVC_BACKEND_FLAG_LOCAL, SVC_FRONTEND_FLAG_HAS_AFFINITY, SVC_FRONTEND_FLAG_LOCAL_ONLY,
        SVC_FRONTEND_FLAG_USE_MAGLEV, SVC_LB_ALGO_MAGLEV, SVC_LB_ALGO_RANDOM,
    };
    use aria_core::svc_ops::{
        clear_service_maps_for_tap, compute_maglev_table, ipv4_to_v4mapped,
        restore_service_maps_for_tap, snapshot_service_maps_for_tap, write_maglev_table,
        write_service_backends, write_service_frontends, write_service_revnats, SvcBackendEntry,
        SvcFrontendEntry, SvcMaglevTableEntry, SvcRevNatEntry,
    };

    let mut frontend_entries = Vec::new();
    let mut backend_entries = Vec::new();
    let mut revnat_entries = BTreeMap::new();
    let mut maglev_entries = Vec::new();
    let mut service_id_counter: u32 = 1;

    for program in service_programs {
        let service_id = service_id_counter;
        service_id_counter += 1;

        let local_backends: Vec<&BackendMemberIr> = program
            .backend_set
            .as_ref()
            .map(|bs| {
                bs.backends
                    .iter()
                    .filter(|b| {
                        if b.admin_state == "disabled" || b.resolved_locality != "local" {
                            return false;
                        }
                        // Check health state if health check is configured.
                        if program.health_check.is_some() {
                            if let Some(ip) = b.resolved_ip_hint.as_deref() {
                                let target = crate::health_check::BackendTarget {
                                    service_id: program.service_id.clone(),
                                    backend_id: b.backend_id.clone(),
                                    address: ip.to_string(),
                                    port: b.service_port,
                                };
                                let state = health_executor.get_state(&target);
                                if state == crate::health_check::HealthState::Unhealthy {
                                    return false;
                                }
                            }
                        }
                        true
                    })
                    .collect()
            })
            .unwrap_or_default();

        let backend_count = local_backends.len() as u16;

        let vip_addr = match program.frontend.vip.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(v4)) => ipv4_to_v4mapped(&v4),
            Ok(std::net::IpAddr::V6(v6)) => v6.octets(),
            Err(_) => continue,
        };

        let proto = match program.frontend.protocol.as_str() {
            "tcp" => 6u8,
            "udp" => 17u8,
            _ => continue,
        };

        let is_maglev = program.frontend.lb_policy == "maglev";
        let lb_algo = if is_maglev {
            SVC_LB_ALGO_MAGLEV
        } else {
            SVC_LB_ALGO_RANDOM
        };

        for listener in &program.frontend.listener_ports {
            let mut flags = SVC_FRONTEND_FLAG_LOCAL_ONLY;
            if program.frontend.session_affinity.as_deref() == Some("client_ip") {
                flags |= SVC_FRONTEND_FLAG_HAS_AFFINITY;
            }
            if is_maglev {
                flags |= SVC_FRONTEND_FLAG_USE_MAGLEV;
            }
            frontend_entries.push(SvcFrontendEntry {
                tap_id,
                address: vip_addr,
                port: listener.service_port,
                proto,
                scope: 0,
                service_id,
                backend_count,
                flags,
                lb_algo,
            });
        }

        for (slot, backend) in local_backends.iter().enumerate() {
            let backend_addr = match backend
                .resolved_ip_hint
                .as_deref()
                .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
            {
                Some(std::net::IpAddr::V4(v4)) => ipv4_to_v4mapped(&v4),
                Some(std::net::IpAddr::V6(v6)) => v6.octets(),
                None => continue,
            };

            backend_entries.push(SvcBackendEntry {
                tap_id,
                service_id,
                slot: slot as u16,
                address: backend_addr,
                port: backend.service_port,
                weight: backend.weight,
                flags: SVC_BACKEND_FLAG_LOCAL,
            });

            for listener in &program.frontend.listener_ports {
                let revnat_key = (backend_addr, backend.service_port, proto);
                let revnat_entry = SvcRevNatEntry {
                    tap_id,
                    backend_address: backend_addr,
                    backend_port: backend.service_port,
                    proto,
                    service_address: vip_addr,
                    service_port: listener.service_port,
                };
                match revnat_entries.entry(revnat_key) {
                    Entry::Vacant(entry) => {
                        entry.insert(revnat_entry);
                    }
                    Entry::Occupied(entry) => {
                        let existing = entry.get();
                        if existing.service_address != revnat_entry.service_address
                            || existing.service_port != revnat_entry.service_port
                        {
                            return Err(format!(
                                "service '{}' has ambiguous revnat mapping for backend port {} on tap {}",
                                program.service_id, backend.service_port, tap_id
                            ));
                        }
                    }
                }
            }
        }

        // Compute and store Maglev table if lb_policy is maglev.
        if is_maglev && !local_backends.is_empty() {
            let backend_ids: Vec<(String, u16)> = local_backends
                .iter()
                .filter_map(|b| {
                    b.resolved_ip_hint
                        .as_deref()
                        .map(|ip| (ip.to_string(), b.service_port))
                })
                .collect();
            let table = compute_maglev_table(&backend_ids);
            for (idx, &slot) in table.iter().enumerate() {
                maglev_entries.push(SvcMaglevTableEntry {
                    tap_id,
                    service_id,
                    table_index: idx as u16,
                    backend_slot: slot,
                });
            }
        }
    }

    let revnat_entries = revnat_entries.into_values().collect::<Vec<_>>();
    let snapshot = snapshot_service_maps_for_tap(pin_path, tap_id)
        .map_err(|e| format!("snapshot service maps: {}", e))?;
    clear_service_maps_for_tap(pin_path, tap_id)
        .map_err(|e| format!("clear service maps: {}", e))?;

    let write_result = (|| {
        let frontends = write_service_frontends(pin_path, &frontend_entries)
            .map_err(|e| format!("write frontends: {}", e))?;
        let backends = write_service_backends(pin_path, &backend_entries)
            .map_err(|e| format!("write backends: {}", e))?;
        let revnats = write_service_revnats(pin_path, &revnat_entries)
            .map_err(|e| format!("write revnats: {}", e))?;
        if !maglev_entries.is_empty() {
            write_maglev_table(pin_path, &maglev_entries)
                .map_err(|e| format!("write maglev: {}", e))?;
        }
        Ok((frontends, backends, revnats))
    })();

    if let Err(error) = &write_result {
        let _ = clear_service_maps_for_tap(pin_path, tap_id);
        if let Err(restore_error) = restore_service_maps_for_tap(pin_path, &snapshot) {
            return Err(format!("{}; rollback failed: {}", error, restore_error));
        }
    }

    write_result
}

