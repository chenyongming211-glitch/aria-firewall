//! HealthCheck executor — probes backends and tracks health state.
//!
//! Runs in user-space (agent side). Does not touch eBPF code.
//! When a backend's health state changes, the caller should re-materialize
//! the service maps to add/remove the backend from the slot array.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, info};

/// Health state of a single backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Unknown,
    Healthy,
    Unhealthy,
}

/// Configuration for a health check probe.
#[derive(Debug, Clone)]
pub struct ProbeConfig {
    pub protocol: String,
    #[allow(dead_code)]
    pub interval: Duration,
    pub timeout: Duration,
    pub healthy_threshold: u32,
    pub unhealthy_threshold: u32,
    pub target_port: Option<u16>,
}

/// Identifies a backend to probe.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct BackendTarget {
    pub service_id: String,
    pub backend_id: String,
    pub address: String,
    pub port: u16,
}

/// Tracks health state for a single backend.
#[derive(Debug, Clone)]
struct BackendHealthTracker {
    state: HealthState,
    consecutive_successes: u32,
    consecutive_failures: u32,
    healthy_threshold: u32,
    unhealthy_threshold: u32,
}

impl BackendHealthTracker {
    fn new(healthy_threshold: u32, unhealthy_threshold: u32) -> Self {
        Self {
            state: HealthState::Unknown,
            consecutive_successes: 0,
            consecutive_failures: 0,
            healthy_threshold,
            unhealthy_threshold,
        }
    }

    /// Record a probe result. Returns true if the health state changed.
    fn record(&mut self, success: bool) -> bool {
        let old_state = self.state;
        if success {
            self.consecutive_successes += 1;
            self.consecutive_failures = 0;
            if self.consecutive_successes >= self.healthy_threshold {
                self.state = HealthState::Healthy;
            }
        } else {
            self.consecutive_failures += 1;
            self.consecutive_successes = 0;
            if self.consecutive_failures >= self.unhealthy_threshold {
                self.state = HealthState::Unhealthy;
            }
        }
        self.state != old_state
    }
}

/// Execute a single TCP connect probe.
async fn probe_tcp(address: &str, port: u16, probe_timeout: Duration) -> bool {
    let addr_str = format!("{}:{}", address, port);
    let addr: SocketAddr = match addr_str.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    timeout(probe_timeout, TcpStream::connect(addr))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
}

/// The health check executor. Manages probe state for all backends.
pub struct HealthCheckExecutor {
    trackers: HashMap<BackendTarget, BackendHealthTracker>,
}

impl HealthCheckExecutor {
    pub fn new() -> Self {
        Self {
            trackers: HashMap::new(),
        }
    }

    /// Run one round of probes for the given backends.
    /// Returns a list of backends whose health state changed.
    pub async fn probe_round(
        &mut self,
        backends: &[(BackendTarget, ProbeConfig)],
    ) -> Vec<(BackendTarget, HealthState)> {
        let mut changed = Vec::new();

        for (target, config) in backends {
            let probe_port = config.target_port.unwrap_or(target.port);
            let success = match config.protocol.as_str() {
                "tcp" => probe_tcp(&target.address, probe_port, config.timeout).await,
                _ => {
                    debug!(
                        backend = %target.backend_id,
                        protocol = %config.protocol,
                        "unsupported probe protocol, treating as healthy"
                    );
                    true
                }
            };

            let tracker = self
                .trackers
                .entry(target.clone())
                .or_insert_with(|| {
                    BackendHealthTracker::new(
                        config.healthy_threshold,
                        config.unhealthy_threshold,
                    )
                });

            if tracker.record(success) {
                info!(
                    service = %target.service_id,
                    backend = %target.backend_id,
                    address = %target.address,
                    port = target.port,
                    new_state = ?tracker.state,
                    "backend health state changed"
                );
                changed.push((target.clone(), tracker.state));
            }
        }

        changed
    }

    /// Get the current health state of a backend.
    pub fn get_state(&self, target: &BackendTarget) -> HealthState {
        self.trackers
            .get(target)
            .map(|t| t.state)
            .unwrap_or(HealthState::Unknown)
    }

    /// Remove trackers for backends that are no longer in the active set.
    #[allow(dead_code)]
    pub fn prune(&mut self, active_backends: &[BackendTarget]) {
        self.trackers
            .retain(|key, _| active_backends.contains(key));
    }
}
