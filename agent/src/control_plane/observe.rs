use super::*;
use serde_json::json;

const OBSERVE_DEFAULT_LIMIT: usize = 200;
const OBSERVE_MAX_LIMIT: usize = 5000;

fn observe_timestamp_or_zero(timestamp: u64) -> String {
    if timestamp == 0 {
        "0".to_string()
    } else {
        timestamp.to_string()
    }
}

fn lb_algo_name(algo: u8) -> String {
    match algo {
        0 => "random".to_string(),
        1 => "maglev".to_string(),
        2 => "hash_src_ip".to_string(),
        _ => format!("algo_{}", algo),
    }
}

fn matches_observe_query(
    event: &aria_api::EventEnvelope,
    query: &aria_api::ObserveQuery,
) -> bool {
    if let Some(event_type) = query.event_type.as_deref() {
        if event.event_type != event_type {
            return false;
        }
    }
    if let Some(src_ip) = query.src_ip.as_deref() {
        if event.src_ip.as_deref() != Some(src_ip) {
            return false;
        }
    }
    if let Some(dst_ip) = query.dst_ip.as_deref() {
        if event.dst_ip.as_deref() != Some(dst_ip) {
            return false;
        }
    }
    if let Some(dst_port) = query.dst_port {
        if event.dst_port != Some(dst_port) {
            return false;
        }
    }
    true
}

fn event_timestamp_key(event: &aria_api::EventEnvelope) -> u64 {
    event.timestamp.parse::<u64>().unwrap_or(0)
}

