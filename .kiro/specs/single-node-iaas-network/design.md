# 技术设计文档：单节点 IaaS 网络最小闭环（Phase 3）

## 概述

本文档定义 Phase 3 单节点 IaaS 网络最小闭环的技术设计方案。基于已有的 L4 LB 数据面（6 个 SVC map + eBPF lb.rs）和 Agent shadow compiler，新增 Port 身份绑定、Anti-Spoof、L3 路由、SecurityGroup 分段检查四个数据面能力（Mode A），并完成 EIP/NAT 的设计预留（Mode B，仅文档不落地代码）。

上游需求：[requirements.md](requirements.md)

## 架构概览

### 新增组件与现有组件的关系

```
Controller (已有)
  ├── Port / SecurityGroup / RouteTable 对象 CRUD (已有)
  └── southbound desired-state 投影 (已有)
         │
         ▼
Agent platform_agent.rs (已有 shadow compiler)
  ├── compile: Port → PortIdentityIr + AntiSpoofIr
  ├── compile: RouteTable → RouteIr
  ├── compile: SecurityGroup → SgRuleIr (ingress/egress 分离)
  └── materialize: 写入 5 个新 eBPF map
         │
         ▼
eBPF Datapath (新增 port.rs + route.rs + sg.rs 模块)
  ├── PORT_IDENTITY_MAP   — tap_id → 端口元数据
  ├── ANTI_SPOOF_MAP      — (tap_id, ip) → 允许标记
  ├── ROUTE_TABLE_V4      — LpmTrie (tap_id+prefix) → RouteValue
  ├── SG_RULE_MAP         — HashMap (tap_id+sg_id+方向+五元组) → verdict
  └── 与现有 LB / CT / Policy pipeline 集成
```

### 数据面流水线（Mode A 完整路径）

```
TC Ingress 入向包:
  ① parse packet (已有)
  ② resolve tap_id (已有)
  ③ PORT_IDENTITY_MAP lookup ← 新增
  ④ Anti-Spoof check (MAC + IP) ← 新增
  ⑤ SG ingress check (pre-route) ← 新增
  ⑥ CT lookup (已有)
  ⑦ LB frontend lookup (已有)
  ⑧ Route lookup (LPM) ← 新增
  ⑨ SG egress check (post-route) ← 新增
  ⑩ forward / drop
```

## 详细设计

### 1. eBPF Map Schema（Mode A）

#### 1.1 PORT_IDENTITY_MAP

用途：tap_id → 端口身份元数据。每个被管理的 Port 一条。

```rust
// --- ebpf/src/common.rs + core/src/common.rs 同时定义 ---

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PortIdentityKey {
    pub tap_id: u32,        // 命名空间隔离
}
// 大小：4 字节

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PortIdentityValue {
    pub mac: [u8; 6],       // 绑定 MAC 地址
    pub flags: u16,         // bit0=anti_spoof_enabled, bit1=has_allowed_pairs
    pub network_id: u32,    // 所属网络 ID（本地分配）
    pub segment_id: u32,    // 所属网段 ID
    pub tenant_id: u32,     // 租户 ID
    pub primary_ipv4: u32,  // 主 IPv4 地址（网络字节序）
    pub primary_ipv6: [u8; 16], // 主 IPv6 地址
    pub sg_id: u32,         // 关联的安全组 ID（本地分配）
    pub ip_count: u16,      // ANTI_SPOOF_MAP 中该 port 的 IP 条目数
    pub pad: [u8; 2],
}
// 大小：48 字节（< 64 字节约束）
```

Map 类型：`HashMap<PortIdentityKey, PortIdentityValue>`，容量 1024。

Flags 位定义：

| Bit | 名称 | 说明 |
|-----|------|------|
| 0 | `ANTI_SPOOF_ENABLED` | 启用 Anti-Spoof 校验（默认 1） |
| 1 | `HAS_ALLOWED_PAIRS` | 存在额外 allowed_address_pairs |

#### 1.2 ANTI_SPOOF_MAP

用途：校验源 IP 是否属于该 tap_id 的允许列表。

