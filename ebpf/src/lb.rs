//! L4 Service Load Balancer — Phase B (node-local, random only).
//!
//! Provides frontend VIP lookup, random backend selection, DNAT rewrite
//! (ingress) and RevNat SNAT rewrite (egress) for the TC pipeline.

use aya_ebpf::bindings::__sk_buff;
use aya_ebpf::helpers::bpf_get_prandom_u32;
use aya_ebpf::helpers::bpf_ktime_get_ns;
use aya_ebpf::helpers::gen::{bpf_l3_csum_replace, bpf_l4_csum_replace, bpf_skb_store_bytes};

use crate::common::{
    SvcAffinityKey, SvcAffinityValue, SvcBackendKey, SvcBackendValue, SvcFrontendKey,
    SvcFrontendValue, SvcLbStatsKey, SvcMaglevKey, SvcRevNatKey, SvcRevNatValue, FLAG_LB_HIT,
    MAGLEV_TABLE_SIZE, SVC_BACKEND_FLAG_LOCAL, SVC_FRONTEND_FLAG_HAS_AFFINITY,
    SVC_LB_ALGO_MAGLEV, SVC_LB_ALGO_RANDOM,
};
use crate::maps::{
    SVC_AFFINITY_MAP, SVC_BACKEND_MAP, SVC_FRONTEND_MAP, SVC_LB_STATS, SVC_LB_STATS_BUF,
    SVC_MAGLEV_MAP, SVC_REVNAT_MAP,
};
use crate::parser::PacketInfo;
use crate::PipelineCtx;

// ---------------------------------------------------------------------------
// Lookup helpers (#[inline(always)])
// ---------------------------------------------------------------------------

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
// DNAT / SNAT rewrite using raw BPF helpers
// ---------------------------------------------------------------------------

/// Rewrite IPv4 dst to backend address + port with incremental checksum.
#[inline(never)]
unsafe fn svc_dnat_v4(
    skb: *mut __sk_buff,
    info: &PacketInfo,
    backend: &SvcBackendValue,
) -> bool {
    if (backend.flags & SVC_BACKEND_FLAG_LOCAL) == 0 {
        return false;
    }

    let ip_off: u32 = if info.vlan_id != 0 { 18 } else { 14 };
    let l4_off: u32 = ip_off + 20;

    let new_ip = u32::from_be_bytes([
        backend.address[12], backend.address[13],
        backend.address[14], backend.address[15],
    ]);
    let old_ip = info.dst_ip;
    let new_port = backend.port;
    let old_port = info.dst_port;

    // Rewrite IP dst (offset 16 in IPv4 header).
    let new_ip_be = new_ip.to_be_bytes();
    if bpf_skb_store_bytes(
        skb, (ip_off + 16) as u32,
        new_ip_be.as_ptr() as *const _,
        4, 0,
    ) < 0 {
        return false;
    }

    // Rewrite L4 dst port (offset 2 in TCP/UDP header).
    let new_port_be = new_port.to_be_bytes();
    if bpf_skb_store_bytes(
        skb, (l4_off + 2) as u32,
        new_port_be.as_ptr() as *const _,
        2, 0,
    ) < 0 {
        return false;
    }

    // Incremental IPv4 header checksum (offset 10).
    bpf_l3_csum_replace(skb, (ip_off + 10) as u32, old_ip as u64, new_ip as u64, 4);

    // Incremental L4 checksum: TCP offset 16, UDP offset 6.
    let csum_off = l4_off + if info.proto == 6 { 16 } else { 6 };
    // BPF_F_PSEUDO_HDR = 0x10 for pseudo-header aware update.
    bpf_l4_csum_replace(skb, csum_off as u32, old_ip as u64, new_ip as u64, 0x10 | 4);
    bpf_l4_csum_replace(skb, csum_off as u32, old_port as u64, new_port as u64, 0x10 | 2);

    true
}

