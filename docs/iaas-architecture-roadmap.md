# Aria IaaS 网络架构原则与演进路线

本文档用于冻结 Aria 面向 IaaS 平面的长期技术方向，避免后续在功能推进过程中回到“单点功能堆叠”模式。它不是一次性的脑暴记录，而是后续架构设计、接口演进、模块拆分和路线优先级判断的统一依据。

Phase 0 的下游正式设计文档统一放在 [RFC Index](rfcs/README.md)。

进入代码阶段前，还必须同时遵守以下实施约束：

- [Aria eBPF 实现约束](ebpf-implementation-constraints.md)
- [Aria 重构护栏](refactor-guardrails.md)

以上两份约束文档与下游 RFC 一起，构成进入 `Phase 1` 前必须遵守的实施红线，不是可选建议。

当前已落地的第一批 RFC：

- [RFC-001 资源模型 v1](rfcs/rfc-001-resource-model.md)
- [RFC-002 统一事件模型 v1](rfcs/rfc-002-event-model.md)
- [RFC-003 Controller-Agent Southbound 协议 v1](rfcs/rfc-003-southbound-protocol.md)
- [RFC-004 Node datapath 编译模型 v1](rfcs/rfc-004-node-datapath-compiler.md)
- [RFC-005 Routing / NAT 基础数据面 v1](rfcs/rfc-005-routing-nat-datapath.md)
- [RFC-006 Diagnose 服务端化与 Relay v1](rfcs/rfc-006-diagnose-relay.md)
- [RFC-007 权限、租户与审计模型 v1](rfcs/rfc-007-tenant-authz-audit.md)
- [RFC-008 Service / Backend / HealthCheck 模型 v1](rfcs/rfc-008-service-backend-healthcheck.md)
- [RFC-009 Multi-node Overlay / Native Routing v1](rfcs/rfc-009-multi-node-networking.md)
- [RFC-010 Northbound API v1](rfcs/rfc-010-northbound-api.md)
- [RFC-011 Datapath Capability Matrix v1](rfcs/rfc-011-datapath-capability-matrix.md)
- [RFC-012 Rollout / Audit / Shadow Mode v1](rfcs/rfc-012-rollout-audit-shadow.md)
- [RFC-013 Controller Deployment Topology v1](rfcs/rfc-013-controller-topology.md)
- [RFC-014 Persistence and Storage Model v1](rfcs/rfc-014-persistence-storage-model.md)
- [RFC-015 UI / Topology / Workflow Model v1](rfcs/rfc-015-ui-topology-workflow.md)
- [RFC-016 Node-local Debug API Boundary v1](rfcs/rfc-016-node-local-debug-api.md)
- [RFC-017 Multi-cluster / Region Model v1](rfcs/rfc-017-multi-cluster-region.md)
- [RFC-018 Upgrade and Migration Model v1](rfcs/rfc-018-upgrade-migration.md)

## 0.1 Phase 1 实施进度

截至 `2026-04-09`，仓库已经新增实验性的 `controller` crate，作为 `RFC-001` 与 `RFC-010` 的第一阶段代码落地：

- 提供平台级 northbound API 骨架
- 提供默认内存态资源存储
- 提供 `/openapi.json` 与 `/docs`
- 首批资源覆盖 `Tenant / Node / Network / Port / SecurityGroup / RouteTable`
- northbound 列表接口已提供第一版分页/过滤骨架：`limit / page_token / label_selector` + 基础对象过滤
- northbound 已提供 `X-Request-Id` 透传/自动生成，错误响应中的 `request_id` 与响应头对齐
- 提供第一阶段 southbound HTTP 骨架，覆盖 `register / desired-state / apply-status / heartbeat / status`
- controller 已通过 store trait 隔离存储边界，并新增可选的文件快照 backend；默认实现仍为内存版

当前实现仍不包含：

- 控制面持久化
- 认证、授权与审计
- southbound 协议编译
- datapath 下发与恢复闭环

因此当前实现只能视为平台控制面的启动骨架，而不是完整的 Phase 1 完成态。

## 1. 产品定位

Aria 的目标不是做“纯 eBPF 实现一切”的实验项目，也不是简单把当前主机防火墙能力继续横向扩展。

Aria 的目标是成为：

