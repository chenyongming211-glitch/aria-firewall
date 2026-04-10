# Implementation Plan: L4 LB Phase B Datapath Implementation

## Overview

Implement the remaining Phase B steps (B3–B6) for node-local L4 service load balancing. Phase B Steps 1–2 (map schema structs in `ebpf/src/common.rs` + `core/src/common.rs`, and `svc_ops` write/clear in `core/src/svc_ops.rs`) are already complete. This plan covers: eBPF LB module (`lb.rs`), TC pipeline integration, agent materialization of compiled service state into pinned maps, and real apply-status reporting.

All code is Rust. eBPF code uses aya-rs. No local compilation — CI validates via GitHub Actions.

## Tasks

- [ ] 1. Create eBPF LB module with frontend lookup helpers
  - [ ] 1.1 Create `ebpf/src/lb.rs` with module boilerplate and add `mod lb` to `ebpf/src/lib.rs`
    - Add necessary imports from `common`, `maps`, `parser`
    - Define `FLAG_LB_HIT: u16 = 1 << 8` in `ebpf/src/common.rs` (next available PipelineCtx flag bit)
    - _Requirements: 5.1, 5.5_
  - [ ] 1.2 Implement `svc_frontend_lookup_v4` inline helper in `lb.rs`
    - Build `SvcFrontendKey` from `(tap_id, ipv4_to_v4mapped(dst_ip), dst_port, proto, scope=0)`
    - Lookup `SVC_FRONTEND_MAP`, return `Option<&SvcFrontendValue>`
    - Annotate `#[inline(always)]`, keep stack under 64 bytes
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5_
  - [ ] 1.3 Implement `svc_frontend_lookup_v6` inline helper in `lb.rs`
    - Build `SvcFrontendKey` from `(tap_id, dst_ip_v6, dst_port, proto, scope=0)`
    - Same lookup pattern as v4 variant
    - Annotate `#[inline(always)]`, keep stack under 64 bytes
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5_
  - [ ] 1.4 Implement `svc_backend_select` inline helper in `lb.rs`
    - Compute slot as `bpf_get_prandom_u32() % backend_count`
    - Build `SvcBackendKey` from `(tap_id, service_id, slot)`
    - Lookup `SVC_BACKEND_MAP`, return `Option<&SvcBackendValue>`
    - Guard: if `lb_algo != SVC_LB_ALGO_RANDOM`, return `None` (Phase B scope guard)
    - Guard: if `backend_count == 0`, return `None`
    - Annotate `#[inline(always)]`, keep stack under 64 bytes
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 10.6_

- [ ] 2. Implement eBPF DNAT and RevNat rewrite functions
  - [ ] 2.1 Implement `svc_dnat_v4` rewrite function in `lb.rs`
    - Rewrite IPv4 dst IP (extract bytes `[12..16]` from V4-mapped-v6 backend address) and dst port
    - Use `bpf_l3_csum_replace` for IPv4 header checksum update
    - Use `bpf_l4_csum_replace` for TCP/UDP checksum update
    - Guard: if backend does not have `SVC_BACKEND_FLAG_LOCAL`, skip rewrite and return
    - Annotate `#[inline(never)]` to isolate stack frame, keep under 256 bytes
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6_
  - [ ] 2.2 Implement `svc_dnat_v6` rewrite function in `lb.rs`
    - Rewrite IPv6 dst IP and dst port
    - Use incremental L4 checksum update (IPv6 has no header checksum)
    - Same `FLAG_LOCAL` guard and `#[inline(never)]` annotation
    - _Requirements: 3.1, 3.3, 3.4, 3.5, 3.6_
  - [ ] 2.3 Implement `svc_revnat_lookup_v4` and `svc_snat_v4` in `lb.rs`
    - `svc_revnat_lookup_v4`: build `SvcRevNatKey` from `(tap_id, src_ip_v4mapped, src_port, proto)`, lookup `SVC_REVNAT_MAP`
    - `svc_snat_v4`: rewrite IPv4 src IP and src port from `SvcRevNatValue`, incremental checksum
    - `svc_revnat_lookup_v4` is `#[inline(always)]`; `svc_snat_v4` is `#[inline(never)]`
    - _Requirements: 4.1, 4.2, 4.3, 4.4_
  - [ ] 2.4 Implement `svc_revnat_lookup_v6` and `svc_snat_v6` in `lb.rs`
    - Same pattern as v4 but for IPv6 source fields
    - _Requirements: 4.1, 4.2, 4.3, 4.4_

