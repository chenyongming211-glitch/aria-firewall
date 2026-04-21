use super::*;
use serde_json::json;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

const NQA_UNHEALTHY_THRESHOLD: f64 = 50.0;
const NQA_DEGRADED_THRESHOLD: f64 = 80.0;
const HTTP_5XX_UNHEALTHY_RATIO: f64 = 10.0;

fn unix_ts_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn next_cause(causes: &mut Vec<String>, cause: String) {
    if !causes.iter().any(|item| item == &cause) {
        causes.push(cause);
    }
}

fn next_suggestion(suggestions: &mut Vec<String>, suggestion: &str) {
    if !suggestions.iter().any(|item| item == suggestion) {
        suggestions.push(suggestion.to_string());
    }
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "warning" => 1,
        "info" => 2,
        _ => 3,
    }
}

fn evidence_type_rank(evidence_type: &str) -> u8 {
    match evidence_type {
        "kernel_drop" => 0,
        "http" => 1,
        "tcprt" => 2,
        "ssl" => 3,
        "request" => 4,
        _ => 5,
    }
}

fn cause_rank(cause: &str) -> u8 {
    if cause.starts_with("kernel_drops=") {
        0
    } else if cause.starts_with("http_5xx_ratio=") {
        1
    } else if cause.starts_with("transport_nqa=") {
        2
    } else if cause == "no_observability_data" {
        3
    } else {
        4
    }
}

fn action_rank(action: &str) -> u8 {
    match action {
        "Inspect kernel drop reasons and last drop locations for the managed instance" => 0,
        "Inspect backend health and upstream HTTP failures for the destination" => 1,
        "Inspect TCP retransmissions and RTT trends for the destination" => 2,
        "Verify that TCP-RT, SSL/HTTP, and kernel-drop observability are enabled for the node" => {
            3
        }
        _ => 4,
    }
}

fn evidence_metric_f64(
    evidence: &aria_api::DiagnoseEvidence,
    key: &str,
) -> Option<f64> {
    evidence.metrics.get(key).and_then(|value| value.as_f64())
}

fn evidence_metric_u64(
    evidence: &aria_api::DiagnoseEvidence,
    key: &str,
) -> Option<u64> {
    evidence.metrics.get(key).and_then(|value| value.as_u64())
}

fn stable_sort_evidence(evidence: &mut [aria_api::DiagnoseEvidence]) {
    evidence.sort_by(|left, right| {
        severity_rank(&left.severity)
            .cmp(&severity_rank(&right.severity))
            .then_with(|| evidence_type_rank(&left.evidence_type).cmp(&evidence_type_rank(&right.evidence_type)))
            .then_with(|| left.title.cmp(&right.title))
    });
}

fn stable_sort_causes(causes: &mut [String]) {
    causes.sort_by(|left, right| {
        cause_rank(left)
            .cmp(&cause_rank(right))
            .then_with(|| left.cmp(right))
    });
}

fn stable_sort_actions(actions: &mut [String]) {
    actions.sort_by(|left, right| {
        action_rank(left)
            .cmp(&action_rank(right))
            .then_with(|| left.cmp(right))
    });
}

fn observed_sources_summary(evidence: &[aria_api::DiagnoseEvidence]) -> String {
    let mut sources = evidence
        .iter()
        .map(|item| item.evidence_type.clone())
        .collect::<Vec<_>>();
    sources.sort_by(|left, right| {
        evidence_type_rank(left)
            .cmp(&evidence_type_rank(right))
            .then_with(|| left.cmp(right))
    });
    sources.dedup();
    sources.join(", ")
}

fn highlight_from_evidence(evidence: &aria_api::DiagnoseEvidence) -> Option<String> {
    match evidence.evidence_type.as_str() {
        "kernel_drop" => evidence_metric_u64(evidence, "total_drops")
            .map(|drops| format!("kernel drops={}", drops)),
        "http" => {
            let ratio_5xx = evidence_metric_f64(evidence, "ratio_5xx")?;
            let requests = evidence_metric_u64(evidence, "requests").unwrap_or(0);
            Some(format!("http 5xx={:.1}% across {} requests", ratio_5xx, requests))
        }
        "tcprt" => {
            let nqa_score = evidence_metric_f64(evidence, "avg_nqa_score")?;
            let flow_count = evidence_metric_u64(evidence, "flow_count").unwrap_or(0);
            Some(format!(
                "transport nqa={:.1} across {} flows",
                nqa_score, flow_count
            ))
        }
        "ssl" => {
            let avg_handshake_us = evidence_metric_f64(evidence, "avg_handshake_us")?;
            let connections = evidence_metric_u64(evidence, "connections").unwrap_or(0);
            Some(format!(
                "ssl handshake={:.1}ms across {} connections",
                avg_handshake_us / 1000.0,
                connections
            ))
        }
        _ => None,
    }
}

