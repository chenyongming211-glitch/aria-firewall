use aya_ebpf::helpers::bpf_ktime_get_ns;
use aya_ebpf::programs::XdpContext;

use crate::common::{CtKey4, CtKey6, PipelineCtx, FLAG_ACL_ON, XDP_DROP};
use crate::maps::{DST_IPV4_TRIE, DST_IPV6_TRIE, SRC_IPV4_TRIE, SRC_IPV6_TRIE};
use crate::parser;

use super::ct_phases::{
    lookup_ipv4, lookup_ipv6, phase_ct_fastpath_xdp_v4, phase_ct_fastpath_xdp_v6, phase_ct_v4,
    phase_ct_v6,
};
use super::ctx::{load_feature_flags_xdp, load_runtime_ctx_xdp};
use super::policy_phases::{
    phase_policy_xdp, phase_post_accept_xdp_v4, phase_post_accept_xdp_v6,
};

#[inline(never)]
pub(crate) unsafe fn try_xdp_firewall(
    ctx: &XdpContext,
    info: *const parser::PacketInfo,
    pipe: *mut PipelineCtx,
) -> Result<u32, ()> {
    let info = &*info;
    let p = &mut *pipe;

    p.now = bpf_ktime_get_ns();
    p.proto = info.proto;
    load_runtime_ctx_xdp(ctx, p);
    load_feature_flags_xdp(p, info);

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
            phase_ct_fastpath_xdp_v6(info, p, &ct_key);
            return Ok(p.action);
        }

        if (p.flags & FLAG_ACL_ON) != 0 {
            p.src_id = lookup_ipv6(&SRC_IPV6_TRIE, p.tap_id, info.src_ip_v6).unwrap_or(0);
            p.dst_id = lookup_ipv6(&DST_IPV6_TRIE, p.tap_id, info.dst_ip_v6).unwrap_or(0);
            phase_policy_xdp(ctx, info, p);
            if p.action == XDP_DROP {
                return Ok(p.action);
            }
        }

        phase_post_accept_xdp_v6(info, p, &ct_key);

        return Ok(p.action);
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
        phase_ct_fastpath_xdp_v4(info, p, &ct_key);
        return Ok(p.action);
    }

    if (p.flags & FLAG_ACL_ON) != 0 {
        p.src_id = lookup_ipv4(&SRC_IPV4_TRIE, p.tap_id, info.src_ip).unwrap_or(0);
        p.dst_id = lookup_ipv4(&DST_IPV4_TRIE, p.tap_id, info.dst_ip).unwrap_or(0);
        phase_policy_xdp(ctx, info, p);
        if p.action == XDP_DROP {
            return Ok(p.action);
        }
    }

    phase_post_accept_xdp_v4(info, p, &ct_key);

    Ok(p.action)
}
