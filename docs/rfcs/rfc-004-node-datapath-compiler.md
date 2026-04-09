# RFC-004：Node Datapath 编译模型 v1

状态：Draft  
阶段：Phase 0 / Phase 2 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

实现约束参考：[Aria eBPF 实现约束](../ebpf-implementation-constraints.md)

## 1. 目标

定义 Aria Agent 在节点侧的 datapath 编译模型，使其从“功能菜单执行器”演进为“平台对象编译器 + 执行器”。

本 RFC 的目标是回答三个问题：

- Controller 下发的平台对象，如何在节点侧被解释和编译
- 编译结果如何映射为可执行 datapath 状态
- 编译、应用、恢复和增量更新如何保持一致性

## 2. 非目标

本 RFC 不直接定义：

- 具体 eBPF map 的最终 C/Rust 结构
- 具体程序 attach 代码
- Controller 的 northbound API
- 事件 schema 细节

## 3. 设计原则

### 3.1 编译必须是确定性的

对于相同的 desired state、相同的节点能力、相同的编译器版本，Agent 必须生成相同的 compiled state。

### 3.2 编译必须是幂等的

重复 full sync 或重复 apply 同一 generation，不应造成附着抖动、对象漂移或状态重复。

### 3.3 编译与应用必须分离

编译阶段生成可审计、可持久化的 compiled state。  
应用阶段负责把 compiled state 写入 runtime，并执行 reconcile。

### 3.4 编译必须能力感知

编译必须显式依赖：

- kernel version
- hook capability
- helper capability
- runtime limits

能力不足时，不应隐式失败，而应进入显式 degraded mode。

### 3.5 编译结果必须可回溯

任何 runtime state 都应该能够追溯到：

- 来源 generation
- 来源对象 ID
- 编译版本
- 能力 profile

### 3.6 编译输出必须满足 eBPF 实现约束

编译器生成的 datapath 结构与调用方式，必须满足：

- stack 预算
- 调用深度预算
- helper 合法性
- 循环上界约束

具体门槛见 [Aria eBPF 实现约束](../ebpf-implementation-constraints.md)。

### 3.7 外部实现只做选择性借鉴，不重写已完成骨架

Node datapath 编译模型允许借鉴成熟项目，但借鉴边界必须清晰：

- 当前已落地的 `controller / northbound / southbound / desired_state_cache / compiled_state / runtime_plan / runtime_inventory / runtime_intent` 骨架，默认继续沿 Aria 自身模型演进，不因对标外部项目而整体返工
- `Service Domain` 后续优先借鉴 Cilium 在 `frontend / backend / revnat / affinity / maglev / socket lb / packet lb` 上的职责拆分
- `Route Domain / 多节点 reachability / service IP advertisement / host endpoint policy` 等后续能力，可在对应 RFC 中优先借鉴 Calico 的边界划分
- 借鉴的重点是 datapath 状态拆分、执行路径和运维边界，不是复制外部项目的 Kubernetes API、CRD 结构或控制器拓扑
- 若参考实现与当前已冻结的编译契约冲突，应先更新 RFC，再调整代码

## 4. 输入与输出

### 4.1 编译输入

Agent 编译器的输入包括：

- southbound 下发的 desired state
- 节点能力报告
- 本地 runtime inventory
- 本地恢复状态与 snapshot

### 4.2 编译输出

编译器应输出：

- `CompiledNodeState`
- `AttachPlan`
- `MapPlan`
- `ReconcilePlan`
- `CompileReport`

## 5. 编译层次

建议采用四层编译模型：

### 5.1 Object Graph

直接来自 Controller 的对象视图：

- Port
- SecurityGroup
- RouteTable
- FloatingIP
- Service
- Chain

该层仍然保留平台对象语义。

### 5.2 Normalized Node Graph

按节点切分、按作用域展开、消除跨节点不可执行语义后的节点局部视图。

该层示例：

- 节点上的有效 Port 集合
- 绑定到 Port 的 SecurityRule 展开结果
- 与本节点相关的 Route 和 NextHop
- 本节点需承载的 FIP / NAT / Service

### 5.3 Datapath IR

这是编译器的核心中间表示。

建议拆为几个 IR 面：

- `ClassifierIR`
- `ActionIR`
- `RouteIR`
- `NatIR`
- `ServiceIR`
- `ChainIR`
- `ObserveIR`

### 5.4 Runtime Materialization

将 IR 具体投影为：

- BPF maps
- attach points
- tc / xdp runtime config
- userspace assist state
- local metadata index

## 6. 编译阶段

### 6.1 Validation

编译前先做对象级校验：

- 引用完整性
- 作用域合法性
- 端口和实例绑定合法性
- 网络地址规划冲突
- NAT / FIP 冲突
- Service / backend 合法性

### 6.2 Normalization

将平台对象压平为节点局部对象：

