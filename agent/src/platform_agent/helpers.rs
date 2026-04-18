use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aria_api::{NodeCapability, PlatformApiError};

use super::ir_types::*;

pub(crate) fn build_backend_choice_shape(
    backend_set: Option<&BackendSetIr>,
    forwarding_mode: &str,
) -> SocketBackendChoiceShape {
    let backends = match backend_set {
        Some(bs) => &bs.backends,
        None => {
            return SocketBackendChoiceShape {
                total_candidates: 0,
                local_candidates: 0,
                remote_candidates: 0,
                total_weight: 0,
                local_weight: 0,
                remote_weight: 0,
                handoff_type: None,
                candidates: Vec::new(),
            };
        }
    };

    let candidates: Vec<SocketBackendCandidate> = backends
        .iter()
        .filter(|b| b.admin_state != "disabled")
        .map(|b| SocketBackendCandidate {
            backend_id: b.backend_id.clone(),
            target_type: b.target_type.clone(),
            resolved_ip_hint: b.resolved_ip_hint.clone(),
            port: b.service_port,
            weight: b.weight,
            locality: b.resolved_locality.clone(),
            forwarding_scope: b.forwarding_scope.clone(),
            health_state: "unknown".to_string(),
        })
        .collect();

    let local_candidates = candidates.iter().filter(|c| c.locality == "local").count();
    let remote_candidates = candidates.iter().filter(|c| c.locality == "remote").count();
    let total_weight: u32 = candidates.iter().map(|c| c.weight as u32).sum();
    let local_weight: u32 = candidates
        .iter()
        .filter(|c| c.locality == "local")
        .map(|c| c.weight as u32)
        .sum();
    let remote_weight: u32 = candidates
        .iter()
        .filter(|c| c.locality == "remote")
        .map(|c| c.weight as u32)
        .sum();

    let handoff_type = if remote_candidates > 0 {
        Some(
            match forwarding_mode {
                "cross_node_overlay" => "overlay",
                "cross_node_hybrid" => "hybrid",
                _ => "native",
            }
            .to_string(),
        )
    } else {
        None
    };

    SocketBackendChoiceShape {
        total_candidates: candidates.len(),
        local_candidates,
        remote_candidates,
        total_weight,
        local_weight,
        remote_weight,
        handoff_type,
        candidates,
    }
}

pub(crate) fn normalize_socket_lb_strategy(lb_policy: &str) -> String {
    match lb_policy {
        "round_robin" | "random" => "random".to_string(),
        "maglev" => "maglev".to_string(),
        "hash_5tuple" | "hash_src_ip" => "deferred_hash".to_string(),
        _ => "unsupported".to_string(),
    }
}

pub(crate) fn normalize_socket_affinity_strategy(session_affinity: Option<&str>) -> String {
    match session_affinity {
        None | Some("none") => "none".to_string(),
        Some("client_ip") => "client_ip".to_string(),
        Some("5tuple") => "deferred_5tuple".to_string(),
        _ => "unsupported".to_string(),
    }
}

pub(crate) fn total_service_listener_ports(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

pub(crate) fn total_anti_spoof_entries(state: &CompiledNodeState) -> usize {
    state
        .port_identities
        .iter()
        .map(|port| port.anti_spoof_entries.len())
        .sum()
}

pub(crate) fn attached_port_count(state: &CompiledNodeState) -> usize {
    if state.port_identities.is_empty() {
        state.port_bindings.len()
    } else {
        state.port_identities.len()
    }
}

pub(crate) fn total_route_v4_entries(state: &CompiledNodeState) -> usize {
    state
        .route_entries
        .iter()
        .filter(|route| !route.is_ipv6)
        .count()
}

pub(crate) fn total_route_v6_entries(state: &CompiledNodeState) -> usize {
    state
        .route_entries
        .iter()
        .filter(|route| route.is_ipv6)
        .count()
}

pub(crate) fn total_service_frontend_runtime_entries(state: &CompiledNodeState) -> usize {
    total_service_listener_ports(state)
}

pub(crate) fn total_socket_lb_frontends(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.exposure_type == "internal")
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

pub(crate) fn total_packet_lb_frontends(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| {
            program.frontend.exposure_type != "internal" || program.frontend.cross_node_forwarding
        })
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

pub(crate) fn total_service_backend_members(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .map(|program| {
            program
                .backend_set
                .as_ref()
                .map(|backend_set| backend_set.backends.len())
                .unwrap_or(0)
        })
        .sum()
}

pub(crate) fn total_backend_member_runtime_entries(state: &CompiledNodeState) -> usize {
    total_service_backend_members(state)
}

pub(crate) fn total_service_forwarding_projections(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .map(|program| {
            usize::from(program.frontend.node_local_forwarding)
                + usize::from(program.frontend.cross_node_forwarding)
        })
        .sum()
}

pub(crate) fn total_node_local_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.node_local_forwarding)
        .count()
}

