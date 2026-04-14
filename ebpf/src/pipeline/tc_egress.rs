use aya_ebpf::bindings::__sk_buff;
use aya_ebpf::helpers::bpf_ktime_get_ns;
use aya_ebpf::programs::TcContext;
use aya_ebpf::EbpfContext;

use crate::common::{
    CtKey4, CtKey6, PipelineCtx, FLAG_ACL_ON, FLAG_LB_ON, FLAG_QOS_ON, IPPROTO_TCP,
    IPPROTO_UDP,
};
use crate::maps::{DST_IPV4_TRIE, DST_IPV6_TRIE, SRC_IPV4_TRIE, SRC_IPV6_TRIE};
use crate::{lb, parser, TC_ACT_SHOT};

use super::ct_phases::{
    lookup_ipv4, lookup_ipv6, phase_ct_fastpath_tc_v4, phase_ct_fastpath_tc_v6, phase_ct_v4,
    phase_ct_v6,
};
use super::ctx::{load_feature_flags_tc, load_runtime_ctx_tc};
use super::policy_phases::{
    phase_flow_tcprt_v4, phase_flow_tcprt_v6, phase_policy_tc, phase_post_accept_tc_v4,
    phase_post_accept_tc_v6,
};
use super::qos_phases::phase_qos_egress_tc;

#[inline(never)]
pub(crate) unsafe fn try_tc_egress(
    ctx: &TcContext,
    info: *const parser::PacketInfo,
    pipe: *mut PipelineCtx,
) -> Result<i32, ()> {
    let info = &*info;
    let p = &mut *pipe;

    p.now = bpf_ktime_get_ns();
    p.proto = info.proto;
    load_runtime_ctx_tc(ctx, p);
    load_feature_flags_tc(p, info);

    // L4 LB RevNat: rewrite backend src -> VIP src (before CT).
    if (p.flags & FLAG_LB_ON) != 0 && (info.proto == IPPROTO_TCP || info.proto == IPPROTO_UDP) {
        let skb = ctx.as_ptr() as *mut __sk_buff;
        if info.is_ipv6 {
            lb::phase_lb_egress_v6(skb, info, p);
        } else {
            lb::phase_lb_egress_v4(skb, info, p);
        }
    }

    if info.is_ipv6 {
        let ct_key = CtKey6 {
            tap_id: p.tap_id,
            src_ip: info.src_ip_v6,
            dst_ip: info.dst_ip_v6,
            src_port: info.src_port,
            dst_port: info.dst_port,
            proto: info.proto,
            pad: [0; 3],
        };

        phase_ct_v6(info, p, &ct_key);

        if p.ct_state >= 2 {
            phase_ct_fastpath_tc_v6(ctx, info, p, &ct_key);
            return Ok(p.action as i32);
        }

        p.src_id = lookup_ipv6(&SRC_IPV6_TRIE, p.tap_id, info.src_ip_v6).unwrap_or(0);
        p.dst_id = lookup_ipv6(&DST_IPV6_TRIE, p.tap_id, info.dst_ip_v6).unwrap_or(0);

        if (p.flags & FLAG_ACL_ON) != 0 {
            phase_policy_tc(ctx, info, p);
            if p.action == TC_ACT_SHOT as u32 {
                return Ok(TC_ACT_SHOT);
            }
        }

        phase_flow_tcprt_v6(info, p, &ct_key);

        if (p.flags & FLAG_QOS_ON) != 0 {
            phase_qos_egress_tc(ctx, info, p);
            if p.action == TC_ACT_SHOT as u32 {
                return Ok(TC_ACT_SHOT);
            }
        }

        phase_post_accept_tc_v6(ctx, info, p, &ct_key);

        return Ok(p.action as i32);
    }

    let ct_key = CtKey4 {
        tap_id: p.tap_id,
        src_ip: info.src_ip,
        dst_ip: info.dst_ip,
        src_port: info.src_port,
        dst_port: info.dst_port,
        proto: info.proto,
        pad: [0; 3],
    };

    phase_ct_v4(info, p, &ct_key);

    if p.ct_state >= 2 {
        phase_ct_fastpath_tc_v4(ctx, info, p, &ct_key);
        return Ok(p.action as i32);
    }

    p.src_id = lookup_ipv4(&SRC_IPV4_TRIE, p.tap_id, info.src_ip).unwrap_or(0);
    p.dst_id = lookup_ipv4(&DST_IPV4_TRIE, p.tap_id, info.dst_ip).unwrap_or(0);

    if (p.flags & FLAG_ACL_ON) != 0 {
        phase_policy_tc(ctx, info, p);
        if p.action == TC_ACT_SHOT as u32 {
            return Ok(TC_ACT_SHOT);
        }
    }

    phase_flow_tcprt_v4(info, p, &ct_key);

    if (p.flags & FLAG_QOS_ON) != 0 {
        phase_qos_egress_tc(ctx, info, p);
        if p.action == TC_ACT_SHOT as u32 {
            return Ok(TC_ACT_SHOT);
        }
    }

    phase_post_accept_tc_v4(ctx, info, p, &ct_key);

    Ok(p.action as i32)
}
