# RFC-005A：单节点 IaaS Map Schema v1

状态：Draft
阶段：Phase 3（Mode A 实现中 / Mode B 设计预留）
上游文档：
- [RFC-005 Routing / NAT 基础数据面 v1](rfc-005-routing-nat-datapath.md)
- [RFC-004 Node datapath 编译模型 v1](rfc-004-node-datapath-compiler.md)
- [Aria eBPF 实现约束](../ebpf-implementation-constraints.md)
- [单节点 IaaS 网络最小闭环设计](../../.kiro/specs/single-node-iaas-network/design.md)

## 1. 目标

冻结 Phase 3 单节点 IaaS 网络最小闭环（Mode A）的 map schema、编译输入约定和 materialize 顺序，并为 Mode B 的 EIP / NAT 设计预留稳定边界。

本文档回答三个问题：

- Phase 3 新增的 `Port / Anti-Spoof / Route / SecurityGroup` 在数据面上分别落到哪些 map
- Agent 在节点侧如何把平台对象编译成这些 map 的稳定 ABI
- Mode B 的 NAT / Floating IP 预留位如何文档化，但不进入本阶段代码实现

## 2. 非目标

本 RFC 不直接定义：

- 多节点 overlay / native routing 的真实实现
- NAT / Floating IP / NAT Gateway 的代码落地
- L4 LB 的 service map schema（已在 RFC-008A 定义）
- northbound / southbound API 细节

## 3. 设计约束

### 3.1 来自 eBPF 实现约束

- 所有结构必须 `#[repr(C)]`
- key/value 必须同时在 `ebpf/src/common.rs` 与 `core/src/common.rs` 中定义
- key 大小尽量控制在 32 字节以内，value 尽量控制在 64 字节以内
- 热路径 helper 使用 `#[inline(always)]`
- 不引入 tail call，不增加不可证明上界的循环

### 3.2 来自 RFC-004 编译模型

- 所有 map key 的第一个逻辑命名空间都是 `tap_id`
- 编译输出必须区分 `desired state / compiled state / runtime state`
- materialize 顺序必须稳定且可恢复
- `LB hit` 与 `Port/Route/SG` 流水线必须共享同一 `PipelineCtx`

### 3.3 当前实现收口原则

- Mode A 只落地 `Port identity / Anti-Spoof / Route / SecurityGroup`
- Mode B 只做文档预留，不落代码
- `SecurityGroup` 统一使用一个 `SG_RULE_MAP`，通过 `direction` 字段区分 ingress / egress
- Aya `LpmTrie::Key` 的 `prefix_len` 由 `Key` 包装结构承载，因此 `ROUTE_TABLE_V4/V6` 的数据 payload 分别是 `tap_id + address` 的 `8` / `20` 字节，而不是把 `prefix_len` 放入 payload

## 4. Map 总览

| Map 名称 | 类型 | Key 数据部分 | 用途 | 阶段 |
| --- | --- | --- | --- | --- |
| `PORT_IDENTITY_MAP` | `HashMap` | `tap_id` | Port 身份绑定 | Mode A |
| `ANTI_SPOOF_MAP` | `HashMap` | `tap_id + address` | 源地址反欺骗 | Mode A |
| `ROUTE_TABLE_V4` | `LpmTrie` | `tap_id + ipv4` (`[u8; 8]`) | IPv4 路由最长前缀匹配 | Mode A |
| `ROUTE_TABLE_V6` | `LpmTrie` | `tap_id + ipv6` (`[u8; 20]`) | IPv6 路由最长前缀匹配 | Mode A |
| `SG_RULE_MAP` | `HashMap` | `tap_id + sg_id + direction + proto + remote_prefix + prefix_len` | 入/出向安全组规则 | Mode A |
| `NAT_TABLE` | `HashMap` | `tap_id + proto + port + nat_type + address` | SNAT / DNAT / FIP 预留 | Mode B |

## 5. Mode A 详细 Schema

### 5.1 PORT_IDENTITY_MAP

用途：`tap_id -> Port` 身份元数据。

