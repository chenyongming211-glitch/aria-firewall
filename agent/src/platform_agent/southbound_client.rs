use std::time::Duration;

use aria_api::{
    ApplyStatusReport, HeartbeatResponse, NodeHealthReport, NodeRegisterRequest,
    NodeRegisterResponse,
};
use serde::de::DeserializeOwned;

use super::helpers::{self, parse_platform_error};
use super::ir_types::CompiledNodeState;

pub(crate) struct SouthboundClient {
    base_url: String,
    client: reqwest::Client,
}

impl SouthboundClient {
    fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    async fn register_node(
        &self,
        node_id: &str,
        request: &NodeRegisterRequest,
    ) -> Result<NodeRegisterResponse, String> {
        let response = self
            .client
            .post(self.url(&format!("/api/v1/southbound/nodes/{node_id}/register")))
            .json(request)
            .send()
            .await
            .map_err(connection_error)?;
        self.parse_response(response).await
    }

    async fn desired_state(
        &self,
        node_id: &str,
        desired_state_url: &str,
    ) -> Result<DesiredStateEnvelope, String> {
        let response = self
            .client
            .get(self.resolve_url(
                desired_state_url,
                &format!("/api/v1/southbound/nodes/{node_id}/desired-state"),
            ))
            .send()
            .await
            .map_err(connection_error)?;
        self.parse_response(response).await
    }

    async fn report_apply_status(
        &self,
        node_id: &str,
        report: &ApplyStatusReport,
    ) -> Result<ApplyStatusResponse, String> {
        let response = self
            .client
            .post(self.url(&format!("/api/v1/southbound/nodes/{node_id}/apply-status")))
            .json(report)
            .send()
            .await
            .map_err(connection_error)?;
        self.parse_response(response).await
    }

    async fn heartbeat(
        &self,
        node_id: &str,
        report: &NodeHealthReport,
    ) -> Result<HeartbeatResponse, String> {
        let response = self
            .client
            .post(self.url(&format!("/api/v1/southbound/nodes/{node_id}/heartbeat")))
            .json(report)
            .send()
            .await
            .map_err(connection_error)?;
        self.parse_response(response).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn resolve_url(&self, desired_state_url: &str, fallback_path: &str) -> String {
        if desired_state_url.starts_with("http://") || desired_state_url.starts_with("https://") {
            desired_state_url.to_string()
        } else if desired_state_url.trim().is_empty() {
            self.url(fallback_path)
        } else {
            self.url(desired_state_url)
        }
    }

    async fn parse_response<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, String> {
        let status = response.status();
        if status.is_success() {
            return response
                .json::<T>()
                .await
                .map_err(|error| format!("failed to decode southbound response: {error}"));
        }

        let message = parse_platform_error(response).await.unwrap_or_else(|| {
            format!("southbound request failed with status {}", status.as_u16())
        });
        Err(message)
    }
}
