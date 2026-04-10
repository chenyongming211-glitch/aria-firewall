# RFC-008：Service / Backend / HealthCheck 模型 v1

状态：Draft  
阶段：Phase 6  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 平台中的四层服务抽象，用于承载：

- VIP
- Backend 选择
- 健康检查
- 会话保持
- 与服务链联动

本 RFC 的目标是冻结服务对象模型和控制面语义，为未来 L4 负载均衡 datapath 提供稳定输入。

## 2. 非目标

本 RFC 不覆盖：

- 完整 L7 网关路由
- 复杂 HTTP header 路由
- 自研全量健康检查执行器实现
- 多区域 GSLB

## 3. 设计原则

### 3.1 Service 是控制面对象，不是 datapath 里的散规则

VIP、后端、健康和策略必须围绕 `Service` 对象表达，而不是用若干孤立 NAT / redirect 规则拼凑。

### 3.2 后端选择必须可观测

每次 service 命中必须能够回答：

- 命中了哪个 service
- 为什么选中该 backend
- backend 当前健康状态如何
- 是否命中会话保持

### 3.3 健康检查结果属于控制面输入

健康检查可以由节点执行、平台汇总，但其结果最终应进入 `BackendSet` 的有效视图，而不是临时脚本式逻辑。

### 3.4 初版先清晰支持 L4

v1 先支持 TCP / UDP 级别服务，不把 L7 网关复杂性引入本 RFC。

### 3.5 L4 负载均衡必须同时覆盖节点内与跨节点转发

Service datapath 的第一版目标不能只停留在“节点内 VIP 命中”。
v1 必须把以下两类路径同时作为正式目标：

- 节点内转发：请求和被选中的 backend 位于同一节点
- 跨节点转发：请求命中本节点 service 入口，但 backend 位于其他节点

因此，后续 `routing / NAT / FloatingIP` 的接口预留和 shadow 骨架，只能作为不阻塞 L4 主路径的前置准备，不能反向改变 Service/LB 的优先级。

## 4. 资源模型

### 4.1 Service

表示一个对外暴露的逻辑服务入口。

建议字段：

- `id`
- `tenant_id`
- `network_id`
- `name`
- `vip`
- `protocol`
- `ports`
- `backend_set_id`
- `session_affinity`
- `lb_policy`
- `exposure_type`
- `status`

说明：

- `exposure_type` 可表示 `internal`、`floating-ip-backed`、`gateway-exposed`

### 4.2 BackendSet

表示一组可供调度的后端。

建议字段：

- `id`
- `service_id`
- `health_check_id`
- `policy`
- `backends`

### 4.3 Backend

表示单个后端成员。

建议字段：

- `id`
- `backend_set_id`
- `target_type`
- `target_ref`
- `ip`
- `port`
- `weight`
- `admin_state`
- `observed_state`

### 4.4 HealthCheck

表示后端健康检查定义。

建议字段：

- `id`
- `type`
- `interval`
- `timeout`
- `healthy_threshold`
- `unhealthy_threshold`
- `target_port`
- `request_template`

## 5. Service 作用域

Service 至少应具备：

- tenant 归属
- network 归属
- 可被 Route / Chain / FloatingIP 引用

Service 不应默认跨租户共享。

## 6. 调度策略

### 6.1 初版建议支持

- `round_robin`
- `maglev`
- `hash_5tuple`
- `hash_src_ip`

### 6.2 会话保持

建议支持：

- `none`
- `client_ip`
- `5tuple`

### 6.3 权重

Backend 应支持权重，但 v1 可允许简单整数权重。

## 7. 健康状态模型

### 7.1 状态来源

Backend 状态可来自：

- active health check
- passive observation
- operator override

### 7.2 状态枚举

建议至少支持：

- `healthy`
- `degraded`
- `unhealthy`
- `unknown`
- `draining`

### 7.3 健康与调度关系

建议：

- `healthy`：可调度
- `degraded`：可调度但可降低权重
- `unhealthy`：默认不调度
- `draining`：仅保留已有会话

## 8. datapath 投影要求