```rust
#[repr(C)]
pub struct PortIdentityKey {
    pub tap_id: u32,
}

#[repr(C)]
pub struct PortIdentityValue {
    pub mac: [u8; 6],
    pub flags: u16,
    pub network_id: u32,
    pub segment_id: u32,
    pub tenant_id: u32,
    pub primary_ipv4: u32,
    pub primary_ipv6: [u8; 16],
    pub sg_id: u32,
    pub ip_count: u16,
    pub pad: [u8; 2],
}
```

说明：

- `flags` 当前至少包含 `PORT_FLAG_ANTI_SPOOF` 与 `PORT_FLAG_HAS_ALLOWED_PAIRS`
- `segment_id` 在节点局部运行态中允许为 0，表示未显式绑定 segment
- `sg_id` 是节点局部分配后的稳定 ID

### 5.2 ANTI_SPOOF_MAP

用途：精确校验 `(tap_id, src_ip)` 是否属于该 Port 允许的固定地址或 `allowed_address_pairs`。

```rust
#[repr(C)]
pub struct AntiSpoofKey {
    pub tap_id: u32,
    pub address: [u8; 16],
}

#[repr(C)]
pub struct AntiSpoofValue {
    pub flags: u8,
    pub pad: [u8; 3],
}
```

说明：

- IPv4 统一使用 v4-mapped-v6 存储
- MAC 校验直接读取 `PORT_IDENTITY_MAP.mac`，不额外引入 MAC map

### 5.3 ROUTE_TABLE_V4 / ROUTE_TABLE_V6

用途：基于 `tap_id` 命名空间的目标地址最长前缀匹配。

```rust
// ROUTE_TABLE_V4 payload: tap_id(4) + ipv4(4) = 8 bytes
// ROUTE_TABLE_V6 payload: tap_id(4) + ipv6(16) = 20 bytes

#[repr(C)]
pub struct RouteValue {
    pub next_hop_ip: [u8; 16],
    pub egress_ifindex: u32,
    pub route_id: u16,
    pub next_hop_type: u8,
    pub priority: u8,
    pub flags: u8,
    pub pad: [u8; 3],
}
```

`next_hop_type` 当前约定：

| 值 | 名称 | 说明 |
| --- | --- | --- |
| `0` | `LOCAL_PORT` | 直接 `bpf_redirect` 到本地 `ifindex` |
| `1` | `GATEWAY` | 交给宿主机/后续网关路径处理 |
| `2` | `BLACKHOLE` | 直接丢弃 |
| `3` | `HOST` | 投递给主机网络栈 |

### 5.4 SG_RULE_MAP

用途：SecurityGroup 规则匹配。当前实现使用单表 + `direction` 字段，而不是 ingress / egress 两张表。

```rust
#[repr(C)]
pub struct SgRuleKey {
    pub tap_id: u32,
    pub sg_id: u32,
    pub direction: u8,
    pub proto: u8,
    pub pad: [u8; 2],
    pub remote_prefix: [u8; 16],
    pub prefix_len: u8,
    pub pad2: [u8; 3],
}

#[repr(C)]
pub struct SgRuleValue {
    pub action: u8,
    pub priority: u8,
    pub port_start: u16,
    pub port_end: u16,
    pub rule_id: u16,
}
```

说明：

- 当前 eBPF 执行路径采用 3 级回退：精确 → `proto=0` → `proto=0 + prefix_len=0`
- 当前编译器只接受 wildcard 或 host-exact selector；更宽的 CIDR selector 保留到后续版本

## 6. Agent 编译与 Materialize 约定

### 6.1 运行时绑定约定

Phase 3 当前实现使用临时 runtime label 约定把 Controller 的 Port 对象绑定到节点实际接口：

- `Port.metadata.labels["runtime.tap_id"]`
- `Port.metadata.labels["runtime.ifindex"]`

这两个字段是当前单节点 Mode A 的物化前提；长期会被更正式的 attachment/runtime binding 机制替代。

### 6.2 编译输出

当前 `CompiledNodeState` 至少产出：

