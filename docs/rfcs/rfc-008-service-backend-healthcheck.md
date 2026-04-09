# RFC-008：Service / Backend / HealthCheck 模型 v1

状态：Draft  
阶段：Phase 6  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 平台中的四层服务抽象，用于承载：

- VIP
- Backend 选择
- 健康检查
- 会话保持
- 与服务链联动

本 RFC 的目标是冻结服务对象模型和控制面语义，为未来 L4 负载均衡 datapath 提供稳定输入。

## 2. 非目标

本 RFC 不覆盖：

- 完整 L7 网关路由
- 复杂 HTTP header 路由
- 自研全量健康检查执行器实现
- 多区域 GSLB

## 3. 设计原则

### 3.1 Service 是控制面对象，不是 datapath 里的散规则

VIP、后端、健康和策略必须围绕 `Service` 对象表达，而不是用若干孤立 NAT / redirect 规则拼凑。

### 3.2 后端选择必须可观测

每次 service 命中必须能够回答：

- 命中了哪个 service
- 为什么选中该 backend
- backend 当前健康状态如何
- 是否命中会话保持

### 3.3 健康检查结果属于控制面输入

健康检查可以由节点执行、平台汇总，但其结果最终应进入 `BackendSet` 的有效视图，而不是临时脚本式逻辑。

### 3.4 初版先清晰支持 L4

v1 先支持 TCP / UDP 级别服务，不把 L7 网关复杂性引入本 RFC。

### 3.5 L4 负载均衡必须同时覆盖节点内与跨节点转发

Service datapath 的第一版目标不能只停留在“节点内 VIP 命中”。
v1 必须把以下两类路径同时作为正式目标：

- 节点内转发：请求和被选中的 backend 位于同一节点
- 跨节点转发：请求命中本节点 service 入口，但 backend 位于其他节点

因此，后续 `routing / NAT / FloatingIP` 的接口预留和 shadow 骨架，只能作为不阻塞 L4 主路径的前置准备，不能反向改变 Service/LB 的优先级。

## 4. 资源模型

### 4.1 Service

表示一个对外暴露的逻辑服务入口。

建议字段：

- `id`
- `tenant_id`
- `network_id`
- `name`
- `vip`
- `protocol`
- `ports`
- `backend_set_id`
- `session_affinity`
- `lb_policy`
- `exposure_type`
- `status`

说明：

- `exposure_type` 可表示 `internal`、`floating-ip-backed`、`gateway-exposed`

### 4.2 BackendSet

表示一组可供调度的后端。

建议字段：

- `id`
- `service_id`
- `health_check_id`
- `policy`
- `backends`

### 4.3 Backend

表示单个后端成员。

建议字段：

- `id`
- `backend_set_id`
- `target_type`
- `target_ref`
- `ip`
- `port`
- `weight`
- `admin_state`
- `observed_state`

### 4.4 HealthCheck

表示后端健康检查定义。

建议字段：

- `id`
- `type`
- `interval`
- `timeout`
- `healthy_threshold`
- `unhealthy_threshold`
- `target_port`
- `request_template`

## 5. Service 作用域

Service 至少应具备：

- tenant 归属
- network 归属
- 可被 Route / Chain / FloatingIP 引用

Service 不应默认跨租户共享。

## 6. 调度策略

### 6.1 初版建议支持

- `round_robin`
- `maglev`
- `hash_5tuple`
- `hash_src_ip`

### 6.2 会话保持

建议支持：

- `none`
- `client_ip`
- `5tuple`

### 6.3 权重

Backend 应支持权重，但 v1 可允许简单整数权重。

## 7. 健康状态模型

### 7.1 状态来源

Backend 状态可来自：

- active health check
- passive observation
- operator override

### 7.2 状态枚举

建议至少支持：

- `healthy`
- `degraded`
- `unhealthy`
- `unknown`
- `draining`

### 7.3 健康与调度关系

建议：

- `healthy`：可调度
- `degraded`：可调度但可降低权重
- `unhealthy`：默认不调度
- `draining`：仅保留已有会话

## 8. datapath 投影要求

Agent 编译后应能生成：

- service id -> VIP 映射
- service port -> backend set 映射
- backend selection map
- session affinity state
- health-filtered backend view
- 节点内转发与跨节点转发所需的 service forwarding projection

## 9. 与 NAT 和 Route 的关系

Service 抽象与路由、NAT 需要明确边界：

- 外部访问可先经 Route/FIP，再命中 Service
- 内部流量可直接命中 Service
- Service 本身不替代 RouteTable
- NAT / Route / FIP 的接口预留不得改变 Service datapath 的主优先级；L4 负载均衡必须按自身路径独立成立

## 10. 与 Chain 的关系

Service 应可以作为 Chain 的一个 hop 或前置入口。

示例：

- 入口 VIP -> 安全检查 Service -> 应用 Service
- Service 命中后，进入后续 Chain hop

## 11. 事件模型要求

Service datapath 相关事件至少应输出：

- `service_id`
- `backend_id`
- `lb_policy`
- `hash_key` 或 hash 摘要
- `affinity_hit`
- `backend_state`

对应事件类型主要为：

- `lb_event`
- `flow_event`
- `drop_event`（无健康后端等场景）

## 12. 当前代码映射与缺口

当前仓库中的：

- `service_chain`

只是链路配置的早期原型，并不等于正式的 `Service` 抽象。

当前缺口：

- 没有正式的 Service / BackendSet / HealthCheck 对象
- 没有 LB datapath
- 没有 backend 健康模型
- 没有 lb_event

## 13. 分阶段落地建议

### 13.1 第一阶段

先定义对象和 northbound API，不急于一次做完所有调度策略。

### 13.2 第二阶段

实现节点级 service 编译与 backend 选择，至少覆盖节点内转发和跨节点转发的统一 service 语义。

### 13.3 第三阶段

引入健康检查与 session affinity。

### 13.4 第四阶段

与 Chain、Relay、Diagnose 做深度联动。

## 14. 验收标准

Service 模型 v1 的验收标准：

- northbound 能表达 VIP、后端、健康检查
- Agent 能把 Service 编译成节点局部 service state
- L4 负载均衡能同时覆盖节点内转发和跨节点转发
- 能输出 backend 选择与健康相关事件
- Diagnose 能基于 Service 维度查询和解释故障
- Agent 能把 Service 编译成节点局部 service state
- datapath 能输出稳定的 backend 选择证据
- Diagnose 能基于 Service 维度查询和解释故障

## 15. 后续拆分建议

- `RFC-008A` LB datapath
- `RFC-008B` health check execution
- `RFC-008C` session affinity model
