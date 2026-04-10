# Requirements Document

## Introduction

Phase B of the Aria L4 Load Balancer implements the first real datapath for node-local service load balancing. Phase A (shadow planning layer) and Phase B Steps 1–2 (map schema structs and svc_ops write/clear operations) are complete. This document covers the remaining Phase B steps: eBPF datapath LB lookup and backend selection with DNAT (B3), agent materialization of compiled service state into pinned maps (B4), end-to-end verification for node-local random LB (B5), and real apply-status reporting after successful materialization (B6).

Phase B scope is strictly limited to: node-local forwarding only, random LB algorithm only, no session affinity, no maglev, no cross-node handoff. The frozen map schema from RFC-008A governs all struct layouts.

## Glossary

- **LB_Module**: The new `ebpf/src/lb.rs` eBPF module responsible for service frontend lookup, backend selection, and DNAT in the TC packet datapath
- **Agent_Materializer**: The component within `agent/src/platform_agent.rs` that converts compiled `ServiceProgramIr` objects into concrete map write calls via `core/src/svc_ops.rs`
- **SVC_FRONTEND_MAP**: The pinned eBPF HashMap keyed by `(tap_id, address, port, proto, scope)` that maps VIP frontends to service metadata
- **SVC_BACKEND_MAP**: The pinned eBPF HashMap keyed by `(tap_id, service_id, slot)` that maps backend slots to target addresses
- **SVC_REVNAT_MAP**: The pinned eBPF HashMap keyed by `(tap_id, backend_address, backend_port, proto)` that maps return traffic back to the original VIP
- **Frontend_Lookup**: The eBPF inline helper that performs a hash map lookup on `SVC_FRONTEND_MAP` using parsed packet destination fields
- **Backend_Selection**: The eBPF inline helper that selects a backend slot using `bpf_get_prandom_u32() % backend_count` and looks up `SVC_BACKEND_MAP`
- **DNAT**: Destination Network Address Translation — rewriting the packet destination IP and port from VIP to the selected backend address
- **RevNat_Lookup**: The eBPF inline helper that checks `SVC_REVNAT_MAP` on return traffic to restore the original VIP as the packet source
- **PipelineCtx**: The per-CPU scratch structure used to pass state between pipeline phases in the eBPF datapath
- **Tap_ID**: The namespace isolation key assigned to each managed interface, used as the first field in all service map keys
- **Service_ID**: A locally-assigned u32 identifier for each service, stable within a compilation generation
- **Slot**: A 0-based compact index into the backend member array for a given service; no holes allowed
- **V4_Mapped_V6**: The `[u8; 16]` address format where IPv4 addresses are stored as `::ffff:x.x.x.x`
- **Apply_Status**: The report sent from agent to controller indicating whether service map materialization succeeded or failed

## Requirements

### Requirement 1: eBPF Frontend Lookup

**User Story:** As the Aria datapath, I want to look up incoming packets against the service frontend map, so that VIP-destined traffic can be identified for load balancing.

#### Acceptance Criteria

1. WHEN a TCP or UDP packet arrives at TC ingress with a destination matching a `SVC_FRONTEND_MAP` entry for the current Tap_ID, THE LB_Module SHALL return the corresponding `SvcFrontendValue` containing `service_id`, `backend_count`, `flags`, and `lb_algo`
2. WHEN a packet destination does not match any `SVC_FRONTEND_MAP` entry for the current Tap_ID, THE LB_Module SHALL return a miss indicator and the packet SHALL continue through the existing pipeline unmodified
3. THE LB_Module SHALL perform the Frontend_Lookup using the packet fields `(tap_id, dst_address, dst_port, proto, scope)` where `dst_address` is in V4_Mapped_V6 format for IPv4 packets
4. THE LB_Module SHALL use `scope = 0` (internal) for TC ingress lookups in Phase B
5. THE Frontend_Lookup helper SHALL be annotated `#[inline(always)]` and SHALL NOT allocate any local variable larger than 64 bytes on the stack

### Requirement 2: eBPF Random Backend Selection

**User Story:** As the Aria datapath, I want to select a backend from the matched service using random selection, so that traffic is distributed across healthy backends.

#### Acceptance Criteria

1. WHEN a Frontend_Lookup returns a hit with `backend_count > 0` and `lb_algo == SVC_LB_ALGO_RANDOM`, THE LB_Module SHALL compute a backend slot as `bpf_get_prandom_u32() % backend_count`
2. WHEN the computed slot is used to look up `SVC_BACKEND_MAP` with key `(tap_id, service_id, slot)`, THE LB_Module SHALL return the corresponding `SvcBackendValue` containing the backend `address` and `port`
3. IF a Frontend_Lookup returns a hit with `backend_count == 0`, THEN THE LB_Module SHALL skip backend selection and the packet SHALL continue through the existing pipeline unmodified
4. IF the `SVC_BACKEND_MAP` lookup for the computed slot fails, THEN THE LB_Module SHALL skip DNAT and the packet SHALL continue through the existing pipeline unmodified
5. THE Backend_Selection helper SHALL be annotated `#[inline(always)]` and SHALL NOT allocate any local variable larger than 64 bytes on the stack

