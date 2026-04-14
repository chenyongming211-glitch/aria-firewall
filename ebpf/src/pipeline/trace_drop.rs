use aya_ebpf::EbpfContext;

use crate::common::{
    PipelineCtx, DROP_ANTI_SPOOF, DROP_PORT_IDENTITY_MISS, DROP_ROUTE_BLACKHOLE, DROP_ROUTE_MISS,
    DROP_SG_EGRESS, DROP_SG_INGRESS, TRACE_RESULT_DROP_ACL, TRACE_RESULT_DROP_ACL_DEFAULT,
    TRACE_RESULT_DROP_ACL_PORT, TRACE_RESULT_DROP_IDENTITY, TRACE_RESULT_DROP_ROUTE,
    TRACE_RESULT_DROP_SECURITY,
};
use crate::{drops, parser, trace};

/// Inline helper: emit a trace event from PipelineCtx.
#[inline(always)]
pub(crate) unsafe fn do_trace<C: EbpfContext>(
    ctx: &C,
    info: &parser::PacketInfo,
    p: &PipelineCtx,
    hook: u8,
    result: u8,
) {
    trace::trace_event(
        ctx,
        p.tap_id,
        info,
        &trace::TraceArgs {
            hook,
            result,
            direction: p.direction,
            ct_state: p.ct_state,
            drop_reason: p.drop_reason,
            _pad: [0; 3],
            src_id: p.src_id,
            dst_id: p.dst_id,
            pkt_len: p.pkt_len,
            now: p.now,
        },
    );
}

#[inline(always)]
pub(crate) fn trace_result_from_drop_reason(drop_reason: u8) -> u8 {
    match drop_reason {
        1 => TRACE_RESULT_DROP_ACL,
        2 => TRACE_RESULT_DROP_ACL_PORT,
        3 => TRACE_RESULT_DROP_ACL_DEFAULT,
        DROP_PORT_IDENTITY_MISS | DROP_ANTI_SPOOF => TRACE_RESULT_DROP_IDENTITY,
        DROP_SG_INGRESS | DROP_SG_EGRESS => TRACE_RESULT_DROP_SECURITY,
        DROP_ROUTE_MISS | DROP_ROUTE_BLACKHOLE => TRACE_RESULT_DROP_ROUTE,
        _ => TRACE_RESULT_DROP_ACL,
    }
}

/// Inline helper: record a drop from PipelineCtx.
#[inline(always)]
pub(crate) unsafe fn do_drop(p: &PipelineCtx) {
    drops::record_drop(&drops::DropArgs {
        tap_id: p.tap_id,
        reason: p.drop_reason,
        direction: p.direction,
        proto: p.proto,
        src_id: p.src_id,
        dst_id: p.dst_id,
        pkt_len: p.pkt_len,
        now: p.now,
        _pad: 0,
    });
}
