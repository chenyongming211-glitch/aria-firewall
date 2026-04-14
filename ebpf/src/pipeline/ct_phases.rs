use aya_ebpf::bindings::__sk_buff;
use aya_ebpf::maps::lpm_trie::Key;
use aya_ebpf::maps::LpmTrie;
use aya_ebpf::programs::TcContext;
use aya_ebpf::EbpfContext;

use crate::common::{
    CtKey4, CtKey6, PipelineCtx, CT_CONTRACT_FAMILY_IPV4, CT_CONTRACT_FAMILY_IPV6,
    CT_CONTRACT_HOOK_TC_INGRESS, CT_CONTRACT_REASON_CT_DISABLED, CT_CONTRACT_REASON_CT_MISS,
    DIR_EGRESS, DIR_INGRESS, DROP_QOS_EGRESS, DROP_ROUTE_MISS, DROP_SG_EGRESS, FLAG_ACL_ON,
    FLAG_CT_HIT, FLAG_IS_FORWARD, FLAG_LB_CT_AFFINITY, FLAG_LB_CT_ENABLED, FLAG_LB_HIT,
    FLAG_MIRROR_ON, FLAG_PORT_RESOLVED, FLAG_QOS_ON, FLAG_TCPRT_ON, FLAG_TRACING, IPPROTO_TCP,
    TRACE_RESULT_DROP_QOS, TRACE_RESULT_DROP_ROUTE, TRACE_RESULT_DROP_SECURITY,
    TRACE_RESULT_PASS, TRACE_TC_DROP, TRACE_TC_EGRESS, TRACE_TC_INGRESS, XDP_PASS,
};
use crate::conntrack::{self, CtLookupResult};
use crate::maps::{DST_IPV4_TRIE, DST_IPV6_TRIE, SRC_IPV4_TRIE, SRC_IPV6_TRIE};
use crate::{ct_contract, lb, mirror, parser, qos, route, runtime, sg, stats, tcprt};

use super::qos_phases::{apply_edt_prio, phase_qos_ingress_tc, should_apply_ingress_qos};
use super::trace_drop::{do_drop, do_trace};

#[inline(always)]
pub(crate) unsafe fn set_matched(p: &mut PipelineCtx, m: &conntrack::MatchedPolicy) {
    p.matched_src_id = m.src_id;
    p.matched_dst_id = m.dst_id;
    p.matched_proto = m.proto;
    p.matched_direction = m.direction;
    p.lb_backend_ip = m.lb_backend_ip;
    p.lb_backend_port = m.lb_backend_port;
    p.lb_slot = m.lb_slot;
    p.lb_service_id = m.lb_service_id;
    p.lb_algo = m.lb_algo;
    p.lb_ct_flags = m.lb_flags;
}

#[inline(always)]
pub(crate) fn get_matched(p: &PipelineCtx) -> conntrack::MatchedPolicy {
    conntrack::MatchedPolicy {
        tap_id: p.tap_id,
        src_id: p.matched_src_id,
        dst_id: p.matched_dst_id,
        proto: p.matched_proto,
        direction: p.matched_direction,
        lb_backend_ip: p.lb_backend_ip,
        lb_backend_port: p.lb_backend_port,
        lb_slot: p.lb_slot,
        lb_service_id: p.lb_service_id,
        lb_algo: p.lb_algo,
        lb_flags: p.lb_ct_flags,
        lb_affinity_hit: if (p.lb_ct_flags & FLAG_LB_CT_AFFINITY) != 0 {
            1
        } else {
            0
        },
    }
}

#[inline(always)]
pub(crate) unsafe fn load_packet_ids(info: &parser::PacketInfo, p: &mut PipelineCtx) {
    if info.is_ipv6 {
        load_packet_ids_v6(info, p);
    } else {
        load_packet_ids_v4(info, p);
    }
}