- 解析 Port 到 Attachment
- 展开 SecurityGroup 到 Port 视图
- 解析 Route 的 next hop
- 展开 AddressSet
- 计算本节点有效 VIP / backend

### 6.3 ID Allocation

为 datapath 运行态分配稳定 ID：

- port local id
- address set id
- policy id
- route id
- nat id
- service id
- backend id
- chain id

要求：

- 在对象不变时尽量保持稳定
- 允许跨 generation 重用

### 6.4 Feature Lowering

将对象语义下沉为功能面 IR：

- Port / Attachment -> port binding IR
- SecurityGroup -> classifier + security action IR
- RouteTable -> route IR
- FloatingIP / NatGateway -> nat IR
- Service / BackendSet -> service IR
- Chain -> chain IR
- Observe config -> observe IR

### 6.5 Runtime Planning

根据 IR 生成 runtime 计划：

- 哪些 map 需要重写
- 哪些 map 可以局部更新
- 哪些 attach 要重做
- 哪些 state 要保留
- 哪些 counters 要迁移

### 6.6 Apply / Reconcile

应用 compiled state 后，执行 runtime reconcile：

- map state check
- attach state check
- fq / qdisc / helper readiness check
- 失败项修复

## 7. CompiledNodeState 结构建议

CompiledNodeState 建议至少包括：

- `generation`
- `compiler_version`
- `node_id`
- `capability_profile`
- `ports`
- `attachments`
- `address_sets`
- `security_program`
- `route_program`
- `nat_program`
- `service_program`
- `chain_program`
- `observe_program`
- `runtime_hints`

## 8. AttachPlan

AttachPlan 用于定义本代 runtime 需要建立的 hook 和 attach 状态。

建议字段：

- `xdp_attaches`
- `tc_ingress_attaches`
- `tc_egress_attaches`
- `cgroup_attaches`
- `userspace_helpers`
- `required_qdisc`

## 9. MapPlan

MapPlan 描述 runtime map 更新计划。

建议把 map 分为以下类别：

- `port maps`
- `security maps`
- `route maps`
- `next-hop maps`
- `nat maps`
- `service maps`
- `chain maps`
- `observe maps`
- `stats maps`

每类 map 应支持：

- full replace
- incremental patch
- no-op reuse

## 10. 编译域划分

为避免未来 datapath 过度耦合，建议按编译域切分：

### 10.1 Port Domain

负责：

- Port
- Attachment
- anti-spoof
- ingress / egress base identity

### 10.2 Security Domain

负责：

- AddressSet
- SecurityGroup
- SecurityRule
- 基础 ACL / stateful firewall

### 10.3 Route Domain

负责：

- RouteTable
- Route
- NextHop
- local forwarding decision

### 10.4 Nat Domain

负责：

- FloatingIP
- NatGateway
- conntrack-coupled rewrite

### 10.5 Service Domain

负责：

- Service
- BackendSet
- Backend
- HealthCheck projection

建议进一步拆为：

- `ServiceFrontendMap`
- `BackendMemberMap`
- `ServiceRevNatMap`
- `ServiceAffinityMap`
- `ServiceMaglevMap`
- `ServiceForwardingProjection`

并在执行路径上显式区分：

- `socket lb path`
- `packet lb path`

### 10.6 Chain Domain

负责：

- Chain
- 条件引流
- hop 编排投影

### 10.7 Observe Domain

负责：

- trace
- drop
- route/nat/lb decision events
- tcprt / ssl / http 投影入口

## 11. 增量更新模型

### 11.1 Full Compile

用于：

- agent 首次启动
- generation gap 过大
- 编译器版本变化
- 能力 profile 变化

### 11.2 Incremental Compile

用于：

- 单对象更新
- backend 变化
- route 更新
- address set 更新

增量编译要求：

- 仅重建受影响域
- 尽量保持无关 map 稳定
- 尽量避免 attach 抖动

## 12. 失败与降级

### 12.1 编译失败

编译失败必须按对象、按域回报，不能仅返回单个总错误。

### 12.2 局部降级

示例：

- 不支持 shaping 时降级到 policing
- 不支持某 trace backend 时切换次优 backend
- 不支持某 hook 时切到备用 hook

### 12.3 不允许的静默降级

以下情况不得静默：

- route 不可编译
- nat 语义丢失
- 安全策略从 enforce 降成 bypass

## 13. Durability 与恢复

Compiled state 应持久化到本地，至少支持：

- generation snapshot
- compile metadata
- attach inventory
- degraded reasons

Agent 重启时，恢复流程建议为：

1. 读取最近 compiled state
2. 恢复 runtime attach inventory
3. 对现有 runtime 做 reconcile
4. 等待 Controller 下发更新 generation

## 14. 与当前代码的映射

当前代码可作为未来编译器种子模块的部分映射：

- `agent/src/control_plane.rs` -> compile/apply 协调器雏形
- `core/src/wal.rs` -> compiled state durability 基础
- `core/src/state.rs` -> 当前局部功能状态机，未来需提升为对象级编译输出
- `core/src/ebpf_ops/*` -> feature-specific materialization 层

