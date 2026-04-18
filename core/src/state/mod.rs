use fslock::LockFile;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use tracing::{info, warn};

pub mod ops;
pub mod types;

#[allow(unused_imports)]
pub use ops::*;
pub use types::*;

pub struct AddRuleResult {
    pub bitmap_idx: Option<u32>,
    pub is_new_port_set: bool,
    /// 如果更新规则导致旧 port set 引用归零，需要清理内核
    pub old_port_set_released: Option<(u32, String)>,
}

pub struct RemoveRuleResult {
    /// 被删除规则的 bitmap_idx（如果有端口过滤）
    pub bitmap_idx: Option<u32>,
    /// 如果 port set 引用计数归零，需要清理内核中的端口条目
    pub port_set_released: Option<String>,
}

fn default_max_port_policies() -> u32 {
    16384
}

fn default_true() -> bool {
    true
}

/// 将用户输入的端口规则归一化为唯一规范形式。
/// 解析 → 按 (start, end, user_action) 排序 → 序列化为 "start[-end]:user_action,..." 形式。
/// 这里持久化的是用户语义：0=pass, 1=drop。
fn normalize_ports(ports_str: &str, default_action: u8) -> Result<String, String> {
    let mut entries: Vec<(u16, u16, u8)> = Vec::new();
    for part in ports_str.split(',') {
        let parts: Vec<&str> = part.trim().split(':').collect();
        let rule_action = match parts.get(1) {
            Some(raw_action) => {
                let action = raw_action
                    .parse::<u8>()
                    .map_err(|_| format!("Invalid action '{}': must be 0 or 1", raw_action))?;
                if action > 1 {
                    return Err(format!("Invalid action {}: must be 0 or 1", action));
                }
                action
            }
            None => default_action,
        };
        if parts[0].contains('-') {
            let range: Vec<&str> = parts[0].split('-').collect();
            if range.len() != 2 {
                return Err("Invalid range format".to_string());
            }
            let start = range[0].trim().parse::<u16>().map_err(|_| "Invalid port")?;
            let end = range[1].trim().parse::<u16>().map_err(|_| "Invalid port")?;
            if start > end {
                return Err(format!("Invalid port range: {}-{}", start, end));
            }
            entries.push((start, end, rule_action));
        } else {
            let port = parts[0].trim().parse::<u16>().map_err(|_| "Invalid port")?;
            entries.push((port, port, rule_action));
        }
    }
    entries.sort();
    let normalized = entries
        .iter()
        .map(|(start, end, act)| {
            if start == end {
                format!("{}:{}", start, act)
            } else {
                format!("{}-{}:{}", start, end, act)
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(normalized)
}

/// 减少端口集引用计数，计数归零时回收 bitmap_idx
fn release_port_set(
    port_sets: &mut HashMap<String, PortSetInfo>,
    free_indices: &mut Vec<u32>,
    bitmap_idx: u32,
) {
    let key_to_remove = port_sets
        .iter()
        .find(|(_, ps)| ps.bitmap_idx == bitmap_idx)
        .map(|(k, _)| k.clone());
    if let Some(key) = key_to_remove {
        if let Some(ps) = port_sets.get_mut(&key) {
            ps.ref_count -= 1;
            if ps.ref_count == 0 {
                free_indices.push(bitmap_idx);
                port_sets.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_state_path() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("/tmp/aria-firewall-test-{}", nanos)
    }

    #[test]
    fn normalize_ports_sorts_and_encodes_actions() {
        let s = "100-200:1,80,443:0";
        let normalized = normalize_ports(s, 0).unwrap();
        // 按 (start,end,act) 排序后应该是 80,100-200,443；act 持久化为用户语义 0=pass 1=drop
        assert_eq!(normalized, "80:0,100-200:1,443:0");
    }

    #[test]
    fn normalize_ports_rejects_invalid_range_and_action() {
        assert!(normalize_ports("200-100", 0).is_err(), "start>end 应报错");
        assert!(normalize_ports("80:2", 0).is_err(), "action>1 应报错");
    }

    #[test]
    fn apply_add_rule_canonicalizes_all_ports_without_bitmap() {
        let mut state = FirewallState::default();

        let result = state
            .apply_add_rule(1, 2, 6, 0, Some(" ALL "), 0)
            .expect("apply_add_rule should accept case-insensitive all");

        assert!(result.bitmap_idx.is_none(), "'all' 不应分配位图");
        assert!(state.port_sets.is_empty(), "'all' 不应创建 port set");
        assert_eq!(state.rules.len(), 1);
        assert_eq!(state.rules[0].ports.as_deref(), Some("all"));
    }

    #[test]
    fn port_sets_refcount_and_reuse_bitmap_idx() {
        let state_path = unique_state_path();
        let mgr = StateManager::new(&state_path);

        // 新建两条规则，端口集字符串相同，应共享同一个 bitmap_idx，ref_count=2
        let r1 = mgr
            .add_rule(1, 2, 6, 0, Some("80,100-200"), 0)
            .expect("add_rule 1");
        let r2 = mgr
            .add_rule(3, 4, 6, 0, Some("80,100-200"), 0)
            .expect("add_rule 2");

        let idx1 = r1.bitmap_idx.expect("bitmap_idx for r1");
        let idx2 = r2.bitmap_idx.expect("bitmap_idx for r2");
        assert_eq!(idx1, idx2, "相同端口集应复用同一 bitmap_idx");

        // 删除第一条规则，不应释放 port set（引用从 2→1）
        let rm1 = mgr.remove_rule(1, 2, 6, 0).expect("remove_rule 1");
        assert_eq!(rm1.bitmap_idx, Some(idx1));
        assert!(
            rm1.port_set_released.is_none(),
            "仍有引用时不应标记 port_set_released"
        );

        // 删除第二条规则，引用归零，应回收 bitmap_idx 并报告释放的端口集
        let rm2 = mgr.remove_rule(3, 4, 6, 0).expect("remove_rule 2");
        assert_eq!(rm2.bitmap_idx, Some(idx1));
        assert!(
            rm2.port_set_released.is_some(),
            "最后一个引用删除后应标记 port_set_released"
        );

        // 再添加一个不同端口集的规则，应复用刚刚回收的 bitmap_idx（free list）
        let r3 = mgr
            .add_rule(5, 6, 6, 0, Some("443"), 0)
            .expect("add_rule 3");
        let idx3 = r3.bitmap_idx.expect("bitmap_idx for r3");
        assert_eq!(
            idx3, idx1,
            "新端口集应优先复用 free_bitmap_indices 中的 idx"
        );
    }

    #[test]
    fn tap_id_round_trips_in_state_file() {
        let state_path = unique_state_path();
        let mgr = StateManager::new(&state_path);

        assert_eq!(
            mgr.get_tap_id().unwrap(),
            0,
            "default tap_id should be unassigned"
        );

        mgr.set_tap_id(42).expect("set tap_id");
        assert_eq!(mgr.get_tap_id().unwrap(), 42, "tap_id should be persisted");

        let reloaded = StateManager::new(&state_path);
        assert_eq!(
            reloaded.get_tap_id().unwrap(),
            42,
            "tap_id should survive reload"
        );
    }
}