/// Rewrite IPv6 dst to backend address + port. L4 checksum only.
#[inline(never)]
unsafe fn svc_dnat_v6(
    skb: *mut __sk_buff,
    info: &PacketInfo,
    backend: &SvcBackendValue,
) -> bool {
    if (backend.flags & SVC_BACKEND_FLAG_LOCAL) == 0 {
        return false;
    }

    let ip_off: u32 = if info.vlan_id != 0 { 18 } else { 14 };
    let l4_off: u32 = ip_off + 40;

    // Rewrite IPv6 dst (offset 24, 16 bytes).
    if bpf_skb_store_bytes(
        skb, (ip_off + 24) as u32,
        backend.address.as_ptr() as *const _,
        16, 0,
    ) < 0 {
        return false;
    }

    // Rewrite L4 dst port.
    let new_port = backend.port;
    let new_port_be = new_port.to_be_bytes();
    if bpf_skb_store_bytes(
        skb, (l4_off + 2) as u32,
        new_port_be.as_ptr() as *const _,
        2, 0,
    ) < 0 {
        return false;
    }

    // L4 checksum update for IPv6 dst change (4 words).
    let csum_off = l4_off + if info.proto == 6 { 16 } else { 6 };
    let old_addr = info.dst_ip_v6;
    let new_addr = backend.address;
    let mut i = 0u32;
    while i < 4 {
        let off = (i * 4) as usize;
        let old_w = u32::from_be_bytes([old_addr[off], old_addr[off+1], old_addr[off+2], old_addr[off+3]]);
        let new_w = u32::from_be_bytes([new_addr[off], new_addr[off+1], new_addr[off+2], new_addr[off+3]]);
        if old_w != new_w {
            bpf_l4_csum_replace(skb, csum_off as u32, old_w as u64, new_w as u64, 0x10 | 4);
        }
        i += 1;
    }
    let old_port = info.dst_port;
    bpf_l4_csum_replace(skb, csum_off as u32, old_port as u64, new_port as u64, 0x10 | 2);

    true
}

/// RevNat: rewrite IPv4 src to VIP address + port.
#[inline(never)]
unsafe fn svc_snat_v4(
    skb: *mut __sk_buff,
    info: &PacketInfo,
    revnat: &SvcRevNatValue,
) -> bool {
    let ip_off: u32 = if info.vlan_id != 0 { 18 } else { 14 };
    let l4_off: u32 = ip_off + 20;

    let new_ip = u32::from_be_bytes([
        revnat.service_address[12], revnat.service_address[13],
        revnat.service_address[14], revnat.service_address[15],
    ]);
    let old_ip = info.src_ip;
    let new_port = revnat.service_port;
    let old_port = info.src_port;

    // Rewrite IP src (offset 12).
    let new_ip_be = new_ip.to_be_bytes();
    if bpf_skb_store_bytes(
        skb, (ip_off + 12) as u32,
        new_ip_be.as_ptr() as *const _,
        4, 0,
    ) < 0 {
        return false;
    }

    // Rewrite L4 src port (offset 0).
    let new_port_be = new_port.to_be_bytes();
    if bpf_skb_store_bytes(
        skb, l4_off as u32,
        new_port_be.as_ptr() as *const _,
        2, 0,
    ) < 0 {
        return false;
    }

    // Checksums.
    bpf_l3_csum_replace(skb, (ip_off + 10) as u32, old_ip as u64, new_ip as u64, 4);
    let csum_off = l4_off + if info.proto == 6 { 16 } else { 6 };
    bpf_l4_csum_replace(skb, csum_off as u32, old_ip as u64, new_ip as u64, 0x10 | 4);
    bpf_l4_csum_replace(skb, csum_off as u32, old_port as u64, new_port as u64, 0x10 | 2);

    true
}

/// RevNat: rewrite IPv6 src to VIP address + port.
#[inline(never)]
unsafe fn svc_snat_v6(
    skb: *mut __sk_buff,
    info: &PacketInfo,
    revnat: &SvcRevNatValue,
) -> bool {
    let ip_off: u32 = if info.vlan_id != 0 { 18 } else { 14 };
    let l4_off: u32 = ip_off + 40;

    // Rewrite IPv6 src (offset 8, 16 bytes).
    if bpf_skb_store_bytes(
        skb, (ip_off + 8) as u32,
        revnat.service_address.as_ptr() as *const _,
        16, 0,
    ) < 0 {
        return false;
    }

    // Rewrite L4 src port.
    let new_port = revnat.service_port;
    let new_port_be = new_port.to_be_bytes();
    if bpf_skb_store_bytes(
        skb, l4_off as u32,
        new_port_be.as_ptr() as *const _,
        2, 0,
    ) < 0 {
        return false;
    }

    // L4 checksum for src IP + port.
    let csum_off = l4_off + if info.proto == 6 { 16 } else { 6 };
    let old_addr = info.src_ip_v6;
    let new_addr = revnat.service_address;
    let mut i = 0u32;
    while i < 4 {
        let off = (i * 4) as usize;
        let old_w = u32::from_be_bytes([old_addr[off], old_addr[off+1], old_addr[off+2], old_addr[off+3]]);
        let new_w = u32::from_be_bytes([new_addr[off], new_addr[off+1], new_addr[off+2], new_addr[off+3]]);
        if old_w != new_w {
            bpf_l4_csum_replace(skb, csum_off as u32, old_w as u64, new_w as u64, 0x10 | 4);
        }
        i += 1;
    }
    let old_port = info.src_port;
    bpf_l4_csum_replace(skb, csum_off as u32, old_port as u64, new_port as u64, 0x10 | 2);

    true
}

