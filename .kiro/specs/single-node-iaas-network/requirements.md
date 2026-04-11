# 需求文档：单节点 IaaS 网络最小闭环（Phase 3）

## 简介

本文档定义 Aria Firewall 项目 Phase 3 的需求：在单节点范围内实现 IaaS 网络最小可行闭环。Phase 3 建立在 Phase 1（Controller 骨架 + 9 种资源）和 Phase 2（Agent shadow compiler + ServiceIR + L4 LB 完整实现）之上，补齐 Port 绑定、Anti-Spoof、L3 路由、SecurityGroup 分段检查、NAT/EIP 设计预留以及路由/NAT 事件可观测等基础 IaaS 数据面能力。

Phase 3 分为两个模式：
- **Mode A（无 EIP）**：落地代码实现，覆盖 Port 绑定、Anti-Spoof、L3 路由、SecurityGroup 分段检查
- **Mode B（有 EIP）**：仅设计文档，不落地代码，覆盖 SNAT/DNAT/Floating IP/NAT Gateway

上游文档：
- [RFC-005 Routing / NAT 基础数据面 v1](../../../docs/rfcs/rfc-005-routing-nat-datapath.md)
- [RFC-008A Service LB Map Schema v1](../../../docs/rfcs/rfc-008a-service-lb-map-schema.md)
- [Aria eBPF 实现约束](../../../docs/ebpf-implementation-constraints.md)
- [Aria 重构护栏](../../../docs/refactor-guardrails.md)
- [IaaS 网络架构原则与演进路线](../../../docs/iaas-architecture-roadmap.md)

## 术语表

- **Agent**：运行在每个节点上的编译器与执行器，负责将 Desired State 编译为 Runtime State 并写入 eBPF map
- **Controller**：中央控制面，管理资源 CRUD 和配置下发
- **Port**：实例接入点，绑定 MAC 地址、IP 地址、所属网段和租户信息
- **Anti_Spoof**：源地址欺骗防护机制，校验出站包的源 MAC/IP 是否与 Port 绑定一致
- **SecurityGroup**：安全组，包含一组入向/出向安全规则
- **RouteTable**：路由表，包含一组前缀到下一跳的映射
- **LPM**：Longest Prefix Match，最长前缀匹配
- **PORT_IDENTITY_MAP**：eBPF HashMap，存储 tap_id 到 Port 元数据的绑定关系
- **ANTI_SPOOF_MAP**：eBPF HashMap，存储允许的源 MAC/IP 组合用于反欺骗校验
- **ROUTE_TABLE**：eBPF LpmTrie，存储目的前缀到下一跳的路由条目
- **RouteValue**：路由查找结果结构体，对齐到 32 字节，包含 priority 字段
- **SG_RULE_MAP**：eBPF HashMap，存储带 `direction` 字段的安全组规则
- **NAT_TABLE**：eBPF HashMap，存储 NAT 映射条目（Mode B 设计预留）
- **EIP**：Elastic IP / Floating IP，弹性公网地址
- **SNAT**：Source NAT，源地址转换
- **DNAT**：Destination NAT，目的地址转换
- **NAT_Gateway**：共享出口 SNAT 对象
- **tap_id**：共享 runtime 下的命名空间隔离标识符
- **scratch_map**：per-CPU PerCpuArray，用于避免在 eBPF 栈上分配大对象
- **Materialize**：Agent 将编译后的状态写入 eBPF map 的过程
- **TC_Ingress**：Linux TC ingress hook，数据面入向主处理点
- **TC_Egress**：Linux TC egress hook，数据面出向主处理点
- **Mode_A**：无 EIP 模式，本阶段落地代码实现
- **Mode_B**：有 EIP 模式，本阶段仅设计文档不落地代码

## 需求

### 需求 1：Port 绑定与身份管理

**用户故事：** 作为平台运维人员，我希望每个实例端口在数据面有明确的身份绑定，以便后续的 Anti-Spoof、路由和安全组检查能基于端口身份进行决策。

#### 验收标准

1. WHEN Agent 收到包含 Port 对象的 Desired State，THE Agent SHALL 将每个 Port 的元数据（MAC 地址、固定 IP 列表、网段 ID、租户 ID、anti_spoof 开关）编译为 PortIdentityValue 结构体并写入 PORT_IDENTITY_MAP
2. THE PORT_IDENTITY_MAP SHALL 使用 tap_id 作为 key 的一部分以实现命名空间隔离
3. THE PortIdentityValue 结构体 SHALL 使用 `#[repr(C)]` 布局，同时在 `ebpf/src/common.rs` 和 `core/src/common.rs` 中定义
4. WHEN 数据面收到一个包，THE eBPF_Pipeline SHALL 通过 PORT_IDENTITY_MAP 查找入向端口的身份信息，并将结果写入 per-CPU scratch 上下文
5. IF PORT_IDENTITY_MAP 查找未命中，THEN THE eBPF_Pipeline SHALL 丢弃该包并记录 drop_event（原因为 port_identity_miss）
6. WHEN Port 对象被删除，THE Agent SHALL 从 PORT_IDENTITY_MAP 中移除对应条目
7. WHEN Port 对象被更新（MAC 地址、IP 列表或安全组变更），THE Agent SHALL 原子更新 PORT_IDENTITY_MAP 中对应条目
8. THE PortIdentityValue 结构体大小 SHALL 不超过 64 字节以控制 eBPF map value 的栈压力

