# Phase 4 实施方案：统一事件模型与 Diagnose 平台化

> 基于 RFC-002（统一事件模型）和 RFC-006（Diagnose 服务端化与 Relay）

## 0. 当前基线

当前仓库已有的观测能力：

| 能力 | 数据源 | 当前入口 | 存储 |
|------|--------|---------|------|
| TCP-RT | eBPF `TCPRT_TABLE_V4/V6` | `ariactl tcprt` | eBPF LRU map |
| SSL/TLS | eBPF uprobe `SSL_CONN_TABLE` | `ariactl ssl` | eBPF LRU map |
| HTTP | eBPF uprobe `SSL_HTTP_TABLE` | `ariactl ssl http` | eBPF LRU map |
| Trace | eBPF `TRACE_LOG` + PerfEventArray | `ariactl trace` | eBPF map + perf ring |
| Drops (firewall) | eBPF `DROP_REASON_STATS` | `ariactl stats --drops` | eBPF PerCpu map |
| Drops (kernel) | eBPF kprobe `KERNEL_DROP_STATS` | `ariactl drops` | eBPF LRU PerCpu map |
| Flow stats | eBPF `FLOW_STATS_V4/V6` | `ariactl stats --flows` | eBPF LRU PerCpu map |
| LB stats | eBPF `SVC_LB_STATS` | `ariactl stats --lb` | eBPF PerCpu map |
| Diagnose | CLI 拼接 tcprt + ssl + http + drops | `ariactl diagnose` | 无持久化 |

关键缺口：
- 没有统一事件 envelope
- Diagnose 是 CLI 侧拼接，不是服务端 API
- 没有结构化 verdict / evidence / candidate causes
- 没有跨节点聚合

## 1. 分步策略

Phase 4 拆成 3 个可独立交付的子阶段：

```
Phase 4.1：统一事件 schema + Agent 服务端 Diagnose API
Phase 4.2：统一事件查询接口（observe API）
Phase 4.3：Controller 侧 Diagnose 代理（跨节点聚合预留）
```

Phase 5（Relay + 多节点聚合）不在本方案范围内。

## 2. Phase 4.1：统一事件 schema + Agent Diagnose API

### 2.1 目标

- 定义统一事件 Rust 类型（`EventEnvelope` + typed payload）
- 把 CLI `diagnose` 逻辑搬到 Agent 服务端
- Agent 返回结构化 `DiagnoseResponse`（verdict + evidence + causes）

### 2.2 共享类型（`api/src/platform/event.rs`）

```rust
pub struct EventEnvelope {
    pub event_id: String,
    pub event_type: String,       // flow / drop / tcprt / ssl / http / lb / trace
    pub timestamp: String,
    pub node_id: String,
    pub direction: String,        // ingress / egress
    pub verdict: String,          // pass / drop / redirect / nat / lb / mirror
    pub hook: String,             // xdp / tc_ingress / tc_egress / uprobe
    // 可选关联字段
    pub tenant_id: Option<String>,
    pub network_id: Option<String>,
    pub port_id: Option<String>,
    pub instance_id: Option<String>,
    pub flow_id: Option<String>,
    pub service_id: Option<String>,
    pub trace_id: Option<String>,
    // 流字段
    pub src_ip: Option<String>,
    pub dst_ip: Option<String>,
    pub src_port: Option<u16>,
    pub dst_port: Option<u16>,
    pub protocol: Option<String>,
    // typed payload（JSON value，按 event_type 解析）
    pub payload: serde_json::Value,
}
```

不需要在 eBPF 侧改任何东西。统一 envelope 是用户态的聚合视图，eBPF map 数据通过现有 `core::*_ops` 读取后转换。

### 2.3 Diagnose API 类型（`api/src/platform/diagnose.rs`）

```rust
pub struct DiagnoseRequest {
    pub dst_ip: String,
    pub dst_port: u16,
    pub chain: Option<String>,
    pub time_window_seconds: Option<u64>,
}

pub struct DiagnoseResponse {
    pub diagnose_id: String,
    pub verdict: String,          // healthy / degraded / unhealthy / unknown
    pub summary: String,
    pub evidence: Vec<DiagnoseEvidence>,
    pub candidate_causes: Vec<String>,
    pub suggested_actions: Vec<String>,
}

pub struct DiagnoseEvidence {
    pub evidence_type: String,    // tcprt / ssl / http / drop / kernel_drop
    pub severity: String,         // info / warning / critical
    pub title: String,
    pub summary: String,
    pub metrics: serde_json::Value,
}
```

### 2.4 Agent 服务端 handler

新增 `agent/src/api_handlers/diagnose.rs`：
- `POST /api/v1/{instance}/diagnose` 接收 `DiagnoseRequest`
- 内部调用 `core::tcprt_ops`、`core::ct_ops`、SSL/HTTP map 读取、kernel drop 读取
- 组装 evidence，计算 verdict，返回 `DiagnoseResponse`
- 逻辑从 `user/src/commands/diagnose.rs` 搬过来，但输出结构化 JSON 而不是 println

### 2.5 实施步骤