Agent 编译后应能生成：

- service id -> VIP 映射
- service port -> backend set 映射
- backend selection map
- session affinity state
- health-filtered backend view
- 节点内转发与跨节点转发所需的 service forwarding projection

## 9. 与 NAT 和 Route 的关系

Service 抽象与路由、NAT 需要明确边界：

- 外部访问可先经 Route/FIP，再命中 Service
- 内部流量可直接命中 Service
- Service 本身不替代 RouteTable
- NAT / Route / FIP 的接口预留不得改变 Service datapath 的主优先级；L4 负载均衡必须按自身路径独立成立

## 10. 与 Chain 的关系

Service 应可以作为 Chain 的一个 hop 或前置入口。

示例：

- 入口 VIP -> 安全检查 Service -> 应用 Service
- Service 命中后，进入后续 Chain hop

## 11. 事件模型要求

Service datapath 相关事件至少应输出：

- `service_id`
- `backend_id`
- `lb_policy`
- `hash_key` 或 hash 摘要
- `affinity_hit`
- `backend_state`

对应事件类型主要为：

- `lb_event`
- `flow_event`
- `drop_event`（无健康后端等场景）

## 12. 当前代码映射与缺口

当前仓库中的：

- `service_chain`

只是链路配置的早期原型，并不等于正式的 `Service` 抽象。

当前缺口：

- 没有正式的 Service / BackendSet / HealthCheck 对象
- 没有 LB datapath
- 没有 backend 健康模型
- 没有 lb_event

## 13. 分阶段落地建议

### 13.1 第一阶段

先定义对象和 northbound API，不急于一次做完所有调度策略。

### 13.2 第二阶段

实现节点级 service 编译与 backend 选择，至少覆盖节点内转发和跨节点转发的统一 service 语义。

### 13.3 第三阶段

引入健康检查与 session affinity。

### 13.4 第四阶段

与 Chain、Relay、Diagnose 做深度联动。

## 14. 验收标准

Service 模型 v1 的验收标准：

- northbound 能表达 VIP、后端、健康检查
- Agent 能把 Service 编译成节点局部 service state
- L4 负载均衡能同时覆盖节点内转发和跨节点转发
- 能输出 backend 选择与健康相关事件
- Diagnose 能基于 Service 维度查询和解释故障
- Agent 能把 Service 编译成节点局部 service state
- datapath 能输出稳定的 backend 选择证据
- Diagnose 能基于 Service 维度查询和解释故障

## 15. 后续拆分建议

- `RFC-008A` LB datapath
- `RFC-008B` health check execution
- `RFC-008C` session affinity model

## 16. 参考实现研究（Cilium）

为避免在 L4 LB 上重复踩坑，Aria 的 service datapath 设计应显式借鉴 Cilium 已验证的分层模式。

### 16.1 借鉴边界

本 RFC 借鉴 Cilium 的目的，是提炼稳定的 L4 LB datapath 拆分方式，而不是把 Aria 已完成的对象层和 shadow compiler 推倒重来。

因此必须坚持：

- 已经完成的 `Service / BackendSet / HealthCheck` 控制面对象层、southbound 投影和 agent shadow 编译链继续保留
- Aria 不复制 Kubernetes `Service` / `NodePort` / `ClusterIP` API 语义，也不复制 kube-proxy replacement 的部署前提
- Aria 只选择性借鉴 Cilium 在 L4 datapath 上已经验证过的状态拆分和执行路径
- `routing / NAT / Floating IP` 仍是协同能力，不得因为参考 Cilium 的实现而反向挤占 `L4 LB + service chain` 主线

### 16.2 本 RFC 以 Cilium 为主参考，而不是 Calico

本 RFC 聚焦 L4 service datapath，因此主参考是 Cilium，而不是 Calico。

原因：

- Cilium 在 `frontend/backend/revnat/affinity/maglev` 这些 L4 LB 核心状态上有更直接的 eBPF 数据面实现
- Cilium 已明确区分 `socket lb path` 与 `packet lb path`，这与 Aria 的下一阶段落地顺序直接对应
- Calico 更适合作为后续 `BGP / service IP advertisement / host endpoint / tiered policy` 的参考实现，相关边界应放到 `RFC-009` 等后续 RFC 中讨论