### 需求 2：Anti-Spoof 执行

**用户故事：** 作为平台安全管理员，我希望每个端口默认启用源地址欺骗防护，以防止实例伪造源 MAC 或源 IP 发送流量。

#### 验收标准

1. THE Anti_Spoof 检查 SHALL 默认对所有 Port 启用
2. WHEN 数据面在 TC_Ingress 收到实例发出的包，THE eBPF_Pipeline SHALL 校验源 MAC 地址是否与 PORT_IDENTITY_MAP 中绑定的 MAC 一致
3. WHEN 数据面在 TC_Ingress 收到实例发出的包，THE eBPF_Pipeline SHALL 校验源 IP 地址是否在 ANTI_SPOOF_MAP 允许的 IP 列表中
4. IF 源 MAC 或源 IP 校验失败，THEN THE eBPF_Pipeline SHALL 立即丢弃该包
5. IF 源 MAC 或源 IP 校验失败，THEN THE eBPF_Pipeline SHALL 生成 drop_event，原因标识为 anti_spoof_violation
6. WHEN Port 配置了 allowed_address_pairs，THE Agent SHALL 将额外的 MAC/IP 对写入 ANTI_SPOOF_MAP
7. THE ANTI_SPOOF_MAP SHALL 使用 tap_id 作为 key 的一部分以实现命名空间隔离
8. WHERE Port 的 anti_spoof 开关被显式关闭，THE eBPF_Pipeline SHALL 跳过该端口的 Anti-Spoof 校验

### 需求 3：L3 路由与 LPM 前缀匹配

**用户故事：** 作为平台运维人员，我希望同节点不同实例之间的流量能基于路由表进行 L3 转发，以实现受控的实例互通。

#### 验收标准

1. WHEN Agent 收到包含 RouteTable 对象的 Desired State，THE Agent SHALL 将每条路由的目的前缀和下一跳信息编译为 RouteValue 结构体并写入 ROUTE_TABLE（LpmTrie 类型）
2. THE ROUTE_TABLE SHALL 使用 LpmTrie 实现最长前缀匹配
3. THE RouteValue 结构体 SHALL 对齐到 32 字节，并包含 priority 字段用于同前缀多路由优先级选择
4. WHEN 数据面完成 Anti-Spoof 和入向安全组检查后，THE eBPF_Pipeline SHALL 对目的 IP 执行 ROUTE_TABLE LPM 查找
5. IF ROUTE_TABLE 查找未命中，THEN THE eBPF_Pipeline SHALL 丢弃该包并生成 drop_event（原因为 route_miss）
6. WHEN ROUTE_TABLE 查找命中且下一跳类型为本地端口，THE eBPF_Pipeline SHALL 将包转发到目标端口的 ifindex
7. THE ROUTE_TABLE 的 key SHALL 包含 tap_id 以实现命名空间隔离
8. WHEN RouteTable 对象被更新或删除，THE Agent SHALL 同步更新或移除 ROUTE_TABLE 中对应的路由条目
9. THE eBPF_Pipeline 中的路由查找函数 SHALL 使用 `#[inline(always)]` 标记以满足热路径内联要求

### 需求 4：SecurityGroup 分段检查（入向 pre-route + 出向 post-route）

**用户故事：** 作为平台安全管理员，我希望安全组规则在路由前后分别执行入向和出向检查，以实现更精确的安全策略控制。

#### 验收标准

1. THE SecurityGroup 检查 SHALL 拆分为入向（pre-route）和出向（post-route）两个阶段
2. WHEN 数据面在 Anti-Spoof 校验通过后，THE eBPF_Pipeline SHALL 执行入向安全组检查（`SG_RULE_MAP` 中 `direction=ingress` 的查找），在路由查找之前完成
3. WHEN 数据面完成路由查找并确定出向端口后，THE eBPF_Pipeline SHALL 执行出向安全组检查（`SG_RULE_MAP` 中 `direction=egress` 的查找）
4. IF 入向安全组检查拒绝该包，THEN THE eBPF_Pipeline SHALL 立即丢弃该包并生成 drop_event（原因为 sg_ingress_deny）
5. IF 出向安全组检查拒绝该包，THEN THE eBPF_Pipeline SHALL 丢弃该包并生成 drop_event（原因为 sg_egress_deny）
6. THE SG_RULE_MAP SHALL 使用 tap_id 作为 key 的一部分以实现命名空间隔离
7. WHEN Agent 收到包含 SecurityGroup 对象的 Desired State，THE Agent SHALL 将安全规则按方向分别编译写入 SG_RULE_MAP
8. WHILE 已建立连接存在 conntrack 条目，THE eBPF_Pipeline SHALL 通过 conntrack 快速路径跳过重复的安全组规则匹配
9. WHEN SecurityGroup 规则被更新或删除，THE Agent SHALL 同步更新或移除 SG_RULE_MAP 中对应条目

