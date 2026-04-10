# Requirements Document

## Introduction

This feature adds client_ip session affinity (sticky sessions) to the L4 Load Balancer. When a service has session affinity enabled, the eBPF datapath remembers which backend was selected for a given client IP and reuses that selection on subsequent packets. This is Phase C Step 3 of the LB implementation roadmap.

The affinity state lives entirely in the eBPF `SVC_AFFINITY_MAP` (LruHashMap, 65536 entries). The agent sets the `SVC_FRONTEND_FLAG_HAS_AFFINITY` flag on frontend entries; the eBPF program populates and queries the affinity map at runtime. Old entries are automatically evicted by LRU when the map is full.

## Glossary

- **LB_Datapath**: The eBPF TC ingress/egress pipeline in `ebpf/src/lb.rs` that performs L4 load balancing (frontend lookup, backend selection, DNAT, RevNat).
- **Affinity_Lookup**: The `#[inline(always)]` helper in `ebpf/src/lb.rs` that queries `SVC_AFFINITY_MAP` using `SvcAffinityKey` (tap_id + service_id + client_address) and returns the previously selected backend slot if valid.
- **Affinity_Write**: The `#[inline(always)]` helper in `ebpf/src/lb.rs` that inserts or updates an entry in `SVC_AFFINITY_MAP` after a backend is selected, recording the chosen slot and current timestamp.
- **Agent_Materializer**: The `materialize_service_maps` function in `agent/src/platform_agent.rs` that writes compiled service state into pinned eBPF maps.
- **SVC_AFFINITY_MAP**: The `LruHashMap<SvcAffinityKey, SvcAffinityValue>` with 65536 entries, already defined in `ebpf/src/maps.rs`.
- **SvcAffinityKey**: The 24-byte `repr(C)` struct containing `tap_id` (u32), `service_id` (u32), and `client_address` ([u8; 16]), already defined in `ebpf/src/common.rs` and `core/src/common.rs`.
- **SvcAffinityValue**: The 16-byte (aligned) `repr(C)` struct containing `backend_slot` (u16), `pad` ([u8; 2]), and `last_used_ns` (u64), already defined in `ebpf/src/common.rs` and `core/src/common.rs`.
- **SVC_FRONTEND_FLAG_HAS_AFFINITY**: Bit 0 of `SvcFrontendValue.flags`, already defined as constant `1 << 0` in both `ebpf/src/common.rs` and `core/src/common.rs`.
- **Frontend_Value**: The `SvcFrontendValue` struct returned by `SVC_FRONTEND_MAP` lookup, containing `service_id`, `backend_count`, `flags`, and `lb_algo`.
- **PacketInfo**: The parsed packet metadata struct in `ebpf/src/parser.rs`, containing `src_ip` (u32 for IPv4), `src_ip_v6` ([u8; 16] for IPv6), and other fields.
- **Stale_Slot**: A backend slot recorded in the affinity map that is no longer valid because `backend_slot >= frontend.backend_count` (e.g., after a backend was removed and slots were compacted).

## Requirements

### Requirement 1: eBPF Affinity Lookup on Ingress

**User Story:** As a network operator, I want the LB datapath to check for an existing affinity binding before selecting a backend, so that returning clients are directed to the same backend they previously used.

#### Acceptance Criteria

1. WHEN a packet matches a frontend entry with `SVC_FRONTEND_FLAG_HAS_AFFINITY` set, THE LB_Datapath SHALL construct an `SvcAffinityKey` from the packet's `tap_id`, the frontend's `service_id`, and the packet's source IP address (v4-mapped-v6 for IPv4, raw 16 bytes for IPv6).
2. WHEN the affinity key is constructed, THE Affinity_Lookup SHALL query `SVC_AFFINITY_MAP` and return the stored `SvcAffinityValue` if present.
3. WHEN the Affinity_Lookup returns a hit and the stored `backend_slot` is less than `frontend.backend_count`, THE LB_Datapath SHALL use the stored slot to look up the backend in `SVC_BACKEND_MAP`, bypassing random selection.
4. WHEN the Affinity_Lookup returns a hit but the stored `backend_slot` is greater than or equal to `frontend.backend_count` (Stale_Slot), THE LB_Datapath SHALL discard the stale binding and fall through to normal random backend selection.
5. WHEN the Affinity_Lookup returns a miss (no entry in `SVC_AFFINITY_MAP`), THE LB_Datapath SHALL proceed with normal random backend selection.
6. WHEN a frontend entry does not have `SVC_FRONTEND_FLAG_HAS_AFFINITY` set, THE LB_Datapath SHALL skip the affinity lookup entirely and proceed directly to random backend selection.

### Requirement 2: eBPF Affinity Write After Backend Selection

**User Story:** As a network operator, I want the LB datapath to record the selected backend for affinity-enabled services, so that future packets from the same client reuse the same backend.

