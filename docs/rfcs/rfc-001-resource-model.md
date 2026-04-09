# RFC-001：Aria 资源模型 v1

状态：Draft  
阶段：Phase 0  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 面向 IaaS 平台的资源模型 v1，作为：

- northbound API 的基础
- Controller 内部状态模型的基础
- Agent 编译模型的上游输入
- 权限、配额、审计和事件标签的统一参照

本 RFC 的首要目标不是覆盖所有未来对象，而是先冻结平台最小闭环所需的核心对象。

## 2. 非目标

本 RFC 不解决以下问题：

- BGP / EVPN 协议细节
- 完整 L7 路由模型
- UI 展示模型
- 具体数据库表结构
- southbound 传输协议编码格式

## 3. 设计原则

### 3.1 对象必须面向平台，而不是面向单个功能菜单

对象层必须先于 datapath feature 设计。任何新的防火墙、NAT、LB、trace 功能，都应挂在平台对象下，而不是继续扩张“命令集合式”模型。

### 3.2 对象必须支持多租户边界

除 Node 级基础设施对象外，默认所有对象都需要明确隶属：

- `tenant_id`
- `project_id` 或后续等效隔离单元

### 3.3 对象必须可编译

每个对象都必须可以被 Agent 编译成 datapath 所需状态，而不是只能在控制面停留为抽象资源。

### 3.4 对象必须可观测

资源标识应可被带入事件模型、指标标签、审计日志和诊断结果。

## 4. 分层对象视图

### 4.1 租户与平台对象

- `Tenant`
- `Project`（可选，若后续引入更细资源隔离）
- `Node`
- `AgentRegistration`

### 4.2 网络对象

- `Network`
- `Segment`
- `RouteTable`
- `Route`
- `NextHop`

### 4.3 接入对象

- `Port`
- `Attachment`
- `Instance`
- `AllowedAddressPair`
- `AntiSpoofProfile`

### 4.4 安全对象

- `AddressSet`
- `SecurityGroup`
- `SecurityRule`
- `PolicyProfile`

### 4.5 服务对象

- `Service`
- `BackendSet`
- `Backend`
- `HealthCheck`
- `Chain`

### 4.6 出口与地址对象

- `FloatingIP`
- `NatGateway`
- `EgressPolicy`

## 5. 核心对象定义

### 5.1 Tenant

表示租户边界、配额边界和权限边界。

建议字段：

- `id`
- `name`
- `status`
- `labels`
- `quotas`
- `created_at`
- `updated_at`

### 5.2 Node

表示宿主机节点，是 Agent 和 datapath 的运行位置。

建议字段：

- `id`
- `name`
- `mgmt_address`
- `az`
- `labels`
- `capabilities`
- `agent_version`
- `kernel_version`
- `status`

说明：

- `capabilities` 用于记录 datapath 能力探测结果
- Node 对象是 southbound 调度和分片的基础

### 5.3 Network

逻辑网络域，通常可对应 VPC 或租户网络容器。

建议字段：

- `id`
- `tenant_id`
- `name`
- `network_type`
- `ipv4_enabled`
- `ipv6_enabled`
- `route_mode`
- `status`

说明：

- `network_type` 可支持 `l2`、`l3`、`overlay`
- `route_mode` 预留给 `native`、`overlay`、`hybrid`

### 5.4 Segment

表示 Network 下的二层或三层分段，可对应 subnet、bridge domain 或 segment。

建议字段：

- `id`
- `network_id`
- `name`
- `vni`
- `vlan_id`
- `ipv4_cidr`
- `ipv6_cidr`
- `gateway_ipv4`
- `gateway_ipv6`
- `dhcp_enabled`

### 5.5 Port

Port 是 IaaS 模型中最重要的接入对象之一，表示实例接入网络的逻辑端口。

建议字段：