- IaaS 平面的统一网络执行层
- 主机侧与节点侧的高性能数据面平台
- 安全、转发、NAT、负载均衡、服务链、观测的统一承载框架
- 具备集中控制、统一事件模型、可持续演进能力的平台型产品

Aria 不追求替代所有传统网络控制协议栈，也不在短期内重建完整交换机控制平面或完整 L7 代理生态。

## 2. 核心原则

### 2.1 eBPF-first，而不是 eBPF-only

eBPF 负责最关键、最性能敏感、最贴近数据面的能力：

- L2/L3 快速转发
- ACL / 防火墙 / conntrack
- NAT / SNAT / DNAT
- L4 负载均衡
- QoS / mirror / redirect
- route / lb / nat / drop / trace 事件原生采集

用户态负责复杂控制逻辑和平台能力：

- 资源编排
- 路由协议
- 配置分发
- 状态聚合
- 历史存储与查询
- 复杂 L7 能力

### 2.2 状态所有权必须清晰

Aria 的状态分成四层：

- Desired State：控制面中的源事实
- Compiled State：Agent 在本地生成的可执行配置
- Runtime State：eBPF map、pinned links、内核附着状态
- Observability State：事件、流日志、指标、拓扑聚合结果

eBPF map 是执行态缓存，不是最终真相。最终真相必须保存在控制面和本地可恢复状态中。

### 2.3 统一的是分类器，不是把所有功能硬揉成一个动作

Aria 应统一：

- 对象模型
- 条件匹配模型
- 事件模型
- 可观测标签模型

Aria 不应强行把所有动作语义做成一个模糊抽象。推荐的动作族为：

- `forward`
- `drop`
- `redirect`
- `mirror`
- `nat`
- `lb`
- `qos`
- `observe`

### 2.4 数据面与控制面必须解耦

控制面失效不应立即影响已下发的稳定转发。

Observability 子系统失效时，优先丢观测，不能反向拖垮转发。

### 2.5 观测能力不是附属品，而是主能力

Aria 不只输出流量统计，还应输出一致的决策与证据：

- 为什么放行
- 为什么丢弃
- 命中了哪条策略
- 经过了哪条路由
- 命中了哪个后端
- 是否发生 NAT
- 延迟卡在哪一段

### 2.6 能力必须允许降级运行

内核能力、hook 能力、helper 能力、程序大小和 map 规模都会决定功能边界。Aria 必须允许：

- 能力探测
- 按内核版本降级
- 按部署模式裁剪
- 审计模式和 shadow 模式

### 2.7 平台对象优先于功能菜单

未来产品不能继续围绕“group / policy / qos / mirror”这样的功能菜单组织。应围绕平台对象组织：

- Tenant
- Network
- Segment / Subnet
- Port / Attachment
- Instance
- Service
- Route
- Security Group

## 3. 技术边界

### 3.1 适合进入 eBPF 快路径的能力

- L2/L3 转发
- anti-spoof
- ACL / stateful firewall
- conntrack
- NAT / FIP
- L4 负载均衡
- VXLAN / Geneve 封装解封
- egress gateway
- policy routing
- mirroring
- route / nat / lb / drop / trace 事件采集

### 3.2 不应强行塞进 eBPF 的能力

- BGP / EVPN / 动态路由协议计算
- 中央控制器
- 多租户权限系统
- 长期历史存储
- 复杂 HTTP / TLS 路由与代理
- 完整交换控制协议

### 3.3 推荐实现方式总览

| 能力 | 推荐实现方式 | 备注 |
| --- | --- | --- |
| L2/L3 快速转发 | eBPF 为主 | XDP/TC 负责快路径，控制逻辑在用户态 |
| ACL / 防火墙 | eBPF 为主 | XDP 早丢弃，TC 承担复杂路径 |
| Conntrack | eBPF 为主 | 与 NAT / LB 共享状态语义 |
| NAT / SNAT / DNAT | eBPF 为主 | 配合 conntrack 与 checksum 修正 |
| L4 负载均衡 | eBPF 为主 | 一致性哈希、DSR、后端选择 |
| VXLAN / Geneve | eBPF 为主 | 以 TC 为主，XDP 优化为增强项 |
| QoS / mirror / redirect | eBPF 为主 | 作为统一动作族的一部分 |
| 路由协议 | 用户态为主 | 对接 FRR / BIRD 等成熟栈 |
| L7 网关 | 用户态为主 | Envoy / HAProxy / 自研代理 |
| 流事件与指标 | eBPF + 用户态 | eBPF 采集，用户态聚合和导出 |
| 历史查询与拓扑 UI | 用户态为主 | 对接 Prometheus / Loki / ClickHouse |

