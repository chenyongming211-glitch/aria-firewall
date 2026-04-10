# Requirements Document

## Introduction

This feature implements Maglev consistent hashing for the L4 Load Balancer. Maglev is Google's consistent hashing algorithm that builds a fixed-size lookup table (251 entries, a prime number) where each slot maps to a backend. When a packet arrives, its 5-tuple hash modulo the table size yields a slot, which maps to a backend. When backends change, only a minimal number of flows are remapped — the core consistent hashing property.

The implementation is split between agent (user-space) and eBPF (datapath):
- The agent computes the Maglev permutation table from the backend list and writes it to `SVC_MAGLEV_MAP` (a HashMap keyed by `(tap_id, service_id, table_index)`)
- The eBPF datapath hashes the packet 5-tuple, looks up `SVC_MAGLEV_MAP[hash % MAGLEV_TABLE_SIZE]` to get a `backend_slot`, then looks up `SVC_BACKEND_MAP` as usual

This is Phase C Step 4 of the LB implementation roadmap. Phase B (random LB with DNAT/RevNat) and Phase C3 (client_ip session affinity) are already complete.

## Glossary

- **LB_Datapath**: The eBPF TC ingress/egress pipeline in `ebpf/src/lb.rs` that performs L4 load balancing (frontend lookup, backend selection, DNAT, RevNat).
- **Maglev_Table_Generator**: The pure function in the agent (user-space) that takes a list of backend identifiers and produces a `MAGLEV_TABLE_SIZE`-length array where each entry is a `backend_slot` index into `SVC_BACKEND_MAP`.
- **Agent_Materializer**: The `materialize_service_maps` function in `agent/src/platform_agent.rs` that writes compiled service state into pinned eBPF maps.
- **SVC_MAGLEV_MAP**: A `HashMap<SvcMaglevKey, SvcMaglevEntry>` eBPF map that stores per-service Maglev lookup tables. Keyed by `(tap_id, service_id, table_index)`.
- **SvcMaglevKey**: A `repr(C)` struct containing `tap_id` (u32), `service_id` (u32), and `table_index` (u16) plus padding, used as the key for `SVC_MAGLEV_MAP`.
- **SvcMaglevEntry**: A `repr(C)` struct containing `backend_slot` (u16) and padding, used as the value for `SVC_MAGLEV_MAP`. Already defined in RFC-008A.
- **MAGLEV_TABLE_SIZE**: The fixed size of the Maglev lookup table, set to 251 (a prime number, Cilium's default for small deployments).
- **SVC_LB_ALGO_MAGLEV**: The constant value 1 for the `lb_algo` field in `SvcFrontendValue`, already defined in `ebpf/src/common.rs`.
- **SVC_FRONTEND_FLAG_USE_MAGLEV**: Bit 1 of `SvcFrontendValue.flags`, already defined in `ebpf/src/common.rs`.
- **Five_Tuple_Hash**: A hash computed from `(src_ip, dst_ip, src_port, dst_port, proto)` using a fast, non-cryptographic hash function suitable for eBPF (e.g., jhash or a simple xor-shift-rotate combination).
- **Permutation_Table**: The intermediate per-backend `(offset, skip)` pair derived from hashing the backend identifier, used by the Maglev algorithm to fill the lookup table.
- **Consistent_Hashing_Property**: When a backend is added or removed, only approximately `1/N` of the table entries change (where N is the number of backends), preserving existing flow-to-backend mappings.
- **PacketInfo**: The parsed packet metadata struct in `ebpf/src/parser.rs`, containing `src_ip`, `dst_ip`, `src_port`, `dst_port`, `proto`, `src_ip_v6`, `dst_ip_v6`.
- **Frontend_Value**: The `SvcFrontendValue` struct returned by `SVC_FRONTEND_MAP` lookup, containing `service_id`, `backend_count`, `flags`, and `lb_algo`.

## Requirements

### Requirement 1: Agent Computes Maglev Permutation Table

**User Story:** As a platform engineer, I want the agent to compute a Maglev consistent hashing lookup table from the backend list, so that the eBPF datapath can perform O(1) backend selection with minimal flow disruption on backend changes.

#### Acceptance Criteria

1. WHEN a `ServiceProgramIr` has `lb_policy` equal to `"maglev"`, THE Maglev_Table_Generator SHALL compute a lookup table of exactly `MAGLEV_TABLE_SIZE` (251) entries, where each entry contains a valid `backend_slot` in the range `[0, backend_count)`.
2. THE Maglev_Table_Generator SHALL derive per-backend `(offset, skip)` permutation pairs by hashing each backend's stable identifier (the combination of backend IP address and port), where `offset = hash1 % MAGLEV_TABLE_SIZE` and `skip = (hash2 % (MAGLEV_TABLE_SIZE - 1)) + 1`.
3. THE Maglev_Table_Generator SHALL fill the lookup table using the standard Maglev algorithm: iterating through backends in round-robin order, each backend claims the next unclaimed slot in its permutation sequence, until all 251 slots are filled.
4. WHEN the backend list is empty, THE Maglev_Table_Generator SHALL produce an empty table and the Agent_Materializer SHALL NOT write any entries to `SVC_MAGLEV_MAP` for that service.
5. WHEN the backend list contains exactly one backend, THE Maglev_Table_Generator SHALL produce a table where all 251 entries map to `backend_slot` 0.

### Requirement 2: Agent Writes Maglev Table to eBPF Map

**User Story:** As a platform engineer, I want the agent to write the computed Maglev table into the `SVC_MAGLEV_MAP` eBPF map, so that the eBPF datapath can look up backend assignments at runtime.

#### Acceptance Criteria

1. WHEN a service has `lb_policy` equal to `"maglev"` and `backend_count > 0`, THE Agent_Materializer SHALL write exactly `MAGLEV_TABLE_SIZE` (251) entries to `SVC_MAGLEV_MAP`, one for each `table_index` in `[0, 251)`, with the key `SvcMaglevKey { tap_id, service_id, table_index }` and value `SvcMaglevEntry { backend_slot }`.
2. WHEN a service has `lb_policy` equal to `"maglev"`, THE Agent_Materializer SHALL set `lb_algo` to `SVC_LB_ALGO_MAGLEV` (value 1) in every `SvcFrontendEntry` written for that service's listener ports.
3. WHEN a service has `lb_policy` equal to `"maglev"`, THE Agent_Materializer SHALL set `SVC_FRONTEND_FLAG_USE_MAGLEV` (bit 1) in the `flags` field of every `SvcFrontendEntry` written for that service's listener ports.
4. WHEN the agent recompiles service state and a service's backend list changes, THE Agent_Materializer SHALL recompute the Maglev table and overwrite all 251 entries in `SVC_MAGLEV_MAP` for that service.
5. WHEN the agent recompiles service state and a service is removed or changes from `"maglev"` to another `lb_policy`, THE Agent_Materializer SHALL remove all `SVC_MAGLEV_MAP` entries for that service's `(tap_id, service_id)`.

### Requirement 3: eBPF Datapath Maglev Backend Selection

**User Story:** As a network operator, I want the eBPF datapath to use the Maglev lookup table for backend selection when a service is configured with Maglev, so that traffic is distributed consistently across backends.

#### Acceptance Criteria

1. WHEN a packet matches a frontend entry with `lb_algo` equal to `SVC_LB_ALGO_MAGLEV`, THE LB_Datapath SHALL compute a Five_Tuple_Hash from the packet's `(src_ip, dst_ip, src_port, dst_port, proto)`.
2. WHEN the Five_Tuple_Hash is computed, THE LB_Datapath SHALL compute `table_index = hash % MAGLEV_TABLE_SIZE` and look up `SVC_MAGLEV_MAP` with key `SvcMaglevKey { tap_id, service_id, table_index }`.
3. WHEN the `SVC_MAGLEV_MAP` lookup succeeds, THE LB_Datapath SHALL use the returned `backend_slot` to look up `SVC_BACKEND_MAP` with key `SvcBackendKey { tap_id, service_id, backend_slot }`.
4. WHEN the `SVC_MAGLEV_MAP` lookup fails (entry not found), THE LB_Datapath SHALL fall back to random backend selection using `bpf_get_prandom_u32() % backend_count`.
5. WHEN the `SVC_BACKEND_MAP` lookup fails for the Maglev-provided slot, THE LB_Datapath SHALL fall back to random backend selection.
6. WHEN a frontend entry has `lb_algo` equal to `SVC_LB_ALGO_RANDOM`, THE LB_Datapath SHALL continue using the existing random selection path and SHALL NOT query `SVC_MAGLEV_MAP`.

### Requirement 4: Maglev Interacts Correctly with Session Affinity

**User Story:** As a network operator, I want Maglev and session affinity to work together, so that affinity-enabled Maglev services use cached bindings when available and fall back to Maglev selection otherwise.

#### Acceptance Criteria

1. WHEN a frontend has both `SVC_FRONTEND_FLAG_HAS_AFFINITY` and `lb_algo` equal to `SVC_LB_ALGO_MAGLEV`, THE LB_Datapath SHALL first check `SVC_AFFINITY_MAP` for an existing binding.
2. WHEN the affinity lookup returns a hit with a valid slot (`backend_slot < backend_count`), THE LB_Datapath SHALL use the affinity-provided slot, bypassing the Maglev table lookup.
3. WHEN the affinity lookup returns a miss or a stale slot, THE LB_Datapath SHALL fall through to Maglev-based backend selection (Five_Tuple_Hash → `SVC_MAGLEV_MAP` → `SVC_BACKEND_MAP`).
4. WHEN a backend is selected via Maglev (affinity miss or stale), THE LB_Datapath SHALL write the selected slot to `SVC_AFFINITY_MAP` so that subsequent packets from the same client reuse the same backend.

### Requirement 5: Five-Tuple Hash Function for eBPF

**User Story:** As a platform engineer, I want a fast, deterministic hash function for 5-tuple hashing in the eBPF datapath, so that Maglev backend selection is consistent and efficient.

#### Acceptance Criteria

1. THE Five_Tuple_Hash function SHALL be annotated with `#[inline(always)]` and SHALL accept `(src_ip: u32, dst_ip: u32, src_port: u16, dst_port: u16, proto: u8)` for IPv4 and `(src_ip: [u8; 16], dst_ip: [u8; 16], src_port: u16, dst_port: u16, proto: u8)` for IPv6.
2. THE Five_Tuple_Hash function SHALL produce a `u32` output that is deterministic: identical 5-tuple inputs SHALL always produce identical hash outputs.
3. THE Five_Tuple_Hash function SHALL NOT introduce any local variable larger than 64 bytes and SHALL NOT increase the call depth beyond the project soft limit of 5 levels from program entry.
4. THE Five_Tuple_Hash function SHALL distribute hash values uniformly enough that `hash % 251` does not systematically favor any particular table index across typical traffic patterns.

### Requirement 6: SVC_MAGLEV_MAP Schema and Map Definition

**User Story:** As a platform engineer, I want the Maglev map schema defined in both eBPF and user-space code, so that the agent can write entries and the eBPF datapath can read them with a stable ABI.

#### Acceptance Criteria

1. THE `SvcMaglevKey` struct SHALL be defined as `repr(C)` with fields `tap_id: u32`, `service_id: u32`, `table_index: u16`, `pad: [u8; 2]`, totaling 12 bytes, in both `ebpf/src/common.rs` and `core/src/common.rs`.
2. THE `SvcMaglevEntry` struct SHALL be defined as `repr(C)` with fields `backend_slot: u16`, `pad: [u8; 2]`, totaling 4 bytes, in both `ebpf/src/common.rs` and `core/src/common.rs`.
3. THE `SVC_MAGLEV_MAP` SHALL be declared as a `HashMap<SvcMaglevKey, SvcMaglevEntry>` in `ebpf/src/maps.rs` with a maximum capacity sufficient for the expected number of Maglev-enabled services multiplied by `MAGLEV_TABLE_SIZE` (251).
4. THE `MAGLEV_TABLE_SIZE` constant SHALL be defined as `251` (a prime number) in both `ebpf/src/common.rs` and `core/src/common.rs`.

### Requirement 7: Consistent Hashing Property on Backend Changes

**User Story:** As a network operator, I want backend additions and removals to cause minimal flow disruption, so that existing connections are preserved as much as possible.

#### Acceptance Criteria

1. WHEN a backend is added to a Maglev-enabled service, THE Maglev_Table_Generator SHALL recompute the lookup table such that at most `ceil(MAGLEV_TABLE_SIZE / new_backend_count)` entries change compared to the previous table.
2. WHEN a backend is removed from a Maglev-enabled service, THE Maglev_Table_Generator SHALL recompute the lookup table such that only entries previously assigned to the removed backend are reassigned; entries assigned to surviving backends SHALL remain unchanged.
3. FOR ALL valid backend lists with the same set of backend identifiers in any order, THE Maglev_Table_Generator SHALL produce identical lookup tables (order-independent).

### Requirement 8: eBPF Stack and Inline Constraints for Maglev Path

**User Story:** As a platform engineer, I want the Maglev eBPF implementation to comply with verifier constraints, so that the program loads and runs correctly on all supported kernels.

#### Acceptance Criteria

1. THE Five_Tuple_Hash helper SHALL be annotated with `#[inline(always)]` and SHALL NOT introduce any local variable larger than 64 bytes.
2. THE Maglev lookup path (hash computation + `SVC_MAGLEV_MAP` lookup + `SVC_BACKEND_MAP` lookup) SHALL NOT increase the call depth of `phase_lb_ingress_v4` or `phase_lb_ingress_v6` beyond the project soft limit of 5 levels from program entry.
3. THE `SvcMaglevKey` stack allocation (12 bytes) SHALL keep the total additional stack usage of the Maglev path below 32 bytes beyond the existing ingress function stack.
4. THE Maglev lookup path SHALL NOT use any loops or bounded iterators in the eBPF datapath; the lookup is a single hash computation followed by two map lookups.

### Requirement 9: Maglev Table Generator Correctness

**User Story:** As a platform engineer, I want the Maglev table generation algorithm to be correct and complete, so that every table slot maps to a valid backend.

#### Acceptance Criteria

1. FOR ALL non-empty backend lists, THE Maglev_Table_Generator SHALL produce a table where every slot in `[0, MAGLEV_TABLE_SIZE)` contains a valid `backend_slot` in `[0, backend_count)` (no unfilled slots).
2. FOR ALL non-empty backend lists, THE Maglev_Table_Generator SHALL terminate in at most `MAGLEV_TABLE_SIZE * backend_count` iterations (the algorithm's worst-case bound).
3. FOR ALL backend lists with `backend_count >= 2`, THE Maglev_Table_Generator SHALL assign at least one slot to every backend (no backend is starved).
4. FOR ALL valid backend lists, computing the Maglev table and then looking up each table entry in the backend list SHALL produce a valid backend for every slot (round-trip completeness).
