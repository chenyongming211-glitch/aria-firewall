# RFC-013：Controller Deployment Topology v1

状态：Draft  
阶段：Phase 1 / Phase 5 / Phase 8 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria Controller 的部署形态、职责切分和故障域边界，为以下问题提供统一答案：

- Controller 是单体还是拆分部署
- Controller、Relay、Observability 的边界怎么划
- 高可用时哪些组件必须活，哪些组件可降级
- 单机、试验、生产集群部署形态如何兼容

## 2. 非目标

本 RFC 不直接定义：

- 最终 Kubernetes Helm chart
- 最终数据库具体部署参数
- 具体云厂商部署模板

## 3. 设计原则

### 3.1 控制面必须脱离数据面热路径

Controller 不应进入节点侧转发热路径。  
Controller 故障时，已下发 datapath 应继续运行。

### 3.2 部署拓扑必须支持从单机到 HA 平滑演进

Aria 应支持至少三种形态：

- `standalone`
- `single-control-plane`
- `ha-control-plane`

### 3.3 Relay 和 Observability 需要逻辑独立

即使在小规模部署中与 Controller 同机，Relay / Observe 也应在逻辑上作为独立组件建模。

### 3.4 组件职责必须可独立扩缩容

以下组件负载特征不同，不应被永久耦合：

- northbound API
- southbound coordinator
- observe relay
- diagnose / query service
- background reconciler

## 4. 推荐组件划分

建议逻辑上至少划分以下组件：

### 4.1 API Server

负责：

- northbound REST/OpenAPI
- authn/authz 接入
- request validation
- async operation 入口

### 4.2 Reconciler / Orchestrator

负责：

- desired state 计算
- rollout 管理
- generation 推进
- southbound 下发编排

### 4.3 Node Coordinator

负责：

- southbound 连接管理
- node registration
- capability 汇总
- node health 聚合

### 4.4 Observe Relay

负责：

- 实时事件聚合
- observe 查询入口
- diagnose 的实时数据接入

### 4.5 Diagnose / Query Service

负责：

- Diagnose 会话
- 聚合查询
- 关联图和根因候选计算

### 4.6 Background Workers

负责：

- 清理任务
- compaction / retention
- 异步导入导出
- 配额校验后台修复

## 5. 部署模式

### 5.1 Standalone

适合：

- 开发环境
- POC
- 单节点实验

特点：

- API Server、Reconciler、Node Coordinator、Observe Relay 可同进程或同主机
- 仍需保留逻辑边界

### 5.2 Single Control Plane

适合：

- 小规模生产
- 单 AZ 平台

特点：

- 控制面单实例
- 外挂数据库与对象存储
- Relay 可同机但应独立进程

### 5.3 HA Control Plane

适合：

- 多租户生产
- 多节点 IaaS 平台

特点：

- 多副本 API Server
- 多副本 Reconciler（带 leader 选举）
- 多副本 Relay
- 外部共享存储

## 6. 控制面可用性要求

### 6.1 控制面故障

当 Controller 故障时：

- 已下发 datapath 应继续运行
- node-local observe 与 diagnose 可有限工作
- 新配置发布暂停

### 6.2 Relay 故障

当 Relay 故障时：

- 实时多节点 observe 受影响
- 历史存储和节点本地观测应尽量保留

### 6.3 存储故障

当主存储故障时：

- 新 northbound 写入受限
- node 仍可按最近 generation 运行
- 强制 rollback / emergency mode 应可用

## 7. 控制面内部通信

建议分三类通信：

- northbound HTTP/REST
- internal gRPC
- async queue / event bus

建议：

- API Server 到 Reconciler：内部 RPC
- Coordinator 到 Agent：southbound gRPC
- Relay 到 Query/Diagnose：stream/query RPC

## 8. 多租户与故障域

建议至少定义以下故障域：

- cluster / region
- control-plane shard
- tenant
- node

未来如果平台规模扩大，Controller 可按 tenant 或 region 做逻辑分片。

## 9. 与 RFC-003 的关系

`RFC-003` 定义 southbound 协议边界。  
本 RFC 进一步定义 southbound 协议在控制面内部由谁承接：

- API Server 不直接与 Agent 保持长期 southbound 会话
- Node Coordinator 承担 southbound 连接管理

## 10. 与 RFC-006 的关系

`RFC-006` 定义 Diagnose 与 Relay 语义。  
本 RFC 进一步定义：

- Observe Relay 是独立逻辑组件
- Diagnose / Query Service 不应与 northbound API 永久耦合在一个单体里

## 11. 与 RFC-014 的关系

控制器部署拓扑依赖持久化与存储模型，但不绑定到唯一存储产品。  
存储边界详见 `RFC-014`。

## 12. 当前代码映射与缺口

当前仓库中：

- `aria-agent` 仍兼具 node-local control plane 和 node-local API
- 尚无平台级 Controller / Relay / Query Service 拆分

当前缺口：

- 缺少控制器组件边界
- 缺少 HA 和 leader 选举模型
- 缺少 Relay 部署拓扑

## 13. 验收标准

Controller topology v1 的验收标准：

- 能说明从单机到 HA 的演进路线
- 能明确 API Server、Reconciler、Coordinator、Relay、Query Service 的职责
- 能定义控制面故障时的降级语义

## 14. 后续拆分建议

- `RFC-013A` control-plane HA and leader election
- `RFC-013B` shard and tenancy topology
- `RFC-013C` relay and query deployment model