- [ ] 3. Implement eBPF LB phase entry points and pipeline integration
  - [ ] 3.1 Implement `phase_lb_ingress_v4` in `lb.rs`
    - Annotate `#[inline(never)]`
    - Call `svc_frontend_lookup_v4` → if miss, return early
    - Call `svc_backend_select` → if miss or `backend_count == 0`, return early
    - Call `svc_dnat_v4` → if successful, set `FLAG_LB_HIT` on `PipelineCtx.flags`
    - _Requirements: 1.1, 1.2, 2.1, 2.3, 2.4, 3.1, 5.3, 5.6_
  - [ ] 3.2 Implement `phase_lb_ingress_v6` in `lb.rs`
    - Same flow as v4 using v6 variants of each helper
    - _Requirements: 1.1, 1.2, 2.1, 2.3, 2.4, 3.1, 5.3, 5.6_
  - [ ] 3.3 Implement `phase_lb_egress_v4` in `lb.rs`
    - Annotate `#[inline(never)]`
    - Call `svc_revnat_lookup_v4` → if miss, return early
    - Call `svc_snat_v4` to rewrite source
    - _Requirements: 4.1, 4.3, 5.7_
  - [ ] 3.4 Implement `phase_lb_egress_v6` in `lb.rs`
    - Same flow as v4 using v6 variants
    - _Requirements: 4.1, 4.3, 5.7_
  - [ ] 3.5 Integrate LB ingress into `try_tc_ingress` in `ebpf/src/lib.rs`
    - Insert LB phase call after `load_feature_flags_tc` and before CT key construction
    - Guard on `info.proto == IPPROTO_TCP || info.proto == IPPROTO_UDP`
    - Dispatch to `lb::phase_lb_ingress_v4` or `lb::phase_lb_ingress_v6` based on `info.is_ipv6`
    - _Requirements: 5.6_
  - [ ] 3.6 Integrate LB egress into `try_tc_egress` in `ebpf/src/lib.rs`
    - Insert RevNat phase call after `load_feature_flags_tc` and before CT key construction
    - Guard on `info.proto == IPPROTO_TCP || info.proto == IPPROTO_UDP`
    - Dispatch to `lb::phase_lb_egress_v4` or `lb::phase_lb_egress_v6` based on `info.is_ipv6`
    - _Requirements: 5.7_

- [ ] 4. Checkpoint — Review eBPF module
  - Ensure all eBPF code compiles cleanly in CI, review call depth budget, verify no Phase C features leaked in.
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 5. Implement agent service map materialization
  - [ ] 5.1 Implement `materialize_service_maps` function in `agent/src/platform_agent.rs`
    - Accept `pin_path`, `tap_id`, and `&[ServiceProgramIr]` (filtered to `shadow_apply_only == false`)
    - Call `clear_service_maps_for_tap(pin_path, tap_id)` first
    - Allocate `service_id` counter starting from 1, same ID for all listener ports of the same service
    - Return a result struct with counts of written frontends, backends, revnats, and any errors
    - _Requirements: 6.1, 6.6, 6.7, 7.1, 7.2, 7.3_
  - [ ] 5.2 Implement frontend entry building within `materialize_service_maps`
    - For each `ServiceProgramIr`, for each listener port: build `SvcFrontendEntry`
    - Encode VIP address using `ipv4_to_v4mapped` for IPv4
    - Set `backend_count` to the number of local, non-disabled backends actually written
    - Set `lb_algo = SVC_LB_ALGO_RANDOM`, `scope = 0`
    - Call `write_service_frontends`
    - _Requirements: 6.1, 6.2, 6.4, 8.1_
  - [ ] 5.3 Implement backend entry building within `materialize_service_maps`
    - Filter backends: only `admin_state != "disabled"` AND `resolved_locality == "local"`
    - Build compact 0-based slot array (no holes)
    - Encode backend address using `ipv4_to_v4mapped` for IPv4
    - Set `SVC_BACKEND_FLAG_LOCAL` on each backend entry
    - Call `write_service_backends`
    - _Requirements: 6.3, 6.4, 8.2, 10.5_
  - [ ] 5.4 Implement RevNat entry building within `materialize_service_maps`
    - For each unique `(backend_ip, backend_port, protocol)` across written backends: build `SvcRevNatEntry`
    - Map back to the service VIP and service port
    - Encode addresses using `ipv4_to_v4mapped` for IPv4
    - Call `write_service_revnats`
    - _Requirements: 6.5, 8.3_
  - [ ] 5.5 Wire `materialize_service_maps` into the agent reconcile loop
    - Call `materialize_service_maps` during the apply phase when `service_programs` are present and `shadow_apply_only == false`
    - Iterate over affected tap_ids and call materialization per tap
    - Log errors but do not crash the agent loop
    - _Requirements: 6.1, 6.6, 6.7_
  - [ ]* 5.6 Write property test: IPv4 V4-Mapped-V6 Round-Trip
    - **Property 1: IPv4 V4-Mapped-V6 Round-Trip**
    - Use `proptest` to generate arbitrary `Ipv4Addr`, verify `ipv4_to_v4mapped` encoding produces correct prefix and extracting bytes `[12..16]` recovers original octets
    - Add test in `core/src/svc_ops.rs` tests module
    - **Validates: Requirements 1.3, 3.2, 8.1, 8.2, 8.3**
  - [ ]* 5.7 Write property test: Service ID Allocation Is Sequential and Consistent
    - **Property 5: Service ID Allocation Is Sequential and Consistent**
    - Use `proptest` to generate random lists of service programs with varying listener port counts
    - Verify service_id values are sequential from 1 and all listener ports of the same service share the same service_id
    - Add test in `agent/src/platform_agent.rs` tests module
    - **Validates: Requirements 6.2, 7.1, 7.2**
  - [ ]* 5.8 Write property test: Frontend backend_count Matches Written Backend Slots
    - **Property 6: Frontend backend_count Matches Written Backend Slots**
    - Use `proptest` to generate services with mixed backend states (enabled/disabled, local/remote)
    - Verify `backend_count` in each frontend entry equals the number of backend entries actually produced for that service_id
    - Add test in `agent/src/platform_agent.rs` tests module
    - **Validates: Requirements 6.3, 6.4**
  - [ ]* 5.9 Write property test: Backend Slot Compaction Invariant
    - **Property 7: Backend Slot Compaction Invariant**
    - Use `proptest` to generate backend sets with mixed locality and admin_state
    - Verify only local, non-disabled backends are included and slot indices form a compact 0-based sequence `[0, 1, ..., n-1]`
    - Add test in `agent/src/platform_agent.rs` tests module
    - **Validates: Requirements 6.3, 10.5**
  - [ ]* 5.10 Write property test: RevNat Entry Completeness
    - **Property 8: RevNat Entry Completeness**
    - Use `proptest` to generate backend sets, verify exactly one `SvcRevNatEntry` per unique `(backend_ip, backend_port, protocol)` tuple
    - Add test in `agent/src/platform_agent.rs` tests module
    - **Validates: Requirements 6.5**

