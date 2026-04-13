pub fn proto_to_string(proto: u8) -> String {
    match proto {
        0 => "any".to_string(),
        1 => "icmp".to_string(),
        6 => "tcp".to_string(),
        17 => "udp".to_string(),
        _ => format!("{}", proto),
    }
}

pub fn proto_from_string(proto: &str) -> Result<u8, String> {
    match proto.to_lowercase().as_str() {
        "tcp" => Ok(6),
        "udp" => Ok(17),
        "icmp" => Ok(1),
        "any" => Ok(0),
        _ => proto
            .parse::<u8>()
            .map_err(|_| format!("Invalid protocol '{}'", proto)),
    }
}

pub fn action_to_string(action: u8) -> String {
    match action {
        0 => "allow".to_string(),
        1 => "drop".to_string(),
        _ => format!("{}", action),
    }
}

pub fn action_from_string(action: &str) -> Result<u8, String> {
    match action.to_lowercase().as_str() {
        "accept" | "pass" | "allow" => Ok(0),
        "drop" | "deny" => Ok(1),
        _ => Err(format!("Invalid action '{}'", action)),
    }
}

pub fn direction_to_string(direction: u8) -> String {
    match direction {
        0 => "ingress".to_string(),
        1 => "egress".to_string(),
        _ => format!("{}", direction),
    }
}

pub fn direction_from_string(direction: &str) -> Result<u8, String> {
    match direction.to_lowercase().as_str() {
        "ingress" | "in" => Ok(0),
        "egress" | "out" => Ok(1),
        "both" | "all" => Ok(2),
        _ => Err(format!(
            "Invalid direction '{}': must be 'ingress', 'egress', or 'both'",
            direction
        )),
    }
}