## 15. 当前实现状态（2026-04-09）

当前仓库已经开始落第一阶段 agent 编译骨架：

- `aria-agent` 已新增实验性的可选 southbound client
- 当配置 `southbound_controller_url + southbound_node_id` 后，agent 会执行 `register / desired-state / apply-status / heartbeat` 循环
- agent 当前会把 desired-state 缓存到 `${state_path}/platform-agent/desired-state-cache.json`
- agent 当前会把第一版 `CompiledNodeState` 缓存到 `${state_path}/platform-agent/compiled-node-state.json`
- agent 当前会把第一版 `ReconcilePlan` 缓存到 `${state_path}/platform-agent/reconcile-plan.json`
- agent 当前会把第一版 `RuntimePlan` 缓存到 `${state_path}/platform-agent/runtime-plan.json`
- agent 当前会把第一版 `RuntimeInventory` 缓存到 `${state_path}/platform-agent/runtime-inventory.json`
- agent 当前会把第一版 `RuntimeInventoryDiff` 缓存到 `${state_path}/platform-agent/runtime-inventory-diff.json`
- agent 当前会把第一版 `RuntimeIntent` 缓存到 `${state_path}/platform-agent/runtime-intent.json`
- agent 当前会把第一版 `RuntimeExecutionSummary` 缓存到 `${state_path}/platform-agent/runtime-execution-summary.json`
- 第一版编译器已开始把 `Tenant / Network / Port / SecurityGroup / RouteTable / HealthCheck / BackendSet / Service` 下沉为节点局部视图
- `CompiledNodeState` 已开始输出 `identity / ports / security / routes / services / nat` 六个 domain summary，作为后续按编译域分治的第一阶段骨架；其中 `routes` 域对应 routing 预留位，`services` 域对应 `Service / BackendSet / HealthCheck` 的 shadow 编译域，`nat` 域当前仅保留 `SNAT / DNAT / Floating IP` 的 shadow reserved 接口
- `CompiledNodeState` 现已开始在 `services` 域内额外保留第一版 `ServiceIR / BackendSetIR / HealthCheckIR` shadow skeleton，用于表达节点内转发、跨节点转发、frontend listener 和 backend member 的局部视图，但仍不会直接 materialize 到 datapath
- `RuntimePlan / RuntimeInventory` 现已开始把 `services` 域细化成 `service_frontend_catalog / backend_member_catalog / service_forwarding_projection / service_revnat_map / service_affinity_map / service_maglev_map` 等 shadow map family，为后续 L4 datapath 的局部更新和 runtime diff 提供更稳定的规划边界
- `RuntimeIntent / RuntimeExecutionSummary` 现已开始为 `services` 域单独保留 listener / backend member / forwarding projection 的细化摘要，并显式区分 `node_local / cross_node_native / cross_node_overlay / cross_node_hybrid` 等方向，作为后续 runtime apply 和 rollout 观察面的前置骨架
- `RuntimeInventory` 已开始按 `identity / ports / security / routes / services / nat` 汇总 shadow attach/map/domain inventory，作为后续本地 runtime reconcile 的恢复基线
- `RuntimeInventoryDiff` 已开始按编译域汇总 attach/map/domain delta，作为后续 runtime inventory reconcile 与增量 materialization 的轻量差异骨架
- `RuntimeIntent` 已开始把 `reconcile plan + runtime inventory + runtime inventory diff` 收敛成按域的 shadow runtime intent，作为后续 datapath apply/reconcile 的本地执行入口骨架
- `RuntimeExecutionSummary` 已开始把 `compiled state + reconcile plan + runtime intent` 收敛成按域的 shadow execute 摘要，作为后续 runtime apply 结果面和 rollout 观察面的前置骨架
- `ApplyStatusReport` 已开始携带 `domain_statuses`，把 domain-level shadow execute 结果回报给 controller
- 编译输出当前仍是 `shadow compile only`：会产出 `compiled state + reconcile plan + runtime plan + runtime inventory + runtime inventory diff + runtime intent + runtime execution summary + apply report`，不会直接 materialize 到 datapath
- 当前 `apply-status` 主要表达对象校验、降级原因和 shadow compile 结果，尚不代表 datapath 已成功写入

## 16. 当前缺口

当前主要缺口：

- 缺少平台对象到 runtime 的正式 IR
- 缺少 generation 化的 compiled state
- 缺少按编译域分治的结构化 IR 与可执行 runtime inventory / runtime intent projection
- 缺少对象级失败和降级报告

## 17. 验收标准

Node datapath 编译模型 v1 的验收标准：

- 能从平台对象生成节点局部 compiled state
- 能支持 full compile 和 incremental compile
- 能支持 runtime reconcile
- 能支持能力感知与显式 degraded mode
- 能为后续 Routing / NAT / Service 编译提供稳定骨架
