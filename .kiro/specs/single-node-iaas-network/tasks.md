# 实施任务：单节点 IaaS 网络最小闭环（Phase 3）

基于 [design.md](design.md) 拆分的实施任务。每个任务独立可提交，按依赖顺序排列。

## 任务列表

### Task 1: 定义 repr(C) 结构体（ebpf/src/common.rs + core/src/common.rs）
- [x] 在 `ebpf/src/common.rs` 中新增 `PortIdentityKey`、`PortIdentityValue`、`AntiSpoofKey`、`AntiSpoofValue`、`RouteValue`、`SgRuleKey`、`SgRuleValue` 结构体，全部 `#[repr(C)]` + `unsafe impl Pod`
- [x] 在 `core/src/common.rs` 中同步定义相同的结构体（含 `Plain` trait 实现）
- [x] 新增 `PortIdentityValue.flags` 常量：`PORT_FLAG_ANTI_SPOOF = 1`、`PORT_FLAG_HAS_ALLOWED_PAIRS = 2`
- [x] 新增 `RouteValue.next_hop_type` 常量：`NEXT_HOP_LOCAL_PORT = 0`、`NEXT_HOP_GATEWAY = 1`、`NEXT_HOP_BLACKHOLE = 2`、`NEXT_HOP_HOST = 3`
- [x] 新增 `SgRuleKey.direction` 常量：`SG_DIR_INGRESS = 0`、`SG_DIR_EGRESS = 1`
- [x] 新增 drop_reason 常量：`DROP_PORT_IDENTITY_MISS = 20`、`DROP_ANTI_SPOOF = 21`、`DROP_SG_INGRESS = 22`、`DROP_SG_EGRESS = 23`、`DROP_ROUTE_MISS = 24`、`DROP_ROUTE_BLACKHOLE = 25`
- [x] 扩展 `PipelineCtx` 结构体，在末尾追加 `port_network_id`、`port_segment_id`、`port_sg_id`、`route_id`、`route_next_hop_type`、`port_flags`、`route_egress_ifindex` 字段
- [x] 新增 `PipelineCtx.flags` 常量：`FLAG_PORT_RESOLVED = 1 << 9`、`FLAG_ANTI_SPOOF_PASSED = 1 << 10`、`FLAG_ROUTE_RESOLVED = 1 << 11`

相关设计：design.md §1, §3

### Task 2: 声明 eBPF map 定义（ebpf/src/maps.rs）
- [x] 在 `ebpf/src/maps.rs` 中新增 `PORT_IDENTITY_MAP: HashMap<PortIdentityKey, PortIdentityValue>`，容量 1024
- [x] 新增 `ANTI_SPOOF_MAP: HashMap<AntiSpoofKey, AntiSpoofValue>`，容量 8192
- [x] 新增 `ROUTE_TABLE_V4: LpmTrie<[u8; 8], RouteValue>`，容量 4096
- [x] 新增 `ROUTE_TABLE_V6: LpmTrie<[u8; 20], RouteValue>`，容量 2048
- [x] 新增 `SG_RULE_MAP: HashMap<SgRuleKey, SgRuleValue>`，容量 16384
- [x] 在 maps.rs 顶部 `pub use` 中导出新增的结构体类型

相关设计：design.md §1

### Task 3: 实现 ebpf/src/port.rs — Port 身份查找 + Anti-Spoof
- [x] 创建 `ebpf/src/port.rs` 模块
- [x] 实现 `phase_port_identity(p: &mut PipelineCtx) -> bool`：查找 PORT_IDENTITY_MAP，写入 PipelineCtx 的 port_network_id / port_segment_id / port_sg_id / port_flags
- [x] 实现 `phase_anti_spoof_v4(info: &PacketInfo, p: &PipelineCtx) -> bool`：校验源 MAC（与 PORT_IDENTITY_MAP.mac 比较）+ 源 IP（查找 ANTI_SPOOF_MAP）
- [x] 实现 `phase_anti_spoof_v6(info: &PacketInfo, p: &PipelineCtx) -> bool`：IPv6 版本
- [x] 所有函数标记 `#[inline(always)]`
- [x] 在 `ebpf/src/lib.rs` 中 `mod port;` 声明模块

相关设计：design.md §2.2

### Task 4: 实现 ebpf/src/sg.rs — SecurityGroup 规则匹配
- [x] 创建 `ebpf/src/sg.rs` 模块
- [x] 实现 `sg_check(p: &PipelineCtx, info: &PacketInfo, direction: u8) -> bool`：3 级回退查找（精确 → 协议通配 → 全通配）
- [x] 每级查找构造 SgRuleKey 并查 SG_RULE_MAP，命中则返回 action
- [x] 3 级都未命中时默认 deny（安全组默认拒绝）
- [x] 函数标记 `#[inline(always)]`
- [x] 在 `ebpf/src/lib.rs` 中 `mod sg;` 声明模块

相关设计：design.md §2.4