- [ ] 6. Checkpoint — Review agent materialization
  - Ensure all agent code compiles cleanly in CI, review materialization logic, verify no Phase C map writes.
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 7. Implement apply-status reporting
  - [ ] 7.1 Modify `build_runtime_execution_summary` in `agent/src/platform_agent.rs` to report real materialization status
    - When `materialize_service_maps` succeeds: set services domain `execution_status` to `"applied"` and `shadow_apply_only` to `false`
    - When it fails: set `execution_status` to `"failed"` with failure details
    - When no `service_programs` present: set `execution_status` to `"applied"` with zero compiled objects
    - Replace the current shadow-only warning messages for the services domain with real status
    - _Requirements: 9.1, 9.2, 9.3, 9.4_
  - [ ] 7.2 Update `ServiceRuntimeExecutionSummary` to carry materialization result
    - Add fields or adjust existing fields to convey materialization success/failure counts
    - Set `shadow_apply_only` to `false` when real materialization has been performed
    - _Requirements: 9.1, 9.4_

- [ ] 8. Verification and scope guard enforcement
  - [ ] 8.1 Add Phase B scope guard assertions in `lb.rs`
    - Ensure `lb_algo != SVC_LB_ALGO_RANDOM` causes pass-through (no crash, no selection)
    - Ensure no references to `SVC_AFFINITY_MAP` or `SVC_MAGLEV_MAP` in `lb.rs`
    - Ensure no tunnel encapsulation or cross-node forwarding logic
    - _Requirements: 10.1, 10.2, 10.4, 10.6_
  - [ ] 8.2 Add Phase B scope guard in agent materialization
    - Ensure no writes to `SVC_AFFINITY_MAP` or `SVC_MAGLEV_MAP`
    - Ensure only `resolved_locality == "local"` backends are written to `SVC_BACKEND_MAP`
    - _Requirements: 10.3, 10.5_
  - [ ]* 8.3 Write property test: Random Backend Slot Is Always In Range
    - **Property 2: Random Backend Slot Is Always In Range**
    - Use `proptest` to generate `(backend_count: 1..65535u16, random_value: u32)`, verify `random_value % (backend_count as u32) < backend_count as u32`
    - Add test in `core/src/svc_ops.rs` tests module (pure arithmetic property)
    - **Validates: Requirements 2.1**
  - [ ]* 8.4 Write property test: DNAT + RevNat Round-Trip Preserves VIP Identity
    - **Property 4: DNAT + RevNat Round-Trip Preserves VIP Identity**
    - Use `proptest` to generate `(vip_addr, vip_port, backend_addr, backend_port, protocol)` tuples
    - Simulate DNAT rewrite (dst → backend) then RevNat rewrite (src → vip), verify VIP identity is recovered
    - Add test in `core/src/svc_ops.rs` tests module
    - **Validates: Requirements 3.1, 4.1**

- [ ] 9. Final checkpoint — Full CI validation
  - Push all changes, verify GitHub Actions CI passes for ebpf, core, and agent crates.
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation via CI
- Property tests use the `proptest` crate and validate universal correctness properties from the design document
- No local `cargo build` / `cargo check` — all compilation is validated through GitHub Actions CI
- Phase B scope is strictly bounded: no affinity, no maglev, no cross-node handoff