- `id`
- `tenant_id`
- `network_id`
- `segment_id`
- `instance_id`
- `node_id`
- `mac_address`
- `fixed_ips`
- `security_groups`
- `allowed_address_pairs`
- `anti_spoof_enabled`
- `admin_state_up`
- `status`

说明：

- `Port` 是 anti-spoof、route、NAT、security policy 的汇聚点
- 后续 datapath 最小闭环应围绕 Port 展开

### 5.6 Attachment

Attachment 是 Port 到节点侧实际接口的绑定关系，显式描述逻辑对象到运行时对象的映射。

建议字段：

- `id`
- `port_id`
- `node_id`
- `instance_id`
- `host_iface`
- `peer_iface`
- `ifindex`
- `attach_mode`
- `status`

说明：

- `attach_mode` 可表示 `tap`、`veth`、`macvtap` 等
- Attachment 是 southbound 编译模型的重要输入

### 5.7 Instance

表示虚机或容器等工作负载对象。

建议字段：

- `id`
- `tenant_id`
- `name`
- `node_id`
- `state`
- `ports`
- `metadata`

说明：

- Instance 不承担 datapath 细节，但为事件聚合和诊断提供上层语义

### 5.8 AddressSet

AddressSet 是当前 `group` 的长期演进目标，用于聚合 IP/CIDR 集合。

建议字段：

- `id`
- `tenant_id`
- `name`
- `scope`
- `entries`
- `labels`

说明：

- `scope` 应明确 `tenant`、`network`、`node-local`
- AddressSet 为 SecurityRule、NAT、egress policy 复用

### 5.9 SecurityGroup

策略容器，绑定到 Port 或 Instance。

建议字段：

- `id`
- `tenant_id`
- `name`
- `description`
- `rules`
- `status`

### 5.10 SecurityRule

五元组规则与动作定义。

建议字段：

- `id`
- `security_group_id`
- `direction`
- `ethertype`
- `src_selector`
- `dst_selector`
- `protocol`
- `port_range`
- `action`
- `priority`
- `log_enabled`
- `audit_mode`

说明：

- `src_selector` / `dst_selector` 不应只支持 CIDR，应支持 `AddressSetRef`
- `audit_mode` 是未来平台化 rollout 的关键字段

### 5.11 RouteTable

路由表容器，用于组织和下发路由。

建议字段：

- `id`
- `network_id`
- `name`
- `routes`
- `default_route`

### 5.12 Route

单条前缀路由。

建议字段：

- `id`
- `route_table_id`
- `destination`
- `next_hop_type`
- `next_hop_ref`
- `preference`
- `scope`
- `status`

说明：

- `next_hop_type` 支持 `local`、`node`、`gateway`、`service`

### 5.13 FloatingIP

地址映射对象，用于公网入口或固定外部地址映射。

建议字段：

- `id`
- `tenant_id`
- `address`
- `port_id`
- `fixed_ip`
- `nat_mode`
- `status`

### 5.14 NatGateway

出口 NAT 对象，用于租户或网络级出站转换。

建议字段：

- `id`
- `tenant_id`
- `network_id`
- `egress_addresses`
- `schedule_policy`
- `status`

### 5.15 Service

表示一个虚拟服务入口，长期承载 LB 与链路引流。

建议字段：

- `id`
- `tenant_id`
- `network_id`
- `vip`
- `protocol`
- `ports`
- `backend_set_id`
- `session_affinity`
- `lb_policy`
- `status`

### 5.16 BackendSet 与 Backend

后端集合与后端成员。

建议字段：

- `BackendSet.id`
- `BackendSet.service_id`
- `BackendSet.health_check_id`
- `Backend.id`
- `Backend.backend_set_id`
- `Backend.target_type`
- `Backend.target_ref`
- `Backend.ip`
- `Backend.port`
- `Backend.weight`
- `Backend.status`

### 5.17 Chain

服务链对象，承载有序 hop 和条件性引流。

建议字段：

