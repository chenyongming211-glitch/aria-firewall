use std::collections::{BTreeMap, BTreeSet};

use super::helpers::*;
use super::ir_types::*;

pub(crate) fn build_runtime_intent(
    compiled_state: &CompiledNodeState,
    reconcile_plan: &ReconcilePlan,
    runtime_inventory: &RuntimeInventory,
    runtime_inventory_diff: &RuntimeInventoryDiff,
) -> RuntimeIntent {
    let changed_domains = runtime_inventory_diff
        .changed_domains
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let cleanup_domains = runtime_inventory_diff
        .attach_deltas
        .iter()
        .filter(|delta| delta.change_type == "removed" || delta.operation.contains("cleanup"))
        .map(|delta| delta.domain.clone())
        .chain(
            runtime_inventory_diff
                .map_deltas
                .iter()
                .filter(|delta| {
                    delta.change_type == "removed" || delta.operation.contains("cleanup")
                })
                .map(|delta| delta.domain.clone()),
        )
        .collect::<BTreeSet<_>>();

    let reconcile_domains = reconcile_plan
        .actions
        .iter()
        .map(|action| action.domain.as_str())
        .collect::<BTreeSet<_>>();

    let intents = runtime_inventory
        .domain_inventory
        .iter()
        .map(|domain_summary| {
            let changed = changed_domains.contains(&domain_summary.domain)
                || reconcile_domains.contains(domain_summary.domain.as_str())
                || (reconcile_plan.full_reconcile && reconcile_domains.contains("core"));
            let requires_cleanup = cleanup_domains.contains(&domain_summary.domain);
            let reserved_nat_domain = domain_summary.domain == "nat"
                && domain_summary.compiled_objects == 0
                && domain_summary.attach_operations == 0
                && domain_summary.map_operations == 0
                && domain_summary.failed_objects == 0;
            let full_reconcile = !reserved_nat_domain
                && reconcile_plan.full_reconcile
                && (changed || reconcile_domains.contains("core"));
            let desired_action = if reserved_nat_domain {
                "reserve_shadow".to_string()
            } else if full_reconcile {
                "full_shadow_reconcile".to_string()
            } else if requires_cleanup {
                "cleanup_shadow".to_string()
            } else if changed {
                "refresh_shadow".to_string()
            } else {
                "maintain_shadow".to_string()
            };
            let reason = if reserved_nat_domain {
                "reserved_for_snat_dnat_floating_ip".to_string()
            } else if full_reconcile {
                "generation_or_capability_shift".to_string()
            } else if requires_cleanup {
                "runtime_cleanup_required".to_string()
            } else if changed {
                "runtime_inventory_delta".to_string()
            } else {
                "shadow_inventory_stable".to_string()
            };

            RuntimeDomainIntent {
                domain: domain_summary.domain.clone(),
                desired_action,
                reason,
                full_reconcile,
                requires_cleanup,
                changed,
                attach_operations: domain_summary.attach_operations,
                map_operations: domain_summary.map_operations,
                compiled_objects: domain_summary.compiled_objects,
                failed_objects: domain_summary.failed_objects,
                shadow_apply_only: true,
            }
        })
        .collect::<Vec<_>>();

    let service_intent = intents
        .iter()
        .find(|intent| intent.domain == "services")
        .map(|intent| ServiceRuntimeIntentSummary {
            service_count: compiled_state.service_programs.len(),
            frontend_listener_count: total_service_listener_ports(compiled_state),
            frontend_runtime_entry_count: total_service_frontend_runtime_entries(compiled_state),
            socket_lb_frontend_count: total_socket_lb_frontends(compiled_state),
            packet_lb_frontend_count: total_packet_lb_frontends(compiled_state),
            backend_member_count: total_service_backend_members(compiled_state),
            backend_runtime_entry_count: total_backend_member_runtime_entries(compiled_state),
            forwarding_projection_count: total_service_forwarding_projections(compiled_state),
            node_local_service_count: total_node_local_service_programs(compiled_state),
            cross_node_service_count: total_cross_node_service_programs(compiled_state),
            cross_node_native_service_count: total_cross_node_native_service_programs(
                compiled_state,
            ),
            cross_node_overlay_service_count: total_cross_node_overlay_service_programs(
                compiled_state,
            ),
            cross_node_hybrid_service_count: total_cross_node_hybrid_service_programs(
                compiled_state,
            ),
            revnat_reservation_count: total_service_revnat_entries(compiled_state),
            affinity_reservation_count: total_affinity_service_programs(compiled_state),
            maglev_reservation_count: total_maglev_service_programs(compiled_state),
            desired_action: intent.desired_action.clone(),
            requires_cleanup: intent.requires_cleanup,
            changed: intent.changed,
            shadow_apply_only: true,
        });

    RuntimeIntent {
        generation: runtime_inventory.generation.clone(),
        previous_generation: runtime_inventory
            .previous_generation
            .clone()
            .or_else(|| runtime_inventory_diff.previous_generation.clone()),
        compiled_at: runtime_inventory.compiled_at.clone(),
        observed_at: unix_timestamp_string(),
        changed_domains: changed_domains.into_iter().collect(),
        intents,
        service_intent,
        shadow_apply_only: true,
    }
}

