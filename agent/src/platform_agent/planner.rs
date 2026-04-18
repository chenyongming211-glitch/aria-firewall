use std::collections::{BTreeMap, BTreeSet};

use aria_api::NodeCapability;

use super::helpers::*;
use super::ir_types::*;

pub(crate) fn build_reconcile_plan(
    previous_state: Option<&CompiledNodeState>,
    next_state: &CompiledNodeState,
    warnings: Vec<String>,
) -> ReconcilePlan {
    let previous_port_ids = previous_state
        .map(|state| {
            state
                .port_bindings
                .iter()
                .map(|binding| binding.port_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_port_ids = next_state
        .port_bindings
        .iter()
        .map(|binding| binding.port_id.as_str())
        .collect::<BTreeSet<_>>();

    let previous_route_table_ids = previous_state
        .map(|state| {
            state
                .route_tables
                .iter()
                .map(|route_table| route_table.route_table_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_route_table_ids = next_state
        .route_tables
        .iter()
        .map(|route_table| route_table.route_table_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_health_check_ids = previous_state
        .map(|state| {
            state
                .health_checks
                .iter()
                .map(|health_check| health_check.health_check_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_health_check_ids = next_state
        .health_checks
        .iter()
        .map(|health_check| health_check.health_check_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_backend_set_ids = previous_state
        .map(|state| {
            state
                .backend_sets
                .iter()
                .map(|backend_set| backend_set.backend_set_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_backend_set_ids = next_state
        .backend_sets
        .iter()
        .map(|backend_set| backend_set.backend_set_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_service_ids = previous_state
        .map(|state| {
            state
                .services
                .iter()
                .map(|service| service.service_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_service_ids = next_state
        .services
        .iter()
        .map(|service| service.service_id.as_str())
        .collect::<BTreeSet<_>>();

    let ports_removed = previous_port_ids.difference(&next_port_ids).count();
    let route_tables_removed = previous_route_table_ids
        .difference(&next_route_table_ids)
        .count();
    let health_checks_removed = previous_health_check_ids
        .difference(&next_health_check_ids)
        .count();
    let backend_sets_removed = previous_backend_set_ids
        .difference(&next_backend_set_ids)
        .count();
    let services_removed = previous_service_ids.difference(&next_service_ids).count();

    let previous_generation = previous_state.map(|state| state.generation.clone());
    let full_reconcile = previous_state
        .map(|state| {
            state.generation != next_state.generation
                || state.capability_profile != next_state.capability_profile
                || state.full_sync
                || next_state.full_sync
        })
        .unwrap_or(true);

    let mut changed_kinds = Vec::new();
    if previous_state.is_none()
        || next_state.port_bindings.len()
            != previous_state
                .map(|state| state.port_bindings.len())
                .unwrap_or(0)
        || ports_removed > 0
    {
        changed_kinds.push("ports".to_string());
    }
    if previous_state.is_none()
        || next_state.route_tables.len()
            != previous_state
                .map(|state| state.route_tables.len())
                .unwrap_or(0)
        || route_tables_removed > 0
    {
        changed_kinds.push("route_tables".to_string());
    }
    if previous_state.is_none()
        || next_state.security_group_ids
            != previous_state
                .map(|state| state.security_group_ids.clone())
                .unwrap_or_default()
    {
        changed_kinds.push("security_groups".to_string());
    }
    if previous_state.is_none()
        || next_state.network_ids
            != previous_state
                .map(|state| state.network_ids.clone())
                .unwrap_or_default()
    {
        changed_kinds.push("networks".to_string());
    }
    if previous_state.is_none()
        || next_state.tenant_ids
            != previous_state
                .map(|state| state.tenant_ids.clone())
                .unwrap_or_default()
    {
        changed_kinds.push("tenants".to_string());
    }
    if previous_state.is_none()
        || next_state.health_checks.len()
            != previous_state
                .map(|state| state.health_checks.len())
                .unwrap_or(0)
        || health_checks_removed > 0
    {
        changed_kinds.push("health_checks".to_string());
    }
    if previous_state.is_none()
        || next_state.backend_sets.len()
            != previous_state
                .map(|state| state.backend_sets.len())
                .unwrap_or(0)
        || backend_sets_removed > 0
    {
        changed_kinds.push("backend_sets".to_string());
    }
    if previous_state.is_none()
        || next_state.services.len()
            != previous_state
                .map(|state| state.services.len())
                .unwrap_or(0)
        || services_removed > 0
    {
        changed_kinds.push("services".to_string());
    }
    let previous_qos_count = previous_state
        .map(|state| state.qos_policies.len())
        .unwrap_or(0);
    if previous_state.is_none()
        || next_state.qos_policies.len() != previous_qos_count
        || previous_state
            .map(|state| {
                state
                    .qos_policies
                    .iter()
                    .map(|qp| qp.policy_id.clone())
                    .collect::<BTreeSet<_>>()
                    != next_state
                        .qos_policies
                        .iter()
                        .map(|qp| qp.policy_id.clone())
                        .collect::<BTreeSet<_>>()
            })
            .unwrap_or(true)
    {
        changed_kinds.push("qos_policies".to_string());
    }

    let mut actions = Vec::new();
    if full_reconcile {
        actions.push(ReconcileAction {
            domain: "core".to_string(),
            operation: "full_shadow_reconcile".to_string(),
            object_count: next_state.port_bindings.len()
                + next_state.route_tables.len()
                + next_state.health_checks.len()
                + next_state.backend_sets.len()
                + next_state.services.len(),
        });
    }
    if !next_state.port_bindings.is_empty() {
        actions.push(ReconcileAction {
            domain: "ports".to_string(),
            operation: "refresh_shadow_bindings".to_string(),
            object_count: next_state.port_bindings.len(),
        });
    }
    if ports_removed > 0 {
        actions.push(ReconcileAction {
            domain: "ports".to_string(),
            operation: "cleanup_shadow_bindings".to_string(),
            object_count: ports_removed,
        });
    }
    if !next_state.route_tables.is_empty() {
        actions.push(ReconcileAction {
            domain: "routes".to_string(),
            operation: "refresh_shadow_routes".to_string(),
            object_count: next_state.route_tables.len(),
        });
    }
    if route_tables_removed > 0 {
        actions.push(ReconcileAction {
            domain: "routes".to_string(),
            operation: "cleanup_shadow_routes".to_string(),
            object_count: route_tables_removed,
        });
    }
    if !next_state.security_group_ids.is_empty() {
        actions.push(ReconcileAction {
            domain: "security".to_string(),
            operation: "refresh_shadow_security".to_string(),
            object_count: next_state.security_group_ids.len(),
        });
    }
    let service_shadow_count =
        next_state.health_checks.len() + next_state.backend_sets.len() + next_state.services.len();
    if service_shadow_count > 0 {
        actions.push(ReconcileAction {
            domain: "services".to_string(),
            operation: "refresh_shadow_services".to_string(),
            object_count: service_shadow_count,
        });
    }
    let services_cleanup = health_checks_removed + backend_sets_removed + services_removed;
    if services_cleanup > 0 {
        actions.push(ReconcileAction {
            domain: "services".to_string(),
            operation: "cleanup_shadow_services".to_string(),
            object_count: services_cleanup,
        });
    }
    let qos_rule_count: usize = next_state
        .qos_policies
        .iter()
        .map(|qp| qp.rules.len())
        .sum();
    if qos_rule_count > 0 {
        actions.push(ReconcileAction {
            domain: "qos".to_string(),
            operation: "refresh_shadow_qos".to_string(),
            object_count: next_state.qos_policies.len(),
        });
    }
    let qos_removed = previous_qos_count.saturating_sub(next_state.qos_policies.len());
    if qos_removed > 0 {
        actions.push(ReconcileAction {
            domain: "qos".to_string(),
            operation: "cleanup_shadow_qos".to_string(),
            object_count: qos_removed,
        });
    }

    ReconcilePlan {
        generation: next_state.generation.clone(),
        previous_generation,
        compiled_at: next_state.compiled_at.clone(),
        full_reconcile,
        changed_kinds,
        actions,
        warnings,
        shadow_apply_only: true,
    }
}

pub(crate) fn build_runtime_plan(
    previous_state: Option<&CompiledNodeState>,
    next_state: &CompiledNodeState,
    capability: &NodeCapability,
) -> RuntimePlan {
    let previous_port_ids = previous_state
        .map(|state| {
            state
                .port_bindings
                .iter()
                .map(|binding| binding.port_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_port_ids = next_state
        .port_bindings
        .iter()
        .map(|binding| binding.port_id.as_str())
        .collect::<BTreeSet<_>>();

    let previous_route_table_ids = previous_state
        .map(|state| {
            state
                .route_tables
                .iter()
                .map(|route_table| route_table.route_table_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_route_table_ids = next_state
        .route_tables
        .iter()
        .map(|route_table| route_table.route_table_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_health_check_ids = previous_state
        .map(|state| {
            state
                .health_checks
                .iter()
                .map(|health_check| health_check.health_check_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_health_check_ids = next_state
        .health_checks
        .iter()
        .map(|health_check| health_check.health_check_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_backend_set_ids = previous_state
        .map(|state| {
            state
                .backend_sets
                .iter()
                .map(|backend_set| backend_set.backend_set_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_backend_set_ids = next_state
        .backend_sets
        .iter()
        .map(|backend_set| backend_set.backend_set_id.as_str())
        .collect::<BTreeSet<_>>();
    let previous_service_ids = previous_state
        .map(|state| {
            state
                .services
                .iter()
                .map(|service| service.service_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let next_service_ids = next_state
        .services
        .iter()
        .map(|service| service.service_id.as_str())
        .collect::<BTreeSet<_>>();

    let ports_removed = previous_port_ids.difference(&next_port_ids).count();
    let route_tables_removed = previous_route_table_ids
        .difference(&next_route_table_ids)
        .count();
    let health_checks_removed = previous_health_check_ids
        .difference(&next_health_check_ids)
        .count();
    let backend_sets_removed = previous_backend_set_ids
        .difference(&next_backend_set_ids)
        .count();
    let services_removed = previous_service_ids.difference(&next_service_ids).count();
    let previous_service_listener_count = previous_state
        .map(total_service_listener_ports)
        .unwrap_or_default();
    let next_service_listener_count = total_service_listener_ports(next_state);
    let previous_service_frontend_runtime_count = previous_state
        .map(total_service_frontend_runtime_entries)
        .unwrap_or_default();
    let next_service_frontend_runtime_count = total_service_frontend_runtime_entries(next_state);
    let previous_socket_lb_frontend_count = previous_state
        .map(total_socket_lb_frontends)
        .unwrap_or_default();
    let next_socket_lb_frontend_count = total_socket_lb_frontends(next_state);
    let previous_packet_lb_frontend_count = previous_state
        .map(total_packet_lb_frontends)
        .unwrap_or_default();
    let next_packet_lb_frontend_count = total_packet_lb_frontends(next_state);
    let previous_backend_member_count = previous_state
        .map(total_service_backend_members)
        .unwrap_or_default();
    let next_backend_member_count = total_service_backend_members(next_state);
    let previous_backend_runtime_count = previous_state
        .map(total_backend_member_runtime_entries)
        .unwrap_or_default();
    let next_backend_runtime_count = total_backend_member_runtime_entries(next_state);
    let previous_forwarding_projection_count = previous_state
        .map(total_service_forwarding_projections)
        .unwrap_or_default();
    let next_forwarding_projection_count = total_service_forwarding_projections(next_state);
    let previous_service_revnat_count = previous_state
        .map(total_service_revnat_entries)
        .unwrap_or_default();
    let next_service_revnat_count = total_service_revnat_entries(next_state);
    let previous_service_affinity_count = previous_state
        .map(total_affinity_service_programs)
        .unwrap_or_default();
    let next_service_affinity_count = total_affinity_service_programs(next_state);
    let previous_service_maglev_count = previous_state
        .map(total_maglev_service_programs)
        .unwrap_or_default();
    let next_service_maglev_count = total_maglev_service_programs(next_state);
    let previous_port_identity_count = previous_state
        .map(|state| state.port_identities.len())
        .unwrap_or_default();
    let next_port_identity_count = next_state.port_identities.len();
    let previous_anti_spoof_count = previous_state
        .map(total_anti_spoof_entries)
        .unwrap_or_default();
    let next_anti_spoof_count = total_anti_spoof_entries(next_state);
    let previous_sg_rule_count = previous_state
        .map(|state| state.sg_rules.len())
        .unwrap_or_default();
    let next_sg_rule_count = next_state.sg_rules.len();
    let previous_route_v4_count = previous_state
        .map(total_route_v4_entries)
        .unwrap_or_default();
    let next_route_v4_count = total_route_v4_entries(next_state);
    let previous_route_v6_count = previous_state
        .map(total_route_v6_entries)
        .unwrap_or_default();
    let next_route_v6_count = total_route_v6_entries(next_state);

    let mut bindings = Vec::new();
    if capability.supports_tc && next_port_identity_count > 0 {
        bindings.push(AttachBindingPlan {
            hook_family: "tc_ingress".to_string(),
            scope: "port-bindings".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_port_identity_count,
        });
        bindings.push(AttachBindingPlan {
            hook_family: "tc_egress".to_string(),
            scope: "port-bindings".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_port_identity_count,
        });
    }
    if capability.supports_xdp && next_port_identity_count > 0 {
        bindings.push(AttachBindingPlan {
            hook_family: "xdp".to_string(),
            scope: "anti-spoof-fastpath".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_port_identity_count,
        });
    }
    if capability.supports_tc && (next_route_v4_count > 0 || next_route_v6_count > 0) {
        bindings.push(AttachBindingPlan {
            hook_family: "tc_egress".to_string(),
            scope: "route-tables".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_route_v4_count + next_route_v6_count,
        });
    }
    if capability.supports_tc && ports_removed > 0 {
        bindings.push(AttachBindingPlan {
            hook_family: "tc".to_string(),
            scope: "port-bindings".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: ports_removed,
        });
    }
    if capability.supports_tc && route_tables_removed > 0 {
        bindings.push(AttachBindingPlan {
            hook_family: "tc".to_string(),
            scope: "route-tables".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: route_tables_removed,
        });
    }

    let mut required_hooks = bindings
        .iter()
        .map(|binding| binding.hook_family.clone())
        .collect::<Vec<_>>();
    required_hooks.sort();
    required_hooks.dedup();

    let mut required_qdisc = Vec::new();
    if bindings
        .iter()
        .any(|binding| binding.hook_family.starts_with("tc"))
    {
        required_qdisc.push("clsact".to_string());
    }

    let attach_plan = AttachPlan {
        generation: next_state.generation.clone(),
        compiled_at: next_state.compiled_at.clone(),
        required_hooks,
        bindings,
        required_qdisc,
        shadow_apply_only: true,
    };

    let mut entries = Vec::new();
    if !next_state.tenant_ids.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "tenant_index".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.tenant_ids.len(),
        });
    }
    if !next_state.network_ids.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "network_index".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.network_ids.len(),
        });
    }
    if next_sg_rule_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "sg_rule_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_sg_rule_count,
        });
    }
    if next_port_identity_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "port_identity_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_port_identity_count,
        });
    }
    if next_anti_spoof_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "anti_spoof_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_anti_spoof_count,
        });
    }
    if previous_port_identity_count > next_port_identity_count {
        entries.push(MapPlanEntry {
            map_family: "port_identity_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_port_identity_count - next_port_identity_count,
        });
    }
    if previous_anti_spoof_count > next_anti_spoof_count {
        entries.push(MapPlanEntry {
            map_family: "anti_spoof_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_anti_spoof_count - next_anti_spoof_count,
        });
    }
    if next_route_v4_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "route_table_v4".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_route_v4_count,
        });
    }
    if next_route_v6_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "route_table_v6".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_route_v6_count,
        });
    }
    if previous_route_v4_count > next_route_v4_count {
        entries.push(MapPlanEntry {
            map_family: "route_table_v4".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_route_v4_count - next_route_v4_count,
        });
    }
    if previous_route_v6_count > next_route_v6_count {
        entries.push(MapPlanEntry {
            map_family: "route_table_v6".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_route_v6_count - next_route_v6_count,
        });
    }
    if previous_sg_rule_count > next_sg_rule_count {
        entries.push(MapPlanEntry {
            map_family: "sg_rule_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_sg_rule_count - next_sg_rule_count,
        });
    }
    if !next_state.health_checks.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "health_check_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.health_checks.len(),
        });
    }
    if !next_state.backend_sets.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "backend_set_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.backend_sets.len(),
        });
    }
    if !next_state.services.is_empty() {
        entries.push(MapPlanEntry {
            map_family: "service_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_state.services.len(),
        });
    }
    if next_service_listener_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_frontend_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_listener_count,
        });
    }
    if next_service_frontend_runtime_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_frontend_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_frontend_runtime_count,
        });
    }
    if next_socket_lb_frontend_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_socket_lb_projection".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_socket_lb_frontend_count,
        });
    }
    if next_packet_lb_frontend_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_packet_lb_projection".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_packet_lb_frontend_count,
        });
    }
    if next_backend_member_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "backend_member_catalog".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_backend_member_count,
        });
    }
    if next_backend_runtime_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "backend_member_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_backend_runtime_count,
        });
    }
    if next_forwarding_projection_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_forwarding_projection".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_forwarding_projection_count,
        });
    }
    if next_service_revnat_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_revnat_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_revnat_count,
        });
    }
    if next_service_affinity_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_affinity_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_affinity_count,
        });
    }
    if next_service_maglev_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_maglev_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_service_maglev_count,
        });
    }
    if health_checks_removed > 0 {
        entries.push(MapPlanEntry {
            map_family: "health_check_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: health_checks_removed,
        });
    }
    if backend_sets_removed > 0 {
        entries.push(MapPlanEntry {
            map_family: "backend_set_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: backend_sets_removed,
        });
    }
    if services_removed > 0 {
        entries.push(MapPlanEntry {
            map_family: "service_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: services_removed,
        });
    }
    if previous_service_listener_count > next_service_listener_count {
        entries.push(MapPlanEntry {
            map_family: "service_frontend_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_listener_count - next_service_listener_count,
        });
    }
    if previous_service_frontend_runtime_count > next_service_frontend_runtime_count {
        entries.push(MapPlanEntry {
            map_family: "service_frontend_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_frontend_runtime_count
                - next_service_frontend_runtime_count,
        });
    }
    if previous_socket_lb_frontend_count > next_socket_lb_frontend_count {
        entries.push(MapPlanEntry {
            map_family: "service_socket_lb_projection".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_socket_lb_frontend_count - next_socket_lb_frontend_count,
        });
    }
    if previous_packet_lb_frontend_count > next_packet_lb_frontend_count {
        entries.push(MapPlanEntry {
            map_family: "service_packet_lb_projection".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_packet_lb_frontend_count - next_packet_lb_frontend_count,
        });
    }
    if previous_backend_member_count > next_backend_member_count {
        entries.push(MapPlanEntry {
            map_family: "backend_member_catalog".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_backend_member_count - next_backend_member_count,
        });
    }
    if previous_backend_runtime_count > next_backend_runtime_count {
        entries.push(MapPlanEntry {
            map_family: "backend_member_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_backend_runtime_count - next_backend_runtime_count,
        });
    }
    if previous_forwarding_projection_count > next_forwarding_projection_count {
        entries.push(MapPlanEntry {
            map_family: "service_forwarding_projection".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_forwarding_projection_count - next_forwarding_projection_count,
        });
    }
    if previous_service_revnat_count > next_service_revnat_count {
        entries.push(MapPlanEntry {
            map_family: "service_revnat_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_revnat_count - next_service_revnat_count,
        });
    }
    if previous_service_affinity_count > next_service_affinity_count {
        entries.push(MapPlanEntry {
            map_family: "service_affinity_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_affinity_count - next_service_affinity_count,
        });
    }
    if previous_service_maglev_count > next_service_maglev_count {
        entries.push(MapPlanEntry {
            map_family: "service_maglev_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_service_maglev_count - next_service_maglev_count,
        });
    }

    // --- QoS map plan ---
    let next_qos_rule_count: usize = next_state
        .qos_policies
        .iter()
        .map(|qp| {
            let tap_count = next_state
                .port_identities
                .iter()
                .filter(|p| p.network_id == qp.network_id)
                .count()
                .max(1);
            qp.rules.len() * tap_count
        })
        .sum();
    let previous_qos_rule_count: usize = previous_state
        .map(|state| {
            state
                .qos_policies
                .iter()
                .map(|qp| {
                    let tap_count = state
                        .port_identities
                        .iter()
                        .filter(|p| p.network_id == qp.network_id)
                        .count()
                        .max(1);
                    qp.rules.len() * tap_count
                })
                .sum()
        })
        .unwrap_or(0);
    if next_qos_rule_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "qos_config_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_qos_rule_count,
        });
        entries.push(MapPlanEntry {
            map_family: "qos_token_bucket_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_qos_rule_count,
        });
    }
    if previous_qos_rule_count > next_qos_rule_count {
        entries.push(MapPlanEntry {
            map_family: "qos_config_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_qos_rule_count - next_qos_rule_count,
        });
        entries.push(MapPlanEntry {
            map_family: "qos_token_bucket_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_qos_rule_count - next_qos_rule_count,
        });
    }

    // --- MirrorPolicy map plan ---
    let next_mirror_rule_count: usize = next_state
        .mirror_policies
        .iter()
        .map(|mp| {
            let tap_count = next_state
                .port_identities
                .iter()
                .filter(|p| p.network_id == mp.network_id)
                .count()
                .max(1);
            mp.rules.len() * tap_count
        })
        .sum();
    let previous_mirror_rule_count: usize = previous_state
        .map(|state| {
            state
                .mirror_policies
                .iter()
                .map(|mp| {
                    let tap_count = state
                        .port_identities
                        .iter()
                        .filter(|p| p.network_id == mp.network_id)
                        .count()
                        .max(1);
                    mp.rules.len() * tap_count
                })
                .sum()
        })
        .unwrap_or(0);
    if next_mirror_rule_count > 0 {
        entries.push(MapPlanEntry {
            map_family: "mirror_policy_map".to_string(),
            operation: "refresh_shadow".to_string(),
            object_count: next_mirror_rule_count,
        });
    }
    if previous_mirror_rule_count > next_mirror_rule_count {
        entries.push(MapPlanEntry {
            map_family: "mirror_policy_map".to_string(),
            operation: "cleanup_shadow".to_string(),
            object_count: previous_mirror_rule_count - next_mirror_rule_count,
        });
    }

    let map_plan = MapPlan {
        generation: next_state.generation.clone(),
        compiled_at: next_state.compiled_at.clone(),
        entries,
        shadow_apply_only: true,
    };

    RuntimePlan {
        generation: next_state.generation.clone(),
        compiled_at: next_state.compiled_at.clone(),
        attach_plan,
        map_plan,
        shadow_apply_only: true,
    }
}