参考入口：

- [Cilium kube-proxy-free 文档](https://docs.cilium.io/en/stable/network/kubernetes/kubeproxy-free/)
- [Cilium eBPF map 文档](https://docs.cilium.io/en/latest/network/ebpf/maps/)
- [bpf/lib/lb.h](https://github.com/cilium/cilium/blob/main/bpf/lib/lb.h)
- [bpf/bpf_sock.c](https://github.com/cilium/cilium/blob/main/bpf/bpf_sock.c)
- [pkg/loadbalancer/maps/lbmaps.go](https://github.com/cilium/cilium/blob/main/pkg/loadbalancer/maps/lbmaps.go)
- [pkg/loadbalancer/service.go](https://github.com/cilium/cilium/blob/main/pkg/loadbalancer/service.go)
- [pkg/loadbalancer/loadbalancer.go](https://github.com/cilium/cilium/blob/main/pkg/loadbalancer/loadbalancer.go)

从这些实现中，Aria 当前最值得直接借鉴的点有 7 个：

1. Service datapath 不是“一个大 map”，而是一组职责明确的 map / state family：
   - frontend/service map
   - backend/member map
   - reverse NAT map
   - affinity map
   - affinity match map
   - Maglev lookup table
   - socket reverse NAT map
2. 前端匹配与后端选择分离。frontend key 先表达 VIP/port/proto/scope，再通过 backend slot 或 backend id 找后端。
3. 会话保持不是直接绑在 service key 上，而是围绕 `rev_nat_id + client identity` 建立 affinity state。
4. Maglev 不替代 backend map，而是单独维护 lookup table；backend 变化和算法表变化可以分开更新。
5. Socket LB 与 packet LB 分层：
   - socket path 解决本机 connect/sendmsg/recvmsg 的低开销转译
   - packet path 解决非 socket 场景、外部流量、NodePort/LoadBalancer/跨节点 handoff
6. 跨节点转发不是另一套 Service 模型，而是“backend 选中后，根据 forwarding mode 决定 node-local 还是 cross-node handoff”。
7. 健康检查与后端可用性分离。健康检查产出的是 backend 可用视图，而不是直接写死在 frontend 逻辑里。

## 17. Aria 的可落地实现方案

基于上面的参考实现，Aria 的 L4 LB 建议拆成以下 8 个稳定部件。后续代码必须围绕这些部件推进，而不是把 service 功能继续堆进一个泛化的 runtime map。

### 17.1 ServiceFrontendMap

职责：

- 用 `vip + port + proto + scope` 定位一个 service frontend
- 承载 `lb_policy / session_affinity / forwarding_mode / backend_set_id / rev_nat_id`
- 不直接保存健康后端列表

Aria 对应：

- 来源于 `ServiceIR.frontend`
- 当前 `service_catalog + service_frontend_catalog` 是其 shadow 骨架

### 17.2 BackendMemberMap

职责：

- 保存 backend member 的稳定 ID、地址、port、weight、node_id、locality
- 不与 frontend key 混写

Aria 对应：

- 来源于 `BackendSetIR.backends`
- 当前 `backend_member_catalog` 是其 shadow 骨架

### 17.3 ReverseNatMap

职责：

- 记录 service frontend 到后端后的 revnat 索引
- 为 packet path 的返回流恢复 service 语义

Aria 建议：

- 先做 `service_revnat_map` shadow 预留
- 在真正 packet datapath materialization 阶段接入

### 17.4 AffinityMap / AffinityMatchMap

职责：

- `AffinityMap` 记录 `client -> backend` 的粘滞关系
- `AffinityMatchMap` 记录 backend 是否仍属于当前 service 的合法成员

Aria 建议：

- 第一版先只支持 `none` 和 `client_ip`
- 第二版再补 `5tuple`
- 在没有 packet datapath 前，可以先只在 shadow state 中保留结构和预算

### 17.5 MaglevMap

职责：

- 作为独立查表结构维护一致性哈希
- 与 frontend/member map 分离，方便增量重建

Aria 建议：

- 第一版 L4 LB 算法只实现：
  - `random`
  - `maglev`
- `hash_5tuple` 与 `hash_src_ip` 先作为 control-plane 语义保留，可在后续映射到 Maglev 输入或普通哈希

### 17.6 Socket LB Path

职责：

- 处理节点内 connect/sendmsg/recvmsg 的低开销转译
- 优先覆盖 node-local service 命中

Aria 建议：

- 第一阶段先做 TCP/UDP socket connect/sendmsg fast path
- socket path 只负责 frontend 查找、backend 选择、sock revnat
- 如果命中 cross-node backend，只生成 handoff 所需的本地转发表达，不在 socket hook 里硬做复杂跨节点处理

### 17.7 Packet LB Path

职责：

- 处理外部流量、非 socket 场景、NodePort/LoadBalancer/FIP 入口
- 负责真正的包级 rewrite、revnat、跨节点 handoff

Aria 建议：

- 先以 `tc ingress/egress` 为主战场
- 第一阶段不要把 service LB 直接压到 XDP
- 等 packet path 稳定后，再评估是否把早期 frontend 命中或 no-backend fast fail 下沉到 XDP

### 17.8 Forwarding Projection

职责：

- 把 backend 选中结果投影成两类稳定路径：
  - `node_local`
  - `cross_node`

Aria 建议：

- `service_forwarding_projection` 作为正式 runtime family 保留
- 其中必须显式表达：
  - `forwarding_scope`
  - `forwarding_mode`
  - `handoff_target`
  - `needs_revnat`
- `forwarding_mode` 在跨节点场景下应进一步细分为：
  - `cross_node_native`
  - `cross_node_overlay_vxlan`
  - `cross_node_overlay_geneve`
- 在 `Network / Reachability / TunnelEndpoint` 尚未形成正式 encap 对象前，agent 本地 shadow compiler 可以先把 overlay 统一折叠为 `cross_node_overlay` 摘要，不要求提前物化到具体 encap 程序
- 这一步不要与 NAT/FIP 直接耦合；NAT/FIP 只作为后续入口来源，而不是 service datapath 的前置条件

## 18. 具体落地顺序

Aria 的 L4 LB 建议按下面顺序实现，避免一次性同时改 compiler、runtime、packet datapath 和 observability。

### 18.1 第一步：完成 service shadow IR

目标：

- 稳定 `ServiceIR / BackendSetIR / HealthCheckIR`
- 稳定 `service_frontend_catalog / backend_member_catalog / service_forwarding_projection`
- 稳定 `service_intent / service_execution`

状态：

- 这一步已经在进行中

### 18.2 第二步：补第一版 service runtime family

目标：

- `service_frontend_map`
- `backend_member_map`
- `service_revnat_map`（先 shadow 预留）
- `service_affinity_map`（先 shadow 预留）
- `service_maglev_map`（先 shadow 预留）

要求：

- 仍然先不进入真实 datapath
- 先把 map family 和 object count / runtime budget 钉死

### 18.3 第三步：实现 node-local socket LB

目标：

- 对本机 connect/sendmsg/recvmsg 的 service VIP 命中做转译
- 支持 `random / maglev`
- 支持 `session_affinity = none/client_ip`

验收：

- 节点内 service 命中成功转后端
- 本地 runtime execution summary 能解释 backend 选择

### 18.4 第四步：实现 tc packet LB

目标：

- 为非 socket 场景和外部入口提供 packet path
- 接入 `service_revnat_map`
- 生成 `lb_event`

验收：

- 同一 service frontend 在 socket path 与 packet path 上的 backend 选择语义一致

### 18.5 第五步：实现 cross-node forwarding handoff

目标：

- 让 selected backend 为 remote 时，转为 `cross_node` handoff
- 与 Route Domain 协同，但不要求 Route/NAT 先完成全部功能
- `cross_node` handoff 至少支持：
  - `cross_node_native`
  - `cross_node_overlay_vxlan`
  - `cross_node_overlay_geneve`

要求：

- service datapath 必须先能独立表达 cross-node handoff
- 不允许因为 NAT/FIP 未完成而阻塞 service 主路径

### 18.6 第六步：接健康检查与 session affinity 正式运行态

目标：

- 健康检查结果进入 backend 可用视图
- session affinity 进入正式 map/state

### 18.7 第七步：接 ServiceChain 与 Diagnose

目标：

- Service 作为 chain hop 或 chain 入口
- `diagnose` 能输出：
  - 命中了哪个 service
  - 为什么选这个 backend
  - 是 node-local 还是 cross-node
  - 健康状态是否影响了选择

## 19. Aria 当前实现状态

截至 `2026-04-09`，仓库已经完成 `RFC-008` 的第一阶段对象层落地：

- `aria-controller` 已新增实验性的 `Service / BackendSet / HealthCheck` northbound API 骨架
- 当前覆盖 CRUD、OpenAPI、基础分页/过滤、引用完整性校验和删除依赖保护
- `BackendSet` 与 `HealthCheck` 的引用关系、`Service` 与 `BackendSet` 的引用关系，现已由 store mutation 边界原子校验
- controller 当前还会显式拒绝非法 `route_mode`、不存在的 `Backend.port_ref` 目标，以及不包含 listener port 的无效 `Service`
- `routing / NAT / Floating IP` 的预留位不会阻塞该对象层推进
- `southbound desired-state` 现已开始按节点相关网络投影 `Service / BackendSet / HealthCheck`
- `aria-agent` 现已开始在 `services` shadow 域中编译 `Service / BackendSet / HealthCheck`，并把结果写入 `compiled state / reconcile plan / runtime inventory / runtime intent`
- `aria-agent` 当前还会把 `Service / BackendSet / HealthCheck` 进一步收敛成第一版 `ServiceIR / BackendSetIR / HealthCheckIR` shadow skeleton，用于表达 frontend listener、backend member 以及节点内/跨节点转发方向，但仍然不会进入真实 L4 LB datapath
- `services` shadow 域当前还会把 runtime map 规划进一步细化为 `service_frontend_catalog / service_frontend_map / service_socket_lb_projection / service_packet_lb_projection / backend_member_catalog / backend_member_map / service_forwarding_projection / service_revnat_map / service_affinity_map / service_maglev_map`，作为后续节点内与跨节点转发统一 service datapath 的前置边界
- `services` shadow 域的本地 `RuntimeIntent / RuntimeExecutionSummary` 当前也已开始单独保留 listener / frontend runtime entry / socket-lb listener / packet-lb listener / backend member / backend runtime entry / forwarding projection 的细化摘要，并显式区分 `node_local / cross_node_native / cross_node_overlay / cross_node_hybrid` 等方向，以及 revnat / affinity / maglev 的 shadow reservation；其中 reservation 计数已按 frontend listener 粒度统计，但仍然不会进入真实 L4 LB datapath
- `services` shadow 域当前还会额外生成 `socket-selection-plan`，按 internal service listener 产出第一版 node-local socket LB shadow selection 计划，表达 `lb_policy / session_affinity / local backend / remote backend / handoff_required`，用于后续 socket path 的 backend 选择与 handoff 投影，但仍然不会进入真实 L4 LB datapath
- 若节点 `supports_encap = false` 却收到 overlay cross-node forwarding，当前 agent 会显式追加 `overlay_encap_unsupported` degraded reason，而不是把该场景伪装成普通 overlay shadow 计划

当前仍未实现：

- 健康检查执行器
- session affinity 状态
- 节点内转发与跨节点转发的 L4 LB datapath

当前也不打算做的事情包括：

- 为了贴近 Cilium 而重写已完成的 northbound / southbound / shadow compiler 骨架
- 把 L4 LB 直接建模成 Kubernetes `Service` / `NodePort` 兼容层
- 在 L4 LB 尚未落地前，提前让 `routing / NAT / Floating IP` 反客为主
