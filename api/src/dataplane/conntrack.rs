use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConntrackEntry {
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub proto: String,
    pub state: String,
    pub packets: u64,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConntrackResponse {
    pub connections: Vec<ConntrackEntry>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConntrackFlushResponse {
    pub flushed: u64,
}