### Requirement 3: eBPF DNAT Packet Rewrite

**User Story:** As the Aria datapath, I want to rewrite the destination IP and port of VIP-matched packets to the selected backend, so that traffic reaches the correct backend endpoint.

#### Acceptance Criteria

1. WHEN a backend is successfully selected and the backend `flags` has `SVC_BACKEND_FLAG_LOCAL` set, THE LB_Module SHALL rewrite the packet destination IP to the backend `address` and the destination port to the backend `port`
2. WHEN the packet is IPv4 (detected from the parsed `PacketInfo`), THE LB_Module SHALL rewrite the IPv4 destination address by extracting the last 4 bytes from the V4_Mapped_V6 backend address
3. WHEN the DNAT rewrite is performed on a TC context, THE LB_Module SHALL use the aya `TcContext` store helpers or direct packet data writes with proper bounds checks to modify the IP header destination and the L4 header destination port
4. WHEN the DNAT rewrite modifies the IP header, THE LB_Module SHALL recalculate the IP checksum (for IPv4) and the L4 checksum using incremental checksum update helpers
5. IF the selected backend does not have `SVC_BACKEND_FLAG_LOCAL` set, THEN THE LB_Module SHALL skip DNAT and the packet SHALL continue unmodified (cross-node handoff is deferred to Phase C)
6. THE DNAT rewrite function SHALL be annotated `#[inline(never)]` to isolate its stack frame and SHALL keep its stack usage below 256 bytes

### Requirement 4: eBPF RevNat Return Path

**User Story:** As the Aria datapath, I want to restore the original VIP address on return traffic from backends, so that clients see responses from the VIP they connected to.

#### Acceptance Criteria

1. WHEN a packet arrives at TC egress with a source matching a `SVC_REVNAT_MAP` entry for the current Tap_ID, THE LB_Module SHALL rewrite the packet source IP to `service_address` and the source port to `service_port` from the `SvcRevNatValue`
2. WHEN the RevNat rewrite modifies the IP header, THE LB_Module SHALL recalculate the IP checksum (for IPv4) and the L4 checksum using incremental checksum update helpers
3. WHEN a packet source does not match any `SVC_REVNAT_MAP` entry, THE LB_Module SHALL leave the packet unmodified and continue through the existing pipeline
4. THE RevNat_Lookup helper SHALL be annotated `#[inline(always)]` and SHALL NOT allocate any local variable larger than 64 bytes on the stack

### Requirement 5: eBPF Module Structure and Constraints

**User Story:** As a developer, I want the LB module to follow the established eBPF implementation constraints, so that the verifier accepts the program and the datapath remains maintainable.

#### Acceptance Criteria

1. THE LB_Module SHALL be implemented as a new file `ebpf/src/lb.rs` and registered as `mod lb` in `ebpf/src/lib.rs`
2. THE LB_Module SHALL not introduce any tail calls
3. THE LB_Module SHALL keep the total call depth from TC entry point through LB helpers to at most 2 additional levels (staying within the project soft limit of 5–6 total from program entry)
4. THE LB_Module SHALL use per-CPU scratch maps for any intermediate structure larger than 64 bytes, following the existing `PKT_SCRATCH` / `PIPE_SCRATCH` pattern
5. THE LB_Module SHALL access `SVC_FRONTEND_MAP`, `SVC_BACKEND_MAP`, and `SVC_REVNAT_MAP` via the existing `maps` module imports
6. THE LB_Module SHALL integrate into the TC ingress pipeline by being called after packet parsing and tap_id resolution but before the conntrack lookup phase, so that DNAT-rewritten packets enter conntrack with the backend destination
7. THE LB_Module SHALL integrate into the TC egress pipeline by performing RevNat lookup before the existing egress processing, so that return traffic is rewritten before conntrack and QoS evaluation

### Requirement 6: Agent Service Map Materialization

**User Story:** As the Aria agent, I want to write compiled service state into the pinned eBPF maps, so that the datapath can perform real load balancing lookups.

#### Acceptance Criteria