```rust
#[repr(C)]
#[derive(Copy, Clone)]
pub struct AntiSpoofKey {
    pub tap_id: u32,
    pub address: [u8; 16],  // IPv4 用 v4-mapped-v6 格式
}
// 大小：20 字节

#[repr(C)]
#[derive(Copy, Clone)]
pub struct AntiSpoofValue {
    pub flags: u8,          // bit0=is_primary, bit1=is_allowed_pair
    pub pad: [u8; 3],
}
// 大小：4 字节
```

Map 类型：`HashMap<AntiSpoofKey, AntiSpoofValue>`，容量 8192。

说明：
- 每个 Port 的 fixed_ips + allowed_address_pairs 都写入此 map
- eBPF 侧只做 `(tap_id, src_ip)` 精确查找，命中即放行
- MAC 校验直接在 PORT_IDENTITY_MAP 的 `mac` 字段上做，不需要额外 map

#### 1.3 ROUTE_TABLE_V4 / ROUTE_TABLE_V6

用途：目的 IP 最长前缀匹配，返回下一跳信息。

```rust
// ROUTE_TABLE_V4 payload: tap_id(4) + ipv4(4) = 8 字节
// ROUTE_TABLE_V6 payload: tap_id(4) + ipv6(16) = 20 字节
// 复用 aya LpmTrie 的标准 key 格式

#[repr(C)]
#[derive(Copy, Clone)]
pub struct RouteValue {
    pub next_hop_ip: [u8; 16],  // 下一跳 IP（v4-mapped-v6）
    pub egress_ifindex: u32,     // 出向接口 ifindex
    pub route_id: u16,           // 路由 ID（本地分配，用于事件关联）
    pub next_hop_type: u8,       // 0=local_port, 1=gateway, 2=blackhole, 3=host
    pub priority: u8,            // 路由优先级（0 最高）
    pub flags: u8,               // bit0=ecmp_eligible（预留）
    pub pad: [u8; 3],
}
// 大小：32 字节（对齐到 32 字节，满足优化建议 1）
```

Map 类型：
- `LpmTrie<[u8; 8], RouteValue>` — IPv4 路由表，容量 4096
- `LpmTrie<[u8; 20], RouteValue>` — IPv6 路由表，容量 2048

next_hop_type 枚举：

| 值 | 名称 | 说明 |
|----|------|------|
| 0 | `LOCAL_PORT` | 目标在本节点某个 Port，直接 redirect 到 egress_ifindex |
| 1 | `GATEWAY` | 需要经过网关转发（next_hop_ip 有效） |
| 2 | `BLACKHOLE` | 黑洞路由，直接丢弃 |
| 3 | `HOST` | 投递到主机网络栈 |

#### 1.4 SG_RULE_MAP

用途：安全组规则匹配。入向和出向规则共用一个 map，通过 direction 字段区分。

```rust
#[repr(C)]
#[derive(Copy, Clone)]
pub struct SgRuleKey {
    pub tap_id: u32,
    pub sg_id: u32,         // 安全组 ID（本地分配）
    pub direction: u8,      // 0=ingress, 1=egress
    pub proto: u8,          // IPPROTO_TCP=6, IPPROTO_UDP=17, 0=any
    pub pad: [u8; 2],
    pub remote_prefix: [u8; 16], // 远端 IP 前缀（v4-mapped-v6）
    pub prefix_len: u8,     // 前缀长度
    pub pad2: [u8; 3],
}
// 大小：32 字节

#[repr(C)]
#[derive(Copy, Clone)]
pub struct SgRuleValue {
    pub action: u8,         // 0=deny, 1=allow
    pub priority: u8,       // 规则优先级（0 最高）
    pub port_start: u16,    // 端口范围起始（0=any）
    pub port_end: u16,      // 端口范围结束（0=any）
    pub rule_id: u16,       // 规则 ID（用于事件关联）
}
// 大小：8 字节
```

Map 类型：`HashMap<SgRuleKey, SgRuleValue>`，容量 16384。

设计说明：
- 入向和出向规则共用一个 map，通过 `direction` 字段区分（满足优化建议 2：分段检查）
- 安全组规则匹配采用精确查找 + 逐级回退：先精确 (proto, remote_ip/prefix, port)，再回退到 (proto, any, any)，最后回退到 (any, any, any)
- 回退层数固定为 3 级，verifier 可证明上界
- conntrack 快速路径跳过安全组检查（已建立连接不重复匹配）

#### 1.5 Mode B — NAT_TABLE（仅设计预留，不落地代码）