pub(crate) fn build_runtime_inventory(
    previous_state: Option<&CompiledNodeState>,
    compiled_state: &CompiledNodeState,
    runtime_plan: &RuntimePlan,
) -> RuntimeInventory {
    let attach_inventory = runtime_plan
        .attach_plan
        .bindings
        .iter()
        .map(|binding| RuntimeInventoryAttach {
            domain: inventory_domain_from_scope(&binding.scope),
            hook_family: binding.hook_family.clone(),
            scope: binding.scope.clone(),
            operation: binding.operation.clone(),
            object_count: binding.object_count,
        })
        .collect::<Vec<_>>();

    let map_inventory = runtime_plan
        .map_plan
        .entries
        .iter()
        .map(|entry| RuntimeInventoryMapEntry {
            domain: inventory_domain_from_map_family(&entry.map_family),
            map_family: entry.map_family.clone(),
            operation: entry.operation.clone(),
            object_count: entry.object_count,
        })
        .collect::<Vec<_>>();

    let mut attach_counts = BTreeMap::new();
    for entry in &attach_inventory {
        *attach_counts.entry(entry.domain.clone()).or_insert(0usize) += 1;
    }

    let mut map_counts = BTreeMap::new();
    for entry in &map_inventory {
        *map_counts.entry(entry.domain.clone()).or_insert(0usize) += 1;
    }

    let domain_inventory = compiled_state
        .domain_summaries
        .iter()
        .map(|summary| RuntimeInventoryDomainSummary {
            domain: summary.domain.clone(),
            compiled_objects: summary.compiled_objects,
            failed_objects: summary.failed_objects,
            attach_operations: attach_counts.get(&summary.domain).copied().unwrap_or(0),
            map_operations: map_counts.get(&summary.domain).copied().unwrap_or(0),
            status: if summary.status == "shadow_reserved" {
                "shadow_inventory_reserved".to_string()
            } else if summary.failed_objects == 0 {
                "shadow_inventory_ready".to_string()
            } else {
                "shadow_inventory_degraded".to_string()
            },
            shadow_apply_only: true,
        })
        .collect::<Vec<_>>();

    RuntimeInventory {
        generation: runtime_plan.generation.clone(),
        previous_generation: previous_state
            .map(|state| state.generation.clone())
            .filter(|generation| generation != &runtime_plan.generation),
        compiled_at: runtime_plan.compiled_at.clone(),
        observed_at: unix_timestamp_string(),
        compiler_version: compiled_state.compiler_version.clone(),
        required_hooks: runtime_plan.attach_plan.required_hooks.clone(),
        required_qdisc: runtime_plan.attach_plan.required_qdisc.clone(),
        attach_inventory,
        map_inventory,
        domain_inventory,
        shadow_apply_only: true,
    }
}

