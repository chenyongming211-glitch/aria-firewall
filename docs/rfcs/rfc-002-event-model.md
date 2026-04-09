# RFC-002：Aria 统一事件模型 v1

状态：Draft  
阶段：Phase 0  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 的统一事件模型 v1，用于统一承接：

- 流事件
- 丢包事件
- route / nat / lb 决策事件
- trace 事件
- TCP-RT 事件
- SSL / HTTP 事件
- Diagnose 诊断证据

统一事件模型的目的是把当前零散的观测模块汇总成一套可聚合、可存储、可查询、可诊断的通用底座。

## 2. 非目标

本 RFC 不涉及：

- 最终存储引擎选型
- 具体的消息队列实现
- UI 图表设计
- 具体 Prometheus 指标命名

## 3. 设计原则

### 3.1 事件先统一 envelope，再分专用 payload

所有事件必须共享统一公共头。专有字段放入类型化 payload。

### 3.2 事件是决策证据，不只是日志

事件必须能够回答：

- 发生了什么
- 在哪个节点和 hook 上发生
- 命中了哪些 datapath 决策
- 为什么得到当前 verdict

### 3.3 事件必须可关联

不同事件之间必须可以按以下维度关联：

- flow
- instance
- port
- service
- chain
- diagnose session

### 3.4 事件必须可裁剪

高基数事件必须支持：

- sampling
- rate limiting
- backpressure
- drop accounting
- retention policy

## 4. 总体结构

事件统一分为两部分：

- `EventEnvelope`
- `TypedPayload`

逻辑结构：

```text
Event
├── envelope
│   ├── event_id
│   ├── event_type
│   ├── timestamp
│   ├── node_id
│   ├── tenant_id
│   ├── network_id
│   ├── instance_id
│   ├── port_id
│   ├── flow_id
│   ├── hook
│   ├── direction
│   ├── verdict
│   └── correlation_id
└── payload
    └── type specific fields
```

## 5. EventEnvelope

### 5.1 必选字段

| 字段 | 说明 |
| --- | --- |
| `event_id` | 全局唯一事件 ID |
| `event_type` | 事件类型 |
| `timestamp` | 事件时间 |
| `node_id` | 事件来源节点 |
| `direction` | `ingress` 或 `egress` |
| `verdict` | `pass/drop/redirect/nat/lb/mirror/chain-forward` |
| `hook` | 触发位置，如 `xdp`、`tc_ingress`、`tc_egress` |

### 5.2 推荐字段

| 字段 | 说明 |
| --- | --- |
| `tenant_id` | 租户 ID |
| `network_id` | 网络 ID |
| `segment_id` | 分段 ID |
| `instance_id` | 实例 ID |
| `port_id` | 端口 ID |
| `flow_id` | 流 ID |
| `trace_id` | 追踪会话 ID |
| `diagnose_id` | 诊断会话 ID |
| `policy_id` | 命中策略 |
| `route_id` | 命中路由 |
| `service_id` | 命中服务 |
| `backend_id` | 选中的后端 |
| `chain_id` | 命中的服务链 |
| `agent_version` | 事件来源 Agent 版本 |
| `schema_version` | 事件 schema 版本 |

## 6. 标准枚举

### 6.1 event_type

建议初版支持：

- `flow_event`
- `drop_event`
- `route_event`
- `nat_event`
- `lb_event`
- `mirror_event`
- `trace_event`
- `tcprt_event`
- `ssl_event`
- `http_event`
- `chain_event`
- `agent_event`

### 6.2 verdict

建议初版支持：

- `pass`
- `drop`
- `redirect`
- `mirror`
- `nat`
- `lb`
- `chain-forward`
- `observe-only`

### 6.3 hook

建议初版支持：

- `xdp`
- `tc_ingress`
- `tc_egress`
- `cgroup_skb`
- `socket`
- `userspace_proxy`
- `uprobe`

## 7. 基础流字段

以下字段建议作为所有流相关事件可复用字段：

- `src_ip`
- `dst_ip`
- `src_port`
- `dst_port`
- `protocol`
- `ethertype`
- `packet_len`
- `bytes`
- `packets`
- `tcp_flags`

## 8. 各类 payload 定义

### 8.1 FlowPayload

用于基础流量与 verdict 事件。

建议字段：

- `src_ip`
- `dst_ip`
- `src_port`
- `dst_port`
- `protocol`
- `ct_state`
- `matched_classifier`

