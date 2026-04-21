use crate::api_client;

pub(crate) async fn handle(
    client: &api_client::ApiClient,
    instance: &str,
    dst: &str,
    dport: u16,
    chain: Option<&str>,
) -> Result<(), String> {
    let response = client
        .diagnose_instance(
            instance,
            &aria_api::DiagnoseRequest {
                dst_ip: dst.to_string(),
                dst_port: dport,
                chain: chain.map(|value| value.to_string()),
                time_window_seconds: None,
            },
        )
        .await;

    match response {
        Ok(resp) => {
            println!("=== Aria Diagnostic Report ===");
            println!("Target: {}:{}", dst, dport);
            println!("Instance: {}", instance);
            if let Some(chain_name) = chain {
                println!("Chain: {}", chain_name);
            }
            println!();

            println!("--- Verdict: {} ---", resp.verdict.to_uppercase());
            println!("  {}", resp.summary);

            println!("\n--- Evidence ---");
            if resp.evidence.is_empty() {
                println!("  (none)");
            } else {
                for item in &resp.evidence {
                    println!(
                        "  [{}] {}",
                        item.severity.to_uppercase(),
                        item.title
                    );
                    println!("    type: {}", item.evidence_type);
                    println!("    {}", item.summary);
                    if !item.metrics.is_null() {
                        if let Ok(pretty) = serde_json::to_string_pretty(&item.metrics) {
                            for line in pretty.lines() {
                                println!("    {}", line);
                            }
                        }
                    }
                }
            }

            println!("\n--- Candidate Causes ---");
            if resp.candidate_causes.is_empty() {
                println!("  (none)");
            } else {
                for cause in &resp.candidate_causes {
                    println!("  - {}", cause);
                }
            }

            println!("\n--- Suggested Actions ---");
            if resp.suggested_actions.is_empty() {
                println!("  (none)");
            } else {
                for action in &resp.suggested_actions {
                    println!("  - {}", action);
                }
            }
        }
        Err(error) => return Err(error),
    }

    Ok(())
}