```rust
// ⚠️ Phase 3 不落地代码，仅作为 RFC-005A 设计预留

#[repr(C)]
#[derive(Copy, Clone)]
pub struct NatKey {
    pub tap_id: u32,
    pub address: [u8; 16],  // 原始地址
    pub port: u16,           // 原始端口（满足优化建议 3：端口级 DNAT）
    pub proto: u8,           // 协议（满足优化建议 3）
    pub nat_type: u8,        // 0=snat, 1=dnat, 2=fip_1to1
}
// 大小：24 字节

#[repr(C)]
#[derive(Copy, Clone)]
pub struct NatValue {
    pub translated_address: [u8; 16],
    pub translated_port: u16,
    pub flags: u8,           // bit0=conntrack_bind
    pub pad: [u8; 5],
}
// 大小：24 字节
```

NAT 在流水线中的插入位置：路由查找之后、出向安全组检查之前。
NAT 绑定引用存储在 conntrack 条目中，回程包通过 conntrack 反向映射恢复。
checksum 更新复用 LB 已有的 `bpf_l3_csum_replace` / `bpf_l4_csum_replace` helper。
分片包策略：首片正常处理，非首片通过 conntrack 关联。


### 2. eBPF 模块设计

#### 2.1 新增模块

| 文件 | 职责 | 调用层级 |
|------|------|---------|
| `ebpf/src/port.rs` | Port 身份查找 + Anti-Spoof 校验 | 叶子层（inline helper） |
| `ebpf/src/route.rs` | L3 路由 LPM 查找 + 转发决策 | 叶子层（inline helper） |
| `ebpf/src/sg.rs` | SecurityGroup 规则匹配（3 级回退） | 叶子层（inline helper） |

#### 2.2 port.rs 函数签名

```rust
/// Port 身份查找。写入 PipelineCtx 的 port identity 字段。
/// 返回 true 表示查找成功，false 表示未命中（应丢弃）。
#[inline(always)]
pub unsafe fn phase_port_identity(p: &mut PipelineCtx) -> bool;

/// Anti-Spoof 校验。检查源 MAC 和源 IP。
/// 返回 true 表示通过，false 表示违规（应丢弃）。
#[inline(always)]
pub unsafe fn phase_anti_spoof_v4(
    info: &PacketInfo,
    p: &PipelineCtx,
) -> bool;

#[inline(always)]
pub unsafe fn phase_anti_spoof_v6(
    info: &PacketInfo,
    p: &PipelineCtx,
) -> bool;
```

栈预算：每个函数 < 32 字节（只有 key 结构体 + 指针）。

#### 2.3 route.rs 函数签名

```rust
/// IPv4 路由查找。返回 RouteValue 指针或 None。
#[inline(always)]
pub unsafe fn route_lookup_v4(
    tap_id: u32,
    dst_ip: u32,
) -> Option<&'static RouteValue>;

/// IPv6 路由查找。
#[inline(always)]
pub unsafe fn route_lookup_v6(
    tap_id: u32,
    dst_ip: [u8; 16],
) -> Option<&'static RouteValue>;

/// 执行路由转发决策。根据 next_hop_type 决定 redirect / drop / pass。
#[inline(always)]
pub unsafe fn phase_route_forward(
    ctx: &TcContext,
    route: &RouteValue,
    p: &mut PipelineCtx,
) -> i32;
```

栈预算：route_lookup 约 16 字节（LPM key），phase_route_forward 约 8 字节。

#### 2.4 sg.rs 函数签名

```rust
/// 安全组规则匹配（3 级回退查找）。
/// direction: 0=ingress, 1=egress
/// 返回 true 表示允许，false 表示拒绝。
#[inline(always)]
pub unsafe fn sg_check(
    p: &PipelineCtx,
    info: &PacketInfo,
    direction: u8,
) -> bool;
```

3 级回退查找逻辑：
1. 精确匹配：`(tap_id, sg_id, direction, proto, remote_ip/prefix, port_range)`
2. 协议通配：`(tap_id, sg_id, direction, proto, 0.0.0.0/0, any)`
3. 全通配：`(tap_id, sg_id, direction, any, 0.0.0.0/0, any)`