- `port_identities`
- `route_entries`
- `sg_rules`

并保留：

- `port_bindings`
- `anti_spoof_entries`
- `domain_summaries`

### 6.3 Materialize 顺序

当前节点侧 materialize 顺序固定为：

1. `sync_iface_ctx`
2. `write_tap_config`
3. `write_port_identity`
4. `clear/write_anti_spoof_entries`
5. `clear/write_sg_rules`
6. `clear/write_route_entries`
7. `service materialization`

说明：

- 当前 Phase 3 map 写入先于 service map，避免 service runtime 依赖尚未存在的 Port 元数据
- 删除路径遵循 `clear-before-write`
- 同一个 Port 如果 `ifindex` 变化，先清旧 `iface_ctx` 再写新值

## 7. 数据面流水线顺序

当前 Mode A 在 `tc ingress` 中的目标顺序为：

1. parse packet
2. resolve `tap_id`
3. `PORT_IDENTITY_MAP` lookup
4. Anti-Spoof
5. `SG_RULE_MAP` ingress
6. conntrack / LB
7. route lookup
8. `SG_RULE_MAP` egress
9. forward / drop

补充约定：

- `LB hit` 时跳过常规 route lookup
- conntrack 快速路径可跳过重复安全组检查
- 当前 rollout 期间，为避免混合节点把未接入 Controller 的旧端口全部打掉，`Port identity miss` 仍保持兼容回退；待 attachment/runtime binding 完整切换后，再收紧为硬 drop

## 8. Mode B：NAT / Floating IP 设计预留

> 以下内容只定义边界，不进入本阶段代码。

### 8.1 NAT_TABLE Schema

```rust
#[repr(C)]
pub struct NatKey {
    pub tap_id: u32,
    pub address: [u8; 16],
    pub port: u16,
    pub proto: u8,
    pub nat_type: u8,
}

#[repr(C)]
pub struct NatValue {
    pub translated_address: [u8; 16],
    pub translated_port: u16,
    pub flags: u8,
    pub pad: [u8; 5],
}
```

### 8.2 语义约定

- `Floating IP`：一对一 DNAT/SNAT 组合
- `NAT Gateway`：共享 SNAT 出口
- NAT 插入顺序：`Route lookup -> NAT -> SG egress -> forward`
- NAT 绑定与回程恢复依赖 conntrack
- checksum 更新复用现有 LB helper
- 非首片分片包通过 conntrack 关联，不在 NAT 路径重新做五元组决策

### 8.3 事件预留

Mode B 预留 `nat_event`，至少包含：

- `nat_type`
- `original tuple`
- `translated tuple`
- `tap_id`
- `service / route / nat id`（如适用）

## 9. 当前实现状态（2026-04-11）

已完成：

- Mode A 结构体和 map schema 已在 `ebpf/src/common.rs` 与 `core/src/common.rs` 中定义
- `PORT_IDENTITY_MAP / ANTI_SPOOF_MAP / ROUTE_TABLE_V4/V6 / SG_RULE_MAP` 已在 `ebpf/src/maps.rs` 声明
- `port.rs / route.rs / sg.rs` 已接入节点侧数据面模块
- Agent 已开始把 `Port / RouteTable / SecurityGroup` 编译为真实 map 写入条目
- Agent 已开始对 `identity / ports / security / routes` 域回报真实 `applied / failed` 结果

未完成：

- `Port identity miss` 仍是兼容回退，不是最终硬 drop
- `default_route` 仍处于编译 warning 阶段，未进入实际 materialize
- 宽 CIDR selector、安全组 audit mode 仍未下沉到当前 Mode A eBPF 路径
- Mode B NAT / FIP / NAT Gateway 仍然只是文档预留

## 10. 与后续 RFC 的关系

- `RFC-005` 继续定义 Route / NAT / FIP 的平台级能力边界
- `RFC-005A` 冻结单节点 Mode A/Mode B 的节点侧 map schema 和物化顺序
- 多节点 overlay / native routing 继续由 [RFC-009](rfc-009-multi-node-networking.md) 负责
