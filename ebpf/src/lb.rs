//! L4 Service Load Balancer — Phase B (node-local, random only).
//!
//! Provides frontend VIP lookup, random backend selection, DNAT rewrite
//! (ingress) and RevNat SNAT rewrite (egress) for the TC pipeline.

use aya_ebpf::helpers::bpf_get_prandom_u32;
use aya_ebpf::programs::TcContext;

use crate::common::{
    SvcBackendKey, SvcBackendValue, SvcFrontendKey, SvcFrontendValue, SvcRevNatKey,
    SvcRevNatValue, FLAG_LB_HIT, SVC_BACKEND_FLAG_LOCAL, SVC_LB_ALGO_RANDOM,
};
use crate::maps::{SVC_BACKEND_MAP, SVC_FRONTEND_MAP, SVC_REVNAT_MAP};
use crate::parser::PacketInfo;
use crate::PipelineCtx;

// ---------------------------------------------------------------------------
// Lookup helpers (#[inline(always)] — inlined into phase functions)
// ---------------------------------------------------------------------------

/// Build a v4-mapped-v6 address from a raw IPv4 u32 (network byte order).
#[inline(always)]
fn ipv4_to_v4mapped_raw(ip: u32) -> [u8; 16] {
    let b = ip.to_be_bytes();
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, b[0], b[1], b[2], b[3]]
}

#[inline(always)]
unsafe fn svc_frontend_lookup_v4(
    tap_id: u32,
    info: &PacketInfo,
) -> Option<&'static SvcFrontendValue> {
    let key = SvcFrontendKey {
        tap_id,
        address: ipv4_to_v4mapped_raw(info.dst_ip),
        port: info.dst_port,
        proto: info.proto,
        scope: 0,
    };
    SVC_FRONTEND_MAP.get(&key)
}

#[inline(always)]
unsafe fn svc_frontend_lookup_v6(
    tap_id: u32,
    info: &PacketInfo,
) -> Option<&'static SvcFrontendValue> {
    let key = SvcFrontendKey {
        tap_id,
        address: info.dst_ip_v6,
        port: info.dst_port,
        proto: info.proto,
        scope: 0,
    };
    SVC_FRONTEND_MAP.get(&key)
}

#[inline(always)]
unsafe fn svc_backend_select(
    tap_id: u32,
    frontend: &SvcFrontendValue,
) -> Option<&'static SvcBackendValue> {
    if frontend.lb_algo != SVC_LB_ALGO_RANDOM {
        return None;
    }
    if frontend.backend_count == 0 {
        return None;
    }
    let slot = (bpf_get_prandom_u32() % frontend.backend_count as u32) as u16;
    let key = SvcBackendKey {
        tap_id,
        service_id: frontend.service_id,
        slot,
        pad: [0; 2],
    };
    SVC_BACKEND_MAP.get(&key)
}

#[inline(always)]
unsafe fn svc_revnat_lookup_v4(
    tap_id: u32,
    info: &PacketInfo,
) -> Option<&'static SvcRevNatValue> {
    let key = SvcRevNatKey {
        tap_id,
        address: ipv4_to_v4mapped_raw(info.src_ip),
        port: info.src_port,
        proto: info.proto,
        pad: 0,
    };
    SVC_REVNAT_MAP.get(&key)
}

#[inline(always)]
unsafe fn svc_revnat_lookup_v6(
    tap_id: u32,
    info: &PacketInfo,
) -> Option<&'static SvcRevNatValue> {
    let key = SvcRevNatKey {
        tap_id,
        address: info.src_ip_v6,
        port: info.src_port,
        proto: info.proto,
        pad: 0,
    };
    SVC_REVNAT_MAP.get(&key)
}

// ---------------------------------------------------------------------------
// DNAT / SNAT rewrite (#[inline(never)] — isolated stack frame)
// ---------------------------------------------------------------------------

