# RFC-008A：Service LB Map Schema v1

状态：Draft
阶段：Phase B（Phase 6 前置）
上游文档：
- [RFC-008 Service / Backend / HealthCheck 模型 v1](rfc-008-service-backend-healthcheck.md)
- [RFC-004 Node datapath 编译模型 v1](rfc-004-node-datapath-compiler.md)
- [Aria eBPF 实现约束](../ebpf-implementation-constraints.md)

## 1. 目标

冻结 Service Domain 的 eBPF map key/value `repr(C)` 结构，作为：

- agent materialize 的写入目标
- eBPF datapath 的运行时查找输入
- pin 到 bpffs 后的稳定 ABI

本文档一旦冻结，后续修改 key/value 布局需要 pin namespace 迁移。

## 2. 非目标

- 不定义 eBPF 程序逻辑（那是实现阶段的事）
- 不定义 controller northbound API（已在 RFC-008/RFC-010 中定义）
- 不定义 agent shadow compiler 结构（已在 platform_agent.rs 中实现）

## 3. 设计约束

### 3.1 来自 eBPF 实现约束

- 所有结构必须 `#[repr(C)]`，字段对齐
- key 大小尽量控制在 16-32 字节
- value 大小尽量控制在 64 字节以内（避免 stack 压力）
- 必须同时在 `ebpf/src/common.rs` 和 `core/src/common.rs` 定义
- 必须实现 `unsafe impl Pod`

### 3.2 来自 RFC-004 编译模型

- 所有 map key 必须包含 `tap_id`（共享 runtime 下的命名空间隔离）
- map 类型优先选择 `HashMap`（精确查找）或 `Array`（索引查找）
- 避免使用 `LpmTrie`（Service Domain 不需要前缀匹配）

### 3.3 来自 RFC-008 对象模型

- Service 可以有多个 listener port
- BackendSet 可以有多个 backend member
- Backend 有 locality（local/remote）和 weight
- 需要支持 RevNat（反向 NAT 回程映射）
- 需要支持 session affinity
- 需要支持 maglev 一致性哈希（预留）

## 4. Map 总览

| Map 名称 | 类型 | 用途 | 查找路径 |
|----------|------|------|---------|
| `SVC_FRONTEND_MAP` | `HashMap` | VIP+port+proto → service 元数据 + backend 选择入口 | socket LB / packet LB |
| `SVC_BACKEND_MAP` | `HashMap` | service_id + backend_slot → 具体后端地址 | backend 选择后查找 |
| `SVC_REVNAT_MAP` | `HashMap` | 反向 NAT key → 原始 service 信息 | 回程包恢复原始目的 |
| `SVC_AFFINITY_MAP` | `LruHashMap` | 会话亲和 key → 上次选中的 backend | session affinity 命中 |
| `SVC_MAGLEV_MAP` | `Array` | maglev lookup table（预留） | maglev 一致性哈希 |

## 5. 详细 Schema

### 5.1 SVC_FRONTEND_MAP

用途：VIP 入口查找。socket LB 和 packet LB 都从这里开始。

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcFrontendKey {
    pub tap_id: u32,
    pub address: [u8; 16],  // IPv4 映射到 ::ffff:x.x.x.x，统一 16 字节
    pub port: u16,
    pub proto: u8,           // IPPROTO_TCP=6, IPPROTO_UDP=17
    pub scope: u8,           // 0=internal, 1=external
}
// 大小：24 字节

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcFrontendValue {
    pub service_id: u32,     // 本地分配的 service 数字 ID
    pub backend_count: u16,  // 当前有效后端数量（不含 disabled，slot 紧凑排列）
    pub flags: u16,          // bit0=has_affinity, bit1=use_maglev, bit2=local_only, bit3=has_remote
    pub lb_algo: u8,         // 0=random, 1=maglev, 2=hash_src_ip
    pub pad: [u8; 3],
}
// 大小：12 字节
```

说明：
- `address` 统一用 16 字节表示 IPv4/IPv6，IPv4 用 v4-mapped-v6 格式
- `scope` 区分 internal（socket LB 可命中）和 external（仅 packet LB）
- `service_id` 是 agent 在编译阶段分配的本地数字 ID，不是平台 UUID
- `backend_count` 用于 random 选择时的取模上界

### 5.2 SVC_BACKEND_MAP

用途：根据 service_id + slot 查找具体后端。

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcBackendKey {
    pub tap_id: u32,
    pub service_id: u32,
    pub slot: u16,           // 0..backend_count-1
    pub pad: [u8; 2],
}
// 大小：12 字节

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcBackendValue {
    pub address: [u8; 16],   // 后端 IP（v4-mapped-v6）
    pub port: u16,
    pub weight: u16,
    pub flags: u16,          // bit0=local, bit1=remote, bit2=disabled, bit3=draining
    pub pad: [u8; 2],
}
// 大小：24 字节
```