1. WHEN the Agent_Materializer processes a `CompiledNodeState` containing `service_programs` with `shadow_apply_only == false`, THE Agent_Materializer SHALL call `write_service_frontends` for each service listener port, producing one `SvcFrontendEntry` per `(vip, listener_port, protocol)` combination
2. WHEN the Agent_Materializer writes frontend entries, THE Agent_Materializer SHALL assign a locally-stable `service_id` starting from 1, incrementing for each distinct service within the compilation generation
3. WHEN the Agent_Materializer processes backend sets, THE Agent_Materializer SHALL call `write_service_backends` with compact 0-based slot indices, writing only backends where `admin_state != "disabled"` and `resolved_locality == "local"`
4. WHEN the Agent_Materializer writes backend entries, THE Agent_Materializer SHALL set `backend_count` in the corresponding frontend value to the number of actually-written local backend slots
5. WHEN the Agent_Materializer processes backend members, THE Agent_Materializer SHALL call `write_service_revnats` for each `(backend_ip, backend_port, protocol)` combination, mapping back to the service VIP and service port
6. WHEN a new compilation generation arrives, THE Agent_Materializer SHALL call `clear_service_maps_for_tap` for each affected tap_id before writing the new generation, ensuring stale entries are removed
7. IF any `svc_ops` write function returns an error, THEN THE Agent_Materializer SHALL log the error and include the failure in the apply-status report without crashing the agent loop

### Requirement 7: Service ID Allocation

**User Story:** As the Aria agent, I want to allocate stable local service IDs during compilation, so that the eBPF backend map keys remain consistent within a generation.

#### Acceptance Criteria

1. THE Agent_Materializer SHALL allocate `service_id` values as sequential u32 integers starting from 1 within each compilation generation
2. THE Agent_Materializer SHALL assign the same `service_id` to all listener ports belonging to the same service within a single generation
3. WHEN a new compilation generation is processed, THE Agent_Materializer SHALL re-allocate service IDs from 1 (service IDs are generation-scoped, not globally persistent)

### Requirement 8: IPv4 Address Encoding

**User Story:** As the Aria agent, I want to encode IPv4 backend and VIP addresses in V4-mapped-v6 format, so that the eBPF maps use a uniform 16-byte address representation.

#### Acceptance Criteria

1. WHEN the Agent_Materializer writes a frontend entry with an IPv4 VIP address, THE Agent_Materializer SHALL encode the address using `ipv4_to_v4mapped` producing `[0,0,0,0, 0,0,0,0, 0,0,0xff,0xff, a,b,c,d]`
2. WHEN the Agent_Materializer writes a backend entry with an IPv4 backend address, THE Agent_Materializer SHALL encode the address using `ipv4_to_v4mapped`
3. WHEN the Agent_Materializer writes a RevNat entry, THE Agent_Materializer SHALL encode both the backend address and the service address using `ipv4_to_v4mapped` for IPv4 addresses

### Requirement 9: Apply-Status Reporting

**User Story:** As the Aria agent, I want to report real apply-status after service map materialization, so that the controller knows whether the datapath is active.

#### Acceptance Criteria

1. WHEN the Agent_Materializer successfully writes all frontend, backend, and RevNat entries for a generation, THE Agent_Materializer SHALL set the services domain status to `"applied"` in the `ApplyStatusReport`
2. IF any map write operation fails during materialization, THEN THE Agent_Materializer SHALL set the services domain status to `"failed"` and SHALL include the failure details as `ApplyObjectFailure` entries in the report
3. WHEN no `service_programs` are present in the compiled state, THE Agent_Materializer SHALL set the services domain status to `"applied"` with zero compiled objects (empty success)
4. THE Agent_Materializer SHALL replace the current shadow-only warning messages for the services domain with real materialization status after successful map writes

### Requirement 10: Phase B Scope Guard

**User Story:** As a developer, I want explicit scope boundaries for Phase B, so that the implementation does not accidentally include Phase C features.

#### Acceptance Criteria

1. THE LB_Module SHALL NOT implement session affinity lookups against `SVC_AFFINITY_MAP` in Phase B
2. THE LB_Module SHALL NOT implement maglev consistent-hash lookups against `SVC_MAGLEV_MAP` in Phase B
3. THE Agent_Materializer SHALL NOT write entries to `SVC_AFFINITY_MAP` or `SVC_MAGLEV_MAP` in Phase B
4. THE LB_Module SHALL NOT implement cross-node backend handoff (tunnel encapsulation or route-based forwarding) in Phase B
5. THE Agent_Materializer SHALL only write backends with `resolved_locality == "local"` to `SVC_BACKEND_MAP` in Phase B; remote backends SHALL be excluded from the slot array
6. THE LB_Module SHALL only process `lb_algo == SVC_LB_ALGO_RANDOM` in Phase B; other algorithm values SHALL cause the LB_Module to skip backend selection and pass the packet through unmodified