// ---------------------------------------------------------------------------
// Phase entry points — called from TC pipeline in lib.rs
// ---------------------------------------------------------------------------
// 5-tuple hash for Maglev (#[inline(always)])
// ---------------------------------------------------------------------------

#[inline(always)]
fn hash_5tuple_v4(src_ip: u32, dst_ip: u32, src_port: u16, dst_port: u16, proto: u8) -> u32 {
    // Simple xor-rotate hash, sufficient for Maglev table distribution.
    let mut h: u32 = 0x9e37_79b9; // golden ratio seed
    h = h.wrapping_add(src_ip);
    h ^= h.rotate_left(13);
    h = h.wrapping_add(dst_ip);
    h ^= h.rotate_left(7);
    h = h.wrapping_add((src_port as u32) << 16 | dst_port as u32);
    h ^= h.rotate_left(17);
    h = h.wrapping_add(proto as u32);
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h
}

#[inline(always)]
fn hash_5tuple_v6(src_ip: [u8; 16], dst_ip: [u8; 16], src_port: u16, dst_port: u16, proto: u8) -> u32 {
    let mut h: u32 = 0x9e37_79b9;
    let mut i = 0usize;
    while i < 16 {
        let w = u32::from_be_bytes([src_ip[i], src_ip[i+1], src_ip[i+2], src_ip[i+3]]);
        h = h.wrapping_add(w);
        h ^= h.rotate_left(13);
        i += 4;
    }
    i = 0;
    while i < 16 {
        let w = u32::from_be_bytes([dst_ip[i], dst_ip[i+1], dst_ip[i+2], dst_ip[i+3]]);
        h = h.wrapping_add(w);
        h ^= h.rotate_left(7);
        i += 4;
    }
    h = h.wrapping_add((src_port as u32) << 16 | dst_port as u32);
    h ^= h.rotate_left(17);
    h = h.wrapping_add(proto as u32);
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h
}

/// Maglev table lookup: hash → table_index → backend_slot.
#[inline(always)]
unsafe fn maglev_select(
    tap_id: u32,
    service_id: u32,
    hash: u32,
) -> Option<u16> {
    let table_index = (hash % MAGLEV_TABLE_SIZE) as u16;
    let key = SvcMaglevKey {
        tap_id,
        service_id,
        table_index,
        pad: [0; 2],
    };
    SVC_MAGLEV_MAP.get(&key).map(|entry| entry.backend_slot)
}

// ---------------------------------------------------------------------------
// LB stats update (#[inline(always)])
// ---------------------------------------------------------------------------

#[inline(always)]
unsafe fn update_lb_stats(
    tap_id: u32,
    service_id: u32,
    backend_slot: u16,
    lb_algo: u8,
    affinity_hit: u8,
    pkt_len: u32,
) {
    let key = SvcLbStatsKey {
        tap_id,
        service_id,
        backend_slot,
        lb_algo,
        affinity_hit,
    };
    if let Some(val) = SVC_LB_STATS.get_ptr_mut(&key) {
        (*val).packets += 1;
        (*val).bytes += pkt_len as u64;
    } else {
        let val = match SVC_LB_STATS_BUF.get_ptr_mut(0) {
            Some(v) => v,
            None => return,
        };
        (*val).packets = 1;
        (*val).bytes = pkt_len as u64;
        let _ = SVC_LB_STATS.insert(&key, &*val, 0);
    }
}

// ---------------------------------------------------------------------------
// Affinity helpers (#[inline(always)])
// ---------------------------------------------------------------------------

#[inline(always)]
unsafe fn affinity_lookup(
    tap_id: u32,
    service_id: u32,
    client_address: [u8; 16],
) -> Option<u16> {
    let key = SvcAffinityKey {
        tap_id,
        service_id,
        client_address,
    };
    SVC_AFFINITY_MAP.get(&key).map(|val| val.backend_slot)
}

#[inline(always)]
unsafe fn affinity_write(
    tap_id: u32,
    service_id: u32,
    client_address: [u8; 16],
    backend_slot: u16,
) {
    let key = SvcAffinityKey {
        tap_id,
        service_id,
        client_address,
    };
    let val = SvcAffinityValue {
        backend_slot,
        pad: [0; 2],
        last_used_ns: bpf_ktime_get_ns(),
    };
    let _ = SVC_AFFINITY_MAP.insert(&key, &val, 0);
}

// ---------------------------------------------------------------------------
// Phase entry points — called from TC pipeline in lib.rs
// ---------------------------------------------------------------------------

