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

### 2.5 当前代码映射

Phase 4.1 不需要重写数据源，当前仓库里已经有可复用入口：

| 能力 | 当前实现入口 | Phase 4.1 用法 |
|------|-------------|---------------|
| TCP-RT 聚合 | `ControlPlane::filter_tcprt` | 直接生成 TCP 维度 evidence |
| SSL 连接 | `ControlPlane::list_ssl` | 生成 TLS 握手 evidence |
| HTTP 事件 | `ControlPlane::list_ssl_http` | 生成 HTTP 维度 evidence |
| Kernel drops | `ControlPlane::get_kernel_drop_stats` | 生成 drops evidence |
| Diagnose 现有判定逻辑 | `user/src/commands/diagnose.rs` | 抽出为服务端 verdict/evidence 逻辑 |

这意味着第一批实现只需要做“共享 schema + 服务端聚合 + 路由/OpenAPI”，不需要改 eBPF map，也不需要新增 southbound 协议。

### 2.6 可执行拆解

#### Milestone A：共享 schema

目标：先冻结 API 面，确保后续 Agent / Controller / CLI 都复用同一组类型。

文件：
- `api/src/platform/event.rs`
- `api/src/platform/diagnose.rs`
- `api/src/platform/mod.rs`

落地项：
- 新增 `EventEnvelope`
- 新增 `DiagnoseRequest`
- 新增 `DiagnoseResponse`
- 新增 `DiagnoseEvidence`
- 统一 verdict 字符串：`healthy / degraded / unhealthy / unknown`
- 统一 evidence_type 字符串：`tcprt / ssl / http / kernel_drop / request`

验收：
- schema 可被 `aria_api::*` 直接复用
- Agent OpenAPI 可以注册这些类型
- 不引入新的 datapath 依赖

#### Milestone B：Agent Diagnose 核心逻辑

目标：先把当前 CLI 的 diagnose 能力搬到服务端，做成单节点可用 API。

文件：
- `agent/src/control_plane/diagnose.rs`
- `agent/src/control_plane.rs`
- `agent/src/api_handlers/diagnose.rs`
- `agent/src/api_handlers/mod.rs`
- `agent/src/api_routes.rs`
- `agent/src/openapi.rs`

落地项：
- 新增 `ControlPlane::diagnose_instance`
- 复用 `filter_tcprt / list_ssl / list_ssl_http / get_kernel_drop_stats`
- 生成结构化 `evidence`
- 复用现有 CLI 阈值，输出 `verdict / summary / candidate_causes / suggested_actions`
- 注册 `POST /api/v1/{instance}/diagnose`
- 把新接口加入 OpenAPI

验收：
- 给定 `dst_ip + dst_port` 可以返回结构化 JSON
- 当某类数据源不可用时，API 仍返回其他 evidence，不因为单源失败整体报错
- 无需新增 eBPF map、无须修改现有观测采集路径

#### Milestone C：服务端实现与 CLI 逻辑解耦

目标：避免后续 CLI 和 API 再次分叉。

文件：
- `user/src/commands/diagnose.rs`
- `user/src/api_client.rs`

落地项：
- CLI 改为调用 `POST /api/v1/{instance}/diagnose`
- 终端输出从 `DiagnoseResponse` 渲染，而不是再次自行读取 `tcprt / ssl / http / drops`
- 保留当前人类可读输出，但不再复制判定逻辑

验收：
- CLI 与服务端 verdict 一致
- CLI 不再持有独立判定阈值

#### Milestone D：为 Phase 4.2 预留扩展位

目标：让 Phase 4.2 可以直接在当前 schema 上继续推进，而不是重做。

文件：
- `api/src/platform/event.rs`
- `api/src/platform/diagnose.rs`

落地项：
- `DiagnoseEvidence` 允许携带结构化 `metrics`
- `EventEnvelope` 保留 `tenant_id / network_id / port_id / instance_id / service_id / trace_id`
- `DiagnoseRequest` 预留 `chain / time_window_seconds`