fn build_summary(
    verdict: &str,
    req: &aria_api::DiagnoseRequest,
    evidence: &[aria_api::DiagnoseEvidence],
) -> String {
    match verdict {
        "unhealthy" | "degraded" => {
            let mut highlights = evidence
                .iter()
                .filter(|item| item.severity == "critical" || item.severity == "warning")
                .filter_map(highlight_from_evidence)
                .collect::<Vec<_>>();
            highlights.truncate(2);

            if highlights.is_empty() {
                format!("Observed {} signals for {}:{}", verdict, req.dst_ip, req.dst_port)
            } else {
                format!(
                    "Observed {} signals for {}:{}: {}",
                    verdict,
                    req.dst_ip,
                    req.dst_port,
                    highlights.join("; ")
                )
            }
        }
        "healthy" => {
            if evidence.is_empty() {
                format!("No critical signals detected for {}:{}", req.dst_ip, req.dst_port)
            } else {
                format!(
                    "No critical signals detected for {}:{}; observed {}",
                    req.dst_ip,
                    req.dst_port,
                    observed_sources_summary(evidence)
                )
            }
        }
        _ => format!(
            "No structured observability data available for {}:{}",
            req.dst_ip, req.dst_port
        ),
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
            let tcprt_entries = tcprt
                .iter()
                .flat_map(|(_, entries)| entries.iter())
                .collect::<Vec<_>>();
            let flow_count = tcprt_entries.len() as u64;
            let total = flow_count as f64;
            let avg_nqa_score = tcprt_entries
                .iter()
                .map(|entry| entry.nqa_score as f64)
                .sum::<f64>()
                / total;
            let avg_handshake_us = tcprt_entries
                .iter()
                .map(|entry| entry.handshake_us)
                .sum::<f64>()
                / total;
            let avg_rtt_client_us = tcprt_entries
                .iter()
                .map(|entry| entry.rtt_client_us)
                .sum::<f64>()
                / total;
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

            let (severity, title, summary) = if avg_nqa_score < NQA_UNHEALTHY_THRESHOLD {
                verdict = "unhealthy".to_string();
                next_cause(
                    &mut candidate_causes,
                    format!("transport_nqa={:.1}", avg_nqa_score),
                );
                next_suggestion(
                    &mut suggested_actions,
                    "Inspect TCP retransmissions and RTT trends for the destination",
                );
                (
                    "critical",
                    "Transport quality unhealthy",
                    format!(
                        "Average NQA score is {:.1} across {} flows, below the unhealthy threshold of {:.0}",
                        avg_nqa_score,
                        flow_count,
                        NQA_UNHEALTHY_THRESHOLD
                    ),
                )
            } else if avg_nqa_score < NQA_DEGRADED_THRESHOLD {
                if verdict != "unhealthy" {
                    verdict = "degraded".to_string();
                }
                next_cause(
                    &mut candidate_causes,
                    format!("transport_nqa={:.1}", avg_nqa_score),
                );
                next_suggestion(
                    &mut suggested_actions,
                    "Inspect TCP retransmissions and RTT trends for the destination",
                );
                (
                    "warning",
                    "Transport quality degraded",
                    format!(
                        "Average NQA score is {:.1} across {} flows, below the healthy threshold of {:.0}",
                        avg_nqa_score,
                        flow_count,
                        NQA_DEGRADED_THRESHOLD
                    ),
                )
            } else {
                (
                    "info",
                    "Transport quality normal",
                    format!(
                        "Average NQA score is {:.1} across {} observed flows",
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
                    .map(|entry| entry.sni.clone())
                    .collect();
                top_snis.sort();
                top_snis.dedup();
                top_snis.truncate(5);

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

                let (severity, title, summary) = if ratio_5xx > HTTP_5XX_UNHEALTHY_RATIO {
                    verdict = "unhealthy".to_string();
                    next_cause(
                        &mut candidate_causes,
                        format!("http_5xx_ratio={:.1}%", ratio_5xx),
                    );
                    next_suggestion(
                        &mut suggested_actions,
                        "Inspect backend health and upstream HTTP failures for the destination",
                    );
                    (
                        "critical",
                        "HTTP failure ratio elevated",
                        format!(
                            "HTTP 5xx ratio is {:.1}% across {} requests, above the unhealthy threshold of {:.0}%",
                            ratio_5xx,
                            entries.len(),
                            HTTP_5XX_UNHEALTHY_RATIO
                        ),
                    )
                } else {
                    (
                        "info",
                        "HTTP observations available",
                        format!(
                            "Observed {} HTTP requests with average latency {:.1}ms and 5xx ratio {:.1}%",
                            entries.len(),
                            avg_latency_us / 1000.0,
                            ratio_5xx
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
                next_cause(
                    &mut candidate_causes,
                    format!("kernel_drops={}", total_drops),
                );
                next_suggestion(
                    &mut suggested_actions,
                    "Inspect kernel drop reasons and last drop locations for the managed instance",
                );
                let mut reason_totals = std::collections::BTreeMap::<String, u64>::new();
                for entry in &drops {
                    *reason_totals.entry(entry.reason.clone()).or_insert(0) += entry.packets;
                }
                let mut top_reasons = reason_totals.into_iter().collect::<Vec<_>>();
                top_reasons.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
                let top_reasons: Vec<String> = top_reasons
                    .into_iter()
                    .take(5)
                    .map(|(reason, packets)| format!("{}({})", reason, packets))
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
            next_cause(&mut candidate_causes, "no_observability_data".to_string());
            next_suggestion(
                &mut suggested_actions,
                "Verify that TCP-RT, SSL/HTTP, and kernel-drop observability are enabled for the node",
            );
        }

        stable_sort_evidence(&mut evidence);
        stable_sort_causes(&mut candidate_causes);
        stable_sort_actions(&mut suggested_actions);
        let summary = build_summary(&verdict, req, &evidence);

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