| 步骤 | 文件 | 说明 |
|------|------|------|
| 1 | `api/src/platform/event.rs` | EventEnvelope 类型定义 |
| 2 | `api/src/platform/diagnose.rs` | DiagnoseRequest/Response/Evidence 类型 |
| 3 | `api/src/platform/mod.rs` | 注册新模块 |
| 4 | `agent/src/api_handlers/diagnose.rs` | 服务端 diagnose handler |
| 5 | `agent/src/api_routes.rs` | 注册 POST diagnose 路由 |
| 6 | `agent/src/openapi.rs` | OpenAPI 注册 |
| 7 | `user/src/commands/diagnose.rs` | CLI 改为调用 Agent API，展示结构化结果 |

### 2.6 提交策略

2 个 commit：
- Commit A：schema（event.rs + diagnose.rs）
- Commit B：agent handler + CLI 改造

## 3. Phase 4.2：统一事件查询接口（observe API）

### 3.1 目标

- Agent 提供 `GET /api/v1/observe/events` 接口
- 支持按 event_type / src_ip / dst_ip / dst_port / time_range 过滤
- 返回 `Vec<EventEnvelope>`
- 把现有 tcprt / ssl / http / drops / flow_stats / lb_stats 统一转换为 EventEnvelope 格式

### 3.2 实现方式

不改 eBPF map。在 Agent 用户态做转换：

```
core::tcprt_ops::list_tcprt()     → Vec<EventEnvelope> (event_type = "tcprt")
core::ct_ops::list_flows()        → Vec<EventEnvelope> (event_type = "flow")
SSL_CONN_TABLE read               → Vec<EventEnvelope> (event_type = "ssl")
SSL_HTTP_TABLE read                → Vec<EventEnvelope> (event_type = "http")
DROP_REASON_STATS read             → Vec<EventEnvelope> (event_type = "drop")
KERNEL_DROP_STATS read             → Vec<EventEnvelope> (event_type = "kernel_drop")
SVC_LB_STATS read                  → Vec<EventEnvelope> (event_type = "lb")
```

### 3.3 查询参数

```rust
pub struct ObserveQuery {
    pub event_type: Option<String>,
    pub src_ip: Option<String>,
    pub dst_ip: Option<String>,
    pub dst_port: Option<u16>,
    pub limit: Option<usize>,
}
```

### 3.4 实施步骤

| 步骤 | 文件 | 说明 |
|------|------|------|
| 1 | `api/src/platform/observe.rs` | ObserveQuery + ObserveResponse 类型 |
| 2 | `agent/src/api_handlers/observe.rs` | observe handler，读各 map 转换为 EventEnvelope |
| 3 | `agent/src/api_routes.rs` | 注册 GET observe 路由 |
| 4 | `agent/src/openapi.rs` | OpenAPI 注册 |

### 3.5 提交策略

1 个 commit。

## 4. Phase 4.3：Controller 侧 Diagnose 代理

### 4.1 目标

- Controller 提供 `POST /api/v1/diagnose` 接口
- Controller 代理请求到目标节点的 Agent Diagnose API
- 返回聚合结果（单节点场景下直接透传）
- 为 Phase 5 多节点聚合预留接口

### 4.2 实现方式

Controller 通过 southbound 已知每个 node 的地址。收到 diagnose 请求后：
1. 根据目标 IP 确定相关节点（或广播所有节点）
2. 向目标节点的 Agent API 发起 `POST /api/v1/{instance}/diagnose`
3. 聚合结果返回

### 4.3 实施步骤

| 步骤 | 文件 | 说明 |
|------|------|------|
| 1 | `controller/src/api_handlers/diagnose.rs` | Controller diagnose 代理 handler |
| 2 | `controller/src/api_routes.rs` | 注册路由 |
| 3 | `controller/src/openapi.rs` | OpenAPI 注册 |

### 4.4 提交策略

1 个 commit。

## 5. 不做的事情

- 不改 eBPF 数据面代码
- 不引入新的 eBPF map
- 不引入消息队列或外部存储
- 不做 Relay（Phase 5）
- 不做 protobuf/gRPC streaming（Phase 5）
- 不做 UI（Phase 8）

## 6. 风险评估

| 风险 | 等级 | 缓解 |
|------|------|------|
| EventEnvelope 字段过多导致序列化开销 | 低 | Option 字段 skip_serializing_if |
| observe API 读多个 map 延迟高 | 中 | limit 参数 + 按 event_type 过滤减少读取范围 |
| Controller diagnose 代理需要知道 Agent 地址 | 低 | southbound registration 已有 node addresses |

## 7. 验收标准

- `POST /api/v1/{instance}/diagnose` 返回结构化 JSON（verdict + evidence + causes）
- `GET /api/v1/observe/events` 返回统一 EventEnvelope 格式
- `ariactl diagnose` 改为调用 Agent API 并展示结构化结果
- Controller `POST /api/v1/diagnose` 能代理到 Agent 并返回结果
- 全部 CI 绿，零 warning

## 8. 推荐执行顺序

```
Phase 4.1 Commit A：event.rs + diagnose.rs schema          → CI
Phase 4.1 Commit B：agent diagnose handler + CLI 改造       → CI
Phase 4.2：observe API                                      → CI
Phase 4.3：controller diagnose 代理                          → CI
```

总计 4 个 commit，预计改动 ~15 个文件，~1500 行新增。