每级一次 HashMap 查找，共 3 次查找，固定上界。
栈预算：约 40 字节（SgRuleKey 32 字节 + 局部变量）。

### 3. PipelineCtx 扩展

在现有 `PipelineCtx` 结构体末尾新增字段（不破坏已有字段偏移）：

```rust
// --- 新增字段（追加到 PipelineCtx 末尾）---
pub port_network_id: u32,    // PORT_IDENTITY_MAP 查找结果
pub port_segment_id: u32,    // PORT_IDENTITY_MAP 查找结果
pub port_sg_id: u32,         // 关联的安全组 ID
pub route_id: u16,           // 匹配的路由 ID
pub route_next_hop_type: u8, // 路由下一跳类型
pub port_flags: u8,          // bit0=identity_resolved, bit1=anti_spoof_passed
pub route_egress_ifindex: u32, // 路由出向 ifindex
```

新增大小：20 字节。PipelineCtx 总大小从当前约 48 字节增长到约 68 字节，仍在 scratch map 中，不影响栈。

### 4. 流水线集成（ebpf/src/lib.rs）

#### 4.1 TC Ingress 修改

在 `try_tc_ingress` 中，LB 查找之前插入 Port 身份和 Anti-Spoof 阶段：

```rust
// 现有: load_runtime_ctx_tc + load_feature_flags_tc
// ↓ 新增阶段 ↓

// Phase: Port Identity
if !port::phase_port_identity(p) {
    p.drop_reason = DROP_PORT_IDENTITY_MISS;
    do_drop(p);
    return Ok(TC_ACT_SHOT as i32);
}

// Phase: Anti-Spoof (仅当 port_flags.anti_spoof_enabled)
if p.port_flags & PORT_FLAG_ANTI_SPOOF != 0 {
    let spoof_ok = if info.is_ipv6 {
        port::phase_anti_spoof_v6(info, p)
    } else {
        port::phase_anti_spoof_v4(info, p)
    };
    if !spoof_ok {
        p.drop_reason = DROP_ANTI_SPOOF;
        do_drop(p);
        return Ok(TC_ACT_SHOT as i32);
    }
}

// Phase: SG Ingress (pre-route)
if p.port_sg_id != 0 {
    if !sg::sg_check(p, info, SG_DIR_INGRESS) {
        p.drop_reason = DROP_SG_INGRESS;
        do_drop(p);
        return Ok(TC_ACT_SHOT as i32);
    }
}

// 现有: LB frontend lookup → CT → policy → ...
// ↓ 路由查找插入在 CT/policy 之后 ↓

// Phase: Route Lookup (仅当 LB 未命中时)
if p.flags & FLAG_LB_HIT == 0 {
    let route = if info.is_ipv6 {
        route::route_lookup_v6(p.tap_id, info.dst_ip_v6)
    } else {
        route::route_lookup_v4(p.tap_id, info.dst_ip)
    };
    match route {
        Some(r) => {
            p.route_id = r.route_id;
            p.route_next_hop_type = r.next_hop_type;
            p.route_egress_ifindex = r.egress_ifindex;
            // Phase: SG Egress (post-route)
            if p.port_sg_id != 0 {
                if !sg::sg_check(p, info, SG_DIR_EGRESS) {
                    p.drop_reason = DROP_SG_EGRESS;
                    do_drop(p);
                    return Ok(TC_ACT_SHOT as i32);
                }
            }
            return Ok(route::phase_route_forward(ctx, r, p));
        }
        None => {
            p.drop_reason = DROP_ROUTE_MISS;
            do_drop(p);
            return Ok(TC_ACT_SHOT as i32);
        }
    }
}
```

#### 4.2 调用链深度分析

```
try_tc_ingress (层 1, #[inline(never)])
  ├── port::phase_port_identity (层 2, #[inline(always)])
  ├── port::phase_anti_spoof_v4 (层 2, #[inline(always)])
  ├── sg::sg_check (层 2, #[inline(always)])
  ├── lb::phase_lb_ingress_v4 (层 2, #[inline(always)])
  │   └── lb::svc_frontend_lookup_v4 (层 3)
  │       └── lb::svc_backend_select (层 4)
  │           └── lb::svc_dnat_v4 (层 5)
  ├── phase_ct_v4 (层 2)
  ├── route::route_lookup_v4 (层 2, #[inline(always)])
  └── route::phase_route_forward (层 2, #[inline(always)])
```

