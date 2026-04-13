use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub id: u32,
    pub name: String,
    pub cidrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleInfo {
    pub name: Option<String>,
    pub src_group_id: u32,
    pub dst_group_id: u32,
    pub proto: u8,
    pub action: u8,
    pub ports: Option<String>,
    pub bitmap_idx: Option<u32>,
    #[serde(default)]
    pub direction: u8, // 0=ingress, 1=egress
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QosRuleInfo {
    pub group_name: String,
    pub group_id: u32,
    pub direction: u8,
    pub rate_bps: u64,
    pub burst_bytes: u64,
    pub priority: u8,
    #[serde(default)]
    pub mode: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorRuleInfo {
    pub src_group_name: String,
    pub src_group_id: u32,
    pub dst_group_name: String,
    pub dst_group_id: u32,
    pub proto: u8,
    pub direction: u8,
    pub target_iface: String,
    pub target_ifindex: u32,
    pub is_global: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSetInfo {
    pub bitmap_idx: u32,
    pub ports_normalized: String,
    pub ref_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallState {
    pub groups: HashMap<String, GroupInfo>,
    pub rules: Vec<RuleInfo>,
    pub next_group_id: u32,
    pub next_bitmap_idx: u32,
    #[serde(default)]
    pub port_sets: HashMap<String, PortSetInfo>,
    #[serde(default)]
    pub free_bitmap_indices: Vec<u32>,
    #[serde(default = "default_max_port_policies")]
    pub max_port_policies: u32,
    /// Stable per-instance namespace id reserved for the future shared data plane.
    #[serde(default)]
    pub tap_id: u32,
    /// XDP 程序挂载的网卡名
    #[serde(default)]
    pub attached_iface: Option<String>,
    #[serde(default)]
    pub qos_rules: Vec<QosRuleInfo>,
    #[serde(default)]
    pub conntrack_enabled: bool,
    #[serde(default)]
    pub monitoring_enabled: bool,
    #[serde(default = "default_true")]
    pub acl_enabled: bool,
    #[serde(default = "default_true")]
    pub qos_enabled: bool,
    #[serde(default)]
    pub mirror_rules: Vec<MirrorRuleInfo>,
    #[serde(default = "default_true")]
    pub mirror_enabled: bool,
    #[serde(default = "default_true")]
    pub tcprt_enabled: bool,
    #[serde(default)]
    pub ssl_enabled: bool,
    #[serde(default)]
    pub lb_enabled: bool,
}

impl Default for FirewallState {
    fn default() -> Self {
        Self {
            groups: HashMap::new(),
            rules: Vec::new(),
            next_group_id: 1, // ID 0 保留给通配符 "any"
            next_bitmap_idx: 0,
            port_sets: HashMap::new(),
            free_bitmap_indices: Vec::new(),
            max_port_policies: default_max_port_policies(),
            tap_id: 0,
            attached_iface: None,
            qos_rules: Vec::new(),
            conntrack_enabled: true,
            monitoring_enabled: true,
            acl_enabled: true,
            qos_enabled: true,
            mirror_rules: Vec::new(),
            mirror_enabled: true,
            tcprt_enabled: true,
            ssl_enabled: false,
            lb_enabled: false,
        }
    }
}

pub struct StateManager {
    pub(crate) state_file: PathBuf,
}
