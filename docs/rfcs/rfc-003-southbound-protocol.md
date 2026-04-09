# RFC-003：Controller-Agent Southbound 协议 v1

状态：Draft  
阶段：Phase 0  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria Controller 与节点侧 Aria Agent 之间的 southbound 协议边界，用于：

- 配置下发
- 状态回报
- 健康检查
- 能力协商
- 事件流对接

本 RFC 的目标是冻结职责边界与交互模式，而不是直接定义最终 protobuf。

## 2. 非目标

本 RFC 不直接给出：

- 最终 `.proto` 文件
- 消息队列替代方案
- northbound API 设计
- UI 对接协议

## 3. 设计原则

### 3.1 Southbound 应以 gRPC 流式协议为长期形态

原因：

- 需要能力协商
- 需要 full sync 和 incremental update
- 需要状态回报与事件上报
- 需要长连接与版本控制

### 3.2 Southbound 是“对象级配置协议”，不是“命令转发协议”

Controller 不应直接下发“执行某个本地功能命令”。它应下发对象和版本。

Agent 负责：

- 拉取或接收 desired state
- 编译对象
- 执行 reconcile
- 回报执行结果

### 3.3 Southbound 必须支持最终一致，而非强制同步阻塞

Controller 下发的是 desired state。

Agent 回报的是：

- 已应用版本
- 编译状态
- runtime 健康状态
- 局部失败信息

### 3.4 Southbound 必须显式建模能力探测

不同节点可能有不同：

- 内核版本
- helper 能力
- hook 能力
- 可用 offload 能力
- datapath profile

协议中必须有能力协商字段，而不是把差异隐含在日志里。

## 4. 通信角色

### 4.1 Controller

负责：

- 管理节点注册
- 下发对象级 desired state
- 跟踪 agent ack 和应用状态
- 聚合节点健康

### 4.2 Agent

负责：

- 上报注册信息与能力
- 接收 desired state
- 编译成本地执行态
- 回报编译和应用结果
- 流式上报事件与指标摘要

## 5. 核心交互流

### 5.1 Node Register

Agent 启动后向 Controller 注册。

注册内容建议包含：

- `node_id`
- `agent_version`
- `kernel_version`
- `hostname`
- `management_address`
- `capabilities`
- `labels`

### 5.2 Capability Report

Agent 必须显式上报 datapath 能力。

建议包含：

- supported hooks
- supported trace backend
- nat capability
- lb capability
- encap capability
- max map sizes
- observability profile

### 5.3 Desired State Sync

Controller 向 Agent 下发：

- 完整快照
- 或增量变更

建议包含：

- object version
- tenant-scoped objects
- node-scoped objects
- generation id

### 5.4 Apply Result

Agent 回报：

- apply success / failed
- compiled generation
- failed objects
- degraded mode
- warnings

### 5.5 Heartbeat / Node Status

Agent 周期性回报：

- liveness
- local reconcile status
- datapath health
- attach health
- event backlog

### 5.6 Event Stream

Agent 向 Observability Platform 或 Relay 流式发送统一事件。

如果第一阶段尚未拆分 Observe 子平台，可临时由 Controller 旁路承接，但接口不应绑死在 Controller 内部。

## 6. 协议对象

### 6.1 NodeInfo

建议字段：

- `node_id`
- `hostname`
- `agent_version`
- `kernel_version`
- `addresses`
- `labels`

### 6.2 NodeCapability

建议字段：

- `supported_hooks`
- `supports_xdp`
- `supports_tc`
- `supports_socket_lb`
- `supports_trace_ringbuf`
- `supports_nat`
- `supports_lb`
- `supports_encap`
- `supports_qos_shaping`
- `limits`

### 6.3 DesiredStateEnvelope

建议字段：

- `generation`
- `full_sync`
- `issued_at`
- `tenant_objects`
- `node_objects`
- `deletes`

### 6.4 ApplyStatus

建议字段：

- `generation`
- `status`
- `applied_at`
- `compiled_objects`
- `failed_objects`
- `warnings`
- `degraded_reasons`

### 6.5 NodeHealth

建议字段：

- `node_id`
- `agent_uptime`
- `datapath_ready`
- `attached_ports`
- `event_queue_depth`
- `wal_health`
- `last_reconcile_at`
- `last_error`

## 7. 同步模式

### 7.1 Full Sync

适用场景：

- agent 首次启动
- controller 恢复后重新对齐
- 版本升级后做强制收敛

### 7.2 Incremental Update

适用场景：

- 对象新增、更新、删除
- 路由动态变化
- 后端健康状态变化

### 7.3 Reconciliation

Agent 应支持：

- 按 generation 对齐
- 本地 drift 检查
- 局部重编译
- runtime 修复

## 8. 失败处理原则

### 8.1 Controller 不可用

Agent 应继续使用最近一次成功下发的 desired state。

### 8.2 Agent 局部编译失败