最大调用深度：5 层（LB 路径），满足 ≤ 6 层软约束。
新增的 port/route/sg 模块都是层 2 叶子，不增加调用深度。

#### 4.3 栈预算分析

| 函数 | 栈使用估算 | 说明 |
|------|-----------|------|
| `try_tc_ingress` | ~80 字节 | ct_key + 局部变量（已有） |
| `phase_port_identity` | ~8 字节 | PortIdentityKey 4B + 指针 |
| `phase_anti_spoof_v4` | ~24 字节 | AntiSpoofKey 20B + 指针 |
| `sg_check` | ~40 字节 | SgRuleKey 32B + 局部变量 |
| `route_lookup_v4` | ~16 字节 | LPM key 12B + 指针 |
| `phase_route_forward` | ~8 字节 | 仅标量参数 |

由于新增函数全部 `#[inline(always)]`，它们的栈帧会合并到 `try_tc_ingress`。
最坏情况总栈：~80 + 24 + 40 + 16 = ~160 字节，远低于 256 字节软上限。

### 5. core 层新增操作模块

#### 5.1 core/src/port_ops.rs

```rust
/// 写入 PORT_IDENTITY_MAP 条目
pub fn write_port_identity(
    pin_path: &str,
    tap_id: u32,
    value: &PortIdentityValue,
) -> Result<(), String>;

/// 删除 PORT_IDENTITY_MAP 条目
pub fn delete_port_identity(
    pin_path: &str,
    tap_id: u32,
) -> Result<(), String>;

/// 写入 ANTI_SPOOF_MAP 条目（批量）
pub fn write_anti_spoof_entries(
    pin_path: &str,
    tap_id: u32,
    addresses: &[[u8; 16]],
    flags: &[u8],
) -> Result<(), String>;

/// 清除指定 tap_id 的所有 ANTI_SPOOF_MAP 条目
pub fn clear_anti_spoof_entries(
    pin_path: &str,
    tap_id: u32,
    addresses: &[[u8; 16]],
) -> Result<(), String>;
```

#### 5.2 core/src/route_ops.rs

```rust
/// 写入 ROUTE_TABLE_V4 条目
pub fn write_route_v4(
    pin_path: &str,
    tap_id: u32,
    prefix: u32,
    prefix_len: u32,
    value: &RouteValue,
) -> Result<(), String>;

/// 删除 ROUTE_TABLE_V4 条目
pub fn delete_route_v4(
    pin_path: &str,
    tap_id: u32,
    prefix: u32,
    prefix_len: u32,
) -> Result<(), String>;

/// 写入 ROUTE_TABLE_V6 条目
pub fn write_route_v6(
    pin_path: &str,
    tap_id: u32,
    prefix: [u8; 16],
    prefix_len: u32,
    value: &RouteValue,
) -> Result<(), String>;

/// 删除 ROUTE_TABLE_V6 条目
pub fn delete_route_v6(
    pin_path: &str,
    tap_id: u32,
    prefix: [u8; 16],
    prefix_len: u32,
) -> Result<(), String>;
```

#### 5.3 core/src/sg_ops.rs

```rust
/// 写入 SG_RULE_MAP 条目
pub fn write_sg_rule(
    pin_path: &str,
    key: &SgRuleKey,
    value: &SgRuleValue,
) -> Result<(), String>;

/// 删除 SG_RULE_MAP 条目
pub fn delete_sg_rule(
    pin_path: &str,
    key: &SgRuleKey,
) -> Result<(), String>;

/// 批量清除指定 (tap_id, sg_id) 的所有规则
pub fn clear_sg_rules(
    pin_path: &str,
    tap_id: u32,
    sg_id: u32,
    keys: &[SgRuleKey],
) -> Result<(), String>;
```

当前实现还约定了两个临时 runtime label，用于把 Controller 下发的 Port 对象绑定到节点实际接口：

- `Port.metadata.labels["runtime.tap_id"]`
- `Port.metadata.labels["runtime.ifindex"]`

这两个字段是当前 Mode A materialize 的前提，后续会在正式 attachment 模型收口时替换掉。

### 6. Agent Materialize 流程

#### 6.1 新增编译 IR 结构

在 `platform_agent.rs` 中新增：