pub(crate) unsafe fn lookup_ipv4(
    map: &LpmTrie<[u8; 8], u32>,
    tap_id: u32,
    ip: u32,
) -> Option<u32> {
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&tap_id.to_be_bytes());
    bytes[4..].copy_from_slice(&ip.to_be_bytes());
    let key = Key::new(64, bytes);
    map.get(&key).copied()
}

pub(crate) unsafe fn lookup_ipv6(
    map: &LpmTrie<[u8; 20], u32>,
    tap_id: u32,
    ip: [u8; 16],
) -> Option<u32> {
    let mut bytes = [0u8; 20];
    bytes[..4].copy_from_slice(&tap_id.to_be_bytes());
    bytes[4..].copy_from_slice(&ip);
    let key = Key::new(160, bytes);
    map.get(&key).copied()
}

#[inline(never)]
pub(crate) unsafe fn phase_ct_v4(_info: &parser::PacketInfo, p: &mut PipelineCtx, ct_key: &CtKey4) {
    match conntrack::ct_lookup_v4(ct_key, p.now, p.pkt_len) {
        CtLookupResult::Established(matched, is_forward)
        | CtLookupResult::SeenReply(matched, is_forward) => {
            p.ct_state = 2;
            p.flags |= FLAG_CT_HIT;
            if is_forward {
                p.flags |= FLAG_IS_FORWARD;
            }
            set_matched(p, &matched);
        }
        CtLookupResult::NotFound => {
            p.ct_state = 0;
        }
    }
}

#[inline(never)]
pub(crate) unsafe fn phase_ct_v6(_info: &parser::PacketInfo, p: &mut PipelineCtx, ct_key: &CtKey6) {
    match conntrack::ct_lookup_v6(ct_key, p.now, p.pkt_len) {
        CtLookupResult::Established(matched, is_forward)
        | CtLookupResult::SeenReply(matched, is_forward) => {
            p.ct_state = 2;
            p.flags |= FLAG_CT_HIT;
            if is_forward {
                p.flags |= FLAG_IS_FORWARD;
            }
            set_matched(p, &matched);
        }
        CtLookupResult::NotFound => {
            p.ct_state = 0;
        }
    }
}

#[inline(never)]
pub(crate) unsafe fn phase_ct_fastpath_xdp_v4(
    _info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    _ct_key: &CtKey4,
) {
    p.action = XDP_PASS;
}

#[inline(never)]
pub(crate) unsafe fn phase_ct_fastpath_xdp_v6(
    _info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    _ct_key: &CtKey6,
) {
    p.action = XDP_PASS;
}

#[inline(always)]
fn need_ingress_ids(p: &PipelineCtx) -> bool {
    (p.flags & (FLAG_QOS_ON | FLAG_MIRROR_ON | FLAG_TRACING)) != 0
        || stats::monitoring_enabled(p.tap_id)
}

#[inline(always)]
unsafe fn load_packet_ids_v4(info: &parser::PacketInfo, p: &mut PipelineCtx) {
    p.src_id = lookup_ipv4(&SRC_IPV4_TRIE, p.tap_id, info.src_ip).unwrap_or(0);
    p.dst_id = lookup_ipv4(&DST_IPV4_TRIE, p.tap_id, info.dst_ip).unwrap_or(0);
}

#[inline(always)]
unsafe fn load_packet_ids_v6(info: &parser::PacketInfo, p: &mut PipelineCtx) {
    p.src_id = lookup_ipv6(&SRC_IPV6_TRIE, p.tap_id, info.src_ip_v6).unwrap_or(0);
    p.dst_id = lookup_ipv6(&DST_IPV6_TRIE, p.tap_id, info.dst_ip_v6).unwrap_or(0);
}

#[inline(always)]
pub(crate) unsafe fn should_create_ct(p: &PipelineCtx) -> bool {
    (p.flags & FLAG_ACL_ON) != 0
        || stats::monitoring_enabled(p.tap_id)
        || tcprt::tcprt_enabled(p.tap_id)
}

