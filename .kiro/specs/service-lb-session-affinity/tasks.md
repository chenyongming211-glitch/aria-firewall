# Implementation Plan: L4 LB Session Affinity (client_ip)

## Overview

Small, focused change: add two `#[inline(always)]` helpers to `ebpf/src/lb.rs` (~30 lines), integrate them into the existing `phase_lb_ingress_v4/v6` functions, and set the affinity flag in `agent/src/platform_agent.rs` (~5 lines). All infrastructure (map, structs, constants) already exists.

## Tasks

- [ ] 1. Add `affinity_lookup` and `affinity_write` helpers to `ebpf/src/lb.rs`
  - [ ] 1.1 Implement `affinity_lookup` helper
    - Add `#[inline(always)] unsafe fn affinity_lookup(tap_id: u32, service_id: u32, client_address: [u8; 16]) -> Option<u16>`
    - Construct `SvcAffinityKey { tap_id, service_id, client_address }` on the stack
    - Query `SVC_AFFINITY_MAP.get(&key)` and return `Some(val.backend_slot)` on hit, `None` on miss
    - _Requirements: 1.1, 1.2, 4.1, 4.2, 5.1, 5.3_

  - [ ] 1.2 Implement `affinity_write` helper
    - Add `#[inline(always)] unsafe fn affinity_write(tap_id: u32, service_id: u32, client_address: [u8; 16], backend_slot: u16)`
    - Construct `SvcAffinityKey` (24 bytes) and `SvcAffinityValue { backend_slot, pad: [0; 2], last_used_ns: bpf_ktime_get_ns() }` (16 bytes)
    - Insert into `SVC_AFFINITY_MAP` via `map.insert(&key, &val, 0)`
    - _Requirements: 2.1, 2.2, 2.3, 5.2, 5.3_

  - [ ] 1.3 Integrate affinity into `phase_lb_ingress_v4`
    - After `svc_frontend_lookup_v4` succeeds, check `frontend.flags & SVC_FRONTEND_FLAG_HAS_AFFINITY`
    - If set: call `affinity_lookup` with `(tap_id, frontend.service_id, ipv4_to_v4mapped_raw(info.src_ip))`
    - If affinity hit and `slot < frontend.backend_count`: use stored slot to look up `SVC_BACKEND_MAP` directly
    - If affinity miss or stale slot (`slot >= backend_count`): fall through to existing `svc_backend_select`
    - After backend resolved: call `affinity_write` with the selected slot
    - If no affinity flag: skip affinity entirely, use existing `svc_backend_select` path unchanged
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 4.1, 5.4, 6.1, 6.2_

  - [ ] 1.4 Integrate affinity into `phase_lb_ingress_v6`
    - Same logic as v4 but use `info.src_ip_v6` directly (raw 16 bytes) for `client_address`
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 4.2, 5.4, 6.1, 6.2_

- [ ] 2. Set affinity flag in agent `materialize_service_maps`
  - [ ] 2.1 Modify frontend entry construction in `agent/src/platform_agent.rs`
    - In the `materialize_service_maps` function, import `SVC_FRONTEND_FLAG_HAS_AFFINITY` from `aria_core::common`
    - In the frontend entry construction loop, change `flags: SVC_FRONTEND_FLAG_LOCAL_ONLY` to conditionally OR in `SVC_FRONTEND_FLAG_HAS_AFFINITY` when `program.frontend.session_affinity.as_deref() == Some("client_ip")`
    - When `session_affinity` is `"none"`, absent, or any other value: do NOT set the affinity flag
    - _Requirements: 3.1, 3.2, 3.3, 6.3_

- [ ] 3. Checkpoint — Verify CI passes
  - Commit and push changes. Ensure GitHub Actions CI compiles the eBPF program and agent successfully.
  - Ensure the eBPF verifier accepts the modified program (CI smoke test).
  - Ask the user if questions arise.

- [ ] 4. Property-based tests for affinity logic
  - [ ]* 4.1 Write property test: affinity key construction (v4-mapped / v6)
    - **Property 1: Affinity key construction produces correct v4-mapped/v6 client address**
    - Generate random `(tap_id: u32, service_id: u32, src_ip: Ipv4Addr | Ipv6Addr)`, construct `SvcAffinityKey`, assert `client_address` matches expected encoding
    - **Validates: Requirements 1.1, 4.1, 4.2**

  - [ ]* 4.2 Write property test: affinity slot validity decision
    - **Property 2: Affinity slot validity decision is correct**
    - Generate random `(backend_slot: u16, backend_count: u16)` where `backend_count > 0`, assert slot is used iff `backend_slot < backend_count`
    - **Validates: Requirements 1.3, 1.4, 6.1**

  - [ ]* 4.3 Write property test: agent affinity flag iff session_affinity is client_ip
    - **Property 4: Agent sets affinity flag if and only if session_affinity is client_ip**
    - Generate random `ServiceProgramIr` with `session_affinity` ∈ `{Some("client_ip"), Some("none"), None}`, assert flag is set iff `session_affinity == Some("client_ip")`
    - **Validates: Requirements 3.1, 3.2, 6.3**

  - [ ]* 4.4 Write property test: affinity key determinism
    - **Property 5: Affinity key construction is deterministic (same inputs → same key bytes)**
    - Generate random `(tap_id, service_id, client_ip)`, construct key twice, assert byte equality
    - **Validates: Requirements 4.3**

- [ ] 5. Final checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- No local compilation — CI validates via GitHub Actions (per project rules)
- All infrastructure (map, structs, constants) already exists; no new data structures needed
- Property 3 (affinity write records correct slot) is omitted from tasks because it requires a mock eBPF map harness that doesn't exist in this project
- The egress path (`phase_lb_egress_v4/v6`) is unchanged