```rust
struct PortIdentityIr {
    tap_id: u32,
    mac: [u8; 6],
    fixed_ips: Vec<String>,          // "10.0.0.5" 或 "fd00::5"
    allowed_address_pairs: Vec<(String, String)>, // (mac, ip)
    network_id: String,
    segment_id: String,
    tenant_id: String,
    sg_ids: Vec<String>,
    anti_spoof_enabled: bool,
}

struct RouteIr {
    destination_prefix: String,      // "10.0.0.0/24"
    next_hop_ip: Option<String>,
    next_hop_type: String,           // "local_port" | "gateway" | "blackhole" | "host"
    egress_ifindex: Option<u32>,
    priority: u8,
}

struct SgRuleIr {
    sg_id: String,
    direction: String,               // "ingress" | "egress"
    proto: Option<String>,           // "tcp" | "udp" | None (any)
    remote_prefix: Option<String>,   // "10.0.0.0/24" | None (any)
    port_start: Option<u16>,
    port_end: Option<u16>,
    action: String,                  // "allow" | "deny"
    priority: u8,
}
```

#### 6.2 Materialize 函数

新增 `materialize_port_maps` 和 `materialize_route_maps` 和 `materialize_sg_maps`，与现有 `materialize_service_maps` 并列：

```rust
fn materialize_port_maps(
    pin_path: &str,
    compiled_state: &CompiledNodeState,
) -> Result<DomainMaterializeResult, String>;

fn materialize_route_maps(
    pin_path: &str,
    compiled_state: &CompiledNodeState,
) -> Result<DomainMaterializeResult, String>;

fn materialize_sg_maps(
    pin_path: &str,
    compiled_state: &CompiledNodeState,
) -> Result<DomainMaterializeResult, String>;
```

执行顺序：
1. `materialize_port_maps`（先写 PORT_IDENTITY_MAP + ANTI_SPOOF_MAP）
2. `materialize_sg_maps`（再写 SG_RULE_MAP）
3. `materialize_route_maps`（最后写 ROUTE_TABLE）
4. `materialize_service_maps`（已有，不变）

#### 6.3 apply-status 域扩展

在 `domain_statuses` 中新增：
- `identity`：Port 身份写入结果
- `ports`：Anti-Spoof 写入结果
- `security`：SecurityGroup 规则写入结果
- `routes`：路由表写入结果

这些域名与现有 shadow compiler 已预留的域名一致，从 shadow 切换为真实 applied/failed 报告。

### 7. 与现有 LB 数据面的集成

#### 7.1 执行顺序

```
Port Identity → Anti-Spoof → SG Ingress → CT → LB → Route → SG Egress → Forward
```

- Port Identity 和 Anti-Spoof 在 LB 之前执行，确保所有流量都经过身份校验
- LB 命中时（`FLAG_LB_HIT`），跳过 Route 查找，直接使用 LB 选择的后端
- SG Egress 在 Route 之后执行，对路由决策后的出向流量做安全检查

#### 7.2 PipelineCtx flags 扩展

```rust
pub const FLAG_LB_HIT: u16 = 1 << 0;           // 已有
pub const FLAG_PORT_RESOLVED: u16 = 1 << 9;      // 新增
pub const FLAG_ANTI_SPOOF_PASSED: u16 = 1 << 10; // 新增
pub const FLAG_ROUTE_RESOLVED: u16 = 1 << 11;    // 新增
```

#### 7.3 Pin Namespace

所有新增 map 与现有 LB map 共享同一个 pin namespace：
```
/sys/fs/bpf/aria/global-v2/PORT_IDENTITY_MAP
/sys/fs/bpf/aria/global-v2/ANTI_SPOOF_MAP
/sys/fs/bpf/aria/global-v2/ROUTE_TABLE_V4
/sys/fs/bpf/aria/global-v2/ROUTE_TABLE_V6
/sys/fs/bpf/aria/global-v2/SG_RULE_MAP
```

### 8. Mode B — EIP/NAT 设计预留

> ⚠️ 以下内容仅作为设计文档，Phase 3 不落地代码。

#### 8.1 NAT 在流水线中的位置

```
... → Route Lookup → [NAT: SNAT/DNAT/FIP] → SG Egress → Forward
```