- `id`
- `tenant_id`
- `name`
- `match_criteria`
- `hops`
- `fail_open`
- `status`

说明：

- 当前仓库中的 `service_chain` 是未来 `Chain` 的原型，但还缺少平台对象边界与租户语义

## 6. 对象关系

推荐的主关系如下：

- `Tenant` 1:N `Network`
- `Network` 1:N `Segment`
- `Network` 1:N `RouteTable`
- `RouteTable` 1:N `Route`
- `Network` 1:N `Port`
- `Port` N:1 `Instance`
- `Port` N:M `SecurityGroup`
- `SecurityGroup` 1:N `SecurityRule`
- `Service` 1:1 `BackendSet`
- `BackendSet` 1:N `Backend`

## 7. 作用域规则

### 7.1 必须有明确作用域的对象

- `AddressSet`
- `SecurityGroup`
- `RouteTable`
- `Chain`

### 7.2 作用域优先级

推荐的作用域优先级：

- global platform
- tenant
- network
- node-local

原则：

- 尽量避免“看上去全局、实际节点本地”的模糊对象
- 任何跨节点对象都必须可被 southbound 拆解

## 8. northbound API 设计约束

所有平台对象都应具备：

- `GET / LIST`
- `CREATE`
- `UPDATE`
- `DELETE`
- `status`

建议路径风格：

- `/api/v1/tenants`
- `/api/v1/networks`
- `/api/v1/ports`
- `/api/v1/security-groups`
- `/api/v1/routes`
- `/api/v1/services`

不建议长期维持“按功能菜单命名”的顶层 API 路径。

## 9. 与当前代码的映射关系

当前对象与未来对象的对应关系：

- `group` -> `AddressSet`
- `policy` -> `SecurityRule`
- `service_chain` -> `Chain`
- `tap / instance` -> `Attachment + Instance`
- `mirror` -> `SecurityRule` 或 `ObserveAction` 的动作配置
- `qos` -> `TrafficPolicyProfile` 或后续 QoS profile

## 10. 当前缺口

当前仓库还缺以下对象级能力：

- `Tenant`
- `Network / Segment`
- `Port / Attachment`
- `RouteTable / Route`
- `FloatingIP`
- `NatGateway`
- `Service / BackendSet / HealthCheck`

## 11. 验收标准

资源模型 v1 完成的验收标准：

- 可以支持单节点 IaaS 最小闭环所需对象
- 可以作为 northbound API 的输入输出模型
- 可以被 Agent 编译成 datapath 所需配置
- 可以为事件模型提供稳定标签与引用

## 12. 后续拆分建议

本 RFC 后续可继续细拆：

- `RFC-001A` Port / Attachment
- `RFC-001B` Route / NextHop
- `RFC-001C` SecurityGroup / Rule
- `RFC-001D` Service / Backend / HealthCheck

## 13. 当前实现状态（2026-04-09）

当前仓库已经基于本 RFC 落下第一批代码骨架：

- `Tenant`
- `Node`
- `Network`
- `Port`
- `SecurityGroup`
- `RouteTable`

当前 controller 已经开始对这批对象施加第一版平台关系约束：

- `Network.tenant_id`、`SecurityGroup.tenant_id` 必须引用已存在的 `Tenant`
- `Port` 会校验 `tenant/network/node/security_group` 的一阶引用，并要求 `Port.tenant_id` 与 `Network.tenant_id` 一致
- `RouteTable.network_id` 必须引用已存在的 `Network`
- 删除 `Tenant / Node / Network / SecurityGroup` 时会先检查是否仍有下游对象引用

这些对象已经进入共享 `aria-api` schema，并通过实验性的 `controller` crate 暴露为 northbound API。

当前仍未落代码的对象包括但不限于：

- `Segment`
- `Attachment`
- `Instance`
- `AddressSet`
- `FloatingIP`
- `NatGateway`
- `Service / Backend / HealthCheck / Chain`
