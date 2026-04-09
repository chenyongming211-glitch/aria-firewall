# RFC-012：Rollout / Audit / Shadow Mode v1

状态：Draft  
阶段：Phase 0 / Phase 8 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 平台中的安全发布与验证机制，用于降低高风险网络变更带来的爆炸半径。

本 RFC 覆盖：

- rollout 模型
- audit mode
- shadow mode
- 回滚原则
- 风险分级

## 2. 非目标

本 RFC 不直接定义：

- 所有 UI 流程
- 具体流量复制实现细节
- 完整审批系统

## 3. 设计原则

### 3.1 网络变更默认视为高风险变更

以下对象的改动应默认视为高风险：

- SecurityRule
- RouteTable
- FloatingIP
- NatGateway
- Service
- Chain

### 3.2 发布必须有明确阶段

不应只存在“直接 apply”一种模式。至少应支持：

- validate
- audit
- shadow
- canary
- enforce
- rollback

### 3.3 审计和 shadow 必须有结构化证据

不能只是日志打印“看起来正常”。必须能输出：

- 影响资源
- 命中情况
- 潜在 drop
- 潜在 route 变化
- 潜在 backend 变化

### 3.4 变更必须与 generation 绑定

所有 rollout 都必须围绕 generation、scope 和 status 展开。

## 4. Rollout 生命周期

建议定义以下状态：

- `draft`
- `validated`
- `audit`
- `shadow`
- `canary`
- `enforced`
- `rolled_back`
- `failed`

## 5. Validate

Validate 阶段至少应检查：

- 对象引用完整性
- 与 capability matrix 的兼容性
- 与现有对象是否冲突
- 配额是否超限
- 作用域是否合法

Validate 失败时不得进入后续阶段。

## 6. Audit Mode

### 6.1 定义

Audit mode 表示：

- 平台计算目标策略或目标路由语义
- 记录“本应产生的结果”
- 不真正执行高风险阻断或改写

### 6.2 适用场景

- 新安全策略上线前
- 新 route policy 上线前
- 新 Service 入口前

### 6.3 输出要求

Audit mode 应输出：

- would_allow / would_drop
- would_route_to
- would_select_backend
- would_apply_nat

## 7. Shadow Mode

### 7.1 定义

Shadow mode 表示：

- 新 datapath 语义在旁路或并行路径上运行
- 不对真实生产流量 verdict 产生主影响
- 产出差异对比结果

### 7.2 与 Audit 的区别

Audit 更偏“规则级预判”；  
Shadow 更偏“运行级并行验证”。

### 7.3 适用场景

- 新 datapath 编译器版本
- 新 routing / NAT 实现
- 新 LB 策略

## 8. Canary

### 8.1 定义

Canary 表示：

- 在有限 scope 内真正 enforce
- 控制 blast radius

### 8.2 Canary 维度

建议支持：

- 按 node
- 按 tenant
- 按 network
- 按 resource

## 9. Enforce

Enforce 表示正式生效。

正式生效前建议至少满足：

- validate 通过
- audit 或 shadow 结果可接受
- canary 结果健康

## 10. Rollback

### 10.1 原则

Rollback 必须：

- 快于重新诊断整个事故
- 基于 generation 回滚
- 尽可能原子

### 10.2 回滚来源

建议支持：

- 上一 generation
- 指定稳定 generation
- 手工 emergency disable

## 11. 风险分级

建议变更按风险分级：

- `low`
- `medium`
- `high`
- `critical`

示例：

- 单租户新增只读观察配置：`low`
- 新 SecurityRule：`high`
- 核心 Service backend 切换：`high`
- RouteTable 默认路由变更：`critical`

## 12. 与 capability matrix 的关系

Rollout 必须与 `RFC-011` 能力矩阵联动。

如果目标节点能力不足，应在 validate 阶段拒绝或自动限制 rollout scope。

## 13. 与事件模型的关系

Audit、Shadow、Canary 都应产生事件证据。

建议至少有：

- `rollout_event`
- `audit_result_event`
- `shadow_diff_event`

并可与 `flow_event`、`drop_event`、`route_event`、`lb_event` 关联。

## 14. 与 Diagnose 的关系

Diagnose 应能够识别：

- 是否刚发生过 rollout
- 当前异常是否与 rollout generation 相关
- audit / shadow 是否已提前暴露同类问题

## 15. 当前代码映射与缺口

当前仓库已有一些“局部降级”思路和恢复机制，但还没有正式的平台 rollout 模型。

当前缺口：

- 缺少 generation 级 rollout 状态机
- 缺少 audit / shadow 明确定义
- 缺少高风险网络变更的 canary 和 rollback 策略

## 16. 分阶段落地建议

### 16.1 第一阶段

先在控制面建立 rollout 对象和状态机。

### 16.2 第二阶段

先支持安全策略与路由变更的 audit mode。

### 16.3 第三阶段

再支持 shadow mode，优先用于 datapath 版本和 routing / NAT 路径验证。

### 16.4 第四阶段

最后补 canary 与自动 rollback。

## 17. 验收标准

Rollout / audit / shadow v1 的验收标准：

- 关键资源变更可通过统一 rollout 生命周期发布
- audit mode 能输出结构化“本应结果”
- shadow mode 能输出差异证据
- canary 和 rollback 有明确对象模型和状态机

## 18. 后续拆分建议

- `RFC-012A` rollout resource and state machine
- `RFC-012B` audit result schema
- `RFC-012C` shadow diff model