验收：
- 不需要修改 Phase 4.1 API 即可继续接 observe API

### 2.7 当前推荐实施切片

按风险和收益排序，先做下面这一刀：

1. `Milestone A`
2. `Milestone B`
3. 只做到 Agent Diagnose API 可用
4. 暂不切 CLI

原因：
- 这是最小可交付闭环
- 可以先把服务端 contract 冻结住
- 可以避免在未验证 API 之前同时改 CLI，降低回滚成本

### 2.8 实施步骤

| 步骤 | 文件 | 说明 |
|------|------|------|
| 1 | `api/src/platform/event.rs` | EventEnvelope 类型定义 |
| 2 | `api/src/platform/diagnose.rs` | DiagnoseRequest/Response/Evidence 类型 |
| 3 | `api/src/platform/mod.rs` | 注册新模块 |
| 4 | `agent/src/api_handlers/diagnose.rs` | 服务端 diagnose handler |
| 5 | `agent/src/api_routes.rs` | 注册 POST diagnose 路由 |
| 6 | `agent/src/openapi.rs` | OpenAPI 注册 |
| 7 | `user/src/commands/diagnose.rs` | CLI 改为调用 Agent API，展示结构化结果 |

### 2.9 提交顺序

建议至少拆成 3 个 commit：

1. `phase4.1a` schema
   - `api/src/platform/event.rs`
   - `api/src/platform/diagnose.rs`
   - `api/src/platform/mod.rs`

2. `phase4.1b` Agent Diagnose API
   - `agent/src/control_plane/diagnose.rs`
   - `agent/src/api_handlers/diagnose.rs`
   - `agent/src/api_routes.rs`
   - `agent/src/openapi.rs`

3. `phase4.1c` CLI 切换
   - `user/src/api_client.rs`
   - `user/src/commands/diagnose.rs`

### 2.10 当前实施状态

- `[x]` Milestone A：已完成（schema 文件已落地并通过 CI）
- `[x]` Milestone B：已完成（Agent Diagnose API 已落地并通过 CI）
- `[x]` Milestone C：已完成（CLI 已改为调用 Diagnose API 并通过 CI）
- `[~]` Milestone D：已在 schema 中预留 `chain / time_window_seconds` 与事件关联字段

本轮实现目标：
- `[x]` 先把实施清单记录到 docs
- `[x]` 开始落 `Milestone A`
- `[x]` 继续落 `Milestone B`
- `[x]` 开始落 `Milestone C`

### 2.11 提交策略

Phase 4.1 不再建议一次性做成 2 个 commit；推荐按 `schema -> Agent API -> CLI` 三段推进，每段各自过 CI。

## 3. Phase 4.2：统一事件查询接口（observe API）

### 3.1 目标