说明：
- `slot` 是 0-based 索引，agent 在写入时按权重展开或按顺序排列
- random 选择：`bpf_get_prandom_u32() % backend_count` 得到 slot
- maglev 选择：查 `SVC_MAGLEV_MAP` 得到 slot
- `flags.local` 表示后端在本节点，可直接转发
- `flags.remote` 表示后端在其他节点，需要 handoff

### 5.3 SVC_REVNAT_MAP

用途：回程包（从后端返回的响应）恢复原始 VIP 目的地址。

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcRevNatKey {
    pub tap_id: u32,
    pub address: [u8; 16],   // 后端 IP
    pub port: u16,            // 后端 port
    pub proto: u8,
    pub pad: u8,
}
// 大小：24 字节

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcRevNatValue {
    pub service_address: [u8; 16],  // 原始 VIP
    pub service_port: u16,
    pub pad: [u8; 6],
}
// 大小：24 字节（8 字节对齐）
```

说明：
- RevNat 在 node-local 场景下用于把后端响应的源地址恢复为 VIP
- 回程包匹配：`(backend_ip, backend_port, proto)` → `(vip, service_port)`

### 5.4 SVC_AFFINITY_MAP

用途：session affinity（会话保持）。

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcAffinityKey {
    pub tap_id: u32,
    pub service_id: u32,
    pub client_address: [u8; 16],  // 客户端 IP
}
// 大小：24 字节

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcAffinityValue {
    pub backend_slot: u16,
    pub pad: [u8; 2],
    pub last_used_ns: u64,
}
// 大小：12 字节（对齐后 16）
```

说明：
- 使用 `LruPerCpuHashMap`，自动淘汰最久未使用的条目
- `client_ip` affinity：按客户端 IP 查找上次选中的 backend slot
- `last_used_ns` 用于 agent 侧判断是否过期（eBPF 侧由 LRU 自动管理）

### 5.5 SVC_MAGLEV_MAP（预留）

用途：maglev 一致性哈希查找表。

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SvcMaglevEntry {
    pub backend_slot: u16,
    pub pad: [u8; 2],
}
// 大小：4 字节
```

说明：
- 使用 `Array`，大小为 `service_count * MAGLEV_TABLE_SIZE`
- `MAGLEV_TABLE_SIZE` 建议 251（素数，Cilium 默认值）
- 查找：`hash(5tuple) % MAGLEV_TABLE_SIZE` → `backend_slot`
- Phase B 不实现，仅预留 map 定义和 schema

## 6. Map 容量规划

| Map | 建议初始容量 | 说明 |
|-----|------------|------|
| `SVC_FRONTEND_MAP` | 4096 | 每个 (vip, port, proto, scope) 一条 |
| `SVC_BACKEND_MAP` | 16384 | 每个 service 的每个 backend slot 一条 |
| `SVC_REVNAT_MAP` | 4096 | 每个 (backend_ip, backend_port, proto) 一条 |
| `SVC_AFFINITY_MAP` | 65536 | LRU，按客户端 IP 自动淘汰 |
| `SVC_MAGLEV_MAP` | 0 | Phase B 不分配，预留定义 |

## 7. 数据面查找流程

### 7.1 Socket LB 路径（connect-time）

```
connect(VIP:port) 
  → SVC_FRONTEND_MAP lookup (tap_id, vip, port, proto, scope=internal)
  → 命中：读 service_id, backend_count, flags
  → 如果 has_affinity：SVC_AFFINITY_MAP lookup
    → 命中且 slot < backend_count：复用 slot
    → 未命中：random/maglev 选择 slot
  → SVC_BACKEND_MAP lookup (tap_id, service_id, slot)
  → 得到 backend_ip, backend_port
  → 如果 local：直接 DNAT connect 目标
  → 如果 remote：标记需要 handoff（Phase C）
```

### 7.2 Packet LB 路径（TC ingress/egress）

```
packet dst=VIP:port
  → SVC_FRONTEND_MAP lookup (tap_id, vip, port, proto, scope)
  → 命中：同上选择 backend
  → DNAT packet header
  → 更新 conntrack
  → SVC_REVNAT_MAP 写入（用于回程恢复）
  → forward
