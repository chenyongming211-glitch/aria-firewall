use aya::maps::{HashMap, Map, MapData};

use crate::common::{
    SvcBackendKey, SvcBackendValue, SvcFrontendKey, SvcFrontendValue, SvcRevNatKey, SvcRevNatValue,
};

const SVC_FRONTEND_MAP_NAME: &str = "SVC_FRONTEND_MAP";
const SVC_BACKEND_MAP_NAME: &str = "SVC_BACKEND_MAP";
const SVC_REVNAT_MAP_NAME: &str = "SVC_REVNAT_MAP";

fn open_frontend_map(
    pin_path: &str,
) -> Result<HashMap<MapData, SvcFrontendKey, SvcFrontendValue>, String> {
    let map_path = format!("{}/{}", pin_path, SVC_FRONTEND_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", SVC_FRONTEND_MAP_NAME, e))?;
    HashMap::<_, SvcFrontendKey, SvcFrontendValue>::try_from(Map::HashMap(map_data))
        .map_err(|e| format!("convert {}: {:?}", SVC_FRONTEND_MAP_NAME, e))
}

fn open_backend_map(
    pin_path: &str,
) -> Result<HashMap<MapData, SvcBackendKey, SvcBackendValue>, String> {
    let map_path = format!("{}/{}", pin_path, SVC_BACKEND_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", SVC_BACKEND_MAP_NAME, e))?;
    HashMap::<_, SvcBackendKey, SvcBackendValue>::try_from(Map::HashMap(map_data))
        .map_err(|e| format!("convert {}: {:?}", SVC_BACKEND_MAP_NAME, e))
}

fn open_revnat_map(
    pin_path: &str,
) -> Result<HashMap<MapData, SvcRevNatKey, SvcRevNatValue>, String> {
    let map_path = format!("{}/{}", pin_path, SVC_REVNAT_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", SVC_REVNAT_MAP_NAME, e))?;
    HashMap::<_, SvcRevNatKey, SvcRevNatValue>::try_from(Map::HashMap(map_data))
        .map_err(|e| format!("convert {}: {:?}", SVC_REVNAT_MAP_NAME, e))
}

/// IPv4 address to v4-mapped-v6 representation.
pub fn ipv4_to_v4mapped(ip: &std::net::Ipv4Addr) -> [u8; 16] {
    let octets = ip.octets();
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, octets[0], octets[1], octets[2], octets[3],
    ]
}

/// A single service frontend entry to be written to the datapath.
#[derive(Debug, Clone)]
pub struct SvcFrontendEntry {
    pub tap_id: u32,
    pub address: [u8; 16],
    pub port: u16,
    pub proto: u8,
    pub scope: u8,
    pub service_id: u32,
    pub backend_count: u16,
    pub flags: u16,
    pub lb_algo: u8,
}

/// A single backend member entry to be written to the datapath.
#[derive(Debug, Clone)]
pub struct SvcBackendEntry {
    pub tap_id: u32,
    pub service_id: u32,
    pub slot: u16,
    pub address: [u8; 16],
    pub port: u16,
    pub weight: u16,
    pub flags: u16,
}

/// A single reverse NAT entry to be written to the datapath.
#[derive(Debug, Clone)]
pub struct SvcRevNatEntry {
    pub tap_id: u32,
    pub backend_address: [u8; 16],
    pub backend_port: u16,
    pub proto: u8,
    pub service_address: [u8; 16],
    pub service_port: u16,
}

/// Write a batch of service frontend entries to the pinned map.
pub fn write_service_frontends(
    pin_path: &str,
    entries: &[SvcFrontendEntry],
) -> Result<usize, String> {
    let mut map = open_frontend_map(pin_path)?;
    let mut written = 0;
    for entry in entries {
        let key = SvcFrontendKey {
            tap_id: entry.tap_id,
            address: entry.address,
            port: entry.port,
            proto: entry.proto,
            scope: entry.scope,
        };
        let value = SvcFrontendValue {
            service_id: entry.service_id,
            backend_count: entry.backend_count,
            flags: entry.flags,
            lb_algo: entry.lb_algo,
            pad: [0; 3],
        };
        map.insert(&key, &value, 0)
            .map_err(|e| format!("insert {}: {:?}", SVC_FRONTEND_MAP_NAME, e))?;
        written += 1;
    }
    Ok(written)
}

/// Write a batch of backend member entries to the pinned map.
pub fn write_service_backends(
    pin_path: &str,
    entries: &[SvcBackendEntry],
) -> Result<usize, String> {
    let mut map = open_backend_map(pin_path)?;
    let mut written = 0;
    for entry in entries {
        let key = SvcBackendKey {
            tap_id: entry.tap_id,
            service_id: entry.service_id,
            slot: entry.slot,
            pad: [0; 2],
        };
        let value = SvcBackendValue {
            address: entry.address,
            port: entry.port,
            weight: entry.weight,
            flags: entry.flags,
            pad: [0; 2],
        };
        map.insert(&key, &value, 0)
            .map_err(|e| format!("insert {}: {:?}", SVC_BACKEND_MAP_NAME, e))?;
        written += 1;
    }
    Ok(written)
}

/// Write a batch of reverse NAT entries to the pinned map.
pub fn write_service_revnats(pin_path: &str, entries: &[SvcRevNatEntry]) -> Result<usize, String> {
    let mut map = open_revnat_map(pin_path)?;
    let mut written = 0;
    for entry in entries {
        let key = SvcRevNatKey {
            tap_id: entry.tap_id,
            address: entry.backend_address,
            port: entry.backend_port,
            proto: entry.proto,
            pad: 0,
        };
        let value = SvcRevNatValue {
            service_address: entry.service_address,
            service_port: entry.service_port,
            pad: [0; 6],
        };
        map.insert(&key, &value, 0)
            .map_err(|e| format!("insert {}: {:?}", SVC_REVNAT_MAP_NAME, e))?;
        written += 1;
    }
    Ok(written)
}

/// Remove all service-related entries for a given tap_id from all three maps.
pub fn clear_service_maps_for_tap(pin_path: &str, tap_id: u32) -> Result<(), String> {
    // Frontend map
    if let Ok(mut map) = open_frontend_map(pin_path) {
        let keys_to_remove: Vec<SvcFrontendKey> = map
            .keys()
            .filter_map(|k| k.ok())
            .filter(|k| k.tap_id == tap_id)
            .collect();
        for key in keys_to_remove {
            let _ = map.remove(&key);
        }
    }

    // Backend map
    if let Ok(mut map) = open_backend_map(pin_path) {
        let keys_to_remove: Vec<SvcBackendKey> = map
            .keys()
            .filter_map(|k| k.ok())
            .filter(|k| k.tap_id == tap_id)
            .collect();
        for key in keys_to_remove {
            let _ = map.remove(&key);
        }
    }

    // RevNat map
    if let Ok(mut map) = open_revnat_map(pin_path) {
        let keys_to_remove: Vec<SvcRevNatKey> = map
            .keys()
            .filter_map(|k| k.ok())
            .filter(|k| k.tap_id == tap_id)
            .collect();
        for key in keys_to_remove {
            let _ = map.remove(&key);
        }
    }

    Ok(())
}

// --- Maglev consistent hashing ---

use crate::common::{SvcMaglevEntry, SvcMaglevKey, MAGLEV_TABLE_SIZE};

const SVC_MAGLEV_MAP_NAME: &str = "SVC_MAGLEV_MAP";

fn open_maglev_map(
    pin_path: &str,
) -> Result<HashMap<MapData, SvcMaglevKey, SvcMaglevEntry>, String> {
    let map_path = format!("{}/{}", pin_path, SVC_MAGLEV_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", SVC_MAGLEV_MAP_NAME, e))?;
    HashMap::<_, SvcMaglevKey, SvcMaglevEntry>::try_from(Map::HashMap(map_data))
        .map_err(|e| format!("convert {}: {:?}", SVC_MAGLEV_MAP_NAME, e))
}

/// A Maglev table entry to write: (tap_id, service_id, table_index) → backend_slot.
#[derive(Debug, Clone)]
pub struct SvcMaglevTableEntry {
    pub tap_id: u32,
    pub service_id: u32,
    pub table_index: u16,
    pub backend_slot: u16,
}

/// Write a batch of Maglev table entries to the pinned map.
pub fn write_maglev_table(
    pin_path: &str,
    entries: &[SvcMaglevTableEntry],
) -> Result<usize, String> {
    let mut map = open_maglev_map(pin_path)?;
    let mut written = 0;
    for entry in entries {
        let key = SvcMaglevKey {
            tap_id: entry.tap_id,
            service_id: entry.service_id,
            table_index: entry.table_index,
            pad: [0; 2],
        };
        let value = SvcMaglevEntry {
            backend_slot: entry.backend_slot,
            pad: [0; 2],
        };
        map.insert(&key, &value, 0)
            .map_err(|e| format!("insert {}: {:?}", SVC_MAGLEV_MAP_NAME, e))?;
        written += 1;
    }
    Ok(written)
}

/// Remove all Maglev entries for a given (tap_id, service_id).
pub fn clear_maglev_table_for_service(
    pin_path: &str,
    tap_id: u32,
    service_id: u32,
) -> Result<(), String> {
    if let Ok(mut map) = open_maglev_map(pin_path) {
        for idx in 0..MAGLEV_TABLE_SIZE as u16 {
            let key = SvcMaglevKey {
                tap_id,
                service_id,
                table_index: idx,
                pad: [0; 2],
            };
            let _ = map.remove(&key);
        }
    }
    Ok(())
}

/// Compute a Maglev lookup table from a list of backend identifiers.
/// Each backend is identified by (ip_string, port) for hashing.
/// Returns a Vec of MAGLEV_TABLE_SIZE entries, each containing a backend slot index.
pub fn compute_maglev_table(backends: &[(String, u16)]) -> Vec<u16> {
    let table_size = MAGLEV_TABLE_SIZE as usize;
    let n = backends.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![0u16; table_size];
    }

    // Compute per-backend (offset, skip) permutation pairs.
    let mut permutations: Vec<(usize, usize)> = Vec::with_capacity(n);
    for (ip, port) in backends {
        let mut h1: u32 = 0x9e37_79b9;
        for b in ip.as_bytes() {
            h1 = h1.wrapping_add(*b as u32);
            h1 ^= h1.rotate_left(5);
        }
        h1 = h1.wrapping_add(*port as u32);
        h1 = h1.wrapping_mul(0x85eb_ca6b);

        let mut h2: u32 = 0x517c_c1b7;
        for b in ip.as_bytes() {
            h2 = h2.wrapping_add(*b as u32);
            h2 ^= h2.rotate_left(11);
        }
        h2 = h2.wrapping_add(*port as u32);
        h2 = h2.wrapping_mul(0xc2b2_ae35);

        let offset = (h1 as usize) % table_size;
        let skip = ((h2 as usize) % (table_size - 1)) + 1;
        permutations.push((offset, skip));
    }

    // Fill the table using the standard Maglev algorithm.
    let mut table = vec![u16::MAX; table_size];
    let mut next = vec![0usize; n]; // next position in each backend's permutation
    let mut filled = 0usize;

    'outer: loop {
        for i in 0..n {
            let (offset, skip) = permutations[i];
            let mut pos = (offset + next[i] * skip) % table_size;
            // Find next unclaimed slot in this backend's permutation.
            let mut attempts = 0;
            while table[pos] != u16::MAX {
                next[i] += 1;
                pos = (offset + next[i] * skip) % table_size;
                attempts += 1;
                if attempts >= table_size {
                    break;
                }
            }
            if table[pos] == u16::MAX {
                table[pos] = i as u16;
                next[i] += 1;
                filled += 1;
                if filled >= table_size {
                    break 'outer;
                }
            }
        }
    }

    table
}

// --- LB Statistics ---

use crate::common::{SvcLbStatsKey, SvcLbStatsValue};
use aya::maps::PerCpuValues;

const SVC_LB_STATS_MAP_NAME: &str = "SVC_LB_STATS";

#[derive(Debug, Clone)]
pub struct SvcLbStatsEntry {
    pub tap_id: u32,
    pub service_id: u32,
    pub backend_slot: u16,
    pub lb_algo: u8,
    pub affinity_hit: bool,
    pub packets: u64,
    pub bytes: u64,
}

fn sum_per_cpu_lb_stats(values: PerCpuValues<SvcLbStatsValue>) -> (u64, u64) {
    let mut packets = 0u64;
    let mut bytes = 0u64;
    for v in values.iter() {
        packets += v.packets;
        bytes += v.bytes;
    }
    (packets, bytes)
}

/// Read all LB stats entries, optionally filtered by tap_id.
pub fn get_lb_stats(pin_path: &str, tap_id: Option<u32>) -> Result<Vec<SvcLbStatsEntry>, String> {
    let map_path = format!("{}/{}", pin_path, SVC_LB_STATS_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", SVC_LB_STATS_MAP_NAME, e))?;
    let map = aya::maps::PerCpuHashMap::<_, SvcLbStatsKey, SvcLbStatsValue>::try_from(
        aya::maps::Map::PerCpuHashMap(map_data),
    )
    .map_err(|e| format!("convert {}: {:?}", SVC_LB_STATS_MAP_NAME, e))?;

    let mut entries = Vec::new();
    for item in map.iter() {
        let Ok((key, values)) = item else { continue };
        if let Some(tid) = tap_id {
            if key.tap_id != tid {
                continue;
            }
        }
        let (packets, bytes) = sum_per_cpu_lb_stats(values);
        if packets == 0 {
            continue;
        }
        entries.push(SvcLbStatsEntry {
            tap_id: key.tap_id,
            service_id: key.service_id,
            backend_slot: key.backend_slot,
            lb_algo: key.lb_algo,
            affinity_hit: key.affinity_hit != 0,
            packets,
            bytes,
        });
    }

    entries.sort_by(|a, b| b.packets.cmp(&a.packets));
    Ok(entries)
}