- Agent 提供 `GET /api/v1/observe/events` 接口
- 第一版支持按 `event_type / instance_id / service_id / src_ip / dst_ip / dst_port / protocol / time_range / limit` 过滤
- 返回 `Vec<EventEnvelope>`
- 把现有 tcprt / ssl / http / drops / flow_stats / lb_stats 统一转换为 EventEnvelope 格式
- `time_range` 按“最近 N 秒”的 monotonic 时间窗口过滤；无时间戳的事件在设置 `time_range` 时会被排除

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
    pub instance_id: Option<String>,
    pub service_id: Option<String>,
    pub src_ip: Option<String>,
    pub dst_ip: Option<String>,
    pub dst_port: Option<u16>,
    pub protocol: Option<String>,
    pub time_range: Option<u64>,
    pub limit: Option<usize>,
}
```

### 3.4 实施步骤

| 步骤 | 文件 | 说明 |
|------|------|------|
| 1 | `api/src/platform/observe.rs` | ObserveQuery + ObserveResponse 类型 |
| 2 | `agent/src/control_plane/observe.rs` | 聚合 tcprt / flow / drop / lb / ssl / http / kernel_drop 并转换为 EventEnvelope |
| 3 | `agent/src/api_handlers/observe.rs` | observe handler，调用 ControlPlane 聚合接口 |
| 4 | `agent/src/api_routes.rs` | 注册 GET observe 路由 |
| 5 | `agent/src/openapi.rs` | OpenAPI 注册 |

### 3.5 提交策略

1 个 commit。

### 3.6 当前实施状态

- `[x]` `api/src/platform/observe.rs` 已落地，冻结第一版 `ObserveQuery / ObserveResponse`
- `[x]` `agent/src/control_plane/observe.rs` 已落第一版聚合逻辑，当前覆盖：
  - `tcprt`
  - `flow`
  - `drop`
  - `kernel_drop`
  - `lb`
  - `ssl`
  - `http`
- `[x]` `agent/src/api_handlers/observe.rs`、`agent/src/api_routes.rs`、`agent/src/openapi.rs` 已接线并通过 CI
- `[ ]` CLI 侧暂未消费 observe API，本轮只交付 Agent API
- `[x]` `time_range` 过滤已实现，当前按“最近 N 秒”的 monotonic 时间窗口过滤；无时间戳的事件在设置 `time_range` 时会被排除
- `[x]` 事件排序已稳定：当前按 `timestamp desc -> event_type -> instance_id -> service_id -> event_id` 排序，避免同时间戳结果抖动
- `[x]` richer filters 已补齐第一批：当前额外支持 `instance_id / service_id / protocol` 过滤，直接复用 `EventEnvelope` 顶层字段，便于后续 CLI observe 子命令消费

## 4. Phase 4.3：Controller 侧 Diagnose 代理

### 4.1 目标

- Controller 提供 `POST /api/v1/diagnose` 接口
- Controller 代理请求到目标节点的 Agent Diagnose API
- 返回聚合结果（单节点场景下直接透传）
- 为 Phase 5 多节点聚合预留接口

### 4.2 实现方式

第一版 Controller diagnose proxy 先做显式单节点代理，不提前实现 Phase 5 的跨节点聚合逻辑。

Controller 通过 southbound 已知每个 node 的地址。收到 diagnose 请求后：
1. 请求体显式提供 `node_id + instance + dst_ip + dst_port`
2. Controller 优先从 southbound registration 的 `management` 地址取 Agent 管理地址
3. 如果 registration 里没有管理地址，则回退到 `Node.spec.mgmt_address`
4. Controller 向目标节点的 Agent API 发起 `POST /api/v1/{instance}/diagnose`
5. 单节点场景下直接透传 `DiagnoseResponse`

地址约定：
- 如果地址没有 schema，则默认补 `http://`
- 如果地址没有端口，则默认使用 Agent API 端口 `8080`
- 当前不做“根据目标 IP 自动推断 node”或“广播所有节点聚合”

### 4.3 实施步骤

| 步骤 | 文件 | 说明 |
|------|------|------|
| 1 | `controller/src/api_handlers/diagnose.rs` | Controller diagnose 代理 handler |
| 2 | `controller/src/api_handlers/mod.rs` | 错误映射与 handler 导出 |
| 3 | `controller/src/api_routes.rs` | 注册 `POST /api/v1/diagnose` |
| 4 | `controller/src/openapi.rs` | OpenAPI 注册 |
| 5 | `controller/Cargo.toml` | 引入 `reqwest` 作为上游代理客户端 |

### 4.4 当前落地状态

- `[x]` 共享 schema 已提供 `PlatformDiagnoseRequest`
- `[x]` Controller 已提供 `POST /api/v1/diagnose`
- `[x]` Controller 会代理到 Agent `POST /api/v1/{instance}/diagnose`
- `[x]` OpenAPI 已注册 controller diagnose path
- `[ ]` 自动按 `dst_ip` 推断节点
- `[ ]` 多节点 diagnose 聚合

### 4.5 提交策略

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
