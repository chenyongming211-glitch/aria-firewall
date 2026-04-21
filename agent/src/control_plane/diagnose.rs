use super::*;
use serde_json::json;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

fn unix_ts_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn next_suggestion(suggestions: &mut Vec<String>, suggestion: &str) {
    if !suggestions.iter().any(|item| item == suggestion) {
        suggestions.push(suggestion.to_string());
    }
}

impl ControlPlane {
    pub async fn diagnose_instance(
        &self,
        instance: &str,
        req: &aria_api::DiagnoseRequest,
    ) -> Result<aria_api::DiagnoseResponse, ControlPlaneError> {
        if req.dst_ip.parse::<IpAddr>().is_err() {
            return Err(ControlPlaneError::ValidationError(format!(
                "invalid dst_ip '{}'",
                req.dst_ip
            )));
        }

        self.get_instance(instance).await?;

        let mut evidence = Vec::new();
        let mut candidate_causes = Vec::new();
        let mut suggested_actions = Vec::new();
        let mut verdict = "healthy".to_string();

        let tcprt = self
            .filter_tcprt(&req.dst_ip, req.dst_port)
            .await?
            .into_iter()
            .filter(|(name, _)| name == instance)
            .collect::<Vec<_>>();
        if !tcprt.is_empty() {
            let flow_count: u32 = tcprt.iter().map(|(_, entries)| entries.len() as u32).sum();
            let instance_count = tcprt.len() as f64;
            let avg_nqa_score = tcprt
                .iter()
                .map(|(_, entries)| {
                    let len = entries.len() as f64;
                    entries.iter().map(|entry| entry.nqa_score as f64).sum::<f64>() / len
                })
                .sum::<f64>()
                / instance_count;
            let avg_handshake_us = tcprt
                .iter()
                .map(|(_, entries)| {
                    let len = entries.len() as f64;
                    entries.iter().map(|entry| entry.handshake_us).sum::<f64>() / len
                })
                .sum::<f64>()
                / instance_count;
            let avg_rtt_client_us = tcprt
                .iter()
                .map(|(_, entries)| {
                    let len = entries.len() as f64;
                    entries.iter().map(|entry| entry.rtt_client_us).sum::<f64>() / len
                })
                .sum::<f64>()
                / instance_count;
            let total_retrans_req: u32 = tcprt
                .iter()
                .flat_map(|(_, entries)| entries.iter())
                .map(|entry| entry.retrans_req)
                .sum();
            let total_retrans_resp: u32 = tcprt
                .iter()
                .flat_map(|(_, entries)| entries.iter())
                .map(|entry| entry.retrans_resp)
                .sum();

            let (severity, title, summary) = if avg_nqa_score < 50.0 {
                verdict = "unhealthy".to_string();
                candidate_causes.push(format!("NQA={:.0} (<50)", avg_nqa_score));
                next_suggestion(
                    &mut suggested_actions,
                    "Inspect TCP retransmissions and RTT trends for the destination",
                );
                (
                    "critical",
                    "Transport quality unhealthy",
                    format!(
                        "Average NQA score is {:.1}, below the unhealthy threshold",
                        avg_nqa_score
                    ),
                )
            } else if avg_nqa_score < 80.0 {
                if verdict != "unhealthy" {
                    verdict = "degraded".to_string();
                }
                candidate_causes.push(format!("NQA={:.0} (<80)", avg_nqa_score));
                next_suggestion(
                    &mut suggested_actions,
                    "Inspect TCP retransmissions and RTT trends for the destination",
                );
                (
                    "warning",
                    "Transport quality degraded",
                    format!(
                        "Average NQA score is {:.1}, below the healthy threshold",
                        avg_nqa_score
                    ),
                )
            } else {
                (
                    "info",
                    "Transport quality normal",
                    format!(
                        "Average NQA score is {:.1} with {} observed flows",
                        avg_nqa_score, flow_count
                    ),
                )
            };

            evidence.push(aria_api::DiagnoseEvidence {
                evidence_type: "tcprt".to_string(),
                severity: severity.to_string(),
                title: title.to_string(),
                summary,
                metrics: json!({
                    "instances": tcprt.len(),
                    "flow_count": flow_count,
                    "avg_nqa_score": avg_nqa_score,
                    "avg_handshake_us": avg_handshake_us,
                    "avg_rtt_client_us": avg_rtt_client_us,
                    "total_retrans_req": total_retrans_req,
                    "total_retrans_resp": total_retrans_resp
                }),
            });
        }

        if let Ok(entries) = self.list_ssl(instance, 10_000).await {
            if !entries.is_empty() {
                let total = entries.len() as f64;
                let avg_handshake_us =
                    entries.iter().map(|entry| entry.handshake_us).sum::<f64>() / total;
                let mut top_snis: Vec<String> = entries
                    .iter()
                    .filter(|entry| !entry.sni.is_empty())
                    .take(5)
                    .map(|entry| entry.sni.clone())
                    .collect();
                top_snis.sort();
                top_snis.dedup();

                evidence.push(aria_api::DiagnoseEvidence {
                    evidence_type: "ssl".to_string(),
                    severity: "info".to_string(),
                    title: "SSL handshake observations available".to_string(),
                    summary: format!(
                        "Observed {} SSL handshakes with average latency {:.1}ms",
                        entries.len(),
                        avg_handshake_us / 1000.0
                    ),
                    metrics: json!({
                        "connections": entries.len(),
                        "avg_handshake_us": avg_handshake_us,
                        "top_snis": top_snis
                    }),
                });
            }
        }

        if let Ok(entries) = self.list_ssl_http(instance, 10_000).await {
            if !entries.is_empty() {
                let total = entries.len() as f64;
                let avg_latency_us = entries.iter().map(|entry| entry.latency_us).sum::<f64>() / total;
                let count_5xx = entries
                    .iter()
                    .filter(|entry| (500..600).contains(&entry.status_code))
                    .count();
                let ratio_5xx = (count_5xx as f64 / total) * 100.0;

                let (severity, title, summary) = if ratio_5xx > 10.0 {
                    verdict = "unhealthy".to_string();
                    candidate_causes.push(format!("5xx={:.1}% (>10%)", ratio_5xx));
                    next_suggestion(
                        &mut suggested_actions,
                        "Inspect backend health and upstream HTTP failures for the destination",
                    );
                    (
                        "critical",
                        "HTTP failure ratio elevated",
                        format!("HTTP 5xx ratio is {:.1}% for the requested destination", ratio_5xx),
                    )
                } else {
                    (
                        "info",
                        "HTTP observations available",
                        format!(
                            "Observed {} HTTP requests with average latency {:.1}ms",
                            entries.len(),
                            avg_latency_us / 1000.0
                        ),
                    )
                };

                evidence.push(aria_api::DiagnoseEvidence {
                    evidence_type: "http".to_string(),
                    severity: severity.to_string(),
                    title: title.to_string(),
                    summary,
                    metrics: json!({
                        "requests": entries.len(),
                        "avg_latency_us": avg_latency_us,
                        "count_5xx": count_5xx,
                        "ratio_5xx": ratio_5xx
                    }),
                });
            }
        }

        if let Ok(drops) = self
            .get_kernel_drop_stats(&aria_api::KernelDropQuery {
                instance: Some(instance.to_string()),
                iface: None,
                ifindex: None,
                reason: None,
                top: None,
                include_unattributed: false,
            })
            .await
        {
            let total_drops: u64 = drops.iter().map(|entry| entry.packets).sum();
            if total_drops > 0 {
                verdict = "unhealthy".to_string();
                candidate_causes.push(format!("drops={}", total_drops));
                next_suggestion(
                    &mut suggested_actions,
                    "Inspect kernel drop reasons and last drop locations for the managed instance",
                );
                let top_reasons: Vec<String> = drops
                    .iter()
                    .take(5)
                    .map(|entry| format!("{}({})", entry.reason, entry.packets))
                    .collect();

                evidence.push(aria_api::DiagnoseEvidence {
                    evidence_type: "kernel_drop".to_string(),
                    severity: "critical".to_string(),
                    title: "Kernel drops detected".to_string(),
                    summary: format!("Observed {} dropped packets for the managed instance", total_drops),
                    metrics: json!({
                        "drop_entries": drops.len(),
                        "total_drops": total_drops,
                        "top_reasons": top_reasons
                    }),
                });
            }
        }

        if evidence.is_empty() {
            verdict = "unknown".to_string();
            candidate_causes.push("no_observability_data".to_string());
            next_suggestion(
                &mut suggested_actions,
                "Verify that TCP-RT, SSL/HTTP, and kernel-drop observability are enabled for the node",
            );
        }

        let summary = match verdict.as_str() {
            "unhealthy" => format!(
                "Observed unhealthy signals for {}:{}",
                req.dst_ip, req.dst_port
            ),
            "degraded" => format!(
                "Observed degraded signals for {}:{}",
                req.dst_ip, req.dst_port
            ),
            "healthy" => format!(
                "No critical signals detected for {}:{}",
                req.dst_ip, req.dst_port
            ),
            _ => format!(
                "No structured observability data available for {}:{}",
                req.dst_ip, req.dst_port
            ),
        };

        Ok(aria_api::DiagnoseResponse {
            diagnose_id: format!("diag-{}", unix_ts_string()),
            verdict,
            summary,
            evidence,
            candidate_causes,
            suggested_actions,
        })
    }
}