pub(crate) fn build_runtime_inventory_diff(
    previous_inventory: Option<&RuntimeInventory>,
    current_inventory: &RuntimeInventory,
) -> RuntimeInventoryDiff {
    let mut attach_deltas = Vec::new();
    let mut changed_domains = BTreeSet::new();
    let mut has_cleanup = false;

    let previous_attach = previous_inventory
        .map(|inventory| {
            inventory
                .attach_inventory
                .iter()
                .map(|entry| {
                    (
                        (
                            entry.domain.clone(),
                            entry.hook_family.clone(),
                            entry.scope.clone(),
                            entry.operation.clone(),
                        ),
                        entry.object_count,
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let current_attach = current_inventory
        .attach_inventory
        .iter()
        .map(|entry| {
            (
                (
                    entry.domain.clone(),
                    entry.hook_family.clone(),
                    entry.scope.clone(),
                    entry.operation.clone(),
                ),
                entry.object_count,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let attach_keys = previous_attach
        .keys()
        .cloned()
        .chain(current_attach.keys().cloned())
        .collect::<BTreeSet<_>>();
    for (domain, hook_family, scope, operation) in attach_keys {
        let previous_object_count = previous_attach
            .get(&(
                domain.clone(),
                hook_family.clone(),
                scope.clone(),
                operation.clone(),
            ))
            .copied()
            .unwrap_or(0);
        let current_object_count = current_attach
            .get(&(
                domain.clone(),
                hook_family.clone(),
                scope.clone(),
                operation.clone(),
            ))
            .copied()
            .unwrap_or(0);
        if previous_object_count == current_object_count {
            continue;
        }
        let change_type = if previous_object_count == 0 {
            "added".to_string()
        } else if current_object_count == 0 {
            "removed".to_string()
        } else {
            "updated".to_string()
        };
        if operation.contains("cleanup") || change_type == "removed" {
            has_cleanup = true;
        }
        changed_domains.insert(domain.clone());
        attach_deltas.push(RuntimeInventoryAttachDelta {
            domain,
            hook_family,
            scope,
            operation,
            previous_object_count,
            current_object_count,
            change_type,
        });
    }

    let previous_maps = previous_inventory
        .map(|inventory| {
            inventory
                .map_inventory
                .iter()
                .map(|entry| {
                    (
                        (
                            entry.domain.clone(),
                            entry.map_family.clone(),
                            entry.operation.clone(),
                        ),
                        entry.object_count,
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let current_maps = current_inventory
        .map_inventory
        .iter()
        .map(|entry| {
            (
                (
                    entry.domain.clone(),
                    entry.map_family.clone(),
                    entry.operation.clone(),
                ),
                entry.object_count,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let map_keys = previous_maps
        .keys()
        .cloned()
        .chain(current_maps.keys().cloned())
        .collect::<BTreeSet<_>>();
    let mut map_deltas = Vec::new();
    for (domain, map_family, operation) in map_keys {
        let previous_object_count = previous_maps
            .get(&(domain.clone(), map_family.clone(), operation.clone()))
            .copied()
            .unwrap_or(0);
        let current_object_count = current_maps
            .get(&(domain.clone(), map_family.clone(), operation.clone()))
            .copied()
            .unwrap_or(0);
        if previous_object_count == current_object_count {
            continue;
        }
        let change_type = if previous_object_count == 0 {
            "added".to_string()
        } else if current_object_count == 0 {
            "removed".to_string()
        } else {
            "updated".to_string()
        };
        if operation.contains("cleanup") || change_type == "removed" {
            has_cleanup = true;
        }
        changed_domains.insert(domain.clone());
        map_deltas.push(RuntimeInventoryMapDelta {
            domain,
            map_family,
            operation,
            previous_object_count,
            current_object_count,
            change_type,
        });
    }

    let previous_domains = previous_inventory
        .map(|inventory| {
            inventory
                .domain_inventory
                .iter()
                .map(|domain| (domain.domain.clone(), domain))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let current_domains = current_inventory
        .domain_inventory
        .iter()
        .map(|domain| (domain.domain.clone(), domain))
        .collect::<BTreeMap<_, _>>();
    let domain_keys = previous_domains
        .keys()
        .cloned()
        .chain(current_domains.keys().cloned())
        .collect::<BTreeSet<_>>();
    let mut domain_deltas = Vec::new();
    for domain in domain_keys {
        let previous = previous_domains.get(&domain);
        let current = current_domains.get(&domain);
        let previous_attach_operations = previous
            .map(|summary| summary.attach_operations)
            .unwrap_or(0);
        let current_attach_operations = current
            .map(|summary| summary.attach_operations)
            .unwrap_or(0);
        let previous_map_operations = previous.map(|summary| summary.map_operations).unwrap_or(0);
        let current_map_operations = current.map(|summary| summary.map_operations).unwrap_or(0);
        let previous_compiled_objects = previous
            .map(|summary| summary.compiled_objects)
            .unwrap_or(0);
        let current_compiled_objects = current.map(|summary| summary.compiled_objects).unwrap_or(0);

        if previous_attach_operations == current_attach_operations
            && previous_map_operations == current_map_operations
            && previous_compiled_objects == current_compiled_objects
        {
            continue;
        }

        let change_type = if previous.is_none() {
            "added".to_string()
        } else if current.is_none() {
            "removed".to_string()
        } else {
            "updated".to_string()
        };
        if change_type == "removed" {
            has_cleanup = true;
        }
        changed_domains.insert(domain.clone());
        domain_deltas.push(RuntimeInventoryDomainDelta {
            domain,
            previous_attach_operations,
            current_attach_operations,
            previous_map_operations,
            current_map_operations,
            previous_compiled_objects,
            current_compiled_objects,
            change_type,
        });
    }

    RuntimeInventoryDiff {
        generation: current_inventory.generation.clone(),
        previous_generation: previous_inventory
            .map(|inventory| inventory.generation.clone())
            .filter(|generation| generation != &current_inventory.generation),
        observed_at: unix_timestamp_string(),
        changed_domains: changed_domains.into_iter().collect(),
        attach_deltas,
        map_deltas,
        domain_deltas,
        has_cleanup,
        shadow_apply_only: true,
    }
}

