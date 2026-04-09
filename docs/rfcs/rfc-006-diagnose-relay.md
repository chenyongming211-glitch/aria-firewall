# RFC-006：Diagnose 服务端化与 Relay v1

状态：Draft  
阶段：Phase 4 / Phase 5 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

把当前以 CLI 为主的诊断能力升级为平台级服务能力，并引入多节点聚合层 `Aria Relay`。

本 RFC 的目标包括：

- Diagnose 从 CLI 组合逻辑升级为服务端能力
- 建立统一事件查询入口
- 建立跨节点实时聚合能力
- 为后续拓扑 UI、诊断 API、历史查询打底

## 2. 当前背景

当前仓库中的 `diagnose` 主要仍停留在 CLI 侧，通过拼接多个 API 结果生成摘要。

这意味着当前 Diagnose：

- 不能被外部系统稳定复用
- 缺少统一 session 语义
- 缺少跨节点聚合能力
- 缺少事件级证据模型

因此 Diagnose 必须从“CLI 命令”提升为“平台服务”。

## 3. 非目标

本 RFC 不直接定义：

- 完整 UI
- 历史存储后端
- 最终 protobuf 或 GraphQL schema
- 所有高级根因推理算法

## 4. 设计原则

### 4.1 Diagnose 建立在统一事件模型上

Diagnose 不应维护一套独立数据源。它应建立在 `RFC-002` 统一事件模型之上。

### 4.2 Diagnose 必须可机器消费

Diagnose 不只返回人类可读文本，还应返回：

- verdict
- evidence
- candidate causes
- impacted resources
- remediation hints

### 4.3 Relay 是聚合层，不是强耦合控制面

Relay 的职责是：

- 代理和聚合事件流
- 统一查询
- 维持实时订阅

它不应成为配置控制面必经路径。

### 4.4 单节点能力先服务端化，再做多节点聚合

建议先完成：

1. Agent 内部 Diagnose API
2. 统一事件视图
3. Relay 多节点聚合

## 5. 组件职责

### 5.1 Agent Diagnose Engine

节点侧负责：

- 收集本节点相关事件
- 按目标流 / 目标服务 / 目标实例聚合
- 生成基础 verdict 与证据

### 5.2 Relay

Relay 负责：

- 与多个 Agent 建立长连接
- 聚合多节点实时事件流
- 提供统一实时查询入口
- 做背压与租户隔离

### 5.3 Observability Platform

平台侧负责：

- 持久化
- 历史查询
- 高层拓扑和 dashboard
- 跨时段 Diagnose

## 6. Diagnose 模型

### 6.1 输入对象

Diagnose 请求建议至少支持：

- `target_type`
- `target_value`
- `time_range`
- `tenant_id`
- `network_id`
- `instance_id`
- `service_id`
- `port`
- `protocol`

### 6.2 支持的 target_type

建议初版支持：

- `instance`
- `port`
- `ip_port`
- `service`
- `chain`

### 6.3 输出模型

建议 Diagnose 输出：

- `diagnose_id`
- `verdict`
- `summary`
- `evidence`
- `candidate_causes`
- `suggested_actions`
- `related_events`
- `scope`

### 6.4 verdict 枚举

建议初版支持：

- `healthy`
- `degraded`
- `unhealthy`
- `unknown`

## 7. 证据模型

evidence 建议为结构化对象，而不是纯文本。

建议字段：

- `type`
- `severity`
- `title`
- `summary`
- `event_refs`
- `metric_refs`
- `resource_refs`
- `time_window`

典型证据来源：

- drop_event
- route_event
- nat_event
- tcprt_event
- ssl_event
- http_event

## 8. 根因候选模型

candidate cause 建议为明确分类，而不是随意拼接文字。

建议初版支持：

- `security_policy_deny`
- `anti_spoof_violation`
- `route_missing`
- `nat_misconfiguration`
- `backend_unhealthy`
- `high_retransmission`
- `tls_handshake_failure`
- `http_5xx_spike`
- `event_gap`

## 9. Diagnose 查询模式

### 9.1 同步查询

适合短时窗口、单目标查询。

### 9.2 异步会话

适合复杂场景、跨节点和长窗口分析。

### 9.3 持续观察

适合实时流场景。可视为“diagnose watch”模式。

## 10. 推荐 API 形态

### 10.1 Agent 本地 API

建议长期保留：

- `POST /api/v1/diagnose`
- `GET /api/v1/diagnose/{id}`

节点本地 API 主要用于：

- 单节点运维
- 调试
- 紧急定位

### 10.2 平台聚合 API

建议提供：

- `POST /api/v1/diagnose`
- `GET /api/v1/diagnose/{id}`
- `GET /api/v1/diagnose/{id}/events`
- `GET /api/v1/diagnose/{id}/timeline`

### 10.3 实时观察 API

建议提供：

- `GET /api/v1/observe`
- `GET /api/v1/observe/flows`
- `GET /api/v1/observe/events`

## 11. Relay 设计

### 11.1 Relay 输入

来自 Agent 的：

- 实时事件流
- 健康状态
- 能力信息
- 可选聚合摘要

### 11.2 Relay 输出

Relay 应提供：

- 多节点统一事件流
- 按租户 / 网络 / 服务过滤的 observe
- Diagnose 查询支撑

### 11.3 Relay 必须处理的问题

- 多节点并发订阅
- 事件去重
- 时间窗口排序
- 背压
- tenant 级权限过滤

## 12. Diagnose 引擎推荐流程

建议流程：

1. 解析目标范围
2. 拉取相关事件窗口
3. 按 flow / service / instance 建立关联图
4. 生成关键 verdict 证据
5. 归类候选根因
6. 输出 summary 和 remediation hints

## 13. 当前代码映射

当前已存在的种子能力：

- `tcprt`
- `ssl`
- `http`
- `drops`
- `trace`
- CLI diagnose 组合逻辑

未来应演进为：

- 统一服务端 Diagnose Engine
- 统一 evidence 模型
- Relay 驱动的跨节点查询

## 14. 分阶段落地建议

### 14.1 Phase 4A

先把 Diagnose 做成 Agent 服务端 API，替代 CLI 拼接。

### 14.2 Phase 4B

引入统一事件聚合接口，Diagnose 改为基于统一事件模型。

### 14.3 Phase 5

引入 Relay，实现多节点实时聚合与 Diagnose 跨节点查询。

## 15. 当前缺口

当前主要缺口：

- Diagnose 仍然偏 CLI 逻辑
- 没有统一 diagnose session
- 缺少 Relay
- 缺少 route / nat / lb 证据源

## 16. 验收标准

Diagnose 服务端化与 Relay v1 的验收标准：

- 单节点 Diagnose 可直接由服务端返回结构化结果
- Diagnose 输出包含 verdict、evidence 和 candidate causes
- Relay 能聚合多个 Agent 的实时事件
- 平台侧可按目标资源做跨节点查询

## 17. 后续拆分建议

- `RFC-006A` Diagnose API schema
- `RFC-006B` Relay stream protocol
- `RFC-006C` Timeline / topology view
