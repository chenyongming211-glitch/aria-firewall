use aya::maps::{HashMap, Map, MapData};

use crate::common::{SgRuleKey, SgRuleValue};

const SG_RULE_MAP_NAME: &str = "SG_RULE_MAP";

fn open_sg_rule_map(pin_path: &str) -> Result<HashMap<MapData, SgRuleKey, SgRuleValue>, String> {
    let map_path = format!("{}/{}", pin_path, SG_RULE_MAP_NAME);
    let map_data =
        MapData::from_pin(&map_path).map_err(|e| format!("open {}: {:?}", SG_RULE_MAP_NAME, e))?;
    HashMap::try_from(Map::HashMap(map_data))
        .map_err(|e| format!("convert {}: {:?}", SG_RULE_MAP_NAME, e))
}

pub fn write_sg_rule(pin_path: &str, key: &SgRuleKey, value: &SgRuleValue) -> Result<(), String> {
    let mut map = open_sg_rule_map(pin_path)?;
    map.insert(key, value, 0)
        .map_err(|e| format!("insert {}: {:?}", SG_RULE_MAP_NAME, e))
}

pub fn delete_sg_rule(pin_path: &str, key: &SgRuleKey) -> Result<(), String> {
    let mut map = open_sg_rule_map(pin_path)?;
    match map.remove(key) {
        Ok(()) => Ok(()),
        Err(e) => {
            let err = format!("{:?}", e);
            if err.contains("KeyNotFound") || err.contains("No such file or directory") {
                Ok(())
            } else {
                Err(format!("remove {}: {}", SG_RULE_MAP_NAME, err))
            }
        }
    }
}

pub fn clear_sg_rules(pin_path: &str, tap_id: u32, sg_id: Option<u32>) -> Result<(), String> {
    let mut map = open_sg_rule_map(pin_path)?;
    let keys_to_remove: Vec<SgRuleKey> = map
        .keys()
        .filter_map(|key| key.ok())
        .filter(|key| key.tap_id == tap_id && sg_id.map(|value| key.sg_id == value).unwrap_or(true))
        .collect();
    for key in keys_to_remove {
        let _ = map.remove(&key);
    }
    Ok(())
}