### 需求 5：数据面处理流水线顺序

**用户故事：** 作为平台架构师，我希望数据面处理流水线有明确且固定的阶段顺序，以确保 Anti-Spoof、安全组、路由和 NAT 之间的协同语义正确。

#### 验收标准

1. THE eBPF_Pipeline 的入向处理顺序 SHALL 固定为：Port 身份查找 → Anti-Spoof 校验 → 入向安全组检查 → Conntrack 查询 → 路由查找 → 出向安全组检查 → 转发
2. THE eBPF_Pipeline 中每个阶段 SHALL 作为独立的 inline helper 函数实现，单个函数栈使用不超过 256 字节
3. THE eBPF_Pipeline 从入口到最深叶子的调用链深度 SHALL 不超过 6 层
4. THE eBPF_Pipeline SHALL 使用 PIPE_SCRATCH（per-CPU PerCpuArray）在阶段之间传递上下文，不在栈上分配大于 64 字节的临时对象
5. THE eBPF_Pipeline SHALL 在 PipelineCtx 中新增路由和安全组相关的状态字段（matched_route_id、sg_verdict 等）
6. IF 任何阶段产生 drop 决策，THEN THE eBPF_Pipeline SHALL 立即终止后续阶段并返回 drop

### 需求 6：路由与安全事件可观测

**用户故事：** 作为平台运维人员，我希望路由决策、安全组判定和 Anti-Spoof 违规都能产生可观测事件，以便进行故障排查和审计。

#### 验收标准

1. WHEN 路由查找完成，THE eBPF_Pipeline SHALL 生成 route_event，包含匹配的路由 ID、下一跳类型和出向 ifindex
2. WHEN Anti-Spoof 校验失败，THE eBPF_Pipeline SHALL 生成 drop_event，包含违规的源 MAC、源 IP 和端口 ID
3. WHEN 安全组检查拒绝流量，THE eBPF_Pipeline SHALL 生成 drop_event，包含匹配的安全组 ID、规则方向和拒绝原因
4. THE route_event 和 drop_event 的 payload SHALL 使用 scratch_map 暂存，不在 eBPF 栈上直接构建大于 64 字节的事件结构
5. THE route_event SHALL 复用现有 TraceEvent/TraceEventV6 的 envelope 格式，新增 route 相关字段
6. WHEN 路由查找未命中，THE eBPF_Pipeline SHALL 生成 drop_event（原因为 route_miss），包含目的 IP 和入向端口 ID

### 需求 7：eBPF Map Schema 定义（Mode A）

**用户故事：** 作为数据面开发者，我希望 Phase 3 新增的 eBPF map 结构体有明确的 schema 定义，以确保 Agent 写入和 eBPF 查找的 ABI 一致性。

#### 验收标准

1. THE PORT_IDENTITY_MAP、ANTI_SPOOF_MAP、ROUTE_TABLE、SG_RULE_MAP 的 key/value 结构体 SHALL 同时在 `ebpf/src/common.rs` 和 `core/src/common.rs` 中定义
2. THE 所有新增结构体 SHALL 使用 `#[repr(C)]` 布局并实现 `unsafe impl Pod`
3. THE 所有新增 map key SHALL 包含 tap_id 字段作为第一个成员
4. THE 所有新增 map 定义 SHALL 在 `ebpf/src/maps.rs` 中声明
5. THE RouteValue 结构体 SHALL 对齐到 32 字节并包含 priority 字段、next_hop_type 字段和 egress_ifindex 字段
6. THE 所有新增 map key 大小 SHALL 控制在 32 字节以内
7. THE 所有新增 map value 大小 SHALL 控制在 64 字节以内

### 需求 8：Agent Materialize 流程（Port / Route / SecurityGroup）

**用户故事：** 作为平台开发者，我希望 Agent 能将 Port、RouteTable 和 SecurityGroup 对象编译并写入对应的 eBPF map，以完成从控制面到数据面的闭环。

#### 验收标准