```

### 7.3 RevNat 回程路径

```
packet src=backend_ip:backend_port
  → SVC_REVNAT_MAP lookup (tap_id, backend_ip, backend_port, proto)
  → 命中：SNAT src → service_vip:service_port
  → forward
```

## 8. Agent Materialize 流程

Agent 从 `CompiledNodeState.service_programs` 生成 map 写入：

1. 遍历每个 `ServiceProgramIr`
2. 为每个 service 分配本地 `service_id`（u32，generation 内稳定）
3. 为每个 `listener_port` 写入 `SVC_FRONTEND_MAP`
4. 为每个 backend member 按 slot 写入 `SVC_BACKEND_MAP`
5. 为每个 (backend_ip, backend_port, proto) 写入 `SVC_REVNAT_MAP`
6. 如果 has_affinity：`SVC_AFFINITY_MAP` 由 eBPF 运行时自动填充，agent 不预写

### 8.1 Slot 紧凑排列规则

`SVC_BACKEND_MAP` 的 slot 必须紧凑排列（0..backend_count-1），不允许空洞：

- Agent 在写入时，只写入 `admin_state != disabled` 的后端
- `backend_count` 等于实际写入的有效后端数量
- 删除后端时，agent 必须重新压缩 slot 列表（把最后一个 slot 移到被删除的位置）
- 这样 `bpf_get_prandom_u32() % backend_count` 永远命中有效条目

### 8.2 健康检查与 disabled 后端

Phase B 策略：disabled 后端直接从 slot 列表中物理删除，不保留 disabled 标记。

Phase C 扩展：如果需要优雅排干（draining），可以保留 `flags.draining` 标记，但 `backend_count` 仍然只计算可调度后端。draining 后端通过 affinity 命中，不参与新连接的 random 选择。

## 9. Flags 位定义

### 9.1 SvcFrontendValue.flags

| Bit | 名称 | 说明 |
|-----|------|------|
| 0 | `has_affinity` | 启用 session affinity，查找 SVC_AFFINITY_MAP |
| 1 | `use_maglev` | 使用 maglev 一致性哈希，查找 SVC_MAGLEV_MAP |
| 2 | `local_only` | 所有后端都在本节点，无需 handoff |
| 3 | `has_remote` | 存在跨节点后端，可能需要 handoff |

### 9.2 SvcBackendValue.flags

| Bit | 名称 | 说明 |
|-----|------|------|
| 0 | `local` | 后端在本节点 |
| 1 | `remote` | 后端在其他节点 |
| 2 | `disabled` | 后端被管理员禁用（Phase B 不写入 map） |
| 3 | `draining` | 后端正在排干（Phase C，仅 affinity 命中） |

## 10. ID 分配策略

- `service_id`：agent 本地分配，从 1 开始递增，generation 内稳定
- `slot`：0-based，按 backend 顺序排列，权重展开在 Phase C 实现
- `tap_id`：复用现有共享 runtime 的 tap_id 分配机制

## 11. 与现有 map 的关系

Service Domain 的 map 与现有防火墙 map 完全独立：

- 不复用 `POLICY_TABLE`（那是 ACL 的）
- 不复用 `QOS_CONFIG`（那是 QoS 的）
- 不复用 `CT_TABLE_V4/V6`（但 packet LB 路径会写入 conntrack）
- pin 在同一个共享 namespace 下（`/sys/fs/bpf/aria/global-v2/`）

## 12. 验收标准

- `repr(C)` 结构体在 `ebpf/src/common.rs` 和 `core/src/common.rs` 同时定义
- agent 能正确写入和读取所有 map
- eBPF 程序能正确查找 frontend → backend → revnat
- node-local random LB 端到端可验证
- map schema 变更需要 pin namespace 迁移

## 13. Phase B 实施范围

Phase B 只实现：

- `SVC_FRONTEND_MAP`：完整实现
- `SVC_BACKEND_MAP`：完整实现
- `SVC_REVNAT_MAP`：完整实现
- `SVC_AFFINITY_MAP`：定义 schema，eBPF 侧预留查找位，但不启用
- `SVC_MAGLEV_MAP`：仅定义 schema，不分配容量

Phase C 扩展：

- affinity 启用
- maglev 实现
- cross-node handoff
- packet LB 路径