#[inline(always)]
unsafe fn record_tc_ingress_contract_fallback(p: &PipelineCtx, family: u8) {
    let reason = if runtime::conntrack_enabled(p.tap_id) {
        CT_CONTRACT_REASON_CT_MISS
    } else {
        CT_CONTRACT_REASON_CT_DISABLED
    };
    ct_contract::record_event(&ct_contract::CtContractArgs {
        tap_id: p.tap_id,
        pkt_len: p.pkt_len,
        now: p.now,
        hook: CT_CONTRACT_HOOK_TC_INGRESS,
        family,
        reason,
        _pad: 0,
    });
}

#[inline(always)]
unsafe fn phase_post_accept_tc_ingress(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
) {
    if (p.flags & FLAG_PORT_RESOLVED) != 0 && (p.flags & FLAG_LB_HIT) == 0 {
        let route = if info.is_ipv6 {
            route::route_lookup_v6(p.tap_id, info.dst_ip_v6)
        } else {
            route::route_lookup_v4(p.tap_id, info.dst_ip)
        };

        let Some(route) = route else {
            load_packet_ids(info, p);
            p.drop_reason = DROP_ROUTE_MISS;
            p.action = crate::TC_ACT_SHOT as u32;
            do_drop(p);
            if (p.flags & FLAG_TRACING) != 0 {
                do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_ROUTE);
            }
            return;
        };

        if (p.flags & FLAG_CT_HIT) == 0 && !sg::sg_check(p, info, crate::common::SG_DIR_EGRESS) {
            load_packet_ids(info, p);
            p.drop_reason = DROP_SG_EGRESS;
            p.action = crate::TC_ACT_SHOT as u32;
            do_drop(p);
            if (p.flags & FLAG_TRACING) != 0 {
                do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_SECURITY);
            }
            return;
        }

        let action = route::phase_route_forward(ctx, &route, p);
        if action == crate::TC_ACT_SHOT {
            p.action = crate::TC_ACT_SHOT as u32;
            do_drop(p);
            if (p.flags & FLAG_TRACING) != 0 {
                do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_ROUTE);
            }
            return;
        }

        if stats::monitoring_enabled(p.tap_id)
            || (p.flags & (FLAG_QOS_ON | FLAG_MIRROR_ON | FLAG_TRACING)) != 0
        {
            load_packet_ids(info, p);
        }
        if stats::monitoring_enabled(p.tap_id) {
            stats::update_group_stats(p.tap_id, p.src_id, DIR_EGRESS, p.pkt_len);
            stats::update_group_stats(p.tap_id, p.dst_id, DIR_INGRESS, p.pkt_len);
        }
        if (p.flags & FLAG_MIRROR_ON) != 0 {
            let skb = ctx.as_ptr() as *mut __sk_buff;
            mirror::try_mirror_tc(
                skb,
                p.tap_id,
                p.src_id,
                p.dst_id,
                info.proto,
                DIR_INGRESS,
                p.pkt_len,
            );
        }
        if (p.flags & FLAG_TRACING) != 0 {
            do_trace(ctx, info, p, TRACE_TC_INGRESS, TRACE_RESULT_PASS);
        }
        p.action = action as u32;
        return;
    }

    if stats::monitoring_enabled(p.tap_id) {
        stats::update_group_stats(p.tap_id, p.src_id, DIR_EGRESS, p.pkt_len);
        stats::update_group_stats(p.tap_id, p.dst_id, DIR_INGRESS, p.pkt_len);
    }
    if (p.flags & FLAG_MIRROR_ON) != 0 {
        let skb = ctx.as_ptr() as *mut __sk_buff;
        mirror::try_mirror_tc(
            skb,
            p.tap_id,
            p.src_id,
            p.dst_id,
            info.proto,
            DIR_INGRESS,
            p.pkt_len,
        );
    }
    if (p.flags & FLAG_TRACING) != 0 {
        do_trace(ctx, info, p, TRACE_TC_INGRESS, TRACE_RESULT_PASS);
    }
    p.action = crate::TC_ACT_OK as u32;
}

