# RFC-011：Datapath Capability Matrix v1

状态：Draft  
阶段：Phase 0 / Phase 2 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

实现约束参考：[Aria eBPF 实现约束](../ebpf-implementation-constraints.md)

## 1. 目标

定义 Aria 的 datapath 能力矩阵，用于统一描述：

- 节点支持哪些 hook
- 支持哪些功能面
- 支持哪些观测能力
- 在什么条件下需要降级

本 RFC 的目标是让能力探测、编译决策、调度约束和告警都引用同一份能力语义，而不是依赖散落的 if/else 和经验判断。

## 2. 非目标

本 RFC 不直接定义：

- 最终代码中的具体探测实现
- 所有内核 helper 的枚举清单
- 具体 CI 矩阵命令

## 3. 设计原则

### 3.1 能力必须结构化，而不是文本拼接

节点能力不应只体现在日志里，而应有结构化 profile。

### 3.2 能力必须直接参与编译与调度

能力矩阵不仅用于展示，还必须能驱动：

- Controller 调度与限制
- Agent 编译与降级
- 运维与告警

### 3.3 能力与版本分离

内核版本是重要输入，但不应直接等于功能能力。  
能力矩阵需要表达实际能力，而不是仅表达版本号。

## 4. 能力维度

建议把能力分为六类：

### 4.1 Platform Capability

- `kernel_version`
- `arch`
- `driver_class`
- `bpffs_available`
- `clsact_available`

### 4.2 Hook Capability

- `supports_xdp`
- `supports_tc_ingress`
- `supports_tc_egress`
- `supports_cgroup_skb`
- `supports_socket_hooks`
- `supports_uprobe`

### 4.3 Datapath Feature Capability

- `supports_stateful_firewall`
- `supports_conntrack`
- `supports_nat`
- `supports_fip`
- `supports_lb`
- `supports_overlay_encap`
- `supports_native_routing`
- `supports_qos_shaping`
- `supports_mirror`

### 4.4 Observe Capability

- `supports_trace`
- `supports_trace_ringbuf`
- `supports_trace_perf`
- `supports_drop_reason`
- `supports_tcprt`
- `supports_ssl_observe`
- `supports_http_observe`

### 4.5 Limits

- `max_program_size_profile`
- `max_map_entries`
- `max_ports`
- `max_routes`
- `max_services`
- `max_observe_qps`

### 4.6 Degradation Flags

- `requires_qos_policing_fallback`
- `requires_trace_backend_fallback`
- `requires_node_local_only_mode`
- `disallow_tail_calls`

## 5. Capability Profile

建议 southbound 和本地状态统一使用 `NodeCapabilityProfile`。

建议字段：

- `profile_version`
- `node_id`
- `detected_at`
- `platform`
- `hooks`
- `features`
- `observe`
- `limits`
- `degradations`

## 6. 能力等级

建议把能力按等级归类，便于调度和文档表达：

- `baseline`
- `recommended`
- `advanced`

### 6.1 baseline

满足基础节点运行，但可能有功能降级。

### 6.2 recommended

可稳定运行主流 datapath 与观测能力。

### 6.3 advanced

可开启更复杂或更高性能路径。

## 7. 编译使用方式

Agent 编译器必须把 capability profile 当作一级输入。

编译决策示例：

- 若 `supports_qos_shaping=false`，则 QoS 编译器降级到 policing
- 若 `supports_trace_ringbuf=false` 但 `supports_trace_perf=true`，trace backend 降级
- 若 `supports_overlay_encap=false`，Controller 不应把 overlay 网络下发到该节点

## 8. Controller 使用方式

Controller 至少应在以下场景使用能力矩阵：

- 节点准入
- 对象调度
- rollout 前置校验
- 变更风险判断

## 9. 建议的能力探测阶段

### 9.1 启动时探测

Agent 启动后进行一次完整探测。

### 9.2 周期性刷新

针对可能变化的运行时条件定期刷新：

- attach 能力
- qdisc 状态
- helper 可用性相关派生状态

### 9.3 版本升级后强制重探测

Agent 或内核升级后应强制刷新 profile。

## 10. 能力与降级矩阵

建议每个能力面都配一份标准降级动作：

| 能力不足 | 允许降级 | 不允许静默降级 |
| --- | --- | --- |
| QoS shaping | policing | bypass |
| trace ringbuf | perf trace | trace disabled without report |
| overlay encap | reject object | silent native reroute |
| SSL/HTTP observe | feature disabled with explicit status | fake success |
| LB advanced policy | simpler policy | unmanaged random behavior |

当前项目级默认约束还包括：

- `disallow_tail_calls=true`
- XDP/TC 主路径按低 stack / 低调用深度策略实现

具体规则见 [Aria eBPF 实现约束](../ebpf-implementation-constraints.md)。

## 11. 能力矩阵对运维的价值

能力矩阵还应支持：

- 文档化内核支持级别
- 控制面界面展示节点能力
- rollout 风险提示
- 故障诊断时快速判断“这是 bug 还是 capability gap”

## 12. 与当前代码的关系

当前仓库已经存在部分隐式能力逻辑，例如：

- trace backend 选择
- QoS shaping 回退
- 某些 runtime attach 恢复逻辑

但这些能力尚未被统一结构化表达。

## 13. 当前缺口

当前主要缺口：

- 没有统一 capability profile
- Controller 无法按能力做调度约束
- 编译器缺少正式的 capability 输入契约
- 缺少“功能不可用 vs 功能降级”的统一语义

## 14. 验收标准

Capability matrix v1 的验收标准：

- 节点能上报统一 capability profile
- 编译器能引用 capability profile 做确定性降级
- Controller 能基于能力拒绝不兼容对象下发
- 运维侧能明确区分 capability gap 与实现 bug

## 15. 后续拆分建议

- `RFC-011A` kernel support catalog
- `RFC-011B` runtime probe implementation
- `RFC-011C` capability-aware scheduler rules
