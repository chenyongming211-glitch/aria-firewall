use aya_ebpf::bindings::__sk_buff;
use aya_ebpf::programs::TcContext;
use aya_ebpf::EbpfContext;

use crate::common::{
    PipelineCtx, DROP_QOS_EGRESS, DROP_QOS_INGRESS, TRACE_RESULT_DROP_QOS, TRACE_TC_DROP,
};
use crate::{parser, qos, TC_ACT_SHOT};

use super::trace_drop::{do_drop, do_trace};

#[inline(always)]
pub(crate) unsafe fn should_apply_ingress_qos(p: &PipelineCtx) -> bool {
    // Ingress QoS is a standalone feature. TC ingress can enforce policing
    // on both CT hits and CT-miss fallback paths after doing its own ID lookup.
    (p.flags & crate::common::FLAG_QOS_ON) != 0
}

#[inline(always)]
pub(crate) unsafe fn phase_qos_ingress_tc(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
) {
    if !qos::apply_qos_ingress(p.tap_id, p.src_id, p.dst_id, p.pkt_len, p.now) {
        p.drop_reason = DROP_QOS_INGRESS;
        p.action = TC_ACT_SHOT as u32;
        do_drop(p);
        if (p.flags & crate::common::FLAG_TRACING) != 0 {
            do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_QOS);
        }
    }
}

/// Phase: QoS egress for TC. Sets p.action = TC_ACT_SHOT if dropped.
#[inline(always)]
pub(crate) unsafe fn phase_qos_egress_tc(
    ctx: &TcContext,
    info: &parser::PacketInfo,
    p: &mut PipelineCtx,
) {
    let (edt, prio) = qos::apply_qos_egress(p.tap_id, p.src_id, p.dst_id, p.pkt_len, p.now);
    if edt == u64::MAX {
        p.drop_reason = DROP_QOS_EGRESS;
        p.action = TC_ACT_SHOT as u32;
        do_drop(p);
        if (p.flags & crate::common::FLAG_TRACING) != 0 {
            do_trace(ctx, info, p, TRACE_TC_DROP, TRACE_RESULT_DROP_QOS);
        }
        return;
    }
    apply_edt_prio(ctx, edt, prio);
}

/// Apply EDT timestamp and priority to skb.
#[inline(always)]
pub(crate) unsafe fn apply_edt_prio(ctx: &TcContext, edt: u64, prio: u8) {
    if edt != 0 || prio != 0 {
        let skb = ctx.as_ptr() as *mut __sk_buff;
        if edt != 0 {
            (*skb).tstamp = edt;
        }
        if prio != 0 {
            (*skb).priority = prio as u32;
        }
    }
}
