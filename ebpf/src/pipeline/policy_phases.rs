use aya_ebpf::bindings::__sk_buff;
use aya_ebpf::programs::{TcContext, XdpContext};
use aya_ebpf::EbpfContext;

use crate::common::{
    CtKey4, CtKey6, PipelineCtx, DIR_EGRESS, DIR_INGRESS, FLAG_MIRROR_ON, FLAG_TCPRT_ON,
    FLAG_TRACING, IPPROTO_TCP, TRACE_RESULT_PASS, TRACE_TC_DROP, TRACE_TC_EGRESS, TRACE_XDP_DROP,
    XDP_DROP, XDP_PASS,
};
use crate::{conntrack, mirror, parser, policy, stats, tcprt};

use super::ct_phases::{get_matched, set_matched, should_create_ct};
use super::trace_drop::{do_trace, trace_result_from_drop_reason};

#[inline(never)]
pub(crate) unsafe fn phase_policy_xdp(
    ctx: &XdpContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
) {
    let args = policy::PolicyArgs {
        tap_id: p.tap_id,
        src_id: p.src_id,
        dst_id: p.dst_id,
        proto: p.proto,
        direction: p.direction,
        dst_port: info.dst_port,
        pkt_len: p.pkt_len,
        now: p.now,
    };
    let (result, drop_reason, matched, policy_hit) = policy::evaluate_policy(&args);
    p.action = result;
    p.drop_reason = drop_reason;
    set_matched(p, &matched);

    if result == XDP_DROP {
        if policy_hit {
            stats::update_rule_stats(&matched.to_policy_key(), p.pkt_len, true);
        }
        policy::record_policy_drop(&args, drop_reason);
        if (p.flags & FLAG_TRACING) != 0 {
            do_trace(
                ctx,
                info,
                p,
                TRACE_XDP_DROP,
                trace_result_from_drop_reason(drop_reason),
            );
        }
    }
}

#[inline(always)]
pub(crate) unsafe fn phase_policy_tc(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
) {
    let args = policy::PolicyArgs {
        tap_id: p.tap_id,
        src_id: p.src_id,
        dst_id: p.dst_id,
        proto: info.proto,
        direction: p.direction,
        dst_port: info.dst_port,
        pkt_len: p.pkt_len,
        now: p.now,
    };
    let (result, drop_reason, matched, policy_hit) = policy::evaluate_policy(&args);
    if policy_hit {
        policy::account_policy_result(&args, &matched, result, drop_reason);
    }
    p.drop_reason = drop_reason;
    set_matched(p, &matched);

    if result == XDP_PASS {
        p.action = crate::TC_ACT_OK as u32;
    } else {
        p.action = crate::TC_ACT_SHOT as u32;
        if (p.flags & FLAG_TRACING) != 0 {
            do_trace(
                ctx,
                info,
                p,
                TRACE_TC_DROP,
                trace_result_from_drop_reason(drop_reason),
            );
        }
    }
}

#[inline(never)]
pub(crate) unsafe fn phase_flow_tcprt_v4(
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey4,
) {
    stats::update_flow_stats_v4(ct_key, p.pkt_len, p.now);
    if (p.flags & FLAG_TCPRT_ON) != 0 && info.proto == IPPROTO_TCP {
        tcprt::track_tcp_rt_v4_auto(p.tap_id, info, p.now, false);
    }
}

#[inline(never)]
pub(crate) unsafe fn phase_flow_tcprt_v6(
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey6,
) {
    stats::update_flow_stats_v6(ct_key, p.pkt_len, p.now);
    if (p.flags & FLAG_TCPRT_ON) != 0 && info.proto == IPPROTO_TCP {
        tcprt::track_tcp_rt_v6_auto(p.tap_id, info, p.now, false);
    }
}

#[inline(never)]
pub(crate) unsafe fn phase_post_accept_xdp_v4(
    _info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey4,
) {
    if should_create_ct(p) {
        let matched = get_matched(p);
        conntrack::ct_create_v4(ct_key, p.now, p.pkt_len, &matched);
        p.ct_state = 1;
    }
    p.action = XDP_PASS;
}

#[inline(never)]
pub(crate) unsafe fn phase_post_accept_xdp_v6(
    _info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey6,
) {
    if should_create_ct(p) {
        let matched = get_matched(p);
        conntrack::ct_create_v6(ct_key, p.now, p.pkt_len, &matched);
        p.ct_state = 1;
    }
    p.action = XDP_PASS;
}

#[inline(always)]
pub(crate) unsafe fn phase_post_accept_tc_v4(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey4,
) {
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
    if should_create_ct(p) {
        let matched = get_matched(p);
        conntrack::ct_create_v4(ct_key, p.now, p.pkt_len, &matched);
        p.ct_state = 1;
    }
    if (p.flags & FLAG_TRACING) != 0 {
        do_trace(ctx, info, p, TRACE_TC_EGRESS, TRACE_RESULT_PASS);
    }
    p.action = crate::TC_ACT_OK as u32;
}

#[inline(always)]
pub(crate) unsafe fn phase_post_accept_tc_v6(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
    ct_key: &CtKey6,
) {
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
    if should_create_ct(p) {
        let matched = get_matched(p);
        conntrack::ct_create_v6(ct_key, p.now, p.pkt_len, &matched);
        p.ct_state = 1;
    }
    if (p.flags & FLAG_TRACING) != 0 {
        do_trace(ctx, info, p, TRACE_TC_EGRESS, TRACE_RESULT_PASS);
    }
    p.action = crate::TC_ACT_OK as u32;
}
