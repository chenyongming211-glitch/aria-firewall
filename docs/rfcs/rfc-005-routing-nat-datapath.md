# RFC-005：Routing / NAT 基础数据面 v1

状态：Draft  
阶段：Phase 1 / Phase 3 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

实现约束参考：[Aria eBPF 实现约束](../ebpf-implementation-constraints.md)

## 1. 目标

定义 Aria 在 IaaS 场景下的第一版基础网络数据面闭环。

本 RFC 的目标不是直接覆盖多节点大规模云网络，而是先定义单节点最小 IaaS 闭环：

- Port attachment
- anti-spoof
- L3 routing
- stateful firewall
- SNAT / DNAT / Floating IP
- 与 conntrack 的一致协同
- route / nat / drop 事件输出

## 2. 非目标

本 RFC 不覆盖：

- 多节点 overlay 织网
- BGP / EVPN
- ECMP 大规模路由优化
- L7 代理与网关
- 完整负载均衡实现细节

## 3. 设计原则

### 3.1 先做单节点闭环，再做多节点扩展

如果单节点的 Port、Route、NAT 语义都没有完全收口，直接进入多节点只会放大复杂度。

### 3.2 Routing / NAT 必须与安全和观测协同设计

Routing / NAT 不是独立功能。必须同时考虑：

- anti-spoof
- stateful firewall
- conntrack
- event emission

### 3.3 先支持最清晰的 IPv4 主路径

v1 优先把 IPv4 主路径定义清楚。IPv6、双栈和高级扩展作为后续增强，不阻塞第一版闭环。

### 3.4 route 决策和 nat 决策必须可观测

任何转发与转换都应产生可解释的证据：

- 查到了哪张表
- 选了哪个 next hop
- 是否命中 FIP
- 是否发生 SNAT / DNAT

### 3.5 Routing / NAT 主路径必须优先满足 verifier 约束

Routing / NAT 是未来 datapath 最容易推高复杂度的部分。实现时必须优先满足：

- 不引入大栈对象
- 不引入深调用链
- 不引入无法证明上界的循环
- 不为“优雅抽象”牺牲 verifier 可通过性

具体门槛见 [Aria eBPF 实现约束](../ebpf-implementation-constraints.md)。

### 3.6 Routing / NAT / FloatingIP 预留位不得阻塞 L4 LB 与 Service Chain

在 `routing / NAT / FloatingIP` 仍处于接口预留、shadow 骨架或单节点实现阶段时，不得以它们为前置条件阻塞 `L4 负载均衡` 与 `service chain` 的主路径推进。

换句话说：

- `Route / NAT / FIP` 的预留位可以先存在
- 但 `Service / Backend / Chain` 的对象层和 datapath 主路径必须保持独立推进能力
- 特别是 L4 负载均衡的节点内转发与跨节点转发语义，不应被迫等待 NAT/FIP 功能“完全做完”

## 4. 数据面目标场景

### 4.1 场景 A：实例互通

同节点不同实例之间基于路由和安全组互通。

### 4.2 场景 B：实例出公网

实例访问外部网络，经过 route + SNAT / NAT Gateway。

### 4.3 场景 C：公网入实例

外部地址访问 Floating IP，经 DNAT / FIP 映射进入实例。

### 4.4 场景 D：节点本地服务互通

实例与节点本地服务或主机网络对象之间按受控语义互通。

## 5. 基础对象输入

本 RFC 假设以下对象已存在并可下发到节点：

- Port
- Attachment
- SecurityGroup / Rule
- RouteTable / Route / NextHop
- FloatingIP
- NatGateway

## 6. Hook 选择

### 6.1 主路径

建议：

- `TC ingress`：实例侧入向分类与路由前处理
- `TC egress`：出向转发、NAT、encap 前主处理

### 6.2 可选前置优化

`XDP` 可用于：

- 早期 anti-spoof
- 明显非法包早丢弃
- 简单 fast drop

但 v1 不以 XDP 作为 Routing / NAT 主语义承载点。

## 7. 基础运行时表

建议的 runtime 数据结构分组如下：

### 7.1 Port 表

记录：

- port local id
- ifindex
- mac
- fixed ips
- segment id
- tenant id
- anti-spoof profile

### 7.2 Route 表

记录：

- destination prefix
- route id
- next hop id
- egress port / iface
- route scope

### 7.3 NextHop 表

记录：

- next hop id
- type
- dst ip
- out ifindex
- rewrite metadata