## 4. 分层架构

```text
┌─────────────────────────────────────────────────────────────┐
│                 Aria Controller (Control Plane)             │
│  - 资源模型管理                                             │
│  - 策略编排与审计                                           │
│  - 配置版本化、灰度、回滚                                   │
│  - 租户、权限、配额                                         │
└──────────────────────────┬──────────────────────────────────┘
                           │ northbound / southbound API
┌──────────────────────────┼──────────────────────────────────┐
│                Aria Observability Platform                  │
│  - 统一事件接入与聚合                                       │
│  - 实时流查询 / 历史查询 / 拓扑                             │
│  - Dashboard / Diagnose API / Relay                         │
│  - 对接 Prometheus / Loki / ClickHouse                      │
└──────────────────────────┬──────────────────────────────────┘
                           │ agent streaming / query API
┌──────────────────────────┼──────────────────────────────────┐
│                     Aria Agent (per node)                   │
│  - Desired State 拉取或接收                                 │
│  - 编译高层对象到 datapath 状态                             │
│  - eBPF 程序加载、更新、恢复                                │
│  - 本地 WAL / snapshot / reconcile                          │
│  - 事件采集、指标暴露、节点健康检查                         │
└──────────────────────────┬──────────────────────────────────┘
                           │ bpf syscall / bpffs / netlink
┌──────────────────────────┼──────────────────────────────────┐
│                 eBPF Datapath (XDP / TC / cgroup)           │
│  - 转发 / 路由 / NAT / LB / ACL / QoS / mirror              │
│  - route / nat / lb / drop / trace / tcprt 原生事件         │
│  - 节点级快速执行与决策                                     │
└─────────────────────────────────────────────────────────────┘
```

Observability Platform 单独成层，是因为它最终会成为独立子系统，而不是 Agent 的附属功能。

## 5. 组件职责

### 5.1 Controller

Controller 负责：

- 资源 CRUD
- 节点注册与证书
- 配置版本管理
- 灰度发布和回滚
- 审计与变更追踪
- 租户与权限模型

Controller 不直接进入数据面转发热路径。

### 5.2 Agent

Agent 是节点侧编译器和执行器。

它负责：

- 接收 Desired State
- 生成本地 Compiled State
- 更新 eBPF maps 和附着状态
- 做能力探测和降级
- 负责本地恢复和 reconcile
- 上报事件与指标

### 5.3 Datapath

Datapath 负责：

- 快速匹配
- 动作执行
- 轻量状态维护
- 原生事件采集

Datapath 不承担平台编排逻辑，不承担中心化状态一致性逻辑。

### 5.4 Observability Platform

Observability Platform 负责：

- 接收实时事件流
- 提供实时查询
- 存储历史流日志
- 生成聚合指标和拓扑
- 输出机器可消费的诊断 API

## 6. Hook 使用原则

| Hook | 适用场景 | 原则 |
| --- | --- | --- |
| XDP | 早期丢弃、anti-spoof、简单 fast path、DDoS 前置 | 尽量轻、尽量早、避免复杂 skb 依赖 |
| TC ingress | 入向转发、route、NAT、policy、mirror | 数据面主战场之一 |
| TC egress | 出向路由、encap、LB、NAT、QoS | 数据面主战场之一 |
| cgroup / socket | connect-time steering、本机流量策略 | 用于主机侧优化 |
| userspace proxy / uprobe | TLS 终止、HTTP 路由、复杂 L7 | 不强行塞进 XDP / TC |

原则上不要把“所有能力都优先放在 XDP”当成架构目标。能否放在 XDP，取决于语义、helper、程序复杂度和可维护性。

## 7. 状态与恢复模型

Aria 的恢复模型应长期遵循以下原则：

