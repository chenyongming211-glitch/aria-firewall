#![no_std]
#![no_main]

use aya_ebpf::macros::{classifier, uprobe, uretprobe, xdp};
use aya_ebpf::programs::{ProbeContext, RetProbeContext, TcContext, XdpContext};

mod common;
mod conntrack;
mod ct_contract;
mod drops;
mod kernel_drops;
mod lb;
mod maps;
mod mirror;
mod parser;
mod pipeline;
mod policy;
mod port;
mod qos;
mod route;
mod runtime;
mod sg;
mod ssl;
mod stats;
mod tcprt;
mod trace;

use common::{DIR_EGRESS, DIR_INGRESS, TAP_ID_UNASSIGNED, XDP_PASS};
use pipeline::{
    ctx::parse_tc_packet,
    tc_egress::try_tc_egress,
    tc_ingress::try_tc_ingress,
    xdp::try_xdp_firewall,
};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

const TC_ACT_OK: i32 = 0;
const TC_ACT_SHOT: i32 = 2;

// --- XDP Ingress ---

#[xdp]
pub fn xdp_firewall(ctx: XdpContext) -> u32 {
    let data = ctx.data();
    let data_end = ctx.data_end();
    let pkt_len = (data_end - data) as u32;
    unsafe {
        let info_ptr = match maps::PKT_SCRATCH.get_ptr_mut(0) {
            Some(p) => p,
            None => return XDP_PASS,
        };
        if !parser::parse_eth_ipv4(data, data_end, 0, info_ptr)
            && !parser::parse_eth_ipv6(data, data_end, 0, info_ptr)
        {
            return XDP_PASS;
        }
        let pipe = match maps::PIPE_SCRATCH.get_ptr_mut(0) {
            Some(p) => p,
            None => return XDP_PASS,
        };
        (*pipe).pkt_len = pkt_len;
        (*pipe).tap_id = TAP_ID_UNASSIGNED;
        (*pipe).direction = DIR_INGRESS;
        (*pipe).action = XDP_PASS;
        (*pipe).flags = 0;
        (*pipe).ct_state = 0;
        (*pipe).drop_reason = 0;
        (*pipe).matched_src_id = 0;
        (*pipe).matched_dst_id = 0;
        (*pipe).matched_proto = 0;
        (*pipe).matched_direction = 0;
        (*pipe).port_network_id = 0;
        (*pipe).port_segment_id = 0;
        (*pipe).port_sg_id = 0;
        (*pipe).route_id = 0;
        (*pipe).route_next_hop_type = 0;
        (*pipe).port_flags = 0;
        (*pipe).route_egress_ifindex = 0;
        (*pipe).lb_backend_ip = [0; 16];
        (*pipe).lb_backend_port = 0;
        (*pipe).lb_service_id = 0;
        (*pipe).lb_slot = 0;
        (*pipe).lb_algo = 0;
        (*pipe).lb_ct_flags = 0;
        match try_xdp_firewall(&ctx, info_ptr, pipe) {
            Ok(ret) => ret,
            Err(_) => XDP_PASS,
        }
    }
}

#[classifier]
pub fn tc_egress(ctx: TcContext) -> i32 {
    let pkt_len = ctx.len();
    unsafe {
        let info_ptr = match maps::PKT_SCRATCH.get_ptr_mut(0) {
            Some(p) => p,
            None => return TC_ACT_OK,
        };
        if !parse_tc_packet(&ctx, info_ptr) {
            return TC_ACT_OK;
        }
        let pipe = match maps::PIPE_SCRATCH.get_ptr_mut(0) {
            Some(p) => p,
            None => return TC_ACT_OK,
        };
        (*pipe).pkt_len = pkt_len;
        (*pipe).tap_id = TAP_ID_UNASSIGNED;
        (*pipe).direction = DIR_EGRESS;
        (*pipe).action = TC_ACT_OK as u32;
        (*pipe).flags = 0;
        (*pipe).ct_state = 0;
        (*pipe).drop_reason = 0;
        (*pipe).matched_src_id = 0;
        (*pipe).matched_dst_id = 0;
        (*pipe).matched_proto = 0;
        (*pipe).matched_direction = 0;
        (*pipe).port_network_id = 0;
        (*pipe).port_segment_id = 0;
        (*pipe).port_sg_id = 0;
        (*pipe).route_id = 0;
        (*pipe).route_next_hop_type = 0;
        (*pipe).port_flags = 0;
        (*pipe).route_egress_ifindex = 0;
        (*pipe).lb_backend_ip = [0; 16];
        (*pipe).lb_backend_port = 0;
        (*pipe).lb_service_id = 0;
        (*pipe).lb_slot = 0;
        (*pipe).lb_algo = 0;
        (*pipe).lb_ct_flags = 0;
        match try_tc_egress(&ctx, info_ptr, pipe) {
            Ok(ret) => ret,
            Err(_) => TC_ACT_OK,
        }
    }
}