### 8.2 DropPayload

用于 drop 归因。

建议字段：

- `drop_reason`
- `drop_reason_code`
- `first_observed_at`
- `counter_packets`
- `counter_bytes`
- `iface`

### 8.3 RoutePayload

用于路由查表与转发决策。

建议字段：

- `destination_prefix`
- `next_hop_type`
- `next_hop`
- `egress_iface`
- `table_id`
- `lookup_result`

### 8.4 NatPayload

用于 NAT 转换事件。

建议字段：

- `nat_type`
- `original_src_ip`
- `original_dst_ip`
- `translated_src_ip`
- `translated_dst_ip`
- `translated_src_port`
- `translated_dst_port`
- `ct_hit`

### 8.5 LbPayload

用于负载均衡选择结果。

建议字段：

- `vip`
- `service_port`
- `lb_policy`
- `hash_key`
- `backend_ip`
- `backend_port`
- `backend_weight`
- `session_affinity_hit`

### 8.6 MirrorPayload

建议字段：

- `target_iface`
- `target_node_id`
- `mirror_mode`

### 8.7 TracePayload

建议字段：

- `trace_stage`
- `trace_point`
- `trace_reason`
- `latency_us`
- `decision_path`

### 8.8 TcpRtPayload

建议字段：

- `art_us`
- `rtt_client_us`
- `rtt_server_us`
- `handshake_us`
- `retrans_req`
- `retrans_resp`
- `nqa_score`
- `tcp_state`

### 8.9 SslPayload

建议字段：

- `sni`
- `alpn`
- `version`
- `cipher`
- `handshake_us`
- `error`

### 8.10 HttpPayload

建议字段：

- `method`
- `authority`
- `path`
- `status_code`
- `latency_us`
- `request_bytes`
- `response_bytes`

### 8.11 ChainPayload

建议字段：

- `chain_id`
- `hop_index`
- `hop_type`
- `selected_target`
- `fail_open`

## 9. 关联模型

### 9.1 关联键

统一事件模型必须支持以下关联键：

- `flow_id`
- `trace_id`
- `diagnose_id`
- `service_id`
- `instance_id`
- `port_id`

### 9.2 典型关联路径

推荐支持的查询场景：

- 从 `flow_event` 找到对应 `route_event`
- 从 `flow_event` 找到对应 `nat_event`
- 从 `flow_event` 找到对应 `lb_event`
- 从 `ssl_event` / `http_event` 关联到 `tcprt_event`
- 从 `drop_event` 追溯命中策略与节点

## 10. 采样与保留策略

### 10.1 默认建议

- `drop_event`：默认全量
- `route_event`：默认采样
- `nat_event`：默认采样
- `lb_event`：默认采样
- `trace_event`：会话驱动
- `tcprt_event`：聚合 + 样本混合
- `ssl_event` / `http_event`：按策略开启

### 10.2 背压原则

当事件系统出现背压时：

- 优先降采样高基数事件
- 保留 `drop_event`
- 保留严重 `agent_event`
- 记录事件丢失计数器

## 11. Diagnose 的关系

Diagnose 不应是独立数据源，而应是统一事件模型上的聚合推理层。

Diagnose API 的输入：

- 时间范围
- 目标实例 / 服务 / 目的地址
- 相关事件流

Diagnose API 的输出：

- verdict
- supporting evidence
- root cause candidates
- remediation hints

## 12. 当前代码映射

当前仓库中已存在的观测模块与未来事件类型的映射：

- `drops` -> `drop_event`
- `trace` -> `trace_event`
- `tcprt` -> `tcprt_event`
- `ssl` -> `ssl_event`
- `ssl/http` -> `http_event`
- `mirror stats` -> `mirror_event`
- 未来 routing / lb / nat 功能 -> `route_event` / `lb_event` / `nat_event`

## 13. 当前缺口

当前主要缺口：

- 没有统一 envelope
- 缺少 route / nat / lb 事件类型
- `diagnose` 还在 CLI 侧组合，没有服务端事件聚合入口
- 缺少跨节点聚合层

## 14. 验收标准

事件模型 v1 的验收标准：

- 能承载当前已有观测模块
- 能承载未来 routing / nat / lb 决策事件
- 能支持 Diagnose 服务端化
- 能支持 Relay 和历史查询

## 15. 后续拆分建议

- `RFC-002A` Diagnose API
- `RFC-002B` Relay / streaming query
- `RFC-002C` Metrics projection rules