### 7.4 AntiSpoof 表

记录：

- allowed mac
- allowed ips
- allowed address pairs

### 7.5 Conntrack 表

记录：

- flow tuple
- reverse tuple
- timeout
- nat binding reference
- verdict cache

### 7.6 NAT 表

记录：

- nat id
- type
- original tuple or address
- translated tuple or address
- direction

### 7.7 FloatingIP 表

记录：

- public ip
- private ip
- target port id
- nat mode

## 8. 数据面处理顺序

### 8.1 实例发出的包

建议顺序：

1. 识别 ingress attachment / port
2. anti-spoof 校验
3. 安全策略 ingress / egress 侧判定
4. conntrack 查询
5. route lookup
6. 必要时执行 SNAT / FIP 出向映射
7. 更新 conntrack
8. emit route / nat / flow 事件
9. 转发

### 8.2 节点收到的外部包

建议顺序：

1. 基础 ingress classify
2. 查 FIP / DNAT 规则
3. route lookup / local delivery 判定
4. 安全策略判定
5. conntrack 更新
6. emit route / nat / flow 事件
7. 投递到实例 attachment

### 8.3 已建立连接

已建立连接可走 fast path，但必须保留：

- ct state 检查
- NAT 反向映射
- 基本统计
- 必要时事件采样

## 9. Anti-Spoof 模型

每个 Port 默认启用 anti-spoof。

允许的源属性包括：

- port 绑定 MAC
- fixed IPs
- allowed address pairs

违反 anti-spoof 的流量应：

- 立即丢弃
- 生成 `drop_event`
- 标识为 `anti_spoof_violation`

## 10. Route 决策模型

### 10.1 路由输入

路由查找至少考虑：

- destination IP
- ingress port
- network / segment
- policy route profile（后续扩展）

### 10.2 路由输出

路由决策结果应至少包括：

- matched route id
- next hop type
- egress ifindex
- local delivery / forward / drop verdict

### 10.3 不可路由流量

不可路由流量应：

- 明确 drop
- 生成 `route_event` + `drop_event`

## 11. NAT 模型

### 11.1 NAT 类型

v1 至少支持：

- `snat`
- `dnat`
- `fip_1to1`

### 11.2 FIP 模型

Floating IP 视为一对一的 DNAT / SNAT 组合。

要求：

- 外部访问时 DNAT 到固定地址
- 回程时按 conntrack 做反向 SNAT

### 11.3 NAT Gateway 模型

NatGateway 视为共享的 SNAT 出口对象。

v1 可先简化为：

- 单节点
- 单地址或小地址池
- 无复杂会话迁移

### 11.4 checksum 与分片

NAT 改写必须明确处理：

- checksum 更新
- 分片包策略
- MTU / MSS 风险说明

## 12. 与安全策略的顺序关系

建议原则：

- anti-spoof 先于 security rule
- 对原始流还是转换后流做策略匹配必须固定并文档化

v1 建议：

- 入向公网访问 FIP 时，DNAT 后再按实例侧安全语义判断
- 出向流量以原始实例身份做安全判断，再做 SNAT

## 13. 事件输出

Routing / NAT v1 应新增并稳定输出：

- `route_event`
- `nat_event`
- `drop_event`
- `flow_event`

建议字段：

- route id
- next hop id
- nat id
- original tuple
- translated tuple
- reason

## 14. Agent 编译要求

Agent 编译器应能把以下对象投影到 runtime：

- Port -> Port 表 + AntiSpoof 表
- RouteTable -> Route 表 + NextHop 表
- FloatingIP -> FIP / NAT 表
- NatGateway -> SNAT 规则

## 15. 当前代码映射与缺口

当前仓库已有：

- 基础防火墙、conntrack、qos、mirror、trace、drops

当前缺少：

- route domain
- next hop domain
- nat domain
- port / attachment 平台对象
- route / nat 事件模型

## 16. 第一阶段验收标准

Routing / NAT v1 的验收标准：

- 同节点实例互通可由 RouteTable 控制
- anti-spoof 默认生效
- 实例可通过 SNAT 访问外部地址
- 外部地址可通过 FIP 访问实例
- 所有异常路径都有可解释事件

## 17. 后续拆分建议

- `RFC-005A` 单节点 IaaS map schema（Port / Anti-Spoof / Route / SecurityGroup + Mode B NAT 预留）
- `RFC-005B` FloatingIP / NAT Gateway
- `RFC-005C` Multi-node overlay / native routing