#[inline(always)]
pub(crate) unsafe fn phase_ct_fastpath_tc_ingress_v4(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey4,
) {
    let matched = get_matched(p);

    if (matched.lb_flags & FLAG_LB_CT_ENABLED) != 0 {
        let skb = ctx.as_ptr() as *mut __sk_buff;
        let new_ip = u32::from_be_bytes([
            matched.lb_backend_ip[12],
            matched.lb_backend_ip[13],
            matched.lb_backend_ip[14],
            matched.lb_backend_ip[15],
        ]);
        if lb::apply_dnat_v4_raw(skb, info, new_ip, matched.lb_backend_port) {
            p.flags |= FLAG_LB_HIT;
            let info_mut = (info as *const parser::PacketInfo) as *mut parser::PacketInfo;
            (*info_mut).dst_ip = new_ip;
            (*info_mut).dst_port = matched.lb_backend_port;
            lb::update_lb_stats(
                p.tap_id,
                matched.lb_service_id,
                matched.lb_slot,
                matched.lb_algo,
                matched.lb_affinity_hit,
                p.pkt_len,
            );
        }
    }

    if (p.flags & FLAG_TCPRT_ON) != 0 && info.proto == IPPROTO_TCP {
        if (p.flags & FLAG_IS_FORWARD) != 0 {
            tcprt::track_tcp_rt_v4(ct_key, info, p.now, true, true);
        } else {
            tcprt::track_tcp_rt_v4_rev(p.tap_id, info, p.now, true);
        }
    }

    if stats::monitoring_enabled(p.tap_id) {
        if (p.flags & FLAG_ACL_ON) != 0 {
            let matched = get_matched(p);
            stats::update_rule_stats(&matched.to_policy_key(), p.pkt_len, false);
        }
        stats::update_flow_stats_v4(ct_key, p.pkt_len, p.now);
    }

    if need_ingress_ids(p) {
        load_packet_ids_v4(info, p);
        if should_apply_ingress_qos(p) {
            phase_qos_ingress_tc(ctx, info, p);
            if p.action == crate::TC_ACT_SHOT as u32 {
                return;
            }
        }
    }

    phase_post_accept_tc_ingress(ctx, info, p);
}

#[inline(always)]
pub(crate) unsafe fn phase_ct_fastpath_tc_ingress_v6(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey6,
) {
    let matched = get_matched(p);

    if (matched.lb_flags & FLAG_LB_CT_ENABLED) != 0 {
        let skb = ctx.as_ptr() as *mut __sk_buff;
        if lb::apply_dnat_v6_raw(skb, info, matched.lb_backend_ip, matched.lb_backend_port) {
            p.flags |= FLAG_LB_HIT;
            let info_mut = (info as *const parser::PacketInfo) as *mut parser::PacketInfo;
            (*info_mut).dst_ip_v6 = matched.lb_backend_ip;
            (*info_mut).dst_port = matched.lb_backend_port;
            lb::update_lb_stats(
                p.tap_id,
                matched.lb_service_id,
                matched.lb_slot,
                matched.lb_algo,
                matched.lb_affinity_hit,
                p.pkt_len,
            );
        }
    }

    if (p.flags & FLAG_TCPRT_ON) != 0 && info.proto == IPPROTO_TCP {
        if (p.flags & FLAG_IS_FORWARD) != 0 {
            tcprt::track_tcp_rt_v6(ct_key, info, p.now, true, true);
        } else {
            tcprt::track_tcp_rt_v6_rev(p.tap_id, info, p.now, true);
        }
    }

    if stats::monitoring_enabled(p.tap_id) {
        if (p.flags & FLAG_ACL_ON) != 0 {
            let matched = get_matched(p);
            stats::update_rule_stats(&matched.to_policy_key(), p.pkt_len, false);
        }
        stats::update_flow_stats_v6(ct_key, p.pkt_len, p.now);
    }

    if need_ingress_ids(p) {
        load_packet_ids_v6(info, p);
        if should_apply_ingress_qos(p) {
            phase_qos_ingress_tc(ctx, info, p);
            if p.action == crate::TC_ACT_SHOT as u32 {
                return;
            }
        }
    }

    phase_post_accept_tc_ingress(ctx, info, p);
}

