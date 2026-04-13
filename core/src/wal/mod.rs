use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

pub mod actor;
pub mod entry;

pub use actor::*;
pub use entry::*;

/// Time-based compact interval (5 minutes)
const WAL_COMPACT_INTERVAL_SECS: u64 = 300;
const MAX_BATCH_SIZE: usize = 100;
const WAL_CHANNEL_CAPACITY: usize = 1024;
static LAST_WAL_REPLAY_FAILURES: AtomicU64 = AtomicU64::new(0);

pub fn last_wal_replay_failures() -> u64 {
    LAST_WAL_REPLAY_FAILURES.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalEntry {
    AddGroup {
        name: String,
        cidr: String,
    },
    DeleteGroup {
        name: String,
    },
    AddRule {
        src_id: u32,
        dst_id: u32,
        proto: u8,
        action: u8,
        ports: Option<String>,
        direction: u8,
    },
    RemoveRule {
        src_id: u32,
        dst_id: u32,
        proto: u8,
        direction: u8,
    },
    AddQos {
        group_name: String,
        group_id: u32,
        direction: u8,
        rate_bps: u64,
        burst_bytes: u64,
        priority: u8,
        #[serde(default)]
        mode: u8,
    },
    DeleteQos {
        group_id: u32,
        direction: u8,
    },
    AddMirror {
        src_group_name: String,
        src_group_id: u32,
        dst_group_name: String,
        dst_group_id: u32,
        proto: u8,
        direction: u8,
        target_iface: String,
        target_ifindex: u32,
        is_global: bool,
    },
    DeleteMirror {
        src_group_id: u32,
        dst_group_id: u32,
        proto: u8,
        direction: u8,
        is_global: bool,
    },
    UpdateConfig {
        conntrack: Option<bool>,
        monitoring: Option<bool>,
        #[serde(default)]
        acl: Option<bool>,
        #[serde(default)]
        qos: Option<bool>,
        #[serde(default)]
        mirror: Option<bool>,
        #[serde(default)]
        tcprt: Option<bool>,
        #[serde(default)]
        ssl: Option<bool>,
    },
    SetMaxPortPolicies {
        max: u32,
    },
    SetAttachedIface {
        iface: String,
    },
    ClearAttachedIface,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::FirewallState;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_state_path() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = format!("/tmp/aria-wal-test-{}", nanos);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn wal_append_and_load_roundtrip() {
        let state_path = temp_state_path();

        // Write initial snapshot
        let mut state = FirewallState::default();
        state.add_group("web", "10.0.0.0/24").unwrap();
        let snapshot = serde_json::to_string_pretty(&state).unwrap();
        fs::write(format!("{}/state.json", state_path), &snapshot).unwrap();

        // Append WAL entries
        {
            let mut wal = WalWriter::open(&state_path).unwrap();
            assert_eq!(wal.entry_count(), 0);

            wal.append(&WalEntry::AddGroup {
                name: "db".to_string(),
                cidr: "10.0.1.0/24".to_string(),
            })
            .unwrap();
            wal.append(&WalEntry::AddRule {
                src_id: 1,
                dst_id: 2,
                proto: 6,
                action: 0,
                ports: Some("80".to_string()),
                direction: 0,
            })
            .unwrap();
            assert_eq!(wal.entry_count(), 2);
        }

        // Load with WAL replay
        let loaded = load_with_wal(&state_path);
        assert!(loaded.groups.contains_key("web"), "snapshot group present");
        assert!(loaded.groups.contains_key("db"), "WAL group present");
        assert_eq!(loaded.rules.len(), 1, "WAL rule present");
        assert_eq!(loaded.rules[0].src_group_id, 1);

        // Cleanup
        let _ = fs::remove_dir_all(&state_path);
    }

    #[test]
    fn wal_compact_clears_wal() {
        let state_path = temp_state_path();

        let mut state = FirewallState::default();
        state.add_group("web", "10.0.0.0/24").unwrap();

        // Write initial snapshot
        let snapshot = serde_json::to_string_pretty(&state).unwrap();
        fs::write(format!("{}/state.json", state_path), &snapshot).unwrap();

        let mut wal = WalWriter::open(&state_path).unwrap();
        wal.append(&WalEntry::AddGroup {
            name: "db".to_string(),
            cidr: "10.0.1.0/24".to_string(),
        })
        .unwrap();
        wal.append(&WalEntry::AddGroup {
            name: "cache".to_string(),
            cidr: "10.0.2.0/24".to_string(),
        })
        .unwrap();
        assert_eq!(wal.entry_count(), 2);

        // Apply entries to state for compact
        state.add_group("db", "10.0.1.0/24").unwrap();
        state.add_group("cache", "10.0.2.0/24").unwrap();

        // Compact
        let json = serde_json::to_string_pretty(&state).unwrap();
        wal.compact(&json).unwrap();
        assert_eq!(wal.entry_count(), 0);

        // WAL file should be empty
        let wal_contents = fs::read_to_string(format!("{}/state.wal", state_path)).unwrap();
        assert!(wal_contents.is_empty(), "WAL should be empty after compact");

        // Snapshot should have all groups
        let loaded = load_with_wal(&state_path);
        assert!(loaded.groups.contains_key("web"));
        assert!(loaded.groups.contains_key("db"));
        assert!(loaded.groups.contains_key("cache"));

        let _ = fs::remove_dir_all(&state_path);
    }

    #[test]
    fn wal_skips_corrupt_lines() {
        let state_path = temp_state_path();

        // Write empty snapshot
        fs::write(format!("{}/state.json", state_path), "{}").unwrap();

        // Write WAL with a corrupt line in the middle
        let wal_file = format!("{}/state.wal", state_path);
        let mut f = File::create(&wal_file).unwrap();
        let entry1 = serde_json::to_string(&WalEntry::AddGroup {
            name: "g1".to_string(),
            cidr: "10.0.0.0/24".to_string(),
        })
        .unwrap();
        let entry2 = serde_json::to_string(&WalEntry::AddGroup {
            name: "g2".to_string(),
            cidr: "10.0.1.0/24".to_string(),
        })
        .unwrap();
        writeln!(f, "{}", entry1).unwrap();
        writeln!(f, "{{corrupt json line}}").unwrap();
        writeln!(f, "{}", entry2).unwrap();

        let loaded = load_with_wal(&state_path);
        assert!(
            loaded.groups.contains_key("g1"),
            "entry before corrupt line applied"
        );
        assert!(
            loaded.groups.contains_key("g2"),
            "entry after corrupt line applied"
        );

        let _ = fs::remove_dir_all(&state_path);
    }

    #[test]
    fn wal_empty_file_loads_default() {
        let state_path = temp_state_path();
        // No snapshot, no WAL
        let loaded = load_with_wal(&state_path);
        assert!(loaded.groups.is_empty());
        assert_eq!(loaded.next_group_id, 1);

        let _ = fs::remove_dir_all(&state_path);
    }

    #[test]
    fn apply_all_entry_types() {
        let mut state = FirewallState::default();

        // AddGroup
        apply_wal_entry(
            &mut state,
            WalEntry::AddGroup {
                name: "web".to_string(),
                cidr: "10.0.0.0/24".to_string(),
            },
        );
        assert!(state.groups.contains_key("web"));

        // AddRule
        apply_wal_entry(
            &mut state,
            WalEntry::AddRule {
                src_id: 1,
                dst_id: 0,
                proto: 6,
                action: 0,
                ports: Some("80,443".to_string()),
                direction: 0,
            },
        );
        assert_eq!(state.rules.len(), 1);

        // RemoveRule
        apply_wal_entry(
            &mut state,
            WalEntry::RemoveRule {
                src_id: 1,
                dst_id: 0,
                proto: 6,
                direction: 0,
            },
        );
        assert_eq!(state.rules.len(), 0);

        // DeleteGroup
        apply_wal_entry(
            &mut state,
            WalEntry::DeleteGroup {
                name: "web".to_string(),
            },
        );
        assert!(!state.groups.contains_key("web"));

        // AddQos
        apply_wal_entry(
            &mut state,
            WalEntry::AddQos {
                group_name: "default".to_string(),
                group_id: 0,
                direction: 0,
                rate_bps: 1_000_000,
                burst_bytes: 125_000,
                priority: 1,
                mode: 0,
            },
        );
        assert_eq!(state.qos_rules.len(), 1);

        // DeleteQos
        apply_wal_entry(
            &mut state,
            WalEntry::DeleteQos {
                group_id: 0,
                direction: 0,
            },
        );
        assert_eq!(state.qos_rules.len(), 0);

        // UpdateConfig
        apply_wal_entry(
            &mut state,
            WalEntry::UpdateConfig {
                conntrack: Some(false),
                monitoring: None,
                acl: None,
                qos: None,
                mirror: None,
                tcprt: None,
                ssl: None,
            },
        );
        assert!(!state.conntrack_enabled);
        assert!(state.monitoring_enabled);

        // SetMaxPortPolicies
        apply_wal_entry(&mut state, WalEntry::SetMaxPortPolicies { max: 100 });
        assert_eq!(state.max_port_policies, 100);

        // SetAttachedIface
        apply_wal_entry(
            &mut state,
            WalEntry::SetAttachedIface {
                iface: "eth0".to_string(),
            },
        );
        assert_eq!(state.attached_iface, Some("eth0".to_string()));

        // ClearAttachedIface
        apply_wal_entry(&mut state, WalEntry::ClearAttachedIface);
        assert_eq!(state.attached_iface, None);
    }

    #[test]
    fn wal_writer_resumes_count() {
        let state_path = temp_state_path();

        // Write 3 entries
        {
            let mut wal = WalWriter::open(&state_path).unwrap();
            wal.append(&WalEntry::AddGroup {
                name: "a".to_string(),
                cidr: "10.0.0.0/24".to_string(),
            })
            .unwrap();
            wal.append(&WalEntry::AddGroup {
                name: "b".to_string(),
                cidr: "10.0.1.0/24".to_string(),
            })
            .unwrap();
            wal.append(&WalEntry::AddGroup {
                name: "c".to_string(),
                cidr: "10.0.2.0/24".to_string(),
            })
            .unwrap();
        }

        // Re-open and verify count resumes
        let wal = WalWriter::open(&state_path).unwrap();
        assert_eq!(wal.entry_count(), 3);

        let _ = fs::remove_dir_all(&state_path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wal_client_append_and_load_roundtrip() {
        let state_path = temp_state_path();

        let wal = WalClient::open(&state_path).unwrap();
        wal.append(WalEntry::AddGroup {
            name: "web".to_string(),
            cidr: "10.0.0.0/24".to_string(),
        })
        .await
        .unwrap();
        wal.append(WalEntry::AddGroup {
            name: "db".to_string(),
            cidr: "10.0.1.0/24".to_string(),
        })
        .await
        .unwrap();
        wal.shutdown().await;

        let loaded = load_with_wal(&state_path);
        assert!(loaded.groups.contains_key("web"));
        assert!(loaded.groups.contains_key("db"));

        let _ = fs::remove_dir_all(&state_path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wal_client_compact_clears_wal_and_persists_snapshot() {
        let state_path = temp_state_path();

        let wal = WalClient::open(&state_path).unwrap();
        wal.append(WalEntry::AddGroup {
            name: "web".to_string(),
            cidr: "10.0.0.0/24".to_string(),
        })
        .await
        .unwrap();

        let mut state = FirewallState::default();
        state.add_group("web", "10.0.0.0/24").unwrap();
        wal.compact(serde_json::to_string_pretty(&state).unwrap())
            .await
            .unwrap();
        wal.shutdown().await;

        let wal_contents = fs::read_to_string(format!("{}/state.wal", state_path)).unwrap();
        assert!(wal_contents.is_empty(), "WAL should be empty after compact");

        let loaded = load_with_wal(&state_path);
        assert!(loaded.groups.contains_key("web"));

        let _ = fs::remove_dir_all(&state_path);
    }
}