pub(crate) fn total_cross_node_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.cross_node_forwarding)
        .count()
}

pub(crate) fn total_cross_node_native_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.forwarding_mode == "cross_node_native")
        .count()
}

pub(crate) fn total_cross_node_overlay_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.forwarding_mode == "cross_node_overlay")
        .count()
}

pub(crate) fn total_cross_node_hybrid_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.forwarding_mode == "cross_node_hybrid")
        .count()
}

pub(crate) fn total_service_revnat_entries(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

pub(crate) fn total_affinity_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| {
            program
                .frontend
                .session_affinity
                .as_deref()
                .map(|value| value != "none")
                .unwrap_or(false)
        })
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

pub(crate) fn total_maglev_service_programs(state: &CompiledNodeState) -> usize {
    state
        .service_programs
        .iter()
        .filter(|program| program.frontend.lb_policy == "maglev")
        .map(|program| program.frontend.listener_ports.len())
        .sum()
}

pub(crate) fn inventory_domain_from_scope(scope: &str) -> String {
    match scope {
        "port-bindings" | "anti-spoof-fastpath" => "ports".to_string(),
        "route-tables" => "routes".to_string(),
        _ => "runtime".to_string(),
    }
}

pub(crate) fn inventory_domain_from_map_family(map_family: &str) -> String {
    match map_family {
        "tenant_index" | "network_index" => "identity".to_string(),
        "sg_rule_map" => "security".to_string(),
        "port_identity_map" | "anti_spoof_map" => "ports".to_string(),
        "route_table_v4" | "route_table_v6" => "routes".to_string(),
        "health_check_catalog"
        | "backend_set_catalog"
        | "service_catalog"
        | "service_frontend_catalog"
        | "service_frontend_map"
        | "service_socket_lb_projection"
        | "service_packet_lb_projection"
        | "backend_member_catalog"
        | "backend_member_map"
        | "service_forwarding_projection"
        | "service_revnat_map"
        | "service_affinity_map"
        | "service_maglev_map" => "services".to_string(),
        "nat_program" => "nat".to_string(),
        "qos_config_map" | "qos_token_bucket_map" | "qos_stats_map" => "qos".to_string(),
        "mirror_policy_map" | "mirror_global_map" => "mirror".to_string(),
        _ => "runtime".to_string(),
    }
}

pub(crate) fn capability_profile(capability: &NodeCapability) -> String {
    let mut hooks = capability.supported_hooks.clone();
    hooks.sort();
    let hooks = hooks.join("+");
    let trace = if capability.supports_trace_ringbuf {
        "ringbuf"
    } else {
        "legacy"
    };
    format!("{hooks}:trace={trace}:nat={}", capability.supports_nat)
}

pub(crate) async fn parse_platform_error(response: reqwest::Response) -> Option<String> {
    let status = response.status();
    let body = response.text().await.ok()?;
    if let Ok(error) = serde_json::from_str::<PlatformApiError>(&body) {
        return Some(format!("{}: {}", status.as_u16(), error.message));
    }
    if body.trim().is_empty() {
        None
    } else {
        Some(format!("{}: {}", status.as_u16(), body.trim()))
    }
}

pub(crate) fn connection_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "southbound request timed out".to_string()
    } else if error.is_connect() {
        format!("failed to connect to controller: {error}")
    } else if let Some(status) = error.status() {
        format!("southbound request failed with status {}", status.as_u16())
    } else {
        format!("southbound request failed: {error}")
    }
}

pub(crate) fn hostname() -> String {
    for path in ["/proc/sys/kernel/hostname", "/etc/hostname"] {
        if let Ok(raw) = std::fs::read_to_string(path) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string())
}

pub(crate) fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

pub(crate) fn temp_path(path: &Path) -> PathBuf {
    let mut tmp = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|ext| format!("{ext}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    tmp.set_extension(extension);
    tmp
}
