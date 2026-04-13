use std::collections::{BTreeMap, BTreeSet};

use aria_api::{
    ApplyDomainStatus, ApplyObjectFailure, ApplyStatusReport, ApplyStatusResponse,
    NodeCapability, PlatformApiError,
};
use tracing::{debug, info, warn};

use super::helpers::{self, capability_profile, hostname, unix_timestamp_string};
use super::ir_builders::*;
use super::ir_types::*;

pub(crate) fn build_node_capability(config: &PlatformAgentConfig) -> NodeCapability {
    let mut supported_hooks = vec!["xdp".to_string(), "tc".to_string()];
    supported_hooks.sort();
    supported_hooks.dedup();

    let mut limits = BTreeMap::new();
    limits.insert(
        "max_port_policies".to_string(),
        config.max_port_policies as u64,
    );

    NodeCapability {
        supported_hooks,
        supports_xdp: true,
        supports_tc: true,
        supports_socket_lb: false,
        supports_trace_ringbuf: config.trace_backend == "ringbuf",
        supports_nat: false,
        supports_lb: false,
        supports_encap: false,
        supports_qos_shaping: true,
        limits,
        observability_profile: Some(if config.trace_backend == "ringbuf" {
            "full".to_string()
        } else {
            "standard".to_string()
        }),
    }
}

