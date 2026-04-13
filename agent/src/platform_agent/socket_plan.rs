use super::helpers::{build_backend_choice_shape, normalize_socket_affinity_strategy, normalize_socket_lb_strategy, unix_timestamp_string};
use super::ir_types::*;

pub(crate) fn build_socket_selection_plan(
    previous_state: Option<&CompiledNodeState>,
    compiled_state: &CompiledNodeState,
) -> SocketSelectionPlan {
    let entries = compiled_state
        .service_programs
        .iter()
        .filter(|program| program.frontend.exposure_type == "internal")
        .flat_map(|program| {
            let local_backend_count = program
                .backend_set
                .as_ref()
                .map(|backend_set| backend_set.local_backend_count)
                .unwrap_or(0);
            let remote_backend_count = program
                .backend_set
                .as_ref()
                .map(|backend_set| backend_set.remote_backend_count)
                .unwrap_or(0);
            let backend_choice = build_backend_choice_shape(
                program.backend_set.as_ref(),
                &program.frontend.forwarding_mode,
            );
            program.frontend.listener_ports.iter().map(move |listener| {
                let normalized_lb_strategy =
                    normalize_socket_lb_strategy(&program.frontend.lb_policy);
                let normalized_affinity_strategy = normalize_socket_affinity_strategy(
                    program.frontend.session_affinity.as_deref(),
                );
                SocketSelectionPlanEntry {
                    service_id: program.service_id.clone(),
                    service_name: None,
                    vip: program.frontend.vip.clone(),
                    protocol: program.frontend.protocol.clone(),
                    service_port: listener.service_port,
                    target_port: listener.target_port,
                    lb_policy: program.frontend.lb_policy.clone(),
                    session_affinity: program.frontend.session_affinity.clone(),
                    normalized_lb_strategy,
                    normalized_affinity_strategy,
                    forwarding_mode: program.frontend.forwarding_mode.clone(),
                    local_backend_count,
                    remote_backend_count,
                    handoff_required: remote_backend_count > 0,
                    backend_choice: backend_choice.clone(),
                    shadow_apply_only: true,
                }
            })
        })
        .collect::<Vec<_>>();

    let listener_count = entries.len();
    let node_local_listener_count = entries
        .iter()
        .filter(|entry| !entry.handoff_required)
        .count();
    let cross_node_handoff_listener_count = entries
        .iter()
        .filter(|entry| entry.handoff_required)
        .count();
    let random_listener_count = entries
        .iter()
        .filter(|entry| entry.normalized_lb_strategy == "random")
        .count();
    let maglev_listener_count = entries
        .iter()
        .filter(|entry| entry.normalized_lb_strategy == "maglev")
        .count();
    let deferred_hash_listener_count = entries
        .iter()
        .filter(|entry| entry.normalized_lb_strategy == "deferred_hash")
        .count();
    let unsupported_policy_listener_count = entries
        .iter()
        .filter(|entry| entry.normalized_lb_strategy == "unsupported")
        .count();
    let affinity_listener_count = entries
        .iter()
        .filter(|entry| entry.normalized_affinity_strategy != "none")
        .count();
    let client_ip_affinity_listener_count = entries
        .iter()
        .filter(|entry| entry.normalized_affinity_strategy == "client_ip")
        .count();
    let deferred_affinity_listener_count = entries
        .iter()
        .filter(|entry| entry.normalized_affinity_strategy == "deferred_5tuple")
        .count();
    let unsupported_affinity_listener_count = entries
        .iter()
        .filter(|entry| entry.normalized_affinity_strategy == "unsupported")
        .count();
    let total_backend_candidates: usize = entries
        .iter()
        .map(|entry| entry.backend_choice.total_candidates)
        .sum();
    let total_local_candidates: usize = entries
        .iter()
        .map(|entry| entry.backend_choice.local_candidates)
        .sum();
    let total_remote_candidates: usize = entries
        .iter()
        .map(|entry| entry.backend_choice.remote_candidates)
        .sum();
    let listeners_with_no_backends = entries
        .iter()
        .filter(|entry| entry.backend_choice.total_candidates == 0)
        .count();

    SocketSelectionPlan {
        generation: compiled_state.generation.clone(),
        previous_generation: previous_state
            .map(|state| state.generation.clone())
            .filter(|generation| generation != &compiled_state.generation),
        compiled_at: compiled_state.compiled_at.clone(),
        observed_at: unix_timestamp_string(),
        entries,
        summary: SocketSelectionPlanSummary {
            listener_count,
            node_local_listener_count,
            cross_node_handoff_listener_count,
            random_listener_count,
            maglev_listener_count,
            deferred_hash_listener_count,
            unsupported_policy_listener_count,
            affinity_listener_count,
            client_ip_affinity_listener_count,
            deferred_affinity_listener_count,
            unsupported_affinity_listener_count,
            total_backend_candidates,
            total_local_candidates,
            total_remote_candidates,
            listeners_with_no_backends,
            shadow_apply_only: true,
        },
        shadow_apply_only: true,
    }
}

