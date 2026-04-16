use aya_ebpf::bindings::{__sk_buff, xdp_md};
use aya_ebpf::programs::{TcContext, XdpContext};
use aya_ebpf::EbpfContext;

use crate::common::{
    PipelineCtx, FLAG_ACL_ON, FLAG_CT_ON, FLAG_LB_ON, FLAG_MIRROR_ON, FLAG_MONITORING_ON,
    FLAG_QOS_ON, FLAG_TCPRT_ON, FLAG_TRACING, IPPROTO_TCP, TAP_ID_UNASSIGNED,
};
use crate::{maps, parser, trace};

#[inline(always)]
pub(crate) unsafe fn load_feature_flags_xdp(p: &mut PipelineCtx, info: &parser::PacketInfo) {
    if p.tap_id == TAP_ID_UNASSIGNED {
        // Fallback to global config for CT and monitoring
        if let Some(gcfg) = maps::FIREWALL_CONFIG.get(&0u32) {
            if gcfg.conntrack_enabled != 0 {
                p.flags |= FLAG_CT_ON;
            }
            if gcfg.monitoring_enabled != 0 {
                p.flags |= FLAG_MONITORING_ON;
            }
            if gcfg.acl_enabled != 0 {
                p.flags |= FLAG_ACL_ON;
            }
        } else {
            // Hardcoded defaults: CT and monitoring on, ACL on
            p.flags |= FLAG_CT_ON | FLAG_MONITORING_ON | FLAG_ACL_ON;
        }
    } else if let Some(cfg) = maps::TAP_CONFIG_MAP.get(&p.tap_id) {
        if cfg.conntrack_enabled != 0 {
            p.flags |= FLAG_CT_ON;
        }
        if cfg.monitoring_enabled != 0 {
            p.flags |= FLAG_MONITORING_ON;
        }
        if cfg.acl_enabled != 0 {
            p.flags |= FLAG_ACL_ON;
        }
    } else {
        // No per-tap config, fallback to global
        if let Some(gcfg) = maps::FIREWALL_CONFIG.get(&0u32) {
            if gcfg.conntrack_enabled != 0 {
                p.flags |= FLAG_CT_ON;
            }
            if gcfg.monitoring_enabled != 0 {
                p.flags |= FLAG_MONITORING_ON;
            }
            if gcfg.acl_enabled != 0 {
                p.flags |= FLAG_ACL_ON;
            }
        } else {
            p.flags |= FLAG_CT_ON | FLAG_MONITORING_ON | FLAG_ACL_ON;
        }
    }
    if trace::should_trace(p.tap_id, info) {
        p.flags |= FLAG_TRACING;
    }
}

#[inline(always)]
pub(crate) unsafe fn load_feature_flags_tc(p: &mut PipelineCtx, info: &parser::PacketInfo) {
    if p.tap_id == TAP_ID_UNASSIGNED {
        // Fallback to global config for CT and monitoring
        if let Some(gcfg) = maps::FIREWALL_CONFIG.get(&0u32) {
            if gcfg.conntrack_enabled != 0 {
                p.flags |= FLAG_CT_ON;
            }
            if gcfg.monitoring_enabled != 0 {
                p.flags |= FLAG_MONITORING_ON;
            }
        } else {
            // Hardcoded defaults: CT and monitoring on
            p.flags |= FLAG_CT_ON | FLAG_MONITORING_ON;
        }
        return;
    }
    if let Some(cfg) = maps::TAP_CONFIG_MAP.get(&p.tap_id) {
        if cfg.qos_enabled != 0 {
            p.flags |= FLAG_QOS_ON;
        }
        if cfg.tcprt_enabled != 0 {
            p.flags |= FLAG_TCPRT_ON;
        }
        if cfg.acl_enabled != 0 {
            p.flags |= FLAG_ACL_ON;
        }
        if cfg.mirror_enabled != 0 {
            p.flags |= FLAG_MIRROR_ON;
        }
        if cfg.lb_enabled != 0 {
            p.flags |= FLAG_LB_ON;
        }
        if cfg.conntrack_enabled != 0 {
            p.flags |= FLAG_CT_ON;
        }
        if cfg.monitoring_enabled != 0 {
            p.flags |= FLAG_MONITORING_ON;
        }
    } else {
        // No per-tap config, fallback to global
        if let Some(gcfg) = maps::FIREWALL_CONFIG.get(&0u32) {
            if gcfg.conntrack_enabled != 0 {
                p.flags |= FLAG_CT_ON;
            }
            if gcfg.monitoring_enabled != 0 {
                p.flags |= FLAG_MONITORING_ON;
            }
        } else {
            p.flags |= FLAG_CT_ON | FLAG_MONITORING_ON;
        }
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
