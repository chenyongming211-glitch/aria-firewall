# RFC-017：Multi-cluster / Region Model v1

状态：Draft  
阶段：Post-Phase 8 扩展  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 从单控制域扩展到多 cluster / 多 region 的模型边界。

本 RFC 的目标是先冻结扩展方向，避免在单区域架构尚未稳定前就把核心模型耦合成“天然跨地域复杂系统”。

## 2. 非目标

本 RFC 不直接定义：

- 全球流量调度算法
- 最终跨区域数据库复制技术
- 完整灾备 SOP

## 3. 设计原则

### 3.1 多 cluster / region 是扩展层，不反向污染单区域核心模型

单区域控制面、southbound、资源模型必须先独立成立。

### 3.2 全局对象和区域对象必须分层

建议至少区分：

- global object
- region object
- cluster object
- node object

### 3.3 观测与配置扩展路径可以不同

跨区域 observe 与跨区域配置并不一定需要完全相同的拓扑和时延假设。

### 3.4 灾难恢复与多活是不同问题

本 RFC 应明确区分：

- failover / DR
- multi-active control plane

## 4. 基本层级

建议层级：

- `Global`
- `Region`
- `Cluster`
- `Node`

### 4.1 Global

适合承载：

- 平台级 identity / policy catalog
- 全局租户目录
- 跨 region service catalog

### 4.2 Region

适合承载：

- region 内控制面
- region 内 observe 聚合
- region 内 southbound 管理

### 4.3 Cluster

适合承载：

- 一个控制域内的一组 node
- 共享 southbound / rollout / observe relay

## 5. 对象作用域扩展

部分对象在多 region 下需扩展作用域：

- Tenant：可全局
- Network：通常 region 内
- Segment：cluster / region 内
- Service：可 region 内，也可全局抽象
- Chain：通常 region 内

## 6. 配置拓扑

建议：

- Region 内有独立 Controller
- Global 层负责目录、策略模板、跨区域协调
- 不建议一开始用单个全球控制面直接驱动所有 node

## 7. Observe 拓扑

建议：

- 每个 region 先有本地 Relay / Observe
- 再由 global observe aggregator 做跨区域检索

这样可以减少：

- 跨地域高延迟
- 全局单点压力

## 8. Service 扩展

多 region 后，Service 可能扩展为：

- region-local service
- global service catalog
- failover service

但本 RFC 不要求第一阶段支持全球负载均衡，只要求模型可扩展。

## 9. Routing 扩展

跨 region 不应直接复用单节点 route 模型。  
应增加更高层 reachability 与域间路由语义。

## 10. 身份与权限扩展

多 region 下至少要考虑：

- 全局身份目录
- region 内授权执行
- region 隔离审计

## 11. 灾备模型

建议至少区分两类：

- cold / warm standby
- active-active read or observe

不要在第一版就强制所有控制面组件支持全球多活写入。

## 12. 与现有 RFC 的关系

本 RFC 建立在：

- `RFC-013` 控制器拓扑
- `RFC-014` 持久化模型
- `RFC-009` 多节点网络

之上，是其跨 region 扩展层。

## 13. 当前缺口

当前缺口：

- 还没有 region / cluster 作用域定义
- 还没有 global 与 regional observe 分层
- 还没有跨区域服务和灾备模型

## 14. 验收标准

Multi-cluster / region model v1 的验收标准：

- 不破坏单区域主架构
- 能明确 global / region / cluster / node 的边界
- 能为未来多 region 控制面和 observe 扩展提供方向

## 15. 后续拆分建议

- `RFC-017A` global directory and identity
- `RFC-017B` regional observe federation
- `RFC-017C` region failover and DR model