/// TC ingress LB phase for IPv4.
#[inline(never)]
pub unsafe fn phase_lb_ingress_v4(
    skb: *mut __sk_buff,
    info: &PacketInfo,
    p: &mut PipelineCtx,
) {
    let frontend = match svc_frontend_lookup_v4(p.tap_id, info) {
        Some(f) => f,
        None => return,
    };

    if frontend.backend_count == 0 {
        return;
    }

    let has_affinity = (frontend.flags & SVC_FRONTEND_FLAG_HAS_AFFINITY) != 0;
    let client_addr = ipv4_to_v4mapped_raw(info.src_ip);

    // Determine backend slot: affinity → maglev/random.
    let (slot, aff_hit) = if has_affinity {
        match affinity_lookup(p.tap_id, frontend.service_id, client_addr) {
            Some(s) if s < frontend.backend_count => (s, 1u8),
            _ => (select_slot_by_algo(p.tap_id, frontend, info), 0u8),
        }
    } else {
        (select_slot_by_algo(p.tap_id, frontend, info), 0u8)
    };

    let bkey = SvcBackendKey {
        tap_id: p.tap_id,
        service_id: frontend.service_id,
        slot,
        pad: [0; 2],
    };
    let backend = match SVC_BACKEND_MAP.get(&bkey) {
        Some(b) => b,
        None => return,
    };

    if svc_dnat_v4(skb, info, backend) {
        p.flags |= FLAG_LB_HIT;
        update_lb_stats(p.tap_id, frontend.service_id, slot, frontend.lb_algo, aff_hit, p.pkt_len);
        if has_affinity {
            affinity_write(p.tap_id, frontend.service_id, client_addr, slot);
        }
    }
}

/// Select backend slot based on lb_algo: maglev or random. IPv4 variant.
#[inline(always)]
unsafe fn select_slot_by_algo(
    tap_id: u32,
    frontend: &SvcFrontendValue,
    info: &PacketInfo,
) -> u16 {
    if frontend.lb_algo == SVC_LB_ALGO_MAGLEV {
        let hash = if info.is_ipv6 {
            hash_5tuple_v6(info.src_ip_v6, info.dst_ip_v6, info.src_port, info.dst_port, info.proto)
        } else {
            hash_5tuple_v4(info.src_ip, info.dst_ip, info.src_port, info.dst_port, info.proto)
        };
        if let Some(slot) = maglev_select(tap_id, frontend.service_id, hash) {
            if slot < frontend.backend_count {
                return slot;
            }
        }
    }
    // Fallback: random.
    (bpf_get_prandom_u32() % frontend.backend_count as u32) as u16
}

/// TC ingress LB phase for IPv6.
#[inline(never)]
pub unsafe fn phase_lb_ingress_v6(
    skb: *mut __sk_buff,
    info: &PacketInfo,
    p: &mut PipelineCtx,
) {
    let frontend = match svc_frontend_lookup_v6(p.tap_id, info) {
        Some(f) => f,
        None => return,
    };

    if frontend.backend_count == 0 {
        return;
    }

    let has_affinity = (frontend.flags & SVC_FRONTEND_FLAG_HAS_AFFINITY) != 0;
    let client_addr = info.src_ip_v6;

    let (slot, aff_hit) = if has_affinity {
        match affinity_lookup(p.tap_id, frontend.service_id, client_addr) {
            Some(s) if s < frontend.backend_count => (s, 1u8),
            _ => (select_slot_by_algo(p.tap_id, frontend, info), 0u8),
        }
    } else {
        (select_slot_by_algo(p.tap_id, frontend, info), 0u8)
    };

    let bkey = SvcBackendKey {
        tap_id: p.tap_id,
        service_id: frontend.service_id,
        slot,
        pad: [0; 2],
    };
    let backend = match SVC_BACKEND_MAP.get(&bkey) {
        Some(b) => b,
        None => return,
    };

    if svc_dnat_v6(skb, info, backend) {
        p.flags |= FLAG_LB_HIT;
        update_lb_stats(p.tap_id, frontend.service_id, slot, frontend.lb_algo, aff_hit, p.pkt_len);
        if has_affinity {
            affinity_write(p.tap_id, frontend.service_id, client_addr, slot);
        }
    }
}

/// TC egress RevNat phase for IPv4.
#[inline(never)]
pub unsafe fn phase_lb_egress_v4(
    skb: *mut __sk_buff,
    info: &PacketInfo,
    p: &mut PipelineCtx,
) {
    let revnat = match svc_revnat_lookup_v4(p.tap_id, info) {
        Some(r) => r,
        None => return,
    };
    svc_snat_v4(skb, info, revnat);
}

/// TC egress RevNat phase for IPv6.
#[inline(never)]
pub unsafe fn phase_lb_egress_v6(
    skb: *mut __sk_buff,
    info: &PacketInfo,
    p: &mut PipelineCtx,
) {
    let revnat = match svc_revnat_lookup_v6(p.tap_id, info) {
        Some(r) => r,
        None => return,
    };
    svc_snat_v6(skb, info, revnat);
}