- 中央控制面保存 Desired State
- Agent 本地保存可恢复的 Compiled State
- eBPF map 保存 Runtime State
- 关键本地状态要支持 WAL + snapshot
- Agent 重启后优先恢复已下发策略，再补齐 runtime attach
- Controller 不可用时，已下发配置应继续生效

本地状态建议长期分为：

- `desired_state_cache`
- `compiled_state`
- `runtime_attach_state`
- `event_cursor / observe_cursor`

## 8. 资源模型 v1

### 8.1 核心资源

| 资源 | 说明 |
| --- | --- |
| Tenant | 租户边界、权限和配额单位 |
| Node | 宿主机节点，运行 Agent 和 datapath |
| Network | 逻辑网络域，可对应 VPC |
| Segment / Subnet | 网络分段，承载地址规划与路由边界 |
| Port / Attachment | 实例接入点，绑定 MAC、IP、anti-spoof 属性 |
| Instance | 虚机或容器实例，接入到一个或多个 Port |
| AddressSet | 地址对象集合，用于策略和 NAT 复用 |
| SecurityGroup | 安全策略容器 |
| SecurityRule | 五元组规则及其动作 |
| RouteTable | 路由表对象 |
| Route | 前缀到下一跳的映射 |
| FloatingIP | 公网或弹性地址映射 |
| NatGateway | 出口地址转换对象 |
| Service | VIP 抽象，用于 LB 和服务入口 |
| BackendSet | 后端集合 |
| HealthCheck | 后端健康检查定义 |
| Chain | 服务链定义 |

### 8.2 IaaS 场景必须覆盖的附属对象

以下对象不应被遗漏：

- `AllowedAddressPairs`
- `DHCP / metadata service`
- `anti-spoof profile`
- `egress policy profile`
- `tenant quota`

## 9. 事件模型 v1

所有事件都共享统一 envelope。

### 9.1 公共字段

| 字段 | 说明 |
| --- | --- |
| `timestamp` | 事件发生时间 |
| `node_id` | 节点标识 |
| `tenant_id` | 租户标识 |
| `network_id` | 网络标识 |
| `instance_id` | 实例标识 |
| `port_id` | 接入端口标识 |
| `flow_id` | 流标识 |
| `hook` | 触发 hook |
| `direction` | 入向或出向 |
| `verdict` | `pass/drop/redirect/nat/lb/mirror/chain-forward` |
| `policy_id` | 命中的策略 |
| `route_id` | 命中的路由 |
| `service_id` | 命中的虚拟服务 |
| `backend_id` | 选中的后端 |
| `trace_id` | 关联 trace 或诊断会话 |

### 9.2 事件类型

| 事件类型 | 说明 |
| --- | --- |
| `flow_event` | 基础流事件 |
| `drop_event` | 丢包和原因归类 |
| `route_event` | 路由决策结果 |
| `nat_event` | NAT 映射和转换动作 |
| `lb_event` | LB 选择和哈希结果 |
| `mirror_event` | 镜像动作 |
| `trace_event` | 精细路径追踪 |
| `tcprt_event` | TCP 时延与状态 |
| `ssl_event` | TLS 握手和错误 |
| `http_event` | HTTP 请求、响应和延迟 |
| `chain_event` | 服务链 hop 选择和结果 |

### 9.3 事件系统原则

- 高基数事件必须支持采样
- 事件上报必须支持限流和背压
- 聚合层必须支持冷热分层存储
- 任何事件模型变更都要版本化

## 10. API 与协议边界

### 10.1 Northbound API

Northbound API 面向：

- UI
- 自动化平台
- SDK
- 租户或管理员侧控制面调用

建议长期保持：

- REST + OpenAPI
- 清晰的资源导向路径
- 版本化 API

### 10.2 Southbound API

Southbound API 面向：

- Controller 到 Agent 的配置下发
- Agent 到 Controller 的健康和状态回报

建议长期采用：

- gRPC 流式协议
- 明确的版本号和能力协商
- 支持 full sync 和 incremental update

### 10.3 Node-local API

当前节点本地 API 未来应保留，但定位应调整为：

- 调试入口
- 节点侧只读观测入口
- 紧急运维入口

不再把节点本地“功能菜单型 API”作为长期主控制面接口。

## 11. 分阶段路线图

### Phase 0：架构冻结

