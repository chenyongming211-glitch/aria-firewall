# RFC-014：Persistence and Storage Model v1

状态：Draft  
阶段：Phase 0 / Phase 1 / Phase 5 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 平台各类状态的持久化边界和存储模型，统一回答以下问题：

- 什么是源事实
- 什么必须长期保存
- 什么是缓存、可重建状态
- 哪些数据适合事务型存储，哪些适合时序或日志型存储

## 2. 非目标

本 RFC 不直接绑定：

- 具体数据库品牌
- 具体对象存储厂商
- 具体 ClickHouse / Loki / Prometheus 部署参数

## 3. 设计原则

### 3.1 先定义状态类型，再选存储

不要先定“用什么数据库”，再反过来强迫状态模型适配数据库。

### 3.2 源事实必须唯一

每类状态都必须明确唯一 source of truth，避免：

- 控制面数据库一份
- Agent 本地状态一份
- Observe 存储里又一份

三份互相漂移。

### 3.3 控制面状态与观测数据必须分离

对象状态和观测数据的访问模式、保留策略、规模完全不同，不能混为一类存储。

### 3.4 Agent 本地持久化是恢复层，不是平台主数据库

Agent 本地 WAL / snapshot 用于：

- 重启恢复
- 短期一致性
- 运行态补偿

不是平台长期主数据源。

## 4. 状态分类

建议至少分为五类：

### 4.1 Platform Source of Truth

包括：

- Tenant
- Network
- Segment
- Port
- SecurityGroup
- RouteTable
- FloatingIP
- Service
- Chain

特点：

- 事务型
- 低变更频率
- 强一致要求较高

### 4.2 Desired State Distribution

包括：

- per-node desired generation
- rollout state
- publish records

特点：

- 可重建
- 需要版本化
- 与 southbound 同步强相关

### 4.3 Node Local Recovery State

包括：

- compiled state snapshot
- local WAL
- attach inventory
- reconcile metadata

特点：

- 节点本地
- 用于快速恢复
- 不是跨节点主数据

### 4.4 Observability Hot Data

包括：

- 实时事件
- 流日志
- 指标聚合中间态
- Diagnose 会话暂存

特点：

- 高写入
- 高吞吐
- 时序 / 日志型

### 4.5 Observability Long-term Data

包括：

- 历史事件归档
- 指标长留存
- 审计归档
- 报表材料

## 5. 推荐存储类型

### 5.1 控制面对象存储

建议使用事务型数据库。

适合承载：

- 平台对象
- 关系引用
- RBAC / 配额
- rollout 元数据

### 5.2 Agent 本地存储

建议使用文件系统 + WAL + snapshot 组合。

适合承载：

- compiled state
- attach inventory
- local replay metadata

### 5.3 事件与流日志存储

建议使用日志 / 列式 / 时序类存储。

适合承载：

- flow events
- drop events
- route/nat/lb events
- diagnose evidence records

### 5.4 指标存储

建议使用时序数据库。

适合承载：

- Prometheus 指标
- 预聚合时延指标
- 节点健康指标

### 5.5 审计存储

建议：

- 逻辑上独立于普通 debug log
- 支持结构化检索和长期保留

## 6. 源事实边界

### 6.1 平台对象

唯一 source of truth 在控制面主数据库。

### 6.2 southbound generation

源事实在控制面发布记录。  
Agent 本地仅缓存最近已应用 generation。

### 6.3 node runtime inventory

源事实在节点本地。  
控制面只接收其汇总状态，不反向代替本地事实。

### 6.4 observability event

源事实为事件采集链路中的 append-only event stream。

## 7. 数据保留策略

### 7.1 控制面对象

长期保留，支持软删除和审计追溯。

### 7.2 rollout 记录

建议中长期保留，用于事故回溯和审计。

### 7.3 观测热数据

短期高性能保留，支持快速查询。

### 7.4 观测冷数据

长期归档，成本优先。

## 8. 一致性要求

### 8.1 控制面对象

要求较高一致性。

### 8.2 Agent 本地 compiled state

要求节点内一致性与 crash recovery 能力。

### 8.3 观测数据

允许最终一致和局部延迟，不应反过来阻塞数据面。

## 9. 与现有代码的映射

当前仓库已有：

- `core/src/wal.rs`
- 本地 state snapshot
- pinned maps

这是未来 Node Local Recovery State 的种子能力。

当前缺口：

- 没有平台级 source-of-truth 数据模型
- 没有 rollout/publish 持久化模型
- 没有 observability 热/冷分层模型
- 没有审计独立保留模型

## 10. 数据迁移与恢复原则

建议长期支持：

- 控制面对象版本迁移
- southbound generation 可重算
- Agent 本地 compiled state 版本迁移
- event schema 版本化兼容

## 11. 对 Controller 拓扑的要求

持久化模型必须支持：

- 单实例控制面
- HA 控制面
- Relay 与 Query Service 分离部署

## 12. 对 UI / Workflow 的要求

UI 查询的背后应明确读路径：

- 对象详情 -> 控制面数据库
- 实时 Observe -> Relay / 热数据
- 历史分析 -> 归档或历史查询引擎
- 审计页 -> 审计存储

## 13. 验收标准

Persistence model v1 的验收标准：

- 明确各类状态的 source of truth
- 明确控制面、节点本地、观测和审计的存储边界
- 可支持从单机到 HA 部署拓扑

## 14. 后续拆分建议

- `RFC-014A` platform metadata store
- `RFC-014B` observability hot/cold pipeline
- `RFC-014C` agent local recovery store

## 15. 当前实现状态（2026-04-09）

当前仓库已经落下持久化模型的第一步代码骨架：

- `controller` 已通过 store trait 隔离存储边界
- 已新增可选的文件快照 backend，允许通过 `ARIA_CONTROLLER_STATE_PATH` 持久化平台对象、generation 和 per-node publish 摘要
- southbound runtime（`registration / apply-status / heartbeat / health`）现已明确作为内存态运行状态处理，不再跟随每次心跳重写整份 controller 快照
- southbound 已开始持久化 per-node desired-state publish 摘要，作为 `per-node desired generation / publish records` 的第一阶段雏形
- `aria-agent` 已新增第一版本地 platform-agent 状态目录 `${state_path}/platform-agent/`
- agent 当前会持久化 `desired-state-cache.json`、`compiled-node-state.json`、`reconcile-plan.json`、`runtime-plan.json` 与 `runtime-inventory.json`，作为 `desired state / compiled state / reconcile plan / runtime plan / runtime inventory` 的本地恢复骨架
- 默认实现仍为内存态 backend
- northbound / southbound handler 已只依赖抽象边界，不再直接依赖具体内存实现
- northbound 对象写入当前以“store 内部 mutation + generation bump”为 durability 边界，再写入文件快照
- northbound 的一阶引用完整性校验和依赖删除保护已下沉到 store mutation 边界内，避免 handler 层校验与写入之间的 TOCTOU 窗口
- 当前 file-backed backend 仍是 Phase 0 形态：写路径经 `mutation_lock` 串行，但读路径尚未绑定同一快照边界；文件写失败回滚时，并发读者可能短暂观察到未持久化中间态
- `node` 资源方法在 store 内部保留手工展开实现，因为 `delete_node` 需要额外清理 southbound runtime，不适合完全复用通用 CRUD 宏

当前仍未实现：

- 事务型平台对象存储
- rollout / publish 持久化记录
- observability 热/冷分层存储
- 审计独立保留与查询

因此当前状态只能视为持久化演进的接口前置，而不是 `RFC-014` 的完整实现。
