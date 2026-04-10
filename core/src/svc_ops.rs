use aya::maps::{HashMap, Map, MapData};

use crate::common::{
    SvcBackendKey, SvcBackendValue, SvcFrontendKey, SvcFrontendValue, SvcRevNatKey,
    SvcRevNatValue, TapMapRuntime,
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
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff,
        octets[0], octets[1], octets[2], octets[3],
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
pub fn write_service_revnats(
    pin_path: &str,
    entries: &[SvcRevNatEntry],
) -> Result<usize, String> {
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