#### Acceptance Criteria

1. WHEN a backend is successfully selected for a frontend with `SVC_FRONTEND_FLAG_HAS_AFFINITY` set and the selection came from random selection (affinity miss or stale slot), THE Affinity_Write SHALL insert an entry into `SVC_AFFINITY_MAP` with the affinity key, the selected `backend_slot`, and the current `bpf_ktime_get_ns()` timestamp.
2. WHEN a backend is successfully selected via an affinity hit (existing valid binding), THE Affinity_Write SHALL update the `last_used_ns` field of the existing `SVC_AFFINITY_MAP` entry to the current `bpf_ktime_get_ns()` timestamp.
3. WHEN `SVC_AFFINITY_MAP` is full, THE SVC_AFFINITY_MAP SHALL automatically evict the least-recently-used entry to make room for the new entry (LRU semantics provided by the map type).

### Requirement 3: Agent Sets Affinity Flag on Frontend Entries

**User Story:** As a platform operator, I want the agent to set the affinity flag on frontend map entries when a service has `session_affinity = "client_ip"`, so that the eBPF datapath knows to perform affinity lookups.

#### Acceptance Criteria

1. WHEN a `ServiceProgramIr` has `frontend.session_affinity` equal to `"client_ip"`, THE Agent_Materializer SHALL set `SVC_FRONTEND_FLAG_HAS_AFFINITY` (bit 0) in the `flags` field of every `SvcFrontendEntry` written for that service's listener ports.
2. WHEN a `ServiceProgramIr` has `frontend.session_affinity` equal to `"none"` or is absent, THE Agent_Materializer SHALL NOT set `SVC_FRONTEND_FLAG_HAS_AFFINITY` in the `flags` field.
3. THE Agent_Materializer SHALL NOT write any entries to `SVC_AFFINITY_MAP`; the affinity map is populated exclusively by the eBPF datapath at runtime.

### Requirement 4: IPv4 and IPv6 Affinity Key Construction

**User Story:** As a network operator, I want session affinity to work for both IPv4 and IPv6 traffic, so that all clients get consistent backend selection regardless of IP version.

#### Acceptance Criteria

1. WHEN the packet is IPv4, THE LB_Datapath SHALL construct the `client_address` field of `SvcAffinityKey` using the v4-mapped-v6 representation of the source IPv4 address (identical to the `ipv4_to_v4mapped_raw` helper already used for frontend key construction).
2. WHEN the packet is IPv6, THE LB_Datapath SHALL construct the `client_address` field of `SvcAffinityKey` using the raw 16-byte `src_ip_v6` from `PacketInfo`.
3. FOR ALL valid `SvcAffinityKey` values, constructing the key from a packet and then looking it up in `SVC_AFFINITY_MAP` SHALL produce a hit if and only if a previous packet from the same client IP to the same service on the same tap wrote an entry that has not been evicted.

### Requirement 5: eBPF Stack and Inline Constraints

**User Story:** As a platform engineer, I want the affinity implementation to comply with eBPF verifier constraints, so that the program loads and runs correctly on all supported kernels.

#### Acceptance Criteria

1. THE Affinity_Lookup helper SHALL be annotated with `#[inline(always)]` and SHALL NOT introduce any local variable larger than 64 bytes.
2. THE Affinity_Write helper SHALL be annotated with `#[inline(always)]` and SHALL NOT introduce any local variable larger than 64 bytes.
3. THE `SvcAffinityKey` stack allocation (24 bytes) combined with the `SvcAffinityValue` stack allocation (16 bytes) SHALL keep the total additional stack usage of the affinity path below 64 bytes.
4. THE affinity lookup and write logic SHALL NOT increase the call depth of the `phase_lb_ingress_v4` or `phase_lb_ingress_v6` functions beyond the project soft limit of 5 levels from program entry.

### Requirement 6: Affinity Interaction with Backend Selection

**User Story:** As a network operator, I want affinity to correctly interact with backend changes, so that stale bindings do not cause traffic to be sent to non-existent backends.

#### Acceptance Criteria

1. WHEN the agent recompiles service state and the backend count decreases (backends removed, slots compacted), THE LB_Datapath SHALL detect stale affinity entries at lookup time by comparing `backend_slot >= frontend.backend_count` and fall through to random selection.
2. WHEN a stale affinity entry is detected and a new backend is selected via random selection, THE Affinity_Write SHALL overwrite the stale entry in `SVC_AFFINITY_MAP` with the newly selected slot.
3. WHEN the agent recompiles service state and changes `session_affinity` from `"client_ip"` to `"none"`, THE Agent_Materializer SHALL clear the `SVC_FRONTEND_FLAG_HAS_AFFINITY` flag, causing the eBPF datapath to stop performing affinity lookups; existing affinity map entries for that service SHALL be left to expire via LRU eviction.