/// Rewrite IPv4 dst to backend address + port. Incremental checksum.
#[inline(never)]
unsafe fn svc_dnat_v4(
    ctx: &TcContext,
    info: &PacketInfo,
    backend: &SvcBackendValue,
) -> bool {
    if (backend.flags & SVC_BACKEND_FLAG_LOCAL) == 0 {
        return false;
    }

    // Compute header offsets from TcContext.
    // ETH_HLEN = 14, VLAN adds 4 bytes.
    let ip_hdr_off: usize = if info.vlan_id != 0 { 18 } else { 14 };
    let ihl: usize = 20; // Minimum IPv4 header; sufficient for dst IP at offset 16.
    let l4_hdr_off: usize = ip_hdr_off + ihl;

    let new_ip = u32::from_be_bytes([
        backend.address[12],
        backend.address[13],
        backend.address[14],
        backend.address[15],
    ]);
    let old_ip = info.dst_ip;
    let old_port = info.dst_port;
    let new_port = backend.port;

    // Rewrite IP dst (offset 16 in IPv4 header).
    if ctx
        .store(ip_hdr_off + 16, &new_ip.to_be_bytes(), 0)
        .is_err()
    {
        return false;
    }

    // Rewrite L4 dst port (offset 2 in TCP/UDP header).
    if ctx
        .store(l4_hdr_off + 2, &new_port.to_be_bytes(), 0)
        .is_err()
    {
        return false;
    }

    // Incremental IPv4 header checksum update for dst IP change.
    let _ = ctx.l3_csum_replace(ip_hdr_off + 10, old_ip as u64, new_ip as u64, 4);

    // Incremental L4 checksum update for dst IP + dst port change.
    let csum_off = l4_hdr_off + if info.proto == 6 { 16 } else { 6 };
    let _ = ctx.l4_csum_replace(csum_off, old_ip as u64, new_ip as u64, 0x01 | 4);
    let _ = ctx.l4_csum_replace(csum_off, old_port as u64, new_port as u64, 0x01 | 2);

    true
}

/// Rewrite IPv6 dst to backend address + port. L4 checksum only (no IPv6 header checksum).
#[inline(never)]
unsafe fn svc_dnat_v6(
    ctx: &TcContext,
    info: &PacketInfo,
    backend: &SvcBackendValue,
) -> bool {
    if (backend.flags & SVC_BACKEND_FLAG_LOCAL) == 0 {
        return false;
    }

    let ip_hdr_off: usize = if info.vlan_id != 0 { 18 } else { 14 };
    let l4_hdr_off: usize = ip_hdr_off + 40;

    // Rewrite IPv6 dst (offset 24 in IPv6 header, 16 bytes).
    if ctx.store(ip_hdr_off + 24, &backend.address, 0).is_err() {
        return false;
    }

    // Rewrite L4 dst port.
    let new_port = backend.port;
    if ctx
        .store(l4_hdr_off + 2, &new_port.to_be_bytes(), 0)
        .is_err()
    {
        return false;
    }

    // IPv6 has no header checksum. Update L4 checksum for dst IP + port.
    // Update checksum for each 4-byte word of the 16-byte address change.
    let old_addr = info.dst_ip_v6;
    let new_addr = backend.address;
    let csum_off = l4_hdr_off + if info.proto == 6 { 16 } else { 6 };
    let mut i = 0usize;
    while i < 16 {
        let old_word = u32::from_be_bytes([old_addr[i], old_addr[i + 1], old_addr[i + 2], old_addr[i + 3]]);
        let new_word = u32::from_be_bytes([new_addr[i], new_addr[i + 1], new_addr[i + 2], new_addr[i + 3]]);
        if old_word != new_word {
            let _ = ctx.l4_csum_replace(csum_off, old_word as u64, new_word as u64, 0x01 | 4);
        }
        i += 4;
    }
    let old_port = info.dst_port;
    let _ = ctx.l4_csum_replace(csum_off, old_port as u64, new_port as u64, 0x01 | 2);

    true
}

/// RevNat: rewrite IPv4 src to VIP address + port.
#[inline(never)]
unsafe fn svc_snat_v4(
    ctx: &TcContext,
    info: &PacketInfo,
    revnat: &SvcRevNatValue,
) -> bool {
    let ip_hdr_off: usize = if info.vlan_id != 0 { 18 } else { 14 };
    let l4_hdr_off: usize = ip_hdr_off + 20;

    let new_ip = u32::from_be_bytes([
        revnat.service_address[12],
        revnat.service_address[13],
        revnat.service_address[14],
        revnat.service_address[15],
    ]);
    let old_ip = info.src_ip;
    let old_port = info.src_port;
    let new_port = revnat.service_port;

    // Rewrite IP src (offset 12 in IPv4 header).
    if ctx.store(ip_hdr_off + 12, &new_ip.to_be_bytes(), 0).is_err() {
        return false;
    }

    // Rewrite L4 src port (offset 0 in TCP/UDP header).
    if ctx.store(l4_hdr_off, &new_port.to_be_bytes(), 0).is_err() {
        return false;
    }

    // Incremental checksums.
    let _ = ctx.l3_csum_replace(ip_hdr_off + 10, old_ip as u64, new_ip as u64, 4);
    let csum_off = l4_hdr_off + if info.proto == 6 { 16 } else { 6 };
    let _ = ctx.l4_csum_replace(csum_off, old_ip as u64, new_ip as u64, 0x01 | 4);
    let _ = ctx.l4_csum_replace(csum_off, old_port as u64, new_port as u64, 0x01 | 2);

    true
}