pub(crate) fn compile_desired_state(context: CompilerContext<'_>) -> CompileOutcome {
    let tenant_ids = context
        .desired
        .tenants
        .iter()
        .map(|tenant| tenant.metadata.id.clone())
        .collect::<BTreeSet<_>>();
    let network_by_id = context
        .desired
        .networks
        .iter()
        .map(|network| (network.metadata.id.clone(), network))
        .collect::<BTreeMap<_, _>>();
    let security_group_by_id = context
        .desired
        .security_groups
        .iter()
        .map(|security_group| (security_group.metadata.id.clone(), security_group))
        .collect::<BTreeMap<_, _>>();
    let health_check_by_id = context
        .desired
        .health_checks
        .iter()
        .map(|health_check| (health_check.metadata.id.clone(), health_check))
        .collect::<BTreeMap<_, _>>();
    let port_by_id = context
        .desired
        .ports
        .iter()
        .map(|port| (port.metadata.id.clone(), port))
        .collect::<BTreeMap<_, _>>();

    let mut failed_objects = Vec::new();
    let mut warnings = Vec::new();
    let mut port_bindings = Vec::new();
    let mut port_identities = Vec::new();
    let mut used_tap_ids = BTreeMap::new();
    let mut used_ifindices = BTreeMap::new();

    for port in &context.desired.ports {
        if let Some(bound_node_id) = port.spec.node_id.as_deref() {
            if bound_node_id != context.node_id {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "port".to_string(),
                    id: port.metadata.id.clone(),
                    reason: format!(
                        "port bound to node '{}' instead of '{}'",
                        bound_node_id, context.node_id
                    ),
                });
                continue;
            }
        } else {
            warnings.push(format!(
                "port '{}' has no explicit node binding; treating it as node-local shadow state",
                port.metadata.id
            ));
        }

        if !tenant_ids.is_empty() && !tenant_ids.contains(&port.spec.tenant_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "port".to_string(),
                id: port.metadata.id.clone(),
                reason: format!(
                    "missing tenant '{}' in desired envelope",
                    port.spec.tenant_id
                ),
            });
            continue;
        }

        if !network_by_id.contains_key(&port.spec.network_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "port".to_string(),
                id: port.metadata.id.clone(),
                reason: format!(
                    "missing network '{}' in desired envelope",
                    port.spec.network_id
                ),
            });
            continue;
        }

        let mut missing_security_group = None;
        for security_group_id in &port.spec.security_group_ids {
            if !security_group_by_id.contains_key(security_group_id) {
                missing_security_group = Some(security_group_id.clone());
                break;
            }
        }
        if let Some(security_group_id) = missing_security_group {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "port".to_string(),
                id: port.metadata.id.clone(),
                reason: format!(
                    "missing security group '{}' in desired envelope",
                    security_group_id
                ),
            });
            continue;
        }

        port_bindings.push(CompiledPortBinding {
            port_id: port.metadata.id.clone(),
            tenant_id: port.spec.tenant_id.clone(),
            network_id: port.spec.network_id.clone(),
            segment_id: port.spec.segment_id.clone(),
            security_group_ids: port.spec.security_group_ids.clone(),
            fixed_ips: port.spec.fixed_ips.clone(),
            allowed_address_pairs: port.spec.allowed_address_pairs.clone(),
            mac_address: port.spec.mac_address.clone(),
            anti_spoof_enabled: port.spec.anti_spoof_enabled,
            admin_state_up: port.spec.admin_state_up,
        });

        let tap_id = match required_runtime_label(port, "runtime.tap_id") {
            Ok(value) => value,
            Err(reason) => {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "port".to_string(),
                    id: port.metadata.id.clone(),
                    reason,
                });
                continue;
            }
        };
        let ifindex = match required_runtime_label(port, "runtime.ifindex") {
            Ok(value) => value,
            Err(reason) => {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "port".to_string(),
                    id: port.metadata.id.clone(),
                    reason,
                });
                continue;
            }
        };
        if let Some(existing_port_id) = used_tap_ids.insert(tap_id, port.metadata.id.clone()) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "port".to_string(),
                id: port.metadata.id.clone(),
                reason: format!(
                    "runtime.tap_id '{}' is already claimed by port '{}'",
                    tap_id, existing_port_id
                ),
            });
            continue;
        }
        if let Some(existing_port_id) = used_ifindices.insert(ifindex, port.metadata.id.clone()) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "port".to_string(),
                id: port.metadata.id.clone(),
                reason: format!(
                    "runtime.ifindex '{}' is already claimed by port '{}'",
                    ifindex, existing_port_id
                ),
            });
            continue;
        }
        if !port.spec.admin_state_up {
            warnings.push(format!(
                "port '{}' is administratively down; skipping phase-3 map materialization until it is enabled",
                port.metadata.id
            ));
            continue;
        }

        let mac = match parse_mac_address(&port.spec.mac_address) {
            Ok(value) => value,
            Err(reason) => {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "port".to_string(),
                    id: port.metadata.id.clone(),
                    reason,
                });
                continue;
            }
        };

        let mut anti_spoof_entries = Vec::new();
        let mut primary_ipv4 = 0u32;
        let mut primary_ipv6 = [0u8; 16];
        for fixed_ip in &port.spec.fixed_ips {
            match parse_allowed_ip(fixed_ip) {
                Ok(address) => {
                    if primary_ipv4 == 0 {
                        if let Some(ipv4) = extract_ipv4_u32(&address) {
                            primary_ipv4 = ipv4;
                        }
                    }
                    if primary_ipv6 == [0; 16] {
                        if let Some(ipv6) = extract_ipv6_bytes(&address) {
                            primary_ipv6 = ipv6;
                        }
                    }
                    anti_spoof_entries.push(AntiSpoofIr { address, flags: 1 });
                }
                Err(reason) => {
                    failed_objects.push(ApplyObjectFailure {
                        resource_kind: "port".to_string(),
                        id: port.metadata.id.clone(),
                        reason: format!("invalid fixed_ip '{}': {}", fixed_ip, reason),
                    });
                    anti_spoof_entries.clear();
                    break;
                }
            }
        }
        if anti_spoof_entries.is_empty() && !port.spec.fixed_ips.is_empty() {
            continue;
        }
        for allowed_pair in &port.spec.allowed_address_pairs {
            match parse_allowed_ip(allowed_pair) {
                Ok(address) => anti_spoof_entries.push(AntiSpoofIr { address, flags: 2 }),
                Err(reason) => {
                    failed_objects.push(ApplyObjectFailure {
                        resource_kind: "port".to_string(),
                        id: port.metadata.id.clone(),
                        reason: format!(
                            "invalid allowed_address_pair '{}': {}",
                            allowed_pair, reason
                        ),
                    });
                    anti_spoof_entries.clear();
                    break;
                }
            }
        }
        if anti_spoof_entries.is_empty()
            && (!port.spec.fixed_ips.is_empty() || !port.spec.allowed_address_pairs.is_empty())
        {
            continue;
        }

        port_identities.push(PortIdentityIr {
            port_id: port.metadata.id.clone(),
            network_id: port.spec.network_id.clone(),
            tap_id,
            ifindex,
            sg_program_id: tap_id,
            tenant_local_id: stable_local_id(&port.spec.tenant_id),
            network_local_id: stable_local_id(&port.spec.network_id),
            segment_local_id: port
                .spec
                .segment_id
                .as_deref()
                .map(stable_local_id)
                .unwrap_or(0),
            mac,
            primary_ipv4,
            primary_ipv6,
            anti_spoof_enabled: port.spec.anti_spoof_enabled,
            anti_spoof_entries,
            shadow_apply_only: true,
        });
    }

    let mut compiled_route_tables = Vec::new();
    for route_table in &context.desired.route_tables {
        if !network_by_id.contains_key(&route_table.spec.network_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "route_table".to_string(),
                id: route_table.metadata.id.clone(),
                reason: format!(
                    "missing network '{}' in desired envelope",
                    route_table.spec.network_id
                ),
            });
            continue;
        }

        compiled_route_tables.push(CompiledRouteTableView {
            route_table_id: route_table.metadata.id.clone(),
            network_id: route_table.spec.network_id.clone(),
            route_count: route_table.spec.routes.len(),
            default_route: route_table.spec.default_route.clone(),
        });
    }

    let mut compiled_health_checks = Vec::new();
    for health_check in &context.desired.health_checks {
        if !tenant_ids.is_empty() && !tenant_ids.contains(&health_check.spec.tenant_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "health_check".to_string(),
                id: health_check.metadata.id.clone(),
                reason: format!(
                    "missing tenant '{}' in desired envelope",
                    health_check.spec.tenant_id
                ),
            });
            continue;
        }

        if let Some(network_id) = health_check.spec.network_id.as_deref() {
            if !network_by_id.contains_key(network_id) {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "health_check".to_string(),
                    id: health_check.metadata.id.clone(),
                    reason: format!("missing network '{}' in desired envelope", network_id),
                });
                continue;
            }
        }

        compiled_health_checks.push(CompiledHealthCheckView {
            health_check_id: health_check.metadata.id.clone(),
            tenant_id: health_check.spec.tenant_id.clone(),
            network_id: health_check.spec.network_id.clone(),
            protocol: health_check.spec.protocol.clone(),
            target_port: health_check.spec.target_port,
        });
    }

    let compiled_health_check_ids = compiled_health_checks
        .iter()
        .map(|health_check| health_check.health_check_id.clone())
        .collect::<BTreeSet<_>>();

    let mut compiled_backend_sets = Vec::new();
    for backend_set in &context.desired.backend_sets {
        if !tenant_ids.is_empty() && !tenant_ids.contains(&backend_set.spec.tenant_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "backend_set".to_string(),
                id: backend_set.metadata.id.clone(),
                reason: format!(
                    "missing tenant '{}' in desired envelope",
                    backend_set.spec.tenant_id
                ),
            });
            continue;
        }

        if !network_by_id.contains_key(&backend_set.spec.network_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "backend_set".to_string(),
                id: backend_set.metadata.id.clone(),
                reason: format!(
                    "missing network '{}' in desired envelope",
                    backend_set.spec.network_id
                ),
            });
            continue;
        }

        if let Some(health_check_id) = backend_set.spec.health_check_id.as_deref() {
            if !compiled_health_check_ids.contains(health_check_id)
                && !health_check_by_id.contains_key(health_check_id)
            {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "backend_set".to_string(),
                    id: backend_set.metadata.id.clone(),
                    reason: format!(
                        "missing health_check '{}' in desired envelope",
                        health_check_id
                    ),
                });
                continue;
            }
        }

        let mut local_backend_count = 0usize;
        let mut remote_backend_count = 0usize;
        for backend in &backend_set.spec.backends {
            if let Some(target_ref) = backend.target_ref.as_deref() {
                if let Some(port) = port_by_id.get(target_ref) {
                    let bound_node = port.spec.node_id.as_deref().unwrap_or(context.node_id);
                    if bound_node == context.node_id {
                        local_backend_count += 1;
                    } else {
                        remote_backend_count += 1;
                    }
                } else if backend.node_id.as_deref() == Some(context.node_id)
                    || backend.locality.as_deref() == Some("local")
                {
                    failed_objects.push(ApplyObjectFailure {
                        resource_kind: "backend_set".to_string(),
                        id: backend_set.metadata.id.clone(),
                        reason: format!(
                            "backend '{}' references local port '{}' that is missing from desired envelope",
                            backend.id, target_ref
                        ),
                    });
                    local_backend_count = 0;
                    remote_backend_count = 0;
                    break;
                } else {
                    warnings.push(format!(
                        "backend_set '{}' backend '{}' references remote port '{}' outside node-local desired envelope; treating as remote shadow backend",
                        backend_set.metadata.id, backend.id, target_ref
                    ));
                    remote_backend_count += 1;
                }
            } else if backend.locality.as_deref() == Some("local") {
                local_backend_count += 1;
            } else {
                remote_backend_count += 1;
            }
        }

        if failed_objects.iter().any(|failure| {
            failure.resource_kind == "backend_set" && failure.id == backend_set.metadata.id
        }) {
            continue;
        }

        compiled_backend_sets.push(CompiledBackendSetView {
            backend_set_id: backend_set.metadata.id.clone(),
            tenant_id: backend_set.spec.tenant_id.clone(),
            network_id: backend_set.spec.network_id.clone(),
            health_check_id: backend_set.spec.health_check_id.clone(),
            policy: backend_set.spec.policy.clone(),
            backend_count: backend_set.spec.backends.len(),
            local_backend_count,
            remote_backend_count,
        });
    }

    let compiled_backend_set_ids = compiled_backend_sets
        .iter()
        .map(|backend_set| backend_set.backend_set_id.clone())
        .collect::<BTreeSet<_>>();

    let mut compiled_services = Vec::new();
    for service in &context.desired.services {
        if !tenant_ids.is_empty() && !tenant_ids.contains(&service.spec.tenant_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "service".to_string(),
                id: service.metadata.id.clone(),
                reason: format!(
                    "missing tenant '{}' in desired envelope",
                    service.spec.tenant_id
                ),
            });
            continue;
        }

        if !network_by_id.contains_key(&service.spec.network_id) {
            failed_objects.push(ApplyObjectFailure {
                resource_kind: "service".to_string(),
                id: service.metadata.id.clone(),
                reason: format!(
                    "missing network '{}' in desired envelope",
                    service.spec.network_id
                ),
            });
            continue;
        }

        if let Some(backend_set_id) = service.spec.backend_set_id.as_deref() {
            if !compiled_backend_set_ids.contains(backend_set_id) {
                failed_objects.push(ApplyObjectFailure {
                    resource_kind: "service".to_string(),
                    id: service.metadata.id.clone(),
                    reason: format!(
                        "missing backend_set '{}' in desired envelope",
                        backend_set_id
                    ),
                });
                continue;
            }
        }

        compiled_services.push(CompiledServiceView {
            service_id: service.metadata.id.clone(),
            tenant_id: service.spec.tenant_id.clone(),
            network_id: service.spec.network_id.clone(),
            backend_set_id: service.spec.backend_set_id.clone(),
            vip: service.spec.vip.clone(),
            protocol: service.spec.protocol.clone(),
            port_count: service.spec.ports.len(),
            exposure_type: service.spec.exposure_type.clone(),
        });
    }

    let port_identity_by_port_id = port_identities
        .iter()
        .map(|port| (port.port_id.as_str(), port))
        .collect::<BTreeMap<_, _>>();

    let mut route_entries = Vec::new();
    for route_table in &context.desired.route_tables {
        if !network_by_id.contains_key(&route_table.spec.network_id) {
            continue;
        }

        if route_table.spec.default_route.is_some() {
            warnings.push(format!(
                "route_table '{}' default_route is not materialized yet; use explicit routes during phase-3 mode-a rollout",
                route_table.metadata.id
            ));
        }

        for port_identity in port_identities
            .iter()
            .filter(|port| port.network_id == route_table.spec.network_id)
        {
            for route in &route_table.spec.routes {
                match build_route_ir(route_table, route, port_identity, &port_identity_by_port_id) {
                    Ok(route_ir) => route_entries.push(route_ir),
                    Err(reason) => failed_objects.push(ApplyObjectFailure {
                        resource_kind: "route_table".to_string(),
                        id: route_table.metadata.id.clone(),
                        reason,
                    }),
                }
            }
        }
    }

    let mut sg_rule_entries = BTreeMap::new();
    let mut failed_security_groups = BTreeSet::new();
    for port in &port_bindings {
        let Some(port_identity) = port_identity_by_port_id.get(port.port_id.as_str()) else {
            continue;
        };

        let mut attached_groups = port
            .security_group_ids
            .iter()
            .filter_map(|security_group_id| security_group_by_id.get(security_group_id))
            .collect::<Vec<_>>();
        attached_groups.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

        for security_group in attached_groups {
            let mut rules = security_group.spec.rules.iter().collect::<Vec<_>>();
            rules.sort_by_key(|rule| rule.priority);

            for rule in rules {
                match build_sg_rule_ir(port_identity, security_group.metadata.id.as_str(), rule) {
                    Ok(rule_ir) => {
                        let rule_key = (
                            rule_ir.tap_id,
                            rule_ir.sg_program_id,
                            rule_ir.direction,
                            rule_ir.proto,
                            rule_ir.remote_prefix,
                            rule_ir.prefix_len,
                            rule_ir.port_start,
                            rule_ir.port_end,
                        );
                        sg_rule_entries.entry(rule_key).or_insert(rule_ir);
                    }
                    Err(reason) => {
                        if failed_security_groups.insert(security_group.metadata.id.clone()) {
                            failed_objects.push(ApplyObjectFailure {
                                resource_kind: "security_group".to_string(),
                                id: security_group.metadata.id.clone(),
                                reason,
                            });
                        }
                    }
                }
            }
        }
    }
    let sg_rules = sg_rule_entries.into_values().collect::<Vec<_>>();

    let service_programs = build_service_programs(
        context.desired,
        &compiled_health_checks,
        &compiled_backend_sets,
        &compiled_services,
        context.node_id,
    );
    let overlay_encap_unsupported = !context.capability.supports_encap
        && service_programs
            .iter()
            .any(|program| program.frontend.forwarding_mode == "cross_node_overlay");
    if overlay_encap_unsupported {
        warnings.push(
            "overlay cross-node service forwarding requested but node capability supports_encap=false; keeping shadow plan but marking services degraded"
                .to_string(),
        );
    }

    if context.desired.deletes.is_empty() {
        debug!(
            generation = %context.desired.generation,
            "southbound desired-state contains no explicit delete refs"
        );
    }

    let mut compiled_objects = BTreeMap::new();
    compiled_objects.insert("tenants".to_string(), context.desired.tenants.len());
    compiled_objects.insert("networks".to_string(), context.desired.networks.len());
    compiled_objects.insert("ports".to_string(), port_bindings.len());
    compiled_objects.insert(
        "security_groups".to_string(),
        context.desired.security_groups.len(),
    );
    compiled_objects.insert("route_tables".to_string(), compiled_route_tables.len());
    compiled_objects.insert("health_checks".to_string(), compiled_health_checks.len());
    compiled_objects.insert("backend_sets".to_string(), compiled_backend_sets.len());
    compiled_objects.insert("services".to_string(), compiled_services.len());
    if !context.desired.deletes.is_empty() {
        compiled_objects.insert("deletes".to_string(), context.desired.deletes.len());
    }

    let mut degraded_reasons = vec!["shadow_apply_only".to_string()];
    if !failed_objects.is_empty() {
        degraded_reasons.push("object_validation_failed".to_string());
    }
    if overlay_encap_unsupported {
        degraded_reasons.push("overlay_encap_unsupported".to_string());
    }
    degraded_reasons.sort();
    degraded_reasons.dedup();

    let port_failure_count = failed_objects
        .iter()
        .filter(|failure| failure.resource_kind == "port")
        .count();
    let route_failure_count = failed_objects
        .iter()
        .filter(|failure| failure.resource_kind == "route_table")
        .count();
    let security_failure_count = failed_objects
        .iter()
        .filter(|failure| failure.resource_kind == "security_group")
        .count();
    let service_failure_count = failed_objects
        .iter()
        .filter(|failure| {
            matches!(
                failure.resource_kind.as_str(),
                "health_check" | "backend_set" | "service"
            )
        })
        .count();

    // --- IpGroup compilation ---
    let mut ip_groups = Vec::new();
    let network_id_set: BTreeSet<&str> = network_by_id.keys().map(|s| s.as_str()).collect();
    for ip_group in &context.desired.ip_groups {
        if !network_id_set.contains(ip_group.spec.network_id.as_str()) {
            continue;
        }
        let numeric_id = stable_local_id(&ip_group.metadata.id);
        let mut cidrs = Vec::new();
        for cidr_str in &ip_group.spec.cidrs {
            match aria_core::ebpf_ops::parse_cidr(cidr_str) {
                Ok((ip, prefix_len)) => {
                    let is_ipv6 = matches!(ip, std::net::IpAddr::V6(_));
                    let mut address = [0u8; 16];
                    match ip {
                        std::net::IpAddr::V4(v4) => {
                            let mapped = ipv4_to_v4mapped_bytes(v4.octets());
                            address.copy_from_slice(&mapped);
                        }
                        std::net::IpAddr::V6(v6) => {
                            address.copy_from_slice(&v6.octets());
                        }
                    }
                    cidrs.push(IpGroupCidrIr {
                        cidr: cidr_str.clone(),
                        is_ipv6,
                        address,
                        prefix_len,
                    });
                }
                Err(e) => {
                    warnings.push(format!(
                        "ip_group '{}': invalid CIDR '{}': {}",
                        ip_group.metadata.id, cidr_str, e
                    ));
                }
            }
        }
        ip_groups.push(IpGroupIr {
            ip_group_id: ip_group.metadata.id.clone(),
            numeric_id,
            network_id: ip_group.spec.network_id.clone(),
            cidrs,
            shadow_apply_only: true,
        });
    }

    // --- NetworkPolicy compilation ---
    let mut network_policies = Vec::new();
    for np in &context.desired.network_policies {
        if !network_id_set.contains(np.spec.network_id.as_str()) {
            continue;
        }
        let rules: Vec<NetworkPolicyRuleIr> = np
            .spec
            .rules
            .iter()
            .map(|rule| NetworkPolicyRuleIr {
                src_numeric_id: stable_local_id(&rule.src_ip_group_id),
                dst_numeric_id: stable_local_id(&rule.dst_ip_group_id),
                proto: rule.proto,
                direction: rule.direction,
                action: rule.action,
                ports: rule.ports.clone(),
            })
            .collect();
        network_policies.push(NetworkPolicyIr {
            policy_id: np.metadata.id.clone(),
            network_id: np.spec.network_id.clone(),
            rules,
            shadow_apply_only: true,
        });
    }

    // --- QosPolicy compilation ---
    let mut qos_policies = Vec::new();
    for qp in &context.desired.qos_policies {
        if !network_id_set.contains(qp.spec.network_id.as_str()) {
            continue;
        }
        let rules: Vec<QosPolicyRuleIr> = qp
            .spec
            .rules
            .iter()
            .map(|rule| QosPolicyRuleIr {
                ip_group_numeric_id: stable_local_id(&rule.ip_group_id),
                direction: rule.direction,
                rate_bps: rule.rate_bps,
                burst_bytes: if rule.burst_bytes > 0 {
                    rule.burst_bytes
                } else {
                    aria_core::qos_ops::compute_default_burst(rule.rate_bps)
                },
                priority: rule.priority,
                mode: rule.mode,
            })
            .collect();
        qos_policies.push(QosPolicyIr {
            policy_id: qp.metadata.id.clone(),
            network_id: qp.spec.network_id.clone(),
            rules,
            shadow_apply_only: true,
        });
    }

    compiled_objects.insert("qos_policies".to_string(), qos_policies.len());

    let domain_summaries = vec![
        CompileDomainSummary {
            domain: "identity".to_string(),
            input_objects: context.desired.tenants.len()
                + context.desired.networks.len()
                + context.desired.ip_groups.len(),
            compiled_objects: tenant_ids.len() + network_by_id.len() + ip_groups.len(),
            failed_objects: 0,
            status: "shadow_ready".to_string(),
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "ports".to_string(),
            input_objects: context.desired.ports.len(),
            compiled_objects: port_identities.len(),
            failed_objects: port_failure_count,
            status: if port_failure_count == 0 {
                "shadow_ready".to_string()
            } else {
                "shadow_degraded".to_string()
            },
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "security".to_string(),
            input_objects: context.desired.security_groups.len()
                + context.desired.network_policies.len(),
            compiled_objects: sg_rules.len() + network_policies.len(),
            failed_objects: security_failure_count,
            status: if security_failure_count == 0 {
                "shadow_ready".to_string()
            } else {
                "shadow_degraded".to_string()
            },
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "routes".to_string(),
            input_objects: context.desired.route_tables.len(),
            compiled_objects: route_entries.len(),
            failed_objects: route_failure_count,
            status: if route_failure_count == 0 {
                "shadow_ready".to_string()
            } else {
                "shadow_degraded".to_string()
            },
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "services".to_string(),
            input_objects: context.desired.health_checks.len()
                + context.desired.backend_sets.len()
                + context.desired.services.len(),
            compiled_objects: compiled_health_checks.len()
                + compiled_backend_sets.len()
                + compiled_services.len(),
            failed_objects: service_failure_count,
            status: if service_failure_count == 0 && !overlay_encap_unsupported {
                "shadow_ready".to_string()
            } else {
                "shadow_degraded".to_string()
            },
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "nat".to_string(),
            input_objects: 0,
            compiled_objects: 0,
            failed_objects: 0,
            status: "shadow_reserved".to_string(),
            shadow_apply_only: true,
        },
        CompileDomainSummary {
            domain: "qos".to_string(),
            input_objects: context.desired.qos_policies.len(),
            compiled_objects: qos_policies.len(),
            failed_objects: 0,
            status: if qos_policies.is_empty() {
                "shadow_reserved".to_string()
            } else {
                "shadow_ready".to_string()
            },
            shadow_apply_only: true,
        },
    ];

    let compiled_at = unix_timestamp_string();
    let compiled_state = CompiledNodeState {
        generation: context.desired.generation.clone(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        node_id: context.node_id.to_string(),
        capability_profile: capability_profile(context.capability),
        full_sync: context.desired.full_sync,
        issued_at: context.desired.issued_at.clone(),
        tenant_ids: tenant_ids.into_iter().collect(),
        network_ids: network_by_id.keys().cloned().collect(),
        security_group_ids: security_group_by_id.keys().cloned().collect(),
        port_bindings,
        port_identities,
        route_tables: compiled_route_tables,
        route_entries,
        sg_rules,
        ip_groups,
        network_policies,
        qos_policies,
        health_checks: compiled_health_checks,
        backend_sets: compiled_backend_sets,
        services: compiled_services,
        service_programs,
        domain_summaries,
        warnings: warnings.clone(),
        degraded_reasons: degraded_reasons.clone(),
        compiled_at: compiled_at.clone(),
        shadow_apply_only: true,
    };
    let reconcile_plan = build_reconcile_plan(
        context.previous_compiled_state,
        &compiled_state,
        warnings.clone(),
    );
    let runtime_plan = build_runtime_plan(
        context.previous_compiled_state,
        &compiled_state,
        context.capability,
    );
    let runtime_inventory = build_runtime_inventory(
        context.previous_compiled_state,
        &compiled_state,
        &runtime_plan,
    );
    let runtime_inventory_diff =
        build_runtime_inventory_diff(context.previous_runtime_inventory, &runtime_inventory);
    let runtime_intent = build_runtime_intent(
        &compiled_state,
        &reconcile_plan,
        &runtime_inventory,
        &runtime_inventory_diff,
    );
    let runtime_execution_summary =
        build_runtime_execution_summary(&compiled_state, &reconcile_plan, &runtime_intent);
    let socket_selection_plan =
        build_socket_selection_plan(context.previous_compiled_state, &compiled_state);
    let domain_statuses = runtime_execution_summary
        .domain_summaries
        .iter()
        .map(|summary| ApplyDomainStatus {
            domain: summary.domain.clone(),
            input_objects: summary.input_objects,
            compiled_objects: summary.compiled_objects,
            failed_objects: summary.failed_objects,
            status: summary.execution_status.clone(),
            shadow_apply_only: summary.shadow_apply_only,
        })
        .collect();

    let status = if failed_objects.is_empty() {
        "partial".to_string()
    } else if compiled_objects.values().copied().sum::<usize>() > failed_objects.len() {
        "partial".to_string()
    } else {
        "failed".to_string()
    };

    CompileOutcome {
        compiled_state,
        reconcile_plan,
        runtime_plan,
        runtime_inventory,
        runtime_inventory_diff,
        runtime_intent,
        runtime_execution_summary,
        socket_selection_plan,
        apply_report: ApplyStatusReport {
            generation: context.desired.generation.clone(),
            status,
            applied_at: compiled_at,
            compiled_objects,
            domain_statuses,
            failed_objects,
            warnings,
            degraded_reasons,
        },
    }
}

