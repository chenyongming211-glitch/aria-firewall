use super::*;
use crate::state::FirewallState;

/// Apply a single WAL entry to an in-memory FirewallState.
/// Errors in individual entries are logged and skipped (best-effort replay).
pub fn apply_wal_entry(state: &mut FirewallState, entry: WalEntry) -> bool {
    match entry {
        WalEntry::AddGroup { name, cidr } => {
            if let Err(e) = state.add_group(&name, &cidr) {
                warn!(error = %e, group = %name, cidr = %cidr, "WAL replay AddGroup failed");
                return false;
            }
        }
        WalEntry::DeleteGroup { name } => {
            state.groups.remove(&name);
        }
        WalEntry::AddRule {
            src_id,
            dst_id,
            proto,
            action,
            ports,
            direction,
        } => {
            let ports_ref = ports.as_deref();
            if let Err(e) =
                state.apply_add_rule(src_id, dst_id, proto, action, ports_ref, direction)
            {
                warn!(error = %e, src_id, dst_id, proto, direction, "WAL replay AddRule failed");
                return false;
            }
        }
        WalEntry::RemoveRule {
            src_id,
            dst_id,
            proto,
            direction,
        } => {
            if let Err(e) = state.apply_remove_rule(src_id, dst_id, proto, direction) {
                warn!(error = %e, src_id, dst_id, proto, direction, "WAL replay RemoveRule failed");
                return false;
            }
        }
        WalEntry::AddQos {
            group_name,
            group_id,
            direction,
            rate_bps,
            burst_bytes,
            priority,
            mode,
        } => {
            use crate::state::QosRuleInfo;
            state
                .qos_rules
                .retain(|r| !(r.group_id == group_id && r.direction == direction));
            state.qos_rules.push(QosRuleInfo {
                group_name,
                group_id,
                direction,
                rate_bps,
                burst_bytes,
                priority,
                mode,
            });
        }
        WalEntry::DeleteQos {
            group_id,
            direction,
        } => {
            state
                .qos_rules
                .retain(|r| !(r.group_id == group_id && r.direction == direction));
        }
        WalEntry::AddMirror {
            src_group_name,
            src_group_id,
            dst_group_name,
            dst_group_id,
            proto,
            direction,
            target_iface,
            target_ifindex,
            is_global,
        } => {
            use crate::state::MirrorRuleInfo;
            if is_global {
                state
                    .mirror_rules
                    .retain(|r| !(r.is_global && r.direction == direction));
            } else {
                state.mirror_rules.retain(|r| {
                    !(r.src_group_id == src_group_id
                        && r.dst_group_id == dst_group_id
                        && r.proto == proto
                        && r.direction == direction
                        && !r.is_global)
                });
            }
            state.mirror_rules.push(MirrorRuleInfo {
                src_group_name,
                src_group_id,
                dst_group_name,
                dst_group_id,
                proto,
                direction,
                target_iface,
                target_ifindex,
                is_global,
            });
        }
        WalEntry::DeleteMirror {
            src_group_id,
            dst_group_id,
            proto,
            direction,
            is_global,
        } => {
            if is_global {
                state
                    .mirror_rules
                    .retain(|r| !(r.is_global && r.direction == direction));
            } else {
                state.mirror_rules.retain(|r| {
                    !(r.src_group_id == src_group_id
                        && r.dst_group_id == dst_group_id
                        && r.proto == proto
                        && r.direction == direction
                        && !r.is_global)
                });
            }
        }
        WalEntry::UpdateConfig {
            conntrack,
            monitoring,
            acl,
            qos,
            mirror,
            tcprt,
            ssl,
        } => {
            if let Some(ct) = conntrack {
                state.conntrack_enabled = ct;
            }
            if let Some(mon) = monitoring {
                state.monitoring_enabled = mon;
            }
            if let Some(a) = acl {
                state.acl_enabled = a;
            }
            if let Some(q) = qos {
                state.qos_enabled = q;
            }
            if let Some(m) = mirror {
                state.mirror_enabled = m;
            }
            if let Some(t) = tcprt {
                state.tcprt_enabled = t;
            }
            if let Some(s) = ssl {
                state.ssl_enabled = s;
            }
        }
        WalEntry::SetMaxPortPolicies { max } => {
            state.max_port_policies = max;
        }
        WalEntry::SetAttachedIface { iface } => {
            state.attached_iface = Some(iface);
        }
        WalEntry::ClearAttachedIface => {
            state.attached_iface = None;
        }
    }
    true
}

/// Load state from snapshot + replay WAL entries.
pub fn load_with_wal(state_path: &str) -> FirewallState {
    // 1. Load base snapshot
    let state_file = format!("{}/state.json", state_path);
    let mut state = if let Ok(contents) = fs::read_to_string(&state_file) {
        if !contents.is_empty() {
            serde_json::from_str(&contents).unwrap_or_else(|e| {
                warn!(path = %state_file, error = %e, "failed to parse snapshot; using default state");
                FirewallState::default()
            })
        } else {
            FirewallState::default()
        }
    } else {
        FirewallState::default()
    };

    // 2. Replay WAL
    let wal_path = format!("{}/state.wal", state_path);
    if let Ok(file) = File::open(&wal_path) {
        let reader = BufReader::new(file);
        let mut replayed = 0u64;
        let mut failed = 0u64;
        for (line_num, line_result) in reader.lines().enumerate() {
            match line_result {
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<WalEntry>(&line) {
                        Ok(entry) => {
                            if apply_wal_entry(&mut state, entry) {
                                replayed += 1;
                            } else {
                                failed += 1;
                            }
                        }
                        Err(e) => {
                            warn!(path = %wal_path, line = line_num + 1, error = %e, "skipping corrupt WAL entry");
                            failed += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!(path = %wal_path, line = line_num + 1, error = %e, "read error while replaying WAL");
                    failed += 1;
                    break;
                }
            }
        }
        LAST_WAL_REPLAY_FAILURES.store(failed, Ordering::Relaxed);
        if replayed > 0 {
            info!(path = %wal_path, replayed, "replayed WAL entries");
        }
        if failed > 0 {
            warn!(path = %wal_path, failed, "WAL replay completed with failures");
        }
    } else {
        LAST_WAL_REPLAY_FAILURES.store(0, Ordering::Relaxed);
    }

    state
}
