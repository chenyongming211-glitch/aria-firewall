//! Route lookup and forwarding decisions for the single-node IaaS pipeline.

use aya_ebpf::helpers::gen::bpf_redirect;
use aya_ebpf::maps::lpm_trie::Key;
use aya_ebpf::programs::TcContext;

use crate::common::{
    PipelineCtx, RouteValue, DROP_ROUTE_BLACKHOLE, DROP_ROUTE_MISS, FLAG_ROUTE_RESOLVED,
    NEXT_HOP_BLACKHOLE, NEXT_HOP_GATEWAY, NEXT_HOP_HOST, NEXT_HOP_LOCAL_PORT,
};
use crate::maps::{ROUTE_TABLE_V4, ROUTE_TABLE_V6};

const TC_ACT_OK: i32 = 0;
const TC_ACT_SHOT: i32 = 2;

#[inline(always)]
pub unsafe fn route_lookup_v4(tap_id: u32, dst_ip: u32) -> Option<RouteValue> {
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&tap_id.to_be_bytes());
    bytes[4..].copy_from_slice(&dst_ip.to_be_bytes());
    let key = Key::new(64, bytes);
    ROUTE_TABLE_V4.get(&key).copied()
}

#[inline(always)]
pub unsafe fn route_lookup_v6(tap_id: u32, dst_ip: [u8; 16]) -> Option<RouteValue> {
    let mut bytes = [0u8; 20];
    bytes[..4].copy_from_slice(&tap_id.to_be_bytes());
    bytes[4..].copy_from_slice(&dst_ip);
    let key = Key::new(160, bytes);
    ROUTE_TABLE_V6.get(&key).copied()
}

#[inline(always)]
pub unsafe fn phase_route_forward(
    _ctx: &TcContext,
    route: &RouteValue,
    p: &mut PipelineCtx,
) -> i32 {
    p.route_id = route.route_id;
    p.route_next_hop_type = route.next_hop_type;
    p.route_egress_ifindex = route.egress_ifindex;
    p.flags |= FLAG_ROUTE_RESOLVED;

    match route.next_hop_type {
        NEXT_HOP_LOCAL_PORT => {
            if route.egress_ifindex == 0 {
                p.drop_reason = DROP_ROUTE_MISS;
                TC_ACT_SHOT
            } else {
                bpf_redirect(route.egress_ifindex, 0) as i32
            }
        }
        NEXT_HOP_GATEWAY | NEXT_HOP_HOST => TC_ACT_OK,
        NEXT_HOP_BLACKHOLE => {
            p.drop_reason = DROP_ROUTE_BLACKHOLE;
            TC_ACT_SHOT
        }
        _ => {
            p.drop_reason = DROP_ROUTE_MISS;
            TC_ACT_SHOT
        }
    }
}