#[classifier]
pub fn tc_ingress(ctx: TcContext) -> i32 {
    let pkt_len = ctx.len();
    unsafe {
        let info_ptr = match maps::PKT_SCRATCH.get_ptr_mut(0) {
            Some(p) => p,
            None => return TC_ACT_OK,
        };
        if !parse_tc_packet(&ctx, info_ptr) {
            return TC_ACT_OK;
        }
        let pipe = match maps::PIPE_SCRATCH.get_ptr_mut(0) {
            Some(p) => p,
            None => return TC_ACT_OK,
        };
        (*pipe).pkt_len = pkt_len;
        (*pipe).tap_id = TAP_ID_UNASSIGNED;
        (*pipe).direction = DIR_INGRESS;
        (*pipe).action = TC_ACT_OK as u32;
        (*pipe).flags = 0;
        (*pipe).ct_state = 0;
        (*pipe).drop_reason = 0;
        (*pipe).matched_src_id = 0;
        (*pipe).matched_dst_id = 0;
        (*pipe).matched_proto = 0;
        (*pipe).matched_direction = 0;
        (*pipe).port_network_id = 0;
        (*pipe).port_segment_id = 0;
        (*pipe).port_sg_id = 0;
        (*pipe).route_id = 0;
        (*pipe).route_next_hop_type = 0;
        (*pipe).port_flags = 0;
        (*pipe).route_egress_ifindex = 0;
        (*pipe).lb_backend_ip = [0; 16];
        (*pipe).lb_backend_port = 0;
        (*pipe).lb_service_id = 0;
        (*pipe).lb_slot = 0;
        (*pipe).lb_algo = 0;
        (*pipe).lb_ct_flags = 0;
        match try_tc_ingress(&ctx, info_ptr, pipe) {
            Ok(ret) => ret,
            Err(_) => TC_ACT_OK,
        }
    }
}

// --- SSL uprobe entry points ---

#[uprobe]
pub fn ssl_handshake_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_handshake_entry_impl(&ctx) }
}

#[uretprobe]
pub fn ssl_handshake_return(ctx: RetProbeContext) -> u32 {
    unsafe { ssl::ssl_handshake_return_impl(&ctx) }
}

#[uprobe]
pub fn ssl_connect_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_connect_entry_impl(&ctx) }
}

#[uretprobe]
pub fn ssl_connect_return(ctx: RetProbeContext) -> u32 {
    unsafe { ssl::ssl_connect_return_impl(&ctx) }
}

#[uprobe]
pub fn ssl_accept_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_accept_entry_impl(&ctx) }
}

#[uretprobe]
pub fn ssl_accept_return(ctx: RetProbeContext) -> u32 {
    unsafe { ssl::ssl_accept_return_impl(&ctx) }
}

#[uprobe]
pub fn ssl_set_connect_state(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_set_connect_state_impl(&ctx) }
}

#[uprobe]
pub fn ssl_set_accept_state(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_set_accept_state_impl(&ctx) }
}

#[uprobe]
pub fn ssl_shutdown_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_shutdown_entry_impl(&ctx) }
}

#[uprobe]
pub fn ssl_free_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_free_entry_impl(&ctx) }
}

#[uprobe]
pub fn ssl_set_sni(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_set_sni_impl(&ctx) }
}

#[uprobe]
pub fn ssl_write_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_write_entry_impl(&ctx) }
}

#[uprobe]
pub fn ssl_write_ex_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_write_entry_impl(&ctx) }
}

#[uretprobe]
pub fn ssl_write_return(ctx: RetProbeContext) -> u32 {
    unsafe { ssl::ssl_write_return_impl(&ctx) }
}

#[uretprobe]
pub fn ssl_write_ex_return(ctx: RetProbeContext) -> u32 {
    unsafe { ssl::ssl_write_return_impl(&ctx) }
}

#[uprobe]
pub fn ssl_read_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_read_entry_impl(&ctx) }
}

#[uprobe]
pub fn ssl_read_ex_entry(ctx: ProbeContext) -> u32 {
    unsafe { ssl::ssl_read_ex_entry_impl(&ctx) }
}

#[uretprobe]
pub fn ssl_read_return(ctx: RetProbeContext) -> u32 {
    unsafe { ssl::ssl_read_return_impl(&ctx) }
}

#[uretprobe]
pub fn ssl_read_ex_return(ctx: RetProbeContext) -> u32 {
    unsafe { ssl::ssl_read_ex_return_impl(&ctx) }
}
