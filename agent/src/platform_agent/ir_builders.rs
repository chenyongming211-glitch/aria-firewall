use std::collections::BTreeMap;
use std::net::IpAddr;

use aria_api;
use tracing::warn;

use super::helpers;
use super::ir_types::*;

pub(crate) fn required_runtime_label(port: &aria_api::PortResource, key: &str) -> Result<u32, String> {
    let value = port
        .metadata
        .labels
        .get(key)
        .ok_or_else(|| format!("missing required runtime label '{}'", key))?;
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("runtime label '{}' must be a positive integer", key))?;
    if parsed == 0 {
        return Err(format!("runtime label '{}' must be non-zero", key));
    }
    Ok(parsed)
}

pub(crate) fn parse_mac_address(value: &str) -> Result<[u8; 6], String> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 6 {
        return Err("mac_address must contain 6 octets".to_string());
    }
    let mut mac = [0u8; 6];
    for (idx, part) in parts.iter().enumerate() {
        mac[idx] = u8::from_str_radix(part, 16)
            .map_err(|_| format!("invalid mac_address octet '{}'", part))?;
    }
    Ok(mac)
}

pub(crate) fn parse_allowed_ip(value: &str) -> Result<[u8; 16], String> {
    let ip = strip_cidr_suffix(value)
        .parse::<std::net::IpAddr>()
        .map_err(|error| format!("invalid IP address: {error}"))?;
    Ok(match ip {
        std::net::IpAddr::V4(ipv4) => ipv4_to_v4mapped_bytes(ipv4.octets()),
        std::net::IpAddr::V6(ipv6) => ipv6.octets(),
    })
}

pub(crate) fn extract_ipv4_u32(address: &[u8; 16]) -> Option<u32> {
    if is_v4_mapped(address) {
        Some(u32::from_be_bytes([
            address[12],
            address[13],
            address[14],
            address[15],
        ]))
    } else {
        None
    }
}

pub(crate) fn extract_ipv6_bytes(address: &[u8; 16]) -> Option<[u8; 16]> {
    if is_v4_mapped(address) || address == &[0; 16] {
        None
    } else {
        Some(*address)
    }
}

pub(crate) fn stable_local_id(value: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in value.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}

pub(crate) fn stable_local_id16(value: &str) -> u16 {
    let mut hash = stable_local_id(value) as u16;
    if hash == 0 {
        hash = 1;
    }
    hash
}

pub(crate) fn ipv4_to_v4mapped_bytes(ip: [u8; 4]) -> [u8; 16] {
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, ip[0], ip[1], ip[2], ip[3],
    ]
}

pub(crate) fn is_v4_mapped(address: &[u8; 16]) -> bool {
    address[..12] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff]
}

pub(crate) fn parse_cidr_string(value: &str) -> Result<(std::net::IpAddr, u8), String> {
    let (ip_raw, prefix_raw) = value
        .split_once('/')
        .ok_or_else(|| "expected CIDR notation".to_string())?;
    let ip = ip_raw
        .parse::<std::net::IpAddr>()
        .map_err(|error| format!("invalid CIDR IP '{}': {error}", ip_raw))?;
    let prefix = prefix_raw
        .parse::<u8>()
        .map_err(|_| format!("invalid CIDR prefix '{}'", prefix_raw))?;
    match ip {
        std::net::IpAddr::V4(_) if prefix <= 32 => Ok((ip, prefix)),
        std::net::IpAddr::V6(_) if prefix <= 128 => Ok((ip, prefix)),
        std::net::IpAddr::V4(_) => Err("IPv4 prefix must be <= 32".to_string()),
        std::net::IpAddr::V6(_) => Err("IPv6 prefix must be <= 128".to_string()),
    }
}

pub(crate) fn route_next_hop_type(value: &str) -> Result<u8, String> {
    Ok(match value {
        "local" | "local_port" | "port" => aria_core::common::NEXT_HOP_LOCAL_PORT,
        "gateway" => aria_core::common::NEXT_HOP_GATEWAY,
        "host" | "node" => aria_core::common::NEXT_HOP_HOST,
        "blackhole" => aria_core::common::NEXT_HOP_BLACKHOLE,
        other => return Err(format!("unsupported next_hop_type '{}'", other)),
    })
}