pub(crate) fn build_runtime_execution_summary(
    compiled_state: &CompiledNodeState,
    reconcile_plan: &ReconcilePlan,
    runtime_intent: &RuntimeIntent,
) -> RuntimeExecutionSummary {
    let compile_summary_by_domain = compiled_state
        .domain_summaries
        .iter()
        .map(|summary| (summary.domain.as_str(), summary))
        .collect::<BTreeMap<_, _>>();

    let reconcile_domains = reconcile_plan
        .actions
        .iter()
        .map(|action| action.domain.as_str())
        .collect::<BTreeSet<_>>();

    let domain_summaries = runtime_intent
        .intents
        .iter()
        .map(|intent| {
            let compile_summary = compile_summary_by_domain.get(intent.domain.as_str()).copied();
            let input_objects = compile_summary.map(|summary| summary.input_objects).unwrap_or(0);
            let failed_objects = compile_summary
                .map(|summary| summary.failed_objects)
                .unwrap_or(intent.failed_objects);
            let overlay_encap_unsupported =
                intent.domain == "services"
                    && compiled_state
                        .degraded_reasons
                        .iter()
                        .any(|reason| reason == "overlay_encap_unsupported");
            let execution_status = if intent.desired_action == "reserve_shadow" {
                "shadow_reserved".to_string()
            } else if overlay_encap_unsupported {
                "shadow_execute_degraded".to_string()
            } else if failed_objects > 0 {
                "shadow_execute_degraded".to_string()
            } else if intent.requires_cleanup {
                "shadow_execute_cleanup".to_string()
            } else if intent.changed || reconcile_domains.contains(intent.domain.as_str()) {
                "shadow_execute_planned".to_string()
            } else {
                "shadow_execute_stable".to_string()
            };

            let mut degraded_reasons = Vec::new();
            if failed_objects > 0 {
                degraded_reasons.push("compile_or_inventory_failures_present".to_string());
            }
            if overlay_encap_unsupported {
                degraded_reasons.push("overlay_encap_unsupported".to_string());
            }
            if intent.requires_cleanup {
                degraded_reasons.push("cleanup_required".to_string());
            }
            if reconcile_plan.full_reconcile && intent.full_reconcile {
                degraded_reasons.push("full_reconcile_required".to_string());
            }

            let mut warnings = Vec::new();
            if intent.domain == "services" && intent.changed && intent.compiled_objects > 0 {
                warnings.push(
                    "services shadow execution planned; node-local and cross-node LB datapath not materialized yet"
                        .to_string(),
                );
            }
            if intent.domain == "nat" && intent.desired_action == "reserve_shadow" {
                warnings.push(
                    "nat shadow reservation present; snat/dnat/floating-ip datapath not materialized yet"
                        .to_string(),
                );
            }

            RuntimeExecutionDomainSummary {
                domain: intent.domain.clone(),
                planned_action: intent.desired_action.clone(),
                execution_status,
                requires_cleanup: intent.requires_cleanup,
                changed: intent.changed,
                input_objects,
                compiled_objects: intent.compiled_objects,
                failed_objects,
                warnings,
                degraded_reasons,
                shadow_apply_only: intent.shadow_apply_only,
            }
        })
        .collect::<Vec<_>>();

    let service_execution = runtime_intent.service_intent.as_ref().map(|service_intent| {
        let execution_status = domain_summaries
            .iter()
            .find(|summary| summary.domain == "services")
            .map(|summary| summary.execution_status.clone())
            .unwrap_or_else(|| "shadow_execute_stable".to_string());

        let mut warnings = Vec::new();
        if service_intent.node_local_service_count > 0 {
            warnings.push(
                "node-local service forwarding is still shadow planned; lb datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.socket_lb_frontend_count > 0 {
            warnings.push(
                "socket lb path is still shadow planned for internal service listeners; socket-level frontend translation is not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.frontend_runtime_entry_count > 0 {
            warnings.push(
                "service frontend runtime family is still shadow reserved; frontend lookup datapath is not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.packet_lb_frontend_count > 0 {
            warnings.push(
                "packet lb path is still shadow planned for external or cross-node service listeners; tc packet rewrite path is not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.backend_runtime_entry_count > 0 {
            warnings.push(
                "backend member runtime family is still shadow reserved; backend selection datapath is not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.cross_node_service_count > 0 {
            warnings.push(
                "cross-node service forwarding is still shadow planned; handoff datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.cross_node_native_service_count > 0 {
            warnings.push(
                "native cross-node service forwarding is still shadow planned; route handoff datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.cross_node_overlay_service_count > 0 {
            if compiled_state
                .degraded_reasons
                .iter()
                .any(|reason| reason == "overlay_encap_unsupported")
            {
                warnings.push(
                    "overlay cross-node service forwarding requested but node capability supports_encap=false; shadow compiler marked services degraded"
                        .to_string(),
                );
            } else {
                warnings.push(
                    "overlay cross-node service forwarding is still shadow planned; vxlan/geneve handoff datapath not materialized yet"
                        .to_string(),
                );
            }
        }
        if service_intent.cross_node_hybrid_service_count > 0 {
            warnings.push(
                "hybrid cross-node service forwarding is still shadow planned; overlay/native policy handoff not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.revnat_reservation_count > 0 {
            warnings.push(
                "service revnat runtime family is still shadow reserved; packet return-path datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.affinity_reservation_count > 0 {
            warnings.push(
                "service affinity runtime family is still shadow reserved; session stickiness datapath not materialized yet"
                    .to_string(),
            );
        }
        if service_intent.maglev_reservation_count > 0 {
            warnings.push(
                "service maglev runtime family is still shadow reserved; consistent-hash datapath not materialized yet"
                    .to_string(),
            );
        }

        ServiceRuntimeExecutionSummary {
            service_count: service_intent.service_count,
            frontend_listener_count: service_intent.frontend_listener_count,
            frontend_runtime_entry_count: service_intent.frontend_runtime_entry_count,
            socket_lb_frontend_count: service_intent.socket_lb_frontend_count,
            packet_lb_frontend_count: service_intent.packet_lb_frontend_count,
            backend_member_count: service_intent.backend_member_count,
            backend_runtime_entry_count: service_intent.backend_runtime_entry_count,
            forwarding_projection_count: service_intent.forwarding_projection_count,
            node_local_service_count: service_intent.node_local_service_count,
            cross_node_service_count: service_intent.cross_node_service_count,
            cross_node_native_service_count: service_intent.cross_node_native_service_count,
            cross_node_overlay_service_count: service_intent
                .cross_node_overlay_service_count,
            cross_node_hybrid_service_count: service_intent.cross_node_hybrid_service_count,
            revnat_reservation_count: service_intent.revnat_reservation_count,
            affinity_reservation_count: service_intent.affinity_reservation_count,
            maglev_reservation_count: service_intent.maglev_reservation_count,
            execution_status,
            planned_action: service_intent.desired_action.clone(),
            warnings,
            shadow_apply_only: true,
        }
    });

    RuntimeExecutionSummary {
        generation: runtime_intent.generation.clone(),
        previous_generation: runtime_intent.previous_generation.clone(),
        compiled_at: runtime_intent.compiled_at.clone(),
        observed_at: unix_timestamp_string(),
        changed_domains: runtime_intent.changed_domains.clone(),
        domain_summaries,
        service_execution,
        shadow_apply_only: true,
    }
}