### Task 5: 实现 ebpf/src/route.rs — L3 路由查找 + 转发
- [x] 创建 `ebpf/src/route.rs` 模块
- [x] 实现 `route_lookup_v4(tap_id: u32, dst_ip: u32) -> Option<RouteValue>`：构造 LPM key 查找 ROUTE_TABLE_V4
- [x] 实现 `route_lookup_v6(tap_id: u32, dst_ip: [u8; 16]) -> Option<RouteValue>`：查找 ROUTE_TABLE_V6
- [x] 实现 `phase_route_forward(ctx: &TcContext, route: &RouteValue, p: &mut PipelineCtx) -> i32`：根据 next_hop_type 执行 redirect（LOCAL_PORT）/ pass（GATEWAY/HOST）/ drop（BLACKHOLE）
- [x] 所有函数标记 `#[inline(always)]`
- [x] 在 `ebpf/src/lib.rs` 中 `mod route;` 声明模块

相关设计：design.md §2.3

### Task 6: 集成到 TC Ingress 流水线（ebpf/src/lib.rs）
- [x] 在 `try_tc_ingress` 中，`load_feature_flags_tc` 之后、LB 查找之前，插入 `phase_port_identity` + `phase_anti_spoof` + `sg_check(ingress)` 阶段
- [x] 在 CT/policy 处理之后、返回之前，插入路由查找 + `sg_check(egress)` + `phase_route_forward` 阶段（仅当 `FLAG_LB_HIT` 未设置时）
- [x] 确保 LB 命中时跳过路由查找
- [x] 确保 conntrack 快速路径跳过安全组检查
- [ ] 验证调用链深度不超过 6 层

相关设计：design.md §4

### Task 7: 实现 core/src/port_ops.rs — Port map 读写操作
- [x] 创建 `core/src/port_ops.rs` 模块
- [x] 实现 `write_port_identity`、`delete_port_identity`
- [x] 实现 `write_anti_spoof_entries`、`clear_anti_spoof_entries`
- [x] 在 `core/src/lib.rs` 中 `pub mod port_ops;` 声明模块

相关设计：design.md §5.1

### Task 8: 实现 core/src/route_ops.rs — Route map 读写操作
- [x] 创建 `core/src/route_ops.rs` 模块
- [x] 实现 `write_route_v4`、`delete_route_v4`、`write_route_v6`、`delete_route_v6`
- [x] 在 `core/src/lib.rs` 中 `pub mod route_ops;` 声明模块

相关设计：design.md §5.2

### Task 9: 实现 core/src/sg_ops.rs — SecurityGroup map 读写操作
- [x] 创建 `core/src/sg_ops.rs` 模块
- [x] 实现 `write_sg_rule`、`delete_sg_rule`、`clear_sg_rules`
- [x] 在 `core/src/lib.rs` 中 `pub mod sg_ops;` 声明模块

相关设计：design.md §5.3

### Task 10: Agent materialize — Port / AntiSpoof / Route / SG 写入 eBPF map
- [x] 在 `platform_agent.rs` 中新增 `PortIdentityIr`、`RouteIr`、`SgRuleIr` 编译 IR 结构
- [x] 在 `compile_desired_state` 中新增 Port → PortIdentityIr + AntiSpoofIr 编译逻辑
- [x] 在 `compile_desired_state` 中新增 RouteTable → RouteIr 编译逻辑
- [x] 在 `compile_desired_state` 中新增 SecurityGroup → SgRuleIr 编译逻辑（按 direction 分离）
- [x] 实现 `materialize_port_maps`：写入 PORT_IDENTITY_MAP + ANTI_SPOOF_MAP
- [x] 实现 `materialize_route_maps`：写入 ROUTE_TABLE_V4 / ROUTE_TABLE_V6
- [x] 实现 `materialize_sg_maps`：写入 SG_RULE_MAP
- [x] 在 agent run loop 中按顺序调用：port → sg → route → service
- [x] 在 apply-status 的 domain_statuses 中为 identity / ports / security / routes 域回报真实 applied/failed 结果

相关设计：design.md §6

### Task 11: 写 RFC-005A 文档（含 Mode B NAT 设计预留）
- [x] 创建 `docs/rfcs/rfc-005a-single-node-iaas-map-schema.md`
- [x] 文档化 Mode A 的 5 个 map schema（PORT_IDENTITY_MAP / ANTI_SPOOF_MAP / ROUTE_TABLE / SG_RULE_MAP）
- [x] 文档化 Mode B 的 NAT_TABLE schema（key 含 proto + port）
- [x] 文档化 Floating IP 一对一 DNAT/SNAT 映射语义
- [x] 文档化 NAT Gateway 共享 SNAT 出口语义
- [x] 文档化 NAT 在流水线中的插入位置和 conntrack 协同
- [x] 文档化 checksum 更新和分片包处理策略
- [x] 文档化 nat_event 事件格式
- [x] 明确标注 Mode B 所有内容在 Phase 3 不落地代码

相关设计：design.md §1.5, §8

### Task 12: 更新文档和 README
- [x] 更新 `docs/iaas-architecture-roadmap.md` 的 Phase 3 实施进度
- [x] 更新 `README.md` 的开发进度章节
- [x] 在 `docs/rfcs/README.md` 中添加 RFC-005A 索引条目
- [x] 提交到 GitHub，由 CI 验证编译

相关设计：全部