- 目标：冻结架构原则、对象模型、事件模型和边界
- 产出：架构 RFC、资源模型 RFC、事件模型 RFC、内核能力矩阵
- 参考： [RFC-011 Datapath Capability Matrix v1](rfcs/rfc-011-datapath-capability-matrix.md)
- 验收：团队对组件边界、对象边界、hook 边界无歧义

### Phase 1：控制面骨架

- 目标：建立 Controller 和基本资源 CRUD
- 产出：`Tenant`、`Node`、`Network`、`Subnet`、`Port`、`Instance`、`SecurityGroup`、`RouteTable` 的 northbound API
- 参考： [RFC-010 Northbound API v1](rfcs/rfc-010-northbound-api.md) 和 [RFC-013 Controller Deployment Topology v1](rfcs/rfc-013-controller-topology.md)
- 验收：控制面可生成和下发基础网络配置

### Phase 2：Agent 重构

- 目标：把 Agent 从功能菜单执行器重构为平台对象编译器
- 产出：Desired State -> Compiled State -> Runtime State 编译链路
- 参考： [RFC-004 Node datapath 编译模型 v1](rfcs/rfc-004-node-datapath-compiler.md)
- 验收：Agent 能稳定执行和恢复对象级配置

### Phase 3：单节点 IaaS 网络最小闭环

- 目标：完成单节点基础 IaaS 网络能力
- 产出：Port attachment、anti-spoof、L3 routing、stateful firewall、SNAT/DNAT/FIP
- 参考： [RFC-005 Routing / NAT 基础数据面 v1](rfcs/rfc-005-routing-nat-datapath.md)
- 验收：单节点实例互联、隔离、出公网和安全控制全部跑通

### Phase 4：统一事件模型与 Diagnose 平台化

- 目标：把现有观测能力升级成统一事件系统
- 产出：服务端 Diagnose API、统一事件 schema、实时流查询接口
- 参考： [RFC-002 统一事件模型 v1](rfcs/rfc-002-event-model.md) 和 [RFC-006 Diagnose 服务端化与 Relay v1](rfcs/rfc-006-diagnose-relay.md)
- 验收：任意连接可在平台侧得到统一诊断结果

### Phase 5：观测聚合层

- 目标：引入 Aria Relay 和多节点聚合
- 产出：多节点事件流聚合、租户维度查询、基础拓扑视图
- 参考： [RFC-006 Diagnose 服务端化与 Relay v1](rfcs/rfc-006-diagnose-relay.md)、[RFC-013 Controller Deployment Topology v1](rfcs/rfc-013-controller-topology.md) 和 [RFC-015 UI / Topology / Workflow Model v1](rfcs/rfc-015-ui-topology-workflow.md)
- 验收：跨节点流量可统一检索和聚合

### Phase 6：L4 负载均衡与服务链

- 目标：提供节点级和平台级服务入口能力
- 产出：Service、BackendSet、HealthCheck、LB datapath、动态服务链
- 参考： [RFC-008 Service / Backend / HealthCheck 模型 v1](rfcs/rfc-008-service-backend-healthcheck.md)
- 验收：VIP、后端调度、链路引流和事件追踪完整闭环

### Phase 7：多节点织网

- 目标：形成多节点网络面
- 产出：VXLAN / Geneve 或 native routing、跨节点路由、下一跳模型、BGP 对接
- 参考： [RFC-009 Multi-node Overlay / Native Routing v1](rfcs/rfc-009-multi-node-networking.md)
- 验收：多节点互通、路由生效、观测和策略一致

### Phase 8：平台化完善

- 目标：从“可运行”走向“可运营”
- 产出：RBAC、审计、配额、灰度发布、回滚、历史查询、UI
- 参考： [RFC-007 权限、租户与审计模型 v1](rfcs/rfc-007-tenant-authz-audit.md)、[RFC-012 Rollout / Audit / Shadow Mode v1](rfcs/rfc-012-rollout-audit-shadow.md)、[RFC-014 Persistence and Storage Model v1](rfcs/rfc-014-persistence-storage-model.md)、[RFC-015 UI / Topology / Workflow Model v1](rfcs/rfc-015-ui-topology-workflow.md) 和 [RFC-018 Upgrade and Migration Model v1](rfcs/rfc-018-upgrade-migration.md)
- 验收：具备企业级运维与治理能力