1. WHEN Agent 编译 Desired State，THE Agent SHALL 将 Port 对象编译为 PORT_IDENTITY_MAP 和 ANTI_SPOOF_MAP 的写入条目
2. WHEN Agent 编译 Desired State，THE Agent SHALL 将 RouteTable 对象编译为 ROUTE_TABLE 的写入条目
3. WHEN Agent 编译 Desired State，THE Agent SHALL 将 SecurityGroup 对象按方向拆分编译为 SG_RULE_MAP 的写入条目
4. THE Agent 的 Materialize 流程 SHALL 在 RuntimePlan 中为 identity/ports/security/routes 域分别生成 MapPlanEntry
5. THE Agent 的 Materialize 流程 SHALL 在 RuntimeInventory 中为新增的 map family 生成对应的 inventory 条目
6. WHEN Desired State 的 generation 发生变化，THE Agent SHALL 执行增量 reconcile，仅更新变化的 map 条目
7. THE Agent SHALL 在 apply-status 中为 identity、ports、security、routes 域分别回报 domain_status
8. IF Port 编译失败（缺少必要字段），THEN THE Agent SHALL 在 apply-status 中标记该 Port 为 failed 并记录失败原因

### 需求 9：与现有 LB 数据面的集成

**用户故事：** 作为平台架构师，我希望 Phase 3 的路由和安全组数据面能与已有的 L4 LB 数据面正确协同，以避免功能冲突。

#### 验收标准

1. THE eBPF_Pipeline SHALL 在 LB 查找之前完成 Port 身份查找和 Anti-Spoof 校验
2. WHILE PipelineCtx.flags 中 FLAG_LB_HIT 被设置，THE eBPF_Pipeline SHALL 跳过常规路由查找，使用 LB 选择的后端地址作为转发目标
3. THE Phase 3 新增的 map（PORT_IDENTITY_MAP、ANTI_SPOOF_MAP、ROUTE_TABLE、SG_RULE_MAP）SHALL 与现有 SVC_FRONTEND_MAP 等 LB map 在同一个 pin namespace 下共存
4. THE PipelineCtx 结构体 SHALL 新增 port_identity_resolved 和 route_resolved 标志位，不破坏现有 flags 字段的已用位
5. THE Phase 3 新增功能 SHALL 不修改现有 SVC_FRONTEND_MAP、SVC_BACKEND_MAP、SVC_REVNAT_MAP 等 LB map 的 schema

### 需求 10：Mode B — EIP/NAT 设计预留（仅设计文档，不落地代码）

**用户故事：** 作为平台架构师，我希望 Floating IP、SNAT 和 NAT Gateway 的数据面设计在 Phase 3 完成文档化，以便后续阶段能直接进入实现。

#### 验收标准

1. THE Mode_B 设计文档 SHALL 定义 NAT_TABLE 的 key/value 结构体，key 中包含 proto 和 port 字段以支持端口级 DNAT
2. THE Mode_B 设计文档 SHALL 定义 Floating IP 的一对一 DNAT/SNAT 映射语义
3. THE Mode_B 设计文档 SHALL 定义 NAT Gateway 的共享 SNAT 出口语义
4. THE Mode_B 设计文档 SHALL 定义 NAT 操作在数据面流水线中的插入位置（路由查找之后、出向安全组检查之前）
5. THE Mode_B 设计文档 SHALL 定义 NAT 与 conntrack 的协同语义（NAT 绑定引用存储在 conntrack 条目中）
6. THE Mode_B 设计文档 SHALL 定义 checksum 更新和分片包处理策略
7. THE Mode_B 设计文档 SHALL 明确标注所有 NAT 相关结构体和 map 在 Phase 3 不落地代码，仅作为 RFC 补充
8. THE Mode_B 设计文档 SHALL 定义 nat_event 的事件格式，包含原始五元组、转换后五元组和 NAT 类型

### 需求 11：eBPF 实现约束合规

**用户故事：** 作为数据面开发者，我希望 Phase 3 所有新增 eBPF 代码严格遵守项目的 eBPF 实现约束，以确保 verifier 通过和跨内核兼容性。

#### 验收标准

1. THE Phase 3 新增的每个 eBPF 函数的栈使用 SHALL 不超过 256 字节（软上限），绝不超过 512 字节（硬上限）
2. THE Phase 3 新增的 eBPF 调用链从入口到最深叶子 SHALL 不超过 6 层（软上限），绝不超过 8 层（硬上限）
3. THE Phase 3 SHALL 不引入 tail call
4. THE Phase 3 新增的大于 64 字节的临时对象 SHALL 使用 per-CPU scratch map 存储，不放在函数栈上
5. THE Phase 3 新增的热路径 helper 函数 SHALL 使用 `#[inline(always)]` 标记
6. THE Phase 3 新增的所有循环 SHALL 有 verifier 可证明的固定小上界
7. THE Phase 3 新增的所有报文读取 SHALL 在读取前执行本地 bounds check
