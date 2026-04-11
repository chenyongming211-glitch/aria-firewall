//! SecurityGroup rule matching with 3-level fallback lookup.
//!
//! Provides sg_check() for the TC ingress pipeline.
//! Lookup order: exact → protocol-wildcard → full-wildcard → default deny.

use crate::common::{PipelineCtx, SgRuleKey, SgRuleValue};
use crate::maps::SG_RULE_MAP;
use crate::parser::PacketInfo;

// ---------------------------------------------------------------------------
// Helper: IPv4 → v4-mapped-v6
// ---------------------------------------------------------------------------

#[inline(always)]
fn ipv4_to_v4mapped(ip: u32) -> [u8; 16] {
    let b = ip.to_be_bytes();
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, b[0], b[1], b[2], b[3],
    ]
}

// ---------------------------------------------------------------------------
// Helper: single SG_RULE_MAP lookup
// ---------------------------------------------------------------------------

/// Returns Some(rule) if the key is found, None otherwise.
#[inline(always)]
unsafe fn sg_lookup(
    tap_id: u32,
    sg_id: u32,
    direction: u8,
    proto: u8,
    remote_prefix: [u8; 16],
    prefix_len: u8,
) -> Option<SgRuleValue> {
    let key = SgRuleKey {
        tap_id,
        sg_id,
        direction,
        proto,
        pad: [0; 2],
        remote_prefix,
        prefix_len,
        pad2: [0; 3],
    };
    SG_RULE_MAP.get(&key).copied()
}

#[inline(always)]
fn port_allowed(rule: &SgRuleValue, dst_port: u16) -> bool {
    if rule.port_start == 0 && rule.port_end == 0 {
        return true;
    }
    dst_port >= rule.port_start && dst_port <= rule.port_end
}

// ---------------------------------------------------------------------------
// Phase: SecurityGroup check (3-level fallback)
// ---------------------------------------------------------------------------

/// Check SecurityGroup rules for the given direction.
/// direction: 0 = ingress (pre-route), 1 = egress (post-route).
/// Returns true if allowed, false if denied.
///
/// Lookup order:
///   1. Exact: (tap_id, sg_id, direction, proto, remote_ip, 128)
///   2. Proto wildcard: (tap_id, sg_id, direction, proto, ::/0, 0)
///   3. Full wildcard: (tap_id, sg_id, direction, 0, ::/0, 0)
///   4. Default: deny
#[inline(always)]
pub unsafe fn sg_check(p: &PipelineCtx, info: &PacketInfo, direction: u8) -> bool {
    let sg_id = p.port_sg_id;
    let tap_id = p.tap_id;
    let proto = info.proto;

    if sg_id == 0 {
        return true;
    }

    // Determine remote IP based on direction:
    // ingress → remote is source, egress → remote is destination
    let remote_ip = if direction == 0 {
        // ingress: remote = source
        if info.is_ipv6 {
            info.src_ip_v6
        } else {
            ipv4_to_v4mapped(info.src_ip)
        }
    } else {
        // egress: remote = destination
        if info.is_ipv6 {
            info.dst_ip_v6
        } else {
            ipv4_to_v4mapped(info.dst_ip)
        }
    };

    // Level 1: exact match (proto + remote_ip/128)
    if let Some(rule) = sg_lookup(tap_id, sg_id, direction, proto, remote_ip, 128) {
        return port_allowed(&rule, info.dst_port) && rule.action == 1;
    }

    // Level 2: protocol wildcard (proto + ::/0)
    if let Some(rule) = sg_lookup(tap_id, sg_id, direction, proto, [0; 16], 0) {
        return port_allowed(&rule, info.dst_port) && rule.action == 1;
    }

    // Level 3: full wildcard (any proto + ::/0)
    if let Some(rule) = sg_lookup(tap_id, sg_id, direction, 0, [0; 16], 0) {
        return port_allowed(&rule, info.dst_port) && rule.action == 1;
    }

    // Default: deny
    false
}