Agent 应回报失败对象和失败原因，而不是整体静默失败。

### 8.3 节点能力不足

Agent 应：

- 声明降级模式
- 拒绝不支持对象
- 回报能力不足原因

### 8.4 事件链路拥塞

事件流与配置流必须隔离，避免观测背压影响配置和转发。

## 9. 安全原则

southbound 协议必须支持：

- mTLS
- node identity
- certificate rotation
- least-privilege node authorization
- audit trail

Controller 必须能够识别：

- 节点是谁
- 节点能做什么
- 节点当前在运行什么 generation

## 10. 版本策略

协议版本建议拆分：

- transport version
- schema version
- object model version
- capability profile version

原则：

- Controller 应尽量兼容旧 Agent 的只读健康上报
- 破坏性 southbound 变更必须显式 bump 版本

## 11. 与当前代码的关系

当前仓库中的：

- `agent/src/api_routes.rs`
- `agent/src/control_plane.rs`
- `core/src/wal.rs`

更接近“node-local API + local control plane”。

未来 southbound 协议引入后：

- 节点本地 API 保留为调试入口
- 平台对象配置不再主要通过 node-local 功能菜单 API 下发
- Agent 转为 desired state 编译执行器

## 12. 第一阶段最小协议范围

建议第一阶段只覆盖：

- node registration
- capability report
- full sync desired state
- apply result
- heartbeat

事件流可以先以简单 streaming 原型存在，但不应阻塞 Phase 1 控制面骨架建设。

## 13. 验收标准

southbound 协议 v1 的验收标准：

- 能支持 Controller 下发基础对象
- 能支持 Agent 回报应用状态
- 能支持能力探测与降级说明
- 不依赖节点本地命令式 API 完成主控制面配置

## 14. 后续拆分建议

- `RFC-003A` protobuf schema
- `RFC-003B` certificate and identity
- `RFC-003C` event streaming channel

## 15. 当前实现状态（2026-04-09）

当前仓库已经新增第一阶段 southbound 骨架，但仍是过渡实现：

- 共享消息模型已经进入 `aria-api`
- `controller` 已提供临时 HTTP 形态的 southbound 路由
- 已覆盖 `register / desired-state / apply-status / heartbeat / status`
- desired-state 已按节点维度输出首批对象：`Tenant / Network / Port / SecurityGroup / RouteTable`
- `aria-agent` 已新增实验性的可选 southbound client；配置 `southbound_controller_url + southbound_node_id` 后，会执行 `register / desired-state / apply-status / heartbeat` 循环
- agent 当前会把 desired-state 缓存到 `${state_path}/platform-agent/desired-state-cache.json`，并把第一版节点局部 `compiled state` 缓存到 `${state_path}/platform-agent/compiled-node-state.json`
- 当前 agent 侧 southbound 仍是 `shadow compile only`：会把 `Tenant / Network / Port / SecurityGroup / RouteTable` 编译为节点局部视图并回报 compile/apply 结果，但尚未 materialize 到 datapath
- `apply-status` 已开始携带 `domain_statuses`，按 `identity / ports / security / routes / nat` 汇总各编译域的 shadow 结果；其中 `nat` 域当前仅作为 `SNAT / DNAT / Floating IP` 的 shadow reserved 占位
- `status` 响应中的 `last_seen_at` 仅在节点产生过 southbound 观察记录后才返回，避免伪造时间戳
- controller 已开始记录 per-node desired-state publish 摘要，包含 `generation / issued_at / full_sync / object_counts`，并在同 generation 重复拉取时复用已有发布时间
- `status` 响应已开始派生 `sync_status`，综合 `desired_generation / last_applied_generation / apply-status / health` 反映节点是否追平、失败或降级
- `status` 响应已开始补充 `pending_object_counts`，按对象类型给出当前 generation 仍待 reconcile 的数量，作为后续 incremental update 前的轻量差异摘要；该摘要仅在 publish generation 命中当前 desired generation 时生效，并在 apply 未成功时保守返回 full pending 视图
- `status` 响应已开始补充 `changed_kinds / has_deletes`，用轻量摘要表达当前仍待 reconcile 的资源种类，以及当前 generation 是否包含删除；旧 generation 的 publish 摘要不会再冒充当前状态
- northbound `Node.status` 已开始镜像 southbound 关键运行态，直接暴露 `desired_generation / last_applied_generation / last_seen_at / last_publish_summary / pending_object_counts / changed_kinds / has_deletes / sync_status`
- file-backed controller 已把 `registration / apply-status / heartbeat / health` 视为内存态运行状态，不再在每次心跳时重写整份 controller 快照

当前仍未实现：

- gRPC 流式传输
- mTLS 与 node identity
- full sync / incremental update 双形态协议
- southbound 版本协商
- 事件流与配置流隔离

因此当前实现只能视为本 RFC 的语义骨架和消息基线，不代表最终 transport 已冻结。