pub(crate) fn next_hop_ip_for_port(port_identity: &PortIdentityIr) -> [u8; 16] {
    if port_identity.primary_ipv4 != 0 {
        ipv4_to_v4mapped_bytes(port_identity.primary_ipv4.to_be_bytes())
    } else if port_identity.primary_ipv6 != [0; 16] {
        port_identity.primary_ipv6
    } else {
        [0; 16]
    }
}

pub(crate) fn build_route_ir(
    route_table: &aria_api::RouteTableResource,
    route: &aria_api::RouteSpec,
    port_identity: &PortIdentityIr,
    port_identity_by_port_id: &BTreeMap<&str, &PortIdentityIr>,
) -> Result<RouteIr, String> {
    let (destination_ip, prefix_len) = parse_cidr_string(&route.destination)?;
    let (is_ipv6, destination) = match destination_ip {
        std::net::IpAddr::V4(ipv4) => (false, ipv4_to_v4mapped_bytes(ipv4.octets())),
        std::net::IpAddr::V6(ipv6) => (true, ipv6.octets()),
    };
    let next_hop_type = route_next_hop_type(route.next_hop_type.as_str())?;

    let (next_hop_ip, egress_ifindex) = match next_hop_type {
        aria_core::common::NEXT_HOP_LOCAL_PORT => {
            let target = port_identity_by_port_id
                .get(route.next_hop_ref.as_str())
                .ok_or_else(|| {
                    format!(
                        "route '{}' references local port '{}' that is not materializable on this node",
                        route.destination, route.next_hop_ref
                    )
                })?;
            (next_hop_ip_for_port(target), target.ifindex)
        }
        aria_core::common::NEXT_HOP_GATEWAY => {
            let next_hop_ip = route
                .next_hop_ref
                .parse::<std::net::IpAddr>()
                .ok()
                .map(|ip| match ip {
                    std::net::IpAddr::V4(ipv4) => ipv4_to_v4mapped_bytes(ipv4.octets()),
                    std::net::IpAddr::V6(ipv6) => ipv6.octets(),
                })
                .unwrap_or([0; 16]);
            (next_hop_ip, 0)
        }
        aria_core::common::NEXT_HOP_HOST | aria_core::common::NEXT_HOP_BLACKHOLE => ([0; 16], 0),
        _ => ([0; 16], 0),
    };

    Ok(RouteIr {
        route_table_id: route_table.metadata.id.clone(),
        port_id: port_identity.port_id.clone(),
        tap_id: port_identity.tap_id,
        destination,
        prefix_len,
        is_ipv6,
        next_hop_type,
        next_hop_ref: route.next_hop_ref.clone(),
        next_hop_ip,
        egress_ifindex,
        route_id: stable_local_id16(&format!(
            "{}:{}:{}:{}",
            route_table.metadata.id, port_identity.port_id, route.destination, route.next_hop_ref
        )),
        priority: route.preference.min(u8::MAX as u32) as u8,
        shadow_apply_only: true,
    })
}