### Post-Phase 扩展

- 跨集群 / 跨地域演进参考： [RFC-017 Multi-cluster / Region Model v1](rfcs/rfc-017-multi-cluster-region.md)
- 节点本地调试边界参考： [RFC-016 Node-local Debug API Boundary v1](rfcs/rfc-016-node-local-debug-api.md)
- 平台升级与迁移演进参考： [RFC-018 Upgrade and Migration Model v1](rfcs/rfc-018-upgrade-migration.md)

## 12. 当前代码到未来架构的映射

当前仓库已经具备未来平台的一部分种子能力。

| 当前模块 | 未来定位 |
| --- | --- |
| `agent/src/control_plane.rs` | Agent 本地编译器与执行器内核 |
| `core/src/wal.rs` | Agent 本地 durability 和恢复机制 |
| `agent/src/api_routes.rs` | 节点本地调试与运维 API |
| `agent/src/service_chain.rs` | Chain 资源的早期原型 |
| `agent/src/api_handlers/metrics.rs` | 统一指标导出入口 |
| `tcprt / ssl / trace / drops` | 未来统一事件模型中的专用事件类型 |
| `group / policy / qos / mirror` | 未来平台对象编译后的局部能力，不再是长期顶层模型 |

## 13. 当前最重要的缺口

从平台化角度看，当前最需要补的不是单点 datapath feature，而是以下四类缺口：

- 缺少正式的 IaaS 资源模型
- 缺少 Controller 和 southbound 协议
- 缺少统一事件模型与聚合层
- 缺少 routing / port / anti-spoof / NAT 这一组基础 IaaS 数据面闭环

## 14. 近期行动项

### 14.1 第一优先级

- 编写 [资源模型 RFC](rfcs/rfc-001-resource-model.md)
- 编写 [事件模型 RFC](rfcs/rfc-002-event-model.md)
- 设计 [Controller <-> Agent southbound 协议 RFC](rfcs/rfc-003-southbound-protocol.md)
- 设计 [Northbound API RFC](rfcs/rfc-010-northbound-api.md)
- 确定 [能力探测与降级矩阵 RFC](rfcs/rfc-011-datapath-capability-matrix.md)
- 设计 [Persistence and Storage Model RFC](rfcs/rfc-014-persistence-storage-model.md)

### 14.2 第二优先级

- 设计 `Port / Attachment` 对象
- 设计 `RouteTable / Route / NextHop` 对象
- 设计 `SecurityGroup / Rule` 对象
- 设计 `FloatingIP / NAT` 对象
- 设计 [Rollout / Audit / Shadow RFC](rfcs/rfc-012-rollout-audit-shadow.md)
- 设计 [Controller Deployment Topology RFC](rfcs/rfc-013-controller-topology.md)

### 14.3 第三优先级

- 把当前 `diagnose` 提升为正式服务端 API
- 规划 `Aria Relay`
- 统一 trace / drop / tcprt / ssl / http 的事件 schema

## 15. 明确暂不作为短期目标的事项

以下事项不作为短期阻塞项：

- 自研完整 BGP / EVPN 协议栈
- 自研完整交换机控制平面
- 自研完整 L7 代理生态
- 在第一阶段追求完整 Kubernetes 风格 identity 策略系统
- 把所有现有模块都塞回 XDP

## 16. 文档使用方式

后续任何重大设计变更，都应先检查是否与本文档冲突。

如果出现以下情况，应优先更新本文档或其下游 RFC，而不是先改代码：

- 新增平台级对象
- 修改事件主模型
- 修改 Controller / Agent / Observability 责任边界
- 修改 Phase 路线和优先级

本文档的目标不是限制实现，而是防止路线漂移。

## 17. 变更约束

为保证后续演进不偏离本路线，新增以下约束：

- 任何新增平台级对象，必须先更新对应 RFC，再进入代码实现
- 任何新增主事件类型，必须先更新事件模型 RFC，再进入代码实现
- 任何 Controller 与 Agent 之间的新主交互，必须先更新 southbound RFC
- 如果下游 RFC 与本文档冲突，必须先修订本文档
- 如果实现与 RFC 冲突，必须先修订 RFC，并在变更说明中标注原因