- 入向公网访问 FIP：DNAT 后再按实例侧安全语义判断
- 出向流量：以原始实例身份做安全判断，再做 SNAT

#### 8.2 Floating IP 语义

FIP 视为一对一 DNAT/SNAT 组合：
- 外部访问时：DNAT public_ip → private_ip
- 回程时：通过 conntrack 反向 SNAT private_ip → public_ip

#### 8.3 NAT Gateway 语义

共享 SNAT 出口：
- 多个实例共享一个或一组公网地址
- 通过 conntrack 维护 NAT 绑定
- Phase 3 不实现端口分配算法

#### 8.4 Checksum 与分片

- checksum 更新复用 LB 已有的 `bpf_l3_csum_replace` / `bpf_l4_csum_replace`
- 首片正常处理 NAT，非首片通过 conntrack 关联
- MTU/MSS 风险：NAT 不改变包大小，但 encap 场景需要 MSS clamping（Phase 7）

#### 8.5 nat_event 格式

```rust
// Phase 3 不落地代码
pub struct NatEvent {
    pub timestamp: u64,
    pub tap_id: u32,
    pub nat_type: u8,        // snat/dnat/fip
    pub proto: u8,
    pub pad: [u8; 2],
    pub original_src: [u8; 16],
    pub original_dst: [u8; 16],
    pub original_sport: u16,
    pub original_dport: u16,
    pub translated_src: [u8; 16],
    pub translated_dst: [u8; 16],
    pub translated_sport: u16,
    pub translated_dport: u16,
}
```

### 9. 事件可观测

#### 9.1 新增 drop_reason 常量

```rust
pub const DROP_PORT_IDENTITY_MISS: u8 = 20;
pub const DROP_ANTI_SPOOF: u8 = 21;
pub const DROP_SG_INGRESS: u8 = 22;
pub const DROP_SG_EGRESS: u8 = 23;
pub const DROP_ROUTE_MISS: u8 = 24;
pub const DROP_ROUTE_BLACKHOLE: u8 = 25;
```

#### 9.2 route_event

复用现有 `TraceEvent` envelope，在 `drop_reason` 字段中编码路由结果。
成功路由不单独生成事件（避免热路径开销），仅在 trace 模式下采样输出。
路由失败（miss/blackhole）通过 `do_drop` 统一记录。

### 10. 测试与验收策略

#### 10.1 验收场景

| 场景 | 验证内容 |
|------|---------|
| Port 绑定 | Agent 写入 PORT_IDENTITY_MAP，eBPF 查找命中 |
| Anti-Spoof 拦截 | 伪造源 IP 的包被丢弃，drop_event 记录 anti_spoof_violation |
| 同节点互通 | 两个 Port 之间通过路由表互通 |
| 安全组拦截 | 不在允许规则中的流量被 SG 拒绝 |
| LB + Route 协同 | LB 命中时跳过路由，LB 未命中时走路由 |
| 路由黑洞 | blackhole 路由正确丢弃并记录事件 |

#### 10.2 CI 验证

所有代码通过 GitHub Actions CI 编译验证，不在本地编译。

## 需求追溯矩阵

| 需求 | 设计章节 |
|------|---------|
| 需求 1：Port 绑定 | §1.1 PORT_IDENTITY_MAP, §2.2 port.rs, §6 Materialize |
| 需求 2：Anti-Spoof | §1.2 ANTI_SPOOF_MAP, §2.2 port.rs, §4.1 流水线 |
| 需求 3：L3 路由 | §1.3 ROUTE_TABLE, §2.3 route.rs, §4.1 流水线 |
| 需求 4：SG 分段检查 | §1.4 SG_RULE_MAP, §2.4 sg.rs, §4.1 流水线 |
| 需求 5：流水线顺序 | §4 流水线集成, §4.2 调用链分析, §4.3 栈预算 |
| 需求 6：事件可观测 | §9 事件可观测 |
| 需求 7：Map Schema | §1 全部 Schema 定义 |
| 需求 8：Agent Materialize | §5 core 操作模块, §6 Agent Materialize |
| 需求 9：LB 集成 | §7 与现有 LB 集成 |
| 需求 10：Mode B 设计 | §8 Mode B EIP/NAT 设计预留 |
| 需求 11：eBPF 约束 | §4.2 调用链分析, §4.3 栈预算 |