pub(crate) fn build_sg_rule_ir(
    port_identity: &PortIdentityIr,
    security_group_id: &str,
    rule: &aria_api::SecurityRuleSpec,
) -> Result<SgRuleIr, String> {
    if rule.audit_mode {
        return Err("audit_mode rules are not materialized in phase-3 mode-a".to_string());
    }

    let direction = match rule.direction.as_str() {
        "ingress" => aria_core::common::SG_DIR_INGRESS,
        "egress" => aria_core::common::SG_DIR_EGRESS,
        other => return Err(format!("unsupported direction '{}'", other)),
    };
    let proto = match rule.protocol.as_deref() {
        None | Some("any") => 0,
        Some("tcp") => libc::IPPROTO_TCP as u8,
        Some("udp") => libc::IPPROTO_UDP as u8,
        Some("icmp") => libc::IPPROTO_ICMP as u8,
        Some("icmpv6") => libc::IPPROTO_ICMPV6 as u8,
        Some(other) => return Err(format!("unsupported protocol '{}'", other)),
    };
    let action = match rule.action.as_str() {
        "allow" => 1,
        "deny" => 0,
        other => return Err(format!("unsupported action '{}'", other)),
    };
    let (port_start, port_end) = parse_security_port_range(rule.port_range.as_deref())?;
    let selector = if direction == aria_core::common::SG_DIR_INGRESS {
        rule.src_selector.as_deref()
    } else {
        rule.dst_selector.as_deref()
    };
    let (remote_prefix, prefix_len) =
        parse_security_remote_selector(selector, rule.ethertype.as_str())?;

    Ok(SgRuleIr {
        port_id: port_identity.port_id.clone(),
        tap_id: port_identity.tap_id,
        sg_program_id: port_identity.sg_program_id,
        source_security_group_id: security_group_id.to_string(),
        direction,
        proto,
        remote_prefix,
        prefix_len,
        action,
        priority: rule.priority.min(u8::MAX as u32) as u8,
        port_start,
        port_end,
        rule_id: stable_local_id16(&format!(
            "{}:{}:{}:{}:{:?}:{:?}",
            security_group_id,
            port_identity.port_id,
            rule.direction,
            rule.priority,
            rule.src_selector,
            rule.dst_selector
        )),
        shadow_apply_only: true,
    })
}

pub(crate) fn parse_security_remote_selector(
    selector: Option<&str>,
    ethertype: &str,
) -> Result<([u8; 16], u8), String> {
    let Some(selector) = selector else {
        return Ok(([0; 16], 0));
    };
    if selector.is_empty() {
        return Ok(([0; 16], 0));
    }

    let (ip, prefix_len) = parse_cidr_string(selector)?;
    match ip {
        std::net::IpAddr::V4(ipv4) => {
            if ethertype == "ipv6" {
                return Err("ipv6 rule cannot use an ipv4 selector".to_string());
            }
            if prefix_len == 0 {
                Ok(([0; 16], 0))
            } else if prefix_len == 32 {
                Ok((ipv4_to_v4mapped_bytes(ipv4.octets()), 128))
            } else {
                Err("phase-3 mode-a only materializes ipv4 host selectors or /0".to_string())
            }
        }
        std::net::IpAddr::V6(ipv6) => {
            if ethertype == "ipv4" {
                return Err("ipv4 rule cannot use an ipv6 selector".to_string());
            }
            if prefix_len == 0 {
                Ok(([0; 16], 0))
            } else if prefix_len == 128 {
                Ok((ipv6.octets(), 128))
            } else {
                Err("phase-3 mode-a only materializes ipv6 host selectors or /0".to_string())
            }
        }
    }
}

pub(crate) fn parse_security_port_range(port_range: Option<&str>) -> Result<(u16, u16), String> {
    let Some(port_range) = port_range else {
        return Ok((0, 0));
    };
    let port_range = port_range.trim();
    if port_range.is_empty() {
        return Ok((0, 0));
    }
    if port_range.contains(',') || port_range.contains(':') {
        return Err("phase-3 mode-a only supports a single port or a single range".to_string());
    }
    if let Some((start_raw, end_raw)) = port_range.split_once('-') {
        let start = start_raw
            .trim()
            .parse::<u16>()
            .map_err(|_| "invalid port range start".to_string())?;
        let end = end_raw
            .trim()
            .parse::<u16>()
            .map_err(|_| "invalid port range end".to_string())?;
        if start > end {
            return Err("port range start must be <= end".to_string());
        }
        return Ok((start, end));
    }

    let port = port_range
        .parse::<u16>()
        .map_err(|_| "invalid port".to_string())?;
    Ok((port, port))
}