#[inline(always)]
pub(crate) unsafe fn phase_ct_miss_tc_ingress_v4(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
) {
    if (p.flags & FLAG_TCPRT_ON) != 0 && info.proto == IPPROTO_TCP {
        tcprt::track_tcp_rt_v4_auto(p.tap_id, info, p.now, true);
    }

    let need_ids = need_ingress_ids(p);
    if !need_ids {
        phase_post_accept_tc_ingress(ctx, info, p);
        return;
    }

    record_tc_ingress_contract_fallback(p, CT_CONTRACT_FAMILY_IPV4);
    load_packet_ids_v4(info, p);
    if should_apply_ingress_qos(p) {
        phase_qos_ingress_tc(ctx, info, p);
        if p.action == crate::TC_ACT_SHOT as u32 {
            return;
        }
    }
    phase_post_accept_tc_ingress(ctx, info, p);
}

#[inline(always)]
pub(crate) unsafe fn phase_ct_miss_tc_ingress_v6(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
) {
    if (p.flags & FLAG_TCPRT_ON) != 0 && info.proto == IPPROTO_TCP {
        tcprt::track_tcp_rt_v6_auto(p.tap_id, info, p.now, true);
    }

    let need_ids = need_ingress_ids(p);
    if !need_ids {
        phase_post_accept_tc_ingress(ctx, info, p);
        return;
    }

    record_tc_ingress_contract_fallback(p, CT_CONTRACT_FAMILY_IPV6);
    load_packet_ids_v6(info, p);
    if should_apply_ingress_qos(p) {
        phase_qos_ingress_tc(ctx, info, p);
        if p.action == crate::TC_ACT_SHOT as u32 {
            return;
        }
    }
    phase_post_accept_tc_ingress(ctx, info, p);
}

#[inline(always)]
pub(crate) unsafe fn phase_ct_fastpath_tc_v4(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey4,
) {
    let tracing = (p.flags & FLAG_TRACING) != 0;
    if (p.flags & FLAG_ACL_ON) != 0 {
        let matched = get_matched(p);
        stats::update_rule_stats(&matched.to_policy_key(), p.pkt_len, false);
    }
    stats::update_flow_stats_v4(ct_key, p.pkt_len, p.now);

    if (p.flags & FLAG_TCPRT_ON) != 0 && info.proto == IPPROTO_TCP {
        if (p.flags & FLAG_IS_FORWARD) != 0 {
            tcprt::track_tcp_rt_v4(ct_key, info, p.now, true, false);
        } else {
            tcprt::track_tcp_rt_v4_rev(p.tap_id, info, p.now, false);
        }
    }

    let need_ids = (p.flags & FLAG_QOS_ON) != 0
        || (p.flags & FLAG_MIRROR_ON) != 0
        || stats::monitoring_enabled(p.tap_id);
    if need_ids {
        p.dst_id = lookup_ipv4(&DST_IPV4_TRIE, p.tap_id, info.dst_ip).unwrap_or(0);
        p.src_id = lookup_ipv4(&SRC_IPV4_TRIE, p.tap_id, info.src_ip).unwrap_or(0);
        if (p.flags & FLAG_QOS_ON) != 0 {
            let (edt, prio) = qos::apply_qos_egress(p.tap_id, p.src_id, p.dst_id, p.pkt_len, p.now);
            if edt == u64::MAX {
                p.drop_reason = DROP_QOS_EGRESS;
                p.action = crate::TC_ACT_SHOT as u32;
                do_drop(p);
                if tracing {
                    do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_QOS);
                }
                return;
            }
            apply_edt_prio(ctx, edt, prio);
        }
        stats::update_group_stats(p.tap_id, p.src_id, DIR_EGRESS, p.pkt_len);
        stats::update_group_stats(p.tap_id, p.dst_id, DIR_INGRESS, p.pkt_len);
        if (p.flags & FLAG_MIRROR_ON) != 0 {
            let skb = ctx.as_ptr() as *mut __sk_buff;
            mirror::try_mirror_tc(
                skb,
                p.tap_id,
                p.src_id,
                p.dst_id,
                info.proto,
                DIR_EGRESS,
                p.pkt_len,
            );
        }
        if tracing {
            do_trace(ctx, info, p, TRACE_TC_EGRESS, TRACE_RESULT_PASS);
        }
    } else if tracing {
        do_trace(ctx, info, p, TRACE_TC_EGRESS, TRACE_RESULT_PASS);
    }
    p.action = crate::TC_ACT_OK as u32;
}

