//! Port identity lookup and Anti-Spoof enforcement.
//!
//! Provides phase_port_identity (PORT_IDENTITY_MAP lookup) and
//! phase_anti_spoof_v4 / phase_anti_spoof_v6 (MAC + IP validation)
//! for the TC ingress pipeline.

use crate::common::{AntiSpoofKey, PipelineCtx, PortIdentityKey, FLAG_PORT_RESOLVED};
use crate::maps::{ANTI_SPOOF_MAP, PORT_IDENTITY_MAP};
use crate::parser::PacketInfo;

// ---------------------------------------------------------------------------
// Helper: IPv4 → v4-mapped-v6 (#[inline(always)])
// ---------------------------------------------------------------------------

#[inline(always)]
fn ipv4_to_v4mapped(ip: u32) -> [u8; 16] {
    let b = ip.to_be_bytes();
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, b[0], b[1], b[2], b[3],
    ]
}

// ---------------------------------------------------------------------------
// Helper: compare 6-byte MAC addresses (#[inline(always)])
// ---------------------------------------------------------------------------

#[inline(always)]
fn mac_eq(a: &[u8; 6], b: &[u8; 6]) -> bool {
    a[0] == b[0] && a[1] == b[1] && a[2] == b[2] && a[3] == b[3] && a[4] == b[4] && a[5] == b[5]
}

// ---------------------------------------------------------------------------
// Phase: Port Identity lookup
// ---------------------------------------------------------------------------

/// Look up PORT_IDENTITY_MAP by tap_id. On hit, populate PipelineCtx
/// port identity fields and set FLAG_PORT_RESOLVED. Returns false on miss.
#[inline(always)]
pub unsafe fn phase_port_identity(p: &mut PipelineCtx) -> bool {
    let key = PortIdentityKey { tap_id: p.tap_id };
    let val = match PORT_IDENTITY_MAP.get(&key) {
        Some(v) => v,
        None => return false,
    };
    p.port_network_id = val.network_id;
    p.port_segment_id = val.segment_id;
    p.port_sg_id = val.sg_id;
    p.port_flags = (val.flags & 0xFF) as u8;
    p.flags |= FLAG_PORT_RESOLVED;
    true
}

// ---------------------------------------------------------------------------
// Phase: Anti-Spoof (IPv4)
// ---------------------------------------------------------------------------

/// Validate source MAC against PORT_IDENTITY_MAP mac, then validate
/// source IPv4 against ANTI_SPOOF_MAP. Returns true if both pass.
#[inline(always)]
pub unsafe fn phase_anti_spoof_v4(info: &PacketInfo, p: &PipelineCtx) -> bool {
    // Re-lookup PORT_IDENTITY_MAP to get the bound MAC address.
    let id_key = PortIdentityKey { tap_id: p.tap_id };
    let id_val = match PORT_IDENTITY_MAP.get(&id_key) {
        Some(v) => v,
        None => return false,
    };

    // MAC check: packet src_mac must match port's bound MAC.
    if !mac_eq(&info.src_mac, &id_val.mac) {
        return false;
    }

    // IP check: src_ip must be in the allowed list.
    let spoof_key = AntiSpoofKey {
        tap_id: p.tap_id,
        address: ipv4_to_v4mapped(info.src_ip),
    };
    ANTI_SPOOF_MAP.get(&spoof_key).is_some()
}

// ---------------------------------------------------------------------------
// Phase: Anti-Spoof (IPv6)
// ---------------------------------------------------------------------------

/// Validate source MAC against PORT_IDENTITY_MAP mac, then validate
/// source IPv6 against ANTI_SPOOF_MAP. Returns true if both pass.
#[inline(always)]
pub unsafe fn phase_anti_spoof_v6(info: &PacketInfo, p: &PipelineCtx) -> bool {
    // Re-lookup PORT_IDENTITY_MAP to get the bound MAC address.
    let id_key = PortIdentityKey { tap_id: p.tap_id };
    let id_val = match PORT_IDENTITY_MAP.get(&id_key) {
        Some(v) => v,
        None => return false,
    };

    // MAC check: packet src_mac must match port's bound MAC.
    if !mac_eq(&info.src_mac, &id_val.mac) {
        return false;
    }

    // IP check: src_ip_v6 must be in the allowed list.
    let spoof_key = AntiSpoofKey {
        tap_id: p.tap_id,
        address: info.src_ip_v6,
    };
    ANTI_SPOOF_MAP.get(&spoof_key).is_some()
}