pub(crate) fn build_service_programs(
    desired: &DesiredStateEnvelope,
    compiled_health_checks: &[CompiledHealthCheckView],
    compiled_backend_sets: &[CompiledBackendSetView],
    compiled_services: &[CompiledServiceView],
    node_id: &str,
) -> Vec<ServiceProgramIr> {
    let network_by_id = desired
        .networks
        .iter()
        .map(|network| (network.metadata.id.as_str(), network))
        .collect::<BTreeMap<_, _>>();
    let health_check_by_id = desired
        .health_checks
        .iter()
        .map(|health_check| (health_check.metadata.id.as_str(), health_check))
        .collect::<BTreeMap<_, _>>();
    let backend_set_by_id = desired
        .backend_sets
        .iter()
        .map(|backend_set| (backend_set.metadata.id.as_str(), backend_set))
        .collect::<BTreeMap<_, _>>();
    let service_by_id = desired
        .services
        .iter()
        .map(|service| (service.metadata.id.as_str(), service))
        .collect::<BTreeMap<_, _>>();
    let port_by_id = desired
        .ports
        .iter()
        .map(|port| (port.metadata.id.as_str(), port))
        .collect::<BTreeMap<_, _>>();
    let compiled_backend_set_by_id = compiled_backend_sets
        .iter()
        .map(|backend_set| (backend_set.backend_set_id.as_str(), backend_set))
        .collect::<BTreeMap<_, _>>();
    let compiled_health_check_ids = compiled_health_checks
        .iter()
        .map(|health_check| health_check.health_check_id.as_str())
        .collect::<BTreeSet<_>>();

    compiled_services
        .iter()
        .filter_map(|compiled_service| {
            let service = service_by_id.get(compiled_service.service_id.as_str())?;
            let backend_set_ir =
                compiled_service
                    .backend_set_id
                    .as_deref()
                    .and_then(|backend_set_id| {
                        let backend_set = backend_set_by_id.get(backend_set_id)?;
                        let compiled_backend_set =
                            compiled_backend_set_by_id.get(backend_set_id)?;

                        let backends = backend_set
                            .spec
                            .backends
                            .iter()
                            .map(|backend| build_backend_member_ir(backend, &port_by_id, node_id))
                            .collect::<Vec<_>>();

                        Some(BackendSetIr {
                            backend_set_id: backend_set.metadata.id.clone(),
                            tenant_id: backend_set.spec.tenant_id.clone(),
                            network_id: backend_set.spec.network_id.clone(),
                            selection_policy: backend_set.spec.policy.clone(),
                            health_check_id: backend_set.spec.health_check_id.clone(),
                            local_backend_count: compiled_backend_set.local_backend_count,
                            remote_backend_count: compiled_backend_set.remote_backend_count,
                            backends,
                            shadow_apply_only: true,
                        })
                    });

            let health_check_ir = backend_set_ir
                .as_ref()
                .and_then(|backend_set| backend_set.health_check_id.as_deref())
                .and_then(|health_check_id| {
                    let health_check = health_check_by_id.get(health_check_id)?;
                    if !compiled_health_check_ids.contains(health_check_id) {
                        return None;
                    }
                    Some(HealthCheckIr {
                        health_check_id: health_check.metadata.id.clone(),
                        tenant_id: health_check.spec.tenant_id.clone(),
                        network_id: health_check.spec.network_id.clone(),
                        probe_protocol: health_check.spec.protocol.clone(),
                        interval_seconds: health_check.spec.interval_seconds,
                        timeout_seconds: health_check.spec.timeout_seconds,
                        healthy_threshold: health_check.spec.healthy_threshold,
                        unhealthy_threshold: health_check.spec.unhealthy_threshold,
                        target_port: health_check.spec.target_port,
                        // Keep the shadow IR bounded; the full probe payload can stay in desired state.
                        has_request_template: health_check.spec.request_template.is_some(),
                        shadow_apply_only: true,
                    })
                });

            let node_local_forwarding = backend_set_ir
                .as_ref()
                .map(|backend_set| backend_set.local_backend_count > 0)
                .unwrap_or(false);
            let cross_node_forwarding = backend_set_ir
                .as_ref()
                .map(|backend_set| backend_set.remote_backend_count > 0)
                .unwrap_or(false);
            let route_mode = network_by_id
                .get(compiled_service.network_id.as_str())
                .map(|network| network.spec.route_mode.clone())
                .unwrap_or_else(default_service_route_mode);
            let forwarding_mode =
                derive_service_forwarding_mode(&route_mode, cross_node_forwarding);

            Some(ServiceProgramIr {
                service_id: compiled_service.service_id.clone(),
                backend_set_id: compiled_service.backend_set_id.clone(),
                health_check_id: health_check_ir
                    .as_ref()
                    .map(|health_check| health_check.health_check_id.clone()),
                frontend: ServiceFrontendIr {
                    service_id: compiled_service.service_id.clone(),
                    tenant_id: compiled_service.tenant_id.clone(),
                    network_id: compiled_service.network_id.clone(),
                    route_mode,
                    vip: compiled_service.vip.clone(),
                    protocol: compiled_service.protocol.clone(),
                    lb_policy: service.spec.lb_policy.clone(),
                    session_affinity: service.spec.session_affinity.clone(),
                    exposure_type: compiled_service.exposure_type.clone(),
                    forwarding_mode,
                    listener_ports: service
                        .spec
                        .ports
                        .iter()
                        .map(|port| ServiceFrontendPortIr {
                            name: port.name.clone(),
                            service_port: port.port,
                            target_port: port.target_port,
                        })
                        .collect(),
                    node_local_forwarding,
                    cross_node_forwarding,
                    shadow_apply_only: true,
                },
                backend_set: backend_set_ir,
                health_check: health_check_ir,
                shadow_apply_only: true,
            })
        })
        .collect()
}

