use aya::maps::{HashMap, Map, MapData};

use crate::common::{AntiSpoofKey, AntiSpoofValue, PortIdentityKey, PortIdentityValue};

const PORT_IDENTITY_MAP_NAME: &str = "PORT_IDENTITY_MAP";
const ANTI_SPOOF_MAP_NAME: &str = "ANTI_SPOOF_MAP";

#[derive(Debug, Clone)]
pub struct AntiSpoofEntry {
    pub address: [u8; 16],
    pub flags: u8,
}

fn open_port_identity_map(
    pin_path: &str,
) -> Result<HashMap<MapData, PortIdentityKey, PortIdentityValue>, String> {
    let map_path = format!("{}/{}", pin_path, PORT_IDENTITY_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", PORT_IDENTITY_MAP_NAME, e))?;
    HashMap::try_from(Map::HashMap(map_data))
        .map_err(|e| format!("convert {}: {:?}", PORT_IDENTITY_MAP_NAME, e))
}

fn open_anti_spoof_map(
    pin_path: &str,
) -> Result<HashMap<MapData, AntiSpoofKey, AntiSpoofValue>, String> {
    let map_path = format!("{}/{}", pin_path, ANTI_SPOOF_MAP_NAME);
    let map_data = MapData::from_pin(&map_path)
        .map_err(|e| format!("open {}: {:?}", ANTI_SPOOF_MAP_NAME, e))?;
    HashMap::try_from(Map::HashMap(map_data))
        .map_err(|e| format!("convert {}: {:?}", ANTI_SPOOF_MAP_NAME, e))
}

pub fn write_port_identity(
    pin_path: &str,
    tap_id: u32,
    value: PortIdentityValue,
) -> Result<(), String> {
    let mut map = open_port_identity_map(pin_path)?;
    let key = PortIdentityKey { tap_id };
    map.insert(&key, &value, 0)
        .map_err(|e| format!("insert {}: {:?}", PORT_IDENTITY_MAP_NAME, e))
}

pub fn delete_port_identity(pin_path: &str, tap_id: u32) -> Result<(), String> {
    let mut map = open_port_identity_map(pin_path)?;
    let key = PortIdentityKey { tap_id };
    match map.remove(&key) {
        Ok(()) => Ok(()),
        Err(e) => {
            let err = format!("{:?}", e);
            if err.contains("KeyNotFound") || err.contains("No such file or directory") {
                Ok(())
            } else {
                Err(format!("remove {}: {}", PORT_IDENTITY_MAP_NAME, err))
            }
        }
    }
}

pub fn write_anti_spoof_entries(
    pin_path: &str,
    tap_id: u32,
    entries: &[AntiSpoofEntry],
) -> Result<usize, String> {
    let mut map = open_anti_spoof_map(pin_path)?;
    let mut written = 0usize;
    for entry in entries {
        let key = AntiSpoofKey {
            tap_id,
            address: entry.address,
        };
        let value = AntiSpoofValue {
            flags: entry.flags,
            pad: [0; 3],
        };
        map.insert(&key, &value, 0)
            .map_err(|e| format!("insert {}: {:?}", ANTI_SPOOF_MAP_NAME, e))?;
        written += 1;
    }
    Ok(written)
}

pub fn clear_anti_spoof_entries(pin_path: &str, tap_id: u32) -> Result<(), String> {
    let mut map = open_anti_spoof_map(pin_path)?;
    let keys_to_remove: Vec<AntiSpoofKey> = map
        .keys()
        .filter_map(|key| key.ok())
        .filter(|key| key.tap_id == tap_id)
        .collect();
    for key in keys_to_remove {
        let _ = map.remove(&key);
    }
    Ok(())
}