/// RevNat: rewrite IPv6 src to VIP address + port.
#[inline(never)]
unsafe fn svc_snat_v6(
    ctx: &TcContext,
    info: &PacketInfo,
    revnat: &SvcRevNatValue,
) -> bool {
    let ip_hdr_off = info.l3_offset as usize;
    let l4_hdr_off = info.l4_offset as usize;

    // Rewrite IPv6 src (offset 8 in IPv6 header, 16 bytes).
    if ctx.store(ip_hdr_off + 8, &revnat.service_address, 0).is_err() {
        return false;
    }

    let new_port = revnat.service_port;
    if ctx.store(l4_hdr_off, &new_port.to_be_bytes(), 0).is_err() {
        return false;
    }

    // L4 checksum for src IP + port.
    let old_addr = info.src_ip_v6;
    let new_addr = revnat.service_address;
    let csum_off = l4_hdr_off + if info.proto == 6 { 16 } else { 6 };
    let mut i = 0usize;
    while i < 16 {
        let old_word = u32::from_be_bytes([old_addr[i], old_addr[i + 1], old_addr[i + 2], old_addr[i + 3]]);
        let new_word = u32::from_be_bytes([new_addr[i], new_addr[i + 1], new_addr[i + 2], new_addr[i + 3]]);
        if old_word != new_word {
            let _ = ctx.l4_csum_replace(csum_off, old_word as u64, new_word as u64, 0x01 | 4);
        }
        i += 4;
    }
    let old_port = info.src_port;
    let _ = ctx.l4_csum_replace(csum_off, old_port as u64, new_port as u64, 0x01 | 2);

    true
}

// ---------------------------------------------------------------------------
// Phase entry points (#[inline(never)] — called from TC pipeline)
// ---------------------------------------------------------------------------

/// TC ingress LB phase for IPv4: frontend lookup → backend select → DNAT.
#[inline(never)]
pub unsafe fn phase_lb_ingress_v4(
    ctx: &TcContext,
    info: &PacketInfo,
    p: &mut PipelineCtx,
) {
    let frontend = match svc_frontend_lookup_v4(p.tap_id, info) {
        Some(f) => f,
        None => return,
    };
    let backend = match svc_backend_select(p.tap_id, frontend) {
        Some(b) => b,
        None => return,
    };
    if svc_dnat_v4(ctx, info, backend) {
        p.flags |= FLAG_LB_HIT;
    }
}

/// TC ingress LB phase for IPv6.
#[inline(never)]
pub unsafe fn phase_lb_ingress_v6(
    ctx: &TcContext,
    info: &PacketInfo,
    p: &mut PipelineCtx,
) {
    let frontend = match svc_frontend_lookup_v6(p.tap_id, info) {
        Some(f) => f,
        None => return,
    };
    let backend = match svc_backend_select(p.tap_id, frontend) {
        Some(b) => b,
        None => return,
    };
    if svc_dnat_v6(ctx, info, backend) {
        p.flags |= FLAG_LB_HIT;
    }
}

/// TC egress RevNat phase for IPv4: lookup → SNAT.
#[inline(never)]
pub unsafe fn phase_lb_egress_v4(
    ctx: &TcContext,
    info: &PacketInfo,
    _p: &mut PipelineCtx,
) {
    let revnat = match svc_revnat_lookup_v4(_p.tap_id, info) {
        Some(r) => r,
        None => return,
    };
    svc_snat_v4(ctx, info, revnat);
}

/// TC egress RevNat phase for IPv6.
#[inline(never)]
pub unsafe fn phase_lb_egress_v6(
    ctx: &TcContext,
    info: &PacketInfo,
    _p: &mut PipelineCtx,
) {
    let revnat = match svc_revnat_lookup_v6(_p.tap_id, info) {
        Some(r) => r,
        None => return,
    };
    svc_snat_v6(ctx, info, revnat);
}