#[inline(always)]
pub(crate) unsafe fn phase_ct_fastpath_tc_v6(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey6,
) {
    let tracing = (p.flags & FLAG_TRACING) != 0;
    if (p.flags & FLAG_ACL_ON) != 0 {
        let matched = get_matched(p);
        stats::update_rule_stats(&matched.to_policy_key(), p.pkt_len, false);
    }
    stats::update_flow_stats_v6(ct_key, p.pkt_len, p.now);

    if (p.flags & FLAG_TCPRT_ON) != 0 && info.proto == IPPROTO_TCP {
        if (p.flags & FLAG_IS_FORWARD) != 0 {
            tcprt::track_tcp_rt_v6(ct_key, info, p.now, true, false);
        } else {
            tcprt::track_tcp_rt_v6_rev(p.tap_id, info, p.now, false);
        }
    }

    let need_ids = (p.flags & FLAG_QOS_ON) != 0
        || (p.flags & FLAG_MIRROR_ON) != 0
        || stats::monitoring_enabled(p.tap_id);
    if need_ids {
        p.dst_id = lookup_ipv6(&DST_IPV6_TRIE, p.tap_id, info.dst_ip_v6).unwrap_or(0);
        p.src_id = lookup_ipv6(&SRC_IPV6_TRIE, p.tap_id, info.src_ip_v6).unwrap_or(0);
        if (p.flags & FLAG_QOS_ON) != 0 {
            let (edt, prio) = qos::apply_qos_egress(p.tap_id, p.src_id, p.dst_id, p.pkt_len, p.now);
            if edt == u64::MAX {
                p.drop_reason = DROP_QOS_EGRESS;
                p.action = crate::TC_ACT_SHOT as u32;
                do_drop(p);
                if tracing {
                    do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_QOS);
                }
                return;
            }
            apply_edt_prio(ctx, edt, prio);
        }
        stats::update_group_stats(p.tap_id, p.src_id, DIR_EGRESS, p.pkt_len);
        stats::update_group_stats(p.tap_id, p.dst_id, DIR_INGRESS, p.pkt_len);
        if (p.flags & FLAG_MIRROR_ON) != 0 {
            let skb = ctx.as_ptr() as *mut __sk_buff;
            mirror::try_mirror_tc(
                skb,
                p.tap_id,
                p.src_id,
                p.dst_id,
                info.proto,
                DIR_EGRESS,
                p.pkt_len,
            );
        }
        if tracing {
            do_trace(ctx, info, p, TRACE_TC_EGRESS, TRACE_RESULT_PASS);
        }
    } else if tracing {
        do_trace(ctx, info, p, TRACE_TC_EGRESS, TRACE_RESULT_PASS);
    }
    p.action = crate::TC_ACT_OK as u32;
}