impl ControlPlane {
    pub async fn observe_events(
        &self,
        query: &aria_api::ObserveQuery,
    ) -> Result<aria_api::ObserveResponse, ControlPlaneError> {
        let limit = query
            .limit
            .unwrap_or(OBSERVE_DEFAULT_LIMIT)
            .min(OBSERVE_MAX_LIMIT);

        let mut events = Vec::new();
        let instances = self.list_instances().await;

        let include_type = |name: &str| {
            query
                .event_type
                .as_deref()
                .map(|requested| requested == name)
                .unwrap_or(true)
        };

        if include_type("tcprt") || include_type("flow") || include_type("drop") || include_type("lb") {
            for instance in &instances {
                if include_type("tcprt") {
                    for (idx, entry) in self.list_tcprt(instance, limit).await?.into_iter().enumerate() {
                        events.push(aria_api::EventEnvelope {
                            event_id: format!("tcprt-{}-{}", instance, idx),
                            event_type: "tcprt".to_string(),
                            timestamp: "0".to_string(),
                            node_id: "local".to_string(),
                            direction: "unknown".to_string(),
                            verdict: "observe".to_string(),
                            hook: "tc".to_string(),
                            tenant_id: None,
                            network_id: None,
                            port_id: None,
                            instance_id: Some(instance.clone()),
                            flow_id: None,
                            service_id: None,
                            trace_id: None,
                            src_ip: Some(entry.src_ip),
                            dst_ip: Some(entry.dst_ip),
                            src_port: Some(entry.src_port),
                            dst_port: Some(entry.dst_port),
                            protocol: Some("tcp".to_string()),
                            payload: json!({
                                "handshake_us": entry.handshake_us,
                                "rtt_client_us": entry.rtt_client_us,
                                "rtt_server_us": entry.rtt_server_us,
                                "art_us": entry.art_us,
                                "retrans_req": entry.retrans_req,
                                "retrans_resp": entry.retrans_resp,
                                "request_count": entry.request_count,
                                "state": entry.state,
                                "forward_platform_us": entry.forward_platform_us,
                                "server_network_us": entry.server_network_us,
                                "reverse_platform_us": entry.reverse_platform_us,
                                "nqa_score": entry.nqa_score
                            }),
                        });
                    }
                }

                if include_type("flow") {
                    let (v4, v6) = self.get_top_flows(instance, limit).await?;
                    for (idx, entry) in v4.into_iter().enumerate() {
                        events.push(aria_api::EventEnvelope {
                            event_id: format!("flow4-{}-{}", instance, idx),
                            event_type: "flow".to_string(),
                            timestamp: observe_timestamp_or_zero(entry.last_seen),
                            node_id: "local".to_string(),
                            direction: "unknown".to_string(),
                            verdict: "pass".to_string(),
                            hook: "tc".to_string(),
                            tenant_id: None,
                            network_id: None,
                            port_id: None,
                            instance_id: Some(instance.clone()),
                            flow_id: None,
                            service_id: None,
                            trace_id: None,
                            src_ip: Some(entry.src_ip.to_string()),
                            dst_ip: Some(entry.dst_ip.to_string()),
                            src_port: Some(entry.src_port),
                            dst_port: Some(entry.dst_port),
                            protocol: Some(aria_api::proto_to_string(entry.proto)),
                            payload: json!({
                                "packets": entry.packets,
                                "bytes": entry.bytes,
                                "last_seen": entry.last_seen
                            }),
                        });
                    }
                    for (idx, entry) in v6.into_iter().enumerate() {
                        events.push(aria_api::EventEnvelope {
                            event_id: format!("flow6-{}-{}", instance, idx),
                            event_type: "flow".to_string(),
                            timestamp: observe_timestamp_or_zero(entry.last_seen),
                            node_id: "local".to_string(),
                            direction: "unknown".to_string(),
                            verdict: "pass".to_string(),
                            hook: "tc".to_string(),
                            tenant_id: None,
                            network_id: None,
                            port_id: None,
                            instance_id: Some(instance.clone()),
                            flow_id: None,
                            service_id: None,
                            trace_id: None,
                            src_ip: Some(entry.src_ip.to_string()),
                            dst_ip: Some(entry.dst_ip.to_string()),
                            src_port: Some(entry.src_port),
                            dst_port: Some(entry.dst_port),
                            protocol: Some(aria_api::proto_to_string(entry.proto)),
                            payload: json!({
                                "packets": entry.packets,
                                "bytes": entry.bytes,
                                "last_seen": entry.last_seen
                            }),
                        });
                    }
                }

                if include_type("drop") {
                    let (entries, groups) = self.get_drop_stats(instance).await?;
                    let find_name = |id: u32| -> String {
                        if id == 0 {
                            return "any".to_string();
                        }
                        groups
                            .values()
                            .find(|g| g.id == id)
                            .map(|g| g.name.clone())
                            .unwrap_or_else(|| format!("id:{}", id))
                    };

                    for (idx, entry) in entries.into_iter().enumerate() {
                        events.push(aria_api::EventEnvelope {
                            event_id: format!("drop-{}-{}", instance, idx),
                            event_type: "drop".to_string(),
                            timestamp: observe_timestamp_or_zero(entry.last_seen),
                            node_id: "local".to_string(),
                            direction: aria_api::direction_to_string(entry.direction),
                            verdict: "drop".to_string(),
                            hook: "tc".to_string(),
                            tenant_id: None,
                            network_id: None,
                            port_id: None,
                            instance_id: Some(instance.clone()),
                            flow_id: None,
                            service_id: None,
                            trace_id: None,
                            src_ip: None,
                            dst_ip: None,
                            src_port: None,
                            dst_port: None,
                            protocol: Some(aria_api::proto_to_string(entry.proto)),
                            payload: json!({
                                "reason": aria_core::trace_ops::drop_reason_name(entry.reason),
                                "src_group": find_name(entry.src_id),
                                "src_id": entry.src_id,
                                "dst_group": find_name(entry.dst_id),
                                "dst_id": entry.dst_id,
                                "packets": entry.packets,
                                "bytes": entry.bytes,
                                "last_seen": entry.last_seen
                            }),
                        });
                    }
                }

                if include_type("lb") {
                    for (idx, entry) in self.get_lb_stats(instance).await?.into_iter().enumerate() {
                        events.push(aria_api::EventEnvelope {
                            event_id: format!("lb-{}-{}", instance, idx),
                            event_type: "lb".to_string(),
                            timestamp: "0".to_string(),
                            node_id: "local".to_string(),
                            direction: "unknown".to_string(),
                            verdict: "lb".to_string(),
                            hook: "tc".to_string(),
                            tenant_id: None,
                            network_id: None,
                            port_id: None,
                            instance_id: Some(instance.clone()),
                            flow_id: None,
                            service_id: Some(entry.service_id.to_string()),
                            trace_id: None,
                            src_ip: None,
                            dst_ip: None,
                            src_port: None,
                            dst_port: None,
                            protocol: None,
                            payload: json!({
                                "service_id": entry.service_id,
                                "backend_slot": entry.backend_slot,
                                "lb_algo": lb_algo_name(entry.lb_algo),
                                "affinity_hit": entry.affinity_hit,
                                "packets": entry.packets,
                                "bytes": entry.bytes
                            }),
                        });
                    }
                }
            }
        }

        if include_type("ssl") {
            for (idx, entry) in self.list_ssl_global(limit).await?.into_iter().enumerate() {
                events.push(aria_api::EventEnvelope {
                    event_id: format!("ssl-{}", idx),
                    event_type: "ssl".to_string(),
                    timestamp: observe_timestamp_or_zero(entry.timestamp),
                    node_id: "local".to_string(),
                    direction: "unknown".to_string(),
                    verdict: "observe".to_string(),
                    hook: "uprobe".to_string(),
                    tenant_id: None,
                    network_id: None,
                    port_id: None,
                    instance_id: None,
                    flow_id: None,
                    service_id: None,
                    trace_id: None,
                    src_ip: None,
                    dst_ip: None,
                    src_port: None,
                    dst_port: None,
                    protocol: Some("tcp".to_string()),
                    payload: json!({
                        "seq": entry.seq,
                        "pid": entry.pid,
                        "tid": entry.tid,
                        "handshake_us": entry.handshake_us,
                        "sni": entry.sni
                    }),
                });
            }
        }

        if include_type("http") {
            for (idx, entry) in self.list_ssl_http_global(limit).await?.into_iter().enumerate() {
                events.push(aria_api::EventEnvelope {
                    event_id: format!("http-{}", idx),
                    event_type: "http".to_string(),
                    timestamp: observe_timestamp_or_zero(entry.response_ts),
                    node_id: "local".to_string(),
                    direction: "unknown".to_string(),
                    verdict: "observe".to_string(),
                    hook: "uprobe".to_string(),
                    tenant_id: None,
                    network_id: None,
                    port_id: None,
                    instance_id: None,
                    flow_id: None,
                    service_id: None,
                    trace_id: None,
                    src_ip: None,
                    dst_ip: None,
                    src_port: None,
                    dst_port: None,
                    protocol: Some("tcp".to_string()),
                    payload: json!({
                        "seq": entry.seq,
                        "pid": entry.pid,
                        "tid": entry.tid,
                        "method": entry.method,
                        "path": entry.path,
                        "host": entry.host,
                        "status_code": entry.status_code,
                        "latency_us": entry.latency_us,
                        "request_ts": entry.request_ts,
                        "response_ts": entry.response_ts
                    }),
                });
            }
        }

        if include_type("kernel_drop") {
            let drops = self
                .get_kernel_drop_stats(&aria_api::KernelDropQuery {
                    instance: None,
                    iface: None,
                    ifindex: None,
                    reason: None,
                    top: Some(limit),
                    include_unattributed: false,
                })
                .await?;
            for (idx, entry) in drops.into_iter().enumerate() {
                events.push(aria_api::EventEnvelope {
                    event_id: format!("kernel-drop-{}", idx),
                    event_type: "kernel_drop".to_string(),
                    timestamp: observe_timestamp_or_zero(entry.last_seen_ns),
                    node_id: "local".to_string(),
                    direction: "unknown".to_string(),
                    verdict: "drop".to_string(),
                    hook: "tracepoint".to_string(),
                    tenant_id: None,
                    network_id: None,
                    port_id: None,
                    instance_id: entry.instance.clone(),
                    flow_id: None,
                    service_id: None,
                    trace_id: None,
                    src_ip: None,
                    dst_ip: None,
                    src_port: None,
                    dst_port: None,
                    protocol: Some(entry.proto.clone()),
                    payload: json!({
                        "iface": entry.iface,
                        "ifindex": entry.ifindex,
                        "reason_code": entry.reason_code,
                        "reason": entry.reason,
                        "packets": entry.packets,
                        "bytes": entry.bytes,
                        "last_location": entry.last_location,
                        "location": entry.location,
                        "location_hint": entry.location_hint,
                        "source": entry.source
                    }),
                });
            }
        }

        events.retain(|event| matches_observe_query(event, query));
        events.sort_by(|a, b| event_timestamp_key(b).cmp(&event_timestamp_key(a)));
        events.truncate(limit);

        Ok(aria_api::ObserveResponse { events })
    }
}
