use aya_ebpf::bindings::{__sk_buff, xdp_md};
use aya_ebpf::programs::{TcContext, XdpContext};

use crate::common::{
    PipelineCtx, FLAG_ACL_ON, FLAG_LB_ON, FLAG_MIRROR_ON, FLAG_QOS_ON, FLAG_TCPRT_ON,
    FLAG_TRACING, IPPROTO_TCP, TAP_ID_UNASSIGNED,
};
use crate::{maps, mirror, parser, policy, qos, runtime, tcprt, trace};

#[inline(always)]
pub(crate) unsafe fn load_feature_flags_xdp(p: &mut PipelineCtx, info: &parser::PacketInfo) {
    if policy::acl_enabled(p.tap_id) {
        p.flags |= FLAG_ACL_ON;
    }
    if trace::should_trace(p.tap_id, info) {
        p.flags |= FLAG_TRACING;
    }
}

#[inline(always)]
pub(crate) unsafe fn load_feature_flags_tc(p: &mut PipelineCtx, info: &parser::PacketInfo) {
    if qos::qos_enabled(p.tap_id) {
        p.flags |= FLAG_QOS_ON;
    }
    if tcprt::tcprt_enabled(p.tap_id) {
        p.flags |= FLAG_TCPRT_ON;
    }
    if policy::acl_enabled(p.tap_id) {
        p.flags |= FLAG_ACL_ON;
    }
    if mirror::mirror_enabled(p.tap_id) {
        p.flags |= FLAG_MIRROR_ON;
    }
    if runtime::lb_enabled(p.tap_id) {
        p.flags |= FLAG_LB_ON;
    }
    if trace::should_trace(p.tap_id, info) {
        p.flags |= FLAG_TRACING;
    }
}

#[inline(always)]
unsafe fn resolve_tap_id_for_ifindex(ifindex: u32) -> u32 {
    if ifindex == 0 {
        return TAP_ID_UNASSIGNED;
    }
    if let Some(ctx) = maps::IFACE_CTX_MAP.get(&ifindex) {
        ctx.tap_id
    } else {
        TAP_ID_UNASSIGNED
    }
}

#[inline(always)]
pub(crate) unsafe fn load_runtime_ctx_xdp(ctx: &XdpContext, p: &mut PipelineCtx) {
    let xdp = ctx.as_ptr() as *const xdp_md;
    p.tap_id = resolve_tap_id_for_ifindex((*xdp).ingress_ifindex);
}

#[inline(always)]
pub(crate) unsafe fn load_runtime_ctx_tc(ctx: &TcContext, p: &mut PipelineCtx) {
    let skb = ctx.as_ptr() as *const __sk_buff;
    p.tap_id = resolve_tap_id_for_ifindex((*skb).ifindex);
}

#[inline(always)]
pub(crate) unsafe fn parse_tc_packet(ctx: &TcContext, out: *mut parser::PacketInfo) -> bool {
    let mut data = ctx.data();
    let mut data_end = ctx.data_end();
    let mut parsed = parser::parse_eth_ipv4(data, data_end, 0, out)
        || parser::parse_eth_ipv6(data, data_end, 0, out);
    if !parsed {
        return false;
    }

    let info = &*out;
    // TC direct packet access can stop at the linear head on non-linear skbs.
    // That leaves ports available but zeros TCP seq/flags/payload, which breaks
    // TCP-RT while leaving port-based features apparently healthy. Re-pull only
    // for this suspicious truncated TCP shape and re-parse.
    if info.proto == IPPROTO_TCP && info.tcp_flags == 0 && info.tcp_seq == 0 {
        if ctx.pull_data(0).is_ok() {
            data = ctx.data();
            data_end = ctx.data_end();
            parsed = parser::parse_eth_ipv4(data, data_end, 0, out)
                || parser::parse_eth_ipv6(data, data_end, 0, out);
        }
    }

    parsed
}
