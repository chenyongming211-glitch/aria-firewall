# RFC-009：Multi-node Overlay / Native Routing v1

状态：Draft  
阶段：Phase 7  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 从单节点 IaaS 网络闭环走向多节点网络面的总体方案。

本 RFC 的目标是冻结多节点方向的关键边界：

- 什么时候选择 overlay
- 什么时候选择 native routing
- 节点间如何表达 next hop 和 reachability
- 织网层与控制协议栈如何协同

## 2. 非目标

本 RFC 不直接定义：

- 最终 EVPN 控制平面
- 具体 BGP 配置语法
- 完整 VXLAN/Geneve 程序实现
- 具体 underlay 自动发现协议

## 3. 设计原则

### 3.1 先统一节点内语义，再扩展节点间语义

多节点不应重写单节点对象模型。多节点只是在已有：

- Port
- Route
- NextHop
- Service
- Chain

之上增加跨节点 reachability 和封装/路由能力。

### 3.2 Overlay 与 Native Routing 都应成为可选模式

Aria 不应把自己锁死在单一模式。

建议至少支持：

- `overlay`
- `native`
- `hybrid`

### 3.3 控制协议与数据面执行分离

即使后续对接 FRR/BIRD，协议计算仍在用户态；eBPF 负责执行已收敛的下一跳和转发决策。

### 3.4 可观测性必须跨节点连续

多节点转发后，观测不能断成多段黑盒。至少要能关联：

- ingress node
- egress node
- tunnel / encap hop
- route decision chain

## 4. 模式定义

### 4.1 Overlay

Overlay 模式下：

- 节点间通过 VXLAN / Geneve 等隧道互联
- 实例地址空间不要求 underlay 原生可达
- Node 间维护 VNI / peer / tunnel metadata

适用：

- Underlay 可控性差
- 租户隔离强
- 需要快速交付

### 4.2 Native Routing

Native 模式下：

- 实例或 segment 地址可被 underlay 路由
- Aria 维护 route / next-hop / policy routing
- 可与 FRR/BIRD 协同

适用：

- Underlay 可编程
- 追求更低封装开销
- 需要和现有网络深度协同

### 4.3 Hybrid

Hybrid 模式允许：

- 同一平台中部分网络走 overlay
- 部分网络走 native

Hybrid 是可选高级特性，不是 Phase 7 的阻塞项。

在实现优先级上，应先确保：

- overlay 模式具备完整最小闭环
- native routing 模式具备完整最小闭环

只有在这两条主路径都稳定后，才进入 hybrid 的统一编排与混合调度。

## 5. 节点间对象

多节点需要新增或强化以下对象：

- `NodeLink`
- `Reachability`
- `TunnelEndpoint`
- `PeerNode`
- `RemoteNextHop`

## 6. Reachability 模型

建议控制面维护：

- 哪个前缀在哪个节点可达
- 哪个 VNI 对应哪个 segment
- 哪个 remote next hop 对应哪个 peer node

节点侧编译器应将其投影为：

- remote route
- tunnel peer
- remote next-hop metadata

## 7. Overlay 模型

### 7.1 基础对象

Overlay 模式至少需要：

- `vni`
- `peer node ip`
- `encap type`
- `remote segment mapping`

### 7.2 datapath 责任

datapath 负责：

- encap
- decap
- remote node redirect
- 基本隧道元数据校验

### 7.3 控制面责任

控制面负责：

- VNI 分配
- peer 发现
- reachability 分发

## 8. Native Routing 模型

### 8.1 基础对象

Native routing 需要：

- remote route
- next hop
- egress iface
- optional route source

### 8.2 对接外部路由栈

建议：

- 协议计算由 FRR/BIRD 或等效用户态组件承担
- Aria 接收已收敛路由，编译成 datapath 可执行状态

### 8.3 datapath 责任

datapath 负责：

- route lookup
- next-hop redirect
- optional policy route
- event emission

## 9. 多节点安全边界

跨节点后必须明确：

- remote node 是否可信
- tunnel / native peer 的身份
- 租户流量隔离是否保留
- 跨节点 anti-spoof 如何落地

## 10. 与 Service 的关系

多节点后，Service 需要扩展：

- backend 可分布在不同节点
- health 状态可能跨节点收集
- session affinity 可能需带节点维度

但本 RFC 不定义具体 LB 算法，只定义节点间 reachability 前提。

## 11. 事件与观测要求

多节点网络至少应新增以下可观测字段：

- ingress node
- egress node
- tunnel id / vni
- remote next hop id
- encap type
- overlay/native mode

对应应可产生：

- `route_event`
- `flow_event`
- `drop_event`
- 后续 `overlay_event`（如需要）

## 12. 分阶段落地建议

### 12.1 第一阶段

先定义对象和 reachability 模型。

### 12.2 第二阶段

优先实现 overlay 模式的最小闭环：

- remote route
- tunnel peer
- basic encap/decap

### 12.3 第三阶段

实现 native routing 模式与外部路由栈协同。

### 12.4 第四阶段

支持 hybrid mode 与高级优化。

这一步是增强项，不应阻塞 overlay 与 native 两条主路径的交付验收。

## 13. 当前代码映射与缺口

当前仓库主要仍是单节点 / 节点本地视角。

当前缺口：

- 缺少 remote route 与 peer 模型
- 缺少 overlay / native mode 明确定义
- 缺少多节点 observability 连续性设计
- 缺少与外部路由栈协同的 southbound / adapter

## 14. 验收标准

Multi-node networking v1 的验收标准：

- 能表达 overlay、native、hybrid 三种模式
- 能表达 remote reachability 与 next hop
- 能作为后续 VXLAN/Geneve 或 native routing 的控制面输入
- 不破坏既有单节点对象模型

## 15. 后续拆分建议

- `RFC-009A` overlay reachability and tunnel model
- `RFC-009B` native routing adapter with FRR/BIRD
- `RFC-009C` multi-node observability continuity
