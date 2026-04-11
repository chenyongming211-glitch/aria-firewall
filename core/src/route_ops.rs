use aya::maps::lpm_trie::Key;
use aya::maps::{LpmTrie, Map, MapData};

use crate::common::RouteValue;

const ROUTE_TABLE_V4_MAP_NAME: &str = "ROUTE_TABLE_V4";
const ROUTE_TABLE_V6_MAP_NAME: &str = "ROUTE_TABLE_V6";
const TAP_LPM_PREFIX_BITS: u32 = 32;

fn tap_lpm_key_v4(tap_id: u32, ip: [u8; 4], prefix_len: u8) -> Key<[u8; 8]> {
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&tap_id.to_be_bytes());
    bytes[4..].copy_from_slice(&ip);
    Key::new(TAP_LPM_PREFIX_BITS + prefix_len as u32, bytes)
}

fn tap_lpm_key_v6(tap_id: u32, ip: [u8; 16], prefix_len: u8) -> Key<[u8; 20]> {
    let mut bytes = [0u8; 20];
    bytes[..4].copy_from_slice(&tap_id.to_be_bytes());
    bytes[4..].copy_from_slice(&ip);
    Key::new(TAP_LPM_PREFIX_BITS + prefix_len as u32, bytes)
}

fn open_route_table_v4(pin_path: &str) -> Result<LpmTrie<MapData, [u8; 8], RouteValue>, String> {
    let map_path = format!("{}/{}", pin_path, ROUTE_TABLE_V4_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", ROUTE_TABLE_V4_MAP_NAME, e))?;
    LpmTrie::try_from(Map::LpmTrie(map_data))
        .map_err(|e| format!("convert {}: {:?}", ROUTE_TABLE_V4_MAP_NAME, e))
}

fn open_route_table_v6(pin_path: &str) -> Result<LpmTrie<MapData, [u8; 20], RouteValue>, String> {
    let map_path = format!("{}/{}", pin_path, ROUTE_TABLE_V6_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", ROUTE_TABLE_V6_MAP_NAME, e))?;
    LpmTrie::try_from(Map::LpmTrie(map_data))
        .map_err(|e| format!("convert {}: {:?}", ROUTE_TABLE_V6_MAP_NAME, e))
}

pub fn write_route_v4(
    pin_path: &str,
    tap_id: u32,
    destination: [u8; 4],
    prefix_len: u8,
    value: RouteValue,
) -> Result<(), String> {
    let mut map = open_route_table_v4(pin_path)?;
    let key = tap_lpm_key_v4(tap_id, destination, prefix_len);
    map.insert(&key, &value, 0)
        .map_err(|e| format!("insert {}: {:?}", ROUTE_TABLE_V4_MAP_NAME, e))
}

pub fn delete_route_v4(
    pin_path: &str,
    tap_id: u32,
    destination: [u8; 4],
    prefix_len: u8,
) -> Result<(), String> {
    let mut map = open_route_table_v4(pin_path)?;
    let key = tap_lpm_key_v4(tap_id, destination, prefix_len);
    match map.remove(&key) {
        Ok(()) => Ok(()),
        Err(e) => {
            let err = format!("{:?}", e);
            if err.contains("KeyNotFound") || err.contains("No such file or directory") {
                Ok(())
            } else {
                Err(format!("remove {}: {}", ROUTE_TABLE_V4_MAP_NAME, err))
            }
        }
    }
}

pub fn write_route_v6(
    pin_path: &str,
    tap_id: u32,
    destination: [u8; 16],
    prefix_len: u8,
    value: RouteValue,
) -> Result<(), String> {
    let mut map = open_route_table_v6(pin_path)?;
    let key = tap_lpm_key_v6(tap_id, destination, prefix_len);
    map.insert(&key, &value, 0)
        .map_err(|e| format!("insert {}: {:?}", ROUTE_TABLE_V6_MAP_NAME, e))
}

pub fn delete_route_v6(
    pin_path: &str,
    tap_id: u32,
    destination: [u8; 16],
    prefix_len: u8,
) -> Result<(), String> {
    let mut map = open_route_table_v6(pin_path)?;
    let key = tap_lpm_key_v6(tap_id, destination, prefix_len);
    match map.remove(&key) {
        Ok(()) => Ok(()),
        Err(e) => {
            let err = format!("{:?}", e);
            if err.contains("KeyNotFound") || err.contains("No such file or directory") {
                Ok(())
            } else {
                Err(format!("remove {}: {}", ROUTE_TABLE_V6_MAP_NAME, err))
            }
        }
    }
}

pub fn clear_route_entries_for_tap(pin_path: &str, tap_id: u32) -> Result<(), String> {
    let tap_prefix = tap_id.to_be_bytes();

    if let Ok(mut map) = open_route_table_v4(pin_path) {
        let keys_to_remove: Vec<Key<[u8; 8]>> = map
            .iter()
            .filter_map(|item| item.ok().map(|(key, _)| key))
            .filter(|key| key.data()[..4] == tap_prefix)
            .collect();
        for key in keys_to_remove {
            let _ = map.remove(&key);
        }
    }

    if let Ok(mut map) = open_route_table_v6(pin_path) {
        let keys_to_remove: Vec<Key<[u8; 20]>> = map
            .iter()
            .filter_map(|item| item.ok().map(|(key, _)| key))
            .filter(|key| key.data()[..4] == tap_prefix)
            .collect();
        for key in keys_to_remove {
            let _ = map.remove(&key);
        }
    }

    Ok(())
}