pub(crate) fn build_backend_member_ir(
    backend: &aria_api::BackendTargetSpec,
    port_by_id: &BTreeMap<&str, &aria_api::PortResource>,
    node_id: &str,
) -> BackendMemberIr {
    let mut resolved_ip_hint = backend.ip.clone();
    let mut resolved_locality = backend
        .locality
        .clone()
        .unwrap_or_else(|| "remote".to_string());
    let mut resolution = if backend.target_ref.is_some() {
        "shadow_remote_reference".to_string()
    } else if backend.ip.is_some() {
        "direct_ip".to_string()
    } else {
        "literal_target".to_string()
    };
    let mut resolved_node_id = backend.node_id.clone();

    if let Some(target_ref) = backend.target_ref.as_deref() {
        if let Some(port) = port_by_id.get(target_ref) {
            let bound_node_id = port.spec.node_id.as_deref().unwrap_or(node_id);
            resolved_locality = if bound_node_id == node_id {
                "local".to_string()
            } else {
                "remote".to_string()
            };
            resolved_node_id = port
                .spec
                .node_id
                .clone()
                .or_else(|| backend.node_id.clone());
            resolved_ip_hint = resolved_ip_hint
                .or_else(|| port.spec.fixed_ips.first().map(|ip| strip_cidr_suffix(ip)));
            resolution = "resolved_from_port_ref".to_string();
        }
    } else if backend.node_id.as_deref() == Some(node_id) {
        resolved_locality = "local".to_string();
    }

    let forwarding_scope = if resolved_locality == "local" {
        "node_local".to_string()
    } else {
        "cross_node".to_string()
    };

    BackendMemberIr {
        backend_id: backend.id.clone(),
        target_type: backend.target_type.clone(),
        target_ref: backend.target_ref.clone(),
        resolved_ip_hint,
        service_port: backend.port,
        weight: backend.weight,
        admin_state: backend
            .admin_state
            .clone()
            .unwrap_or_else(|| "enabled".to_string()),
        node_id: resolved_node_id,
        declared_locality: backend.locality.clone(),
        resolved_locality,
        forwarding_scope,
        resolution,
        shadow_apply_only: true,
    }
}

pub(crate) fn strip_cidr_suffix(value: &str) -> String {
    value.split('/').next().unwrap_or(value).to_string()
}

pub(crate) fn default_service_route_mode() -> String {
    "native".to_string()
}

pub(crate) fn default_service_forwarding_mode() -> String {
    "node_local_only".to_string()
}

pub(crate) fn derive_service_forwarding_mode(route_mode: &str, cross_node_forwarding: bool) -> String {
    if !cross_node_forwarding {
        return default_service_forwarding_mode();
    }

    match route_mode {
        "overlay" => "cross_node_overlay".to_string(),
        "hybrid" => "cross_node_hybrid".to_string(),
        _ => "cross_node_native".to_string(),
    }
}

