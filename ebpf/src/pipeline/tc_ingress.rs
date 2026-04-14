use aya_ebpf::bindings::__sk_buff;
use aya_ebpf::helpers::bpf_ktime_get_ns;
use aya_ebpf::programs::TcContext;
use aya_ebpf::EbpfContext;

use crate::common::{
    CtKey4, CtKey6, PipelineCtx, DROP_ANTI_SPOOF, DROP_PORT_IDENTITY_MISS, DROP_SG_INGRESS,
    FLAG_ANTI_SPOOF_PASSED, FLAG_LB_ON, FLAG_TRACING, IPPROTO_TCP, IPPROTO_UDP,
    PORT_FLAG_ANTI_SPOOF, SG_DIR_INGRESS, TRACE_RESULT_DROP_IDENTITY,
    TRACE_RESULT_DROP_SECURITY, TRACE_TC_DROP,
};
use crate::{lb, parser, port, sg, TC_ACT_SHOT};

use super::ct_phases::{
    load_packet_ids, phase_ct_fastpath_tc_ingress_v4, phase_ct_fastpath_tc_ingress_v6,
    phase_ct_miss_tc_ingress_v4, phase_ct_miss_tc_ingress_v6, phase_ct_v4, phase_ct_v6,
};
use super::ctx::{load_feature_flags_tc, load_runtime_ctx_tc};
use super::trace_drop::{do_drop, do_trace};

#[inline(never)]
pub(crate) unsafe fn try_tc_ingress(
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

    if p.tap_id != crate::common::TAP_ID_UNASSIGNED {
        if !port::phase_port_identity(p) {
            load_packet_ids(info, p);
            p.drop_reason = DROP_PORT_IDENTITY_MISS;
            p.action = crate::TC_ACT_SHOT as u32;
            do_drop(p);
            if (p.flags & FLAG_TRACING) != 0 {
                do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_IDENTITY);
            }
            return Ok(TC_ACT_SHOT);
        }

        if (p.port_flags as u16 & PORT_FLAG_ANTI_SPOOF) != 0 {
            let anti_spoof_ok = if info.is_ipv6 {
                port::phase_anti_spoof_v6(info, p)
            } else {
                port::phase_anti_spoof_v4(info, p)
            };
            if !anti_spoof_ok {
                load_packet_ids(info, p);
                p.drop_reason = DROP_ANTI_SPOOF;
                p.action = crate::TC_ACT_SHOT as u32;
                do_drop(p);
                if (p.flags & FLAG_TRACING) != 0 {
                    do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_IDENTITY);
                }
                return Ok(TC_ACT_SHOT);
            }
            p.flags |= FLAG_ANTI_SPOOF_PASSED;
        }

        if !sg::sg_check(p, info, SG_DIR_INGRESS) {
            load_packet_ids(info, p);
            p.drop_reason = DROP_SG_INGRESS;
            p.action = crate::TC_ACT_SHOT as u32;
            do_drop(p);
            if (p.flags & FLAG_TRACING) != 0 {
                do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_SECURITY);
            }
            return Ok(TC_ACT_SHOT);
        }
    }

    // L4 LB: frontend lookup -> backend select -> DNAT (before CT).
    if (p.flags & FLAG_LB_ON) != 0 && (info.proto == IPPROTO_TCP || info.proto == IPPROTO_UDP) {
        let skb = ctx.as_ptr() as *mut __sk_buff;
        if info.is_ipv6 {
            lb::phase_lb_ingress_v6(skb, info, p);
        } else {
            lb::phase_lb_ingress_v4(skb, info, p);
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
            phase_ct_fastpath_tc_ingress_v6(ctx, info, p, &ct_key);
        } else {
            phase_ct_miss_tc_ingress_v6(ctx, info, p);
        }

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
        phase_ct_fastpath_tc_ingress_v4(ctx, info, p, &ct_key);
    } else {
        phase_ct_miss_tc_ingress_v4(ctx, info, p);
    }

    Ok(p.action as i32)
}
