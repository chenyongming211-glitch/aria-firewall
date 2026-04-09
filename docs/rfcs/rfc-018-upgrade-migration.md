# RFC-018：Upgrade and Migration Model v1

状态：Draft  
阶段：Phase 8 / 持续演进  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 平台升级、兼容和迁移的统一模型，避免后续在 Controller、Agent、datapath、schema、observe pipeline 演进时出现不可控断裂。

## 2. 非目标

本 RFC 不直接定义：

- 每个版本的具体发布说明模板
- 具体 CI/CD 工具脚本
- UI 升级向导细节

## 3. 设计原则

### 3.1 升级必须按层次建模

至少区分以下层：

- Controller
- Agent
- Datapath binary
- Southbound schema
- Northbound schema
- Event schema
- Local recovery state

### 3.2 兼容窗口必须显式定义

不能依赖“理论上应该兼容”。每种接口和状态都应定义兼容窗口。

### 3.3 升级和迁移必须可回滚

任何重要升级都应至少具备：

- 版本检查
- 预迁移校验
- 有界 rollback

### 3.4 schema 演进必须版本化

尤其是：

- northbound API
- southbound 协议
- event schema
- local compiled state format

## 4. 升级对象

### 4.1 Controller Upgrade

涉及：

- API Server
- Reconciler
- Coordinator
- Relay / Query Service

### 4.2 Agent Upgrade

涉及：

- agent binary
- local compiler logic
- local state format

### 4.3 Datapath Upgrade

涉及：

- ebpf binary
- map layout
- attach behavior

### 4.4 Schema Upgrade

涉及：

- northbound API version
- southbound schema version
- event schema version

## 5. 兼容策略

### 5.1 Controller / Agent

建议：

- Controller 尽量兼容一个有限版本窗口内的旧 Agent
- Agent 在升级期间应尽量支持旧 generation 的运行

### 5.2 Agent / Datapath

建议：

- Agent 与 datapath binary 应视为紧耦合版本对
- 若版本不匹配，应显式告警，不得静默运行未知组合

### 5.3 Event Schema

建议：

- 新增字段向后兼容
- 语义破坏需 bump schema version

## 6. 升级模式

### 6.1 In-place Upgrade

适合：

- 单节点实验环境
- 小规模平台

### 6.2 Rolling Upgrade

适合：

- 生产平台
- 多节点 / 多副本控制面

### 6.3 Blue/Green or Shadow Upgrade

适合：

- datapath compiler 大改
- observe pipeline 大改
- 高风险 schema 变更

## 7. 升级顺序原则

建议优先顺序：

1. 控制面兼容升级
2. Agent 升级
3. Datapath 升级
4. 高级能力开关启用

避免直接同时升级所有层。

## 8. 数据迁移

### 8.1 控制面对象迁移

必须支持：

- schema migration
- 数据校验
- 失败回滚

### 8.2 Agent 本地状态迁移

必须考虑：

- local WAL
- compiled state snapshot
- attach inventory

### 8.3 事件 schema 迁移

建议：

- 保留 schema version
- 历史数据不强制就地重写
- 查询层兼容旧版本

## 9. Southbound 迁移

升级 southbound 时，必须明确：

- Controller 最低兼容 Agent 版本
- Agent 最低兼容 Controller 版本
- 不兼容时的拒绝行为

## 10. Rollout 与 Upgrade 的关系

升级过程应使用 `RFC-012` rollout 生命周期管理：

- validate
- audit
- shadow
- canary
- enforce
- rollback

## 11. 观测与升级

升级期间应重点观测：

- node degraded reasons
- compile failures
- attach failures
- event backlog
- datapath health

## 12. 与现有代码的关系

当前仓库已具备一些升级相关基础能力：

- local WAL / snapshot
- runtime reconcile
- binary pairing 约束的经验

但尚未形成正式的平台升级模型。

## 13. 当前缺口

当前缺口：

- 没有正式的版本兼容矩阵
- 没有 controller/agent/datapath 分层升级模型
- 没有 local state migration 规范
- 没有基于 rollout 的统一升级流程

## 14. 验收标准

Upgrade and migration model v1 的验收标准：

- 能描述 Controller、Agent、Datapath、Schema 的升级关系
- 能定义兼容窗口与拒绝策略
- 能与 rollout / audit / shadow 模型联动
- 能指导未来真实版本升级设计

## 15. 后续拆分建议

- `RFC-018A` compatibility matrix
- `RFC-018B` local state migration
- `RFC-018C` control-plane rolling upgrade
