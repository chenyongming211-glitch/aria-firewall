# Aria Firewall 全面重构计划 — 大文件拆分（最终版）

> 最后更新: 2026-04-14

## Context

项目有多个超大文件（最大 5,793 行），严重影响可维护性。本计划将所有超大文件按职责拆分为模块目录，每个 Phase 独立可编译，逐步推进。

**约束**: 禁止本地编译，每步必须 push 到 CI 验证。

### 通用策略：先标注再搬运

中高风险 Phase（1/2/5/6）采用两步法，将可见性问题和 import 路径问题解耦到不同 commit：

- **Commit A**: 在原文件内批量调整可见性标注（如需要）→ CI 验证
- **Commit B**: 纯搬运到新文件，调整 import 路径 → CI 验证

这样 Commit B 失败时，100% 是 import 路径问题，不需要猜测是可见性还是路径。

低风险 Phase（0/4/7）无需标注步骤，直接搬运。

---

## Phase 0: `api/src/platform.rs` (1,861 行) — 按资源类型拆分 ✅ 已完成

将 `platform.rs` 转为 `platform/` 目录，每个资源类型一个文件：

```
api/src/platform/
├── mod.rs              (~50 行)  pub mod 声明 + re-export + PlatformApiError
├── metadata.rs         (~40 行)  ResourceMetadata, Create/UpdateMetadata
├── tenant.rs           (~80 行)
├── node.rs             (~200 行) 含 NodeCapability, NodeAddress
├── network.rs          (~100 行)
├── port.rs             (~120 行)
├── security_group.rs   (~130 行)
├── route_table.rs      (~120 行)
├── ip_group.rs         (~100 行)
├── network_policy.rs   (~100 行)
├── qos_policy.rs       (~100 行)
├── health_check.rs     (~120 行)
├── backend_set.rs      (~180 行)
├── service.rs          (~120 行)
└── query_types.rs      (~200 行) 所有 *ListQuery 结构体
```

**关键**: `mod.rs` 做 `pub use` 重导出，`api/src/lib.rs` 的 `pub use platform::*` 不变，下游零改动。

---

## Phase 1: `agent/src/platform_agent.rs` (5,793 行) — 按职责拆分 ✅ 已完成

**风险**: 低（仅 main.rs 引用，完全隔离）
**前置**: 无，可与 Phase 0 并行
**提交**: 2 commits（先标注再搬运）

将 `platform_agent.rs` 转为 `platform_agent/` 目录：

```
agent/src/platform_agent/
├── mod.rs                  (~700 行)  PlatformAgentConfig, start(), PlatformAgent run loop
├── ir_types.rs             (~680 行)  ~45 个 IR/plan 结构体定义
├── compiler.rs             (~920 行)  compile_desired_state()
├── ir_builders.rs          (~550 行)  build_route_ir, build_sg_rule_ir, build_service_programs 等
├── planner.rs              (~1,200 行) reconcile_plan + runtime_plan + runtime_inventory + diff
│                                      (这 4 个函数共享 IR 类型、互相引用，合在一起减少跨模块 import)
├── socket_plan.rs          (~260 行)  build_socket_selection_plan（独立性强，单独放）
├── runtime_materialize.rs  (~500 行)  materialize_phase3_maps, materialize_service_maps
├── runtime_intent.rs       (~350 行)  build_runtime_intent, build_runtime_execution_summary
│                                      (独立于 planner.rs 以控制文件大小，planner.rs 已 ~1200 行)
├── southbound_client.rs    (~110 行)  SouthboundClient
├── state_store.rs          (~180 行)  LocalPlatformStateStore
└── helpers.rs              (~170 行)  工具函数
```

**提交策略**:
1. **标注 commit**: 调整内部函数/结构体的可见性 → CI 验证
2. **搬运 commit**: 创建目录，移动代码到子模块 → CI 验证

---

## Phase 2: `controller/src/store.rs` (3,473 行) ✅ 已完成

**风险**: 低-中（ControllerStore trait 保持不变）
**前置**: 无，可与 Phase 1 并行
**提交**: 2 commits（先标注再搬运）

```
controller/src/store/
├── mod.rs              (~200 行)  SharedStore, StoredResource trait, StoreError, ControllerStore trait
├── resource_store.rs   (~200 行)  ResourceStore<T>, PersistedControllerState
├── validation.rs       (~400 行)  所有 ensure_xxx_inner / validate_xxx_inner 方法
├── protection.rs       (~300 行)  所有 ensure_xxx_delete_allowed_inner 方法
├── southbound.rs       (~300 行)  desired_state_for_node + object_counts
├── in_memory.rs        (~1,200 行) InMemoryControllerStore 结构 + impl (含 CRUD)
│                                  (结构定义和 impl 是同一个类型的两半，拆开会增加阅读成本)
└── file_backed.rs      (~500 行)  FileBackedControllerStore + impl ControllerStore
```

**提交策略**:
1. **标注 commit**: 调整方法可见性 → CI 验证
2. **搬运 commit**: 创建目录，分批搬运 → CI 验证

---

## Phase 3: `controller/src/api_handlers.rs` (2,324 行) ✅ 已完成

**风险**: 低（纯 handler，参照 agent 的 api_handlers 模式）
**前置**: Phase 2 完成（handler 依赖 store）
**提交**: 2 commits

```
controller/src/api_handlers/
├── mod.rs              (~120 行)  AppState, ControllerError, re-exports
├── helpers.rs          (~350 行)  error_details, metadata_from_*, parse_*, paginate
├── health.rs           (~30 行)
├── tenant.rs           (~120 行)
├── node.rs             (~130 行)
├── network.rs          (~120 行)
├── port.rs             (~130 行)
├── security_group.rs   (~130 行)
├── route_table.rs      (~120 行)
├── ip_group.rs         (~130 行)
├── network_policy.rs   (~130 行)
├── qos_policy.rs       (~120 行)
├── health_check.rs     (~130 行)
├── backend_set.rs      (~130 行)
└── service.rs          (~130 行)
```

**提交策略**:
1. mod.rs + helpers + 前 3 个资源 handler → CI 验证
2. 剩余所有资源 handler → CI 验证

---

## Phase 4: `api/src/lib.rs` (1,592 行) — 按功能域拆分 ✅ 已完成

**风险**: 低（纯类型定义）
**前置**: Phase 0 完成
**提交**: 1 commit（直接搬运）

这些类型不只有 agent 使用，controller、user crate 也引用，命名 `dataplane/` 准确表达"节点本地 eBPF 面向的类型"，与 `platform/`（Controller 面向的类型）形成对称。

```
api/src/
├── lib.rs              (~50 行)  ApiError, HealthResponse, mod 声明, pub use re-exports
├── platform/           (Phase 0 已拆)
├── southbound.rs       (保持不变)
├── dataplane/
│   ├── mod.rs          re-exports
│   ├── instance.rs     (~60 行)  InstanceInfo, InstancesResponse
│   ├── group.rs        (~160 行) Group 相关类型
│   ├── policy.rs       (~300 行) Policy 相关类型
│   ├── qos.rs          (~150 行)
│   ├── mirror.rs       (~200 行)
│   ├── conntrack.rs    (~30 行)
│   ├── config.rs       (~80 行)
│   ├── stats.rs        (~120 行) StatsOverview, RuleStats, FlowStats, LbStats
│   ├── tcprt.rs        (~150 行)
│   ├── ssl.rs          (~100 行)
│   ├── trace.rs        (~60 行)
│   ├── drops.rs        (~80 行)
│   └── service_chain.rs (~80 行)
└── helpers.rs          (~60 行)  协议/动作/方向转换函数
```

---

## Phase 5: `agent/src/control_plane.rs` (2,406 行) + `agent/src/instance.rs` (1,178 行) ✅ 已完成（instance.rs 保持原样）

**风险**: 中（11 个文件依赖 ControlPlane）
**前置**: Phase 1 完成
**提交**: 5 commits（先标注 + 按功能域逐步提取 + instance 拆分）

利用已有的 `control_plane/` 子目录模式，提取方法组。所有方法都是 `impl ControlPlane` 块，在子模块中写 `impl ControlPlane` 块（Rust 允许跨文件），无需改接口。

```
agent/src/control_plane/
├── mod.rs (即当前 control_plane.rs 缩减到 ~700 行)
│   └── ControlPlane 结构定义, new(), 核心实例管理, compact, chains, conntrack, config
├── observability.rs   (已存在, 250 行)
├── ssl.rs             (已存在, 103 行)
├── trace.rs           (已存在, 71 行)
├── tcprt.rs           (已存在, 33 行)
├── registration.rs    (~450 行) prepare/publish/abort_managed_registration,
│                       register_system/unregister_instance
├── policy_ops.rs      (~350 行) list/add/delete_policy, validate_policy_ports, rollback
├── qos_ops.rs         (~350 行) list/add/delete_qos, rollback
├── mirror_ops.rs      (~300 行) list/add/delete_mirror, get_mirror_stats
└── group_ops.rs       (~250 行) list/add/delete_group, resolve_group_id
```

`instance.rs` 与 ControlPlane 强耦合（ControlPlane 操作 instance，instance 回调 ControlPlane），拆 control_plane 时理清 instance 的职责边界是自然的。

```
agent/src/instance/
├── mod.rs             (~200 行) FirewallInstance 结构定义, 生命周期管理
├── attachment.rs      (~400 行) XDP/TC attach/detach 逻辑
├── runtime.rs         (~300 行) 运行时状态管理
└── persistence.rs     (~280 行) 状态持久化
```

**提交策略**:
1. **标注 commit**: 调整 ControlPlane/instance 方法可见性 → CI 验证
2. 提取 registration + group_ops → CI 验证
3. 提取 policy_ops + qos_ops → CI 验证
4. 提取 mirror_ops → CI 验证
5. 拆分 instance.rs → CI 验证

---

## Phase 6: `ebpf/src/lib.rs` (1,513 行) ✅ 已完成

**风险**: 中-高（eBPF 编译目标不同，只能 CI 验证）
**前置**: 无
**并行**: **必须独立执行，不与任何其他 Phase 并行**
**提交**: 3 commits（先标注 + 逐步提取）

**特别注意**:
- `#[inline(never)]` 的 3-layer 栈预算约束
- BPF verifier 调用深度限制
- cross-file inlining 行为可能变化
- 如果 CI 失败，回滚成本高，需要更多时间排查

```
ebpf/src/
├── lib.rs              (~200 行) 入口声明 + panic handler + SSL uprobe 入口
├── pipeline/
│   ├── mod.rs
│   ├── xdp.rs          (~130 行) xdp_firewall, try_xdp_firewall
│   ├── tc_ingress.rs   (~180 行) tc_ingress, try_tc_ingress
│   ├── tc_egress.rs    (~130 行) tc_egress, try_tc_egress
│   ├── ctx.rs          (~160 行) load_feature_flags, resolve_tap_id, load_runtime_ctx
│   ├── ct_phases.rs    (~350 行) phase_ct_v4/v6, fastpath, miss variants
│   ├── policy_phases.rs (~200 行) phase_policy_xdp/tc, post_accept
│   ├── qos_phases.rs   (~60 行)  phase_qos_ingress/egress_tc
│   └── trace_drop.rs   (~100 行) do_trace, do_drop
└── (现有模块保持不变)
```

**提交策略**:
1. **标注 commit**: 调整函数可见性 → CI 验证（含 eBPF build）
2. 提取 ctx + trace_drop + qos_phases → CI 验证
3. 提取 xdp + tc_ingress + tc_egress + ct_phases + policy_phases → CI 验证

---

## Phase 7: Core crate 小文件拆分 ✅ 已完成

**风险**: 低（仅 agent 依赖 core）
**前置**: 无，可与 Phase 4/5 并行
**提交**: 1 commit（直接搬运）

### 7a. `core/src/common.rs` (875 行) — 评估后无需拆分
类型与常量交织，875 行纯定义文件不值得拆分。

### 7b. `core/src/state.rs` (879 行) ✅
```
core/src/state/
├── mod.rs       re-exports + 私有辅助函数 + 测试
├── types.rs     FirewallState 结构体 + Default + StateManager
└── ops.rs       impl FirewallState + impl StateManager
```

### 7c. `core/src/wal.rs` (1,017 行) ✅
```
core/src/wal/
├── mod.rs       WalEntry 枚举 + 常量 + re-exports + 测试
├── entry.rs     apply_wal_entry + load_with_wal
└── actor.rs     WalWriter + WalClient + WalActor + WalMessage
```

---

## Phase 8: `ebpf/src/common.rs` (944 行) — 评估后无需拆分

**评估结论**: 纯类型定义 + 常量，944 行内部逻辑连贯，拆分收益低、eBPF 编译风险不值得承担。保持原样。

```
ebpf/src/common/
├── mod.rs           re-exports
├── core_types.rs    PolicyKey/Value, CtKey, PipelineCtx 等
├── lb_types.rs      SvcFrontend/Backend/RevNat/Maglev
├── config_types.rs  FirewallConfig, TapConfig, QosConfig 等
├── ssl_types.rs     SslScratch, SslConnValue, SslHttpValue
└── constants.rs     所有常量
```

**不在本次范围内**:
- `ebpf/src/ssl.rs` (1,204 行) — refactor-guardrails §5.3 标记 SSL 为高风险区，verifier 敏感，scratch/helper/bounds 非常脆弱。拆分收益小、风险高，只在有明确需求时再做。

---

## 执行总览

```
Phase 0 (api/platform.rs)        ✅ 1 commit
Phase 1 (agent/platform_agent)   ✅ 2 commits
Phase 2 (controller/store)       ✅ 2 commits
Phase 3 (controller/api_handlers)✅ 2 commits
Phase 4 (api/lib.rs → dataplane/)✅ 1 commit
Phase 5 (control_plane)          ✅ 4 commits（instance.rs 保持原样）
Phase 6 (ebpf/lib.rs)            ✅ 5 commits
Phase 7a (core/common.rs)        — 跳过（类型常量交织，不值得拆）
Phase 7b (core/state.rs)         ✅ 1 commit
Phase 7c (core/wal.rs)           ✅ 1 commit
Phase 8 (ebpf/common.rs)         — 跳过（纯类型，eBPF 风险不值得）

总提交数: 19 commits，全部 CI 通过
```

## 验证方式

每个 commit push 后检查 GitHub Actions CI:
1. `cargo check` / `cargo build` — workspace 级别编译
2. eBPF target build — bpf-linker 编译
3. 确认无 `unused import` 警告（`pub use` re-export 正确）

## 回滚方案

每个 Phase = 一个或多个独立 commit，可 `git revert` 独立回滚。

## 风险评估

| Phase | 风险 | 原因 |
|-------|------|------|
| 0 | 极低 | 纯类型定义，无行为 |
| 1 | 低 | 仅 main.rs 引用，完全隔离 |
| 2 | 低-中 | ControllerStore trait 不变，in_memory impl 需仔细拆分 |
| 3 | 低 | 纯 handler，参照已有 agent api_handlers 模式 |
| 4 | 低 | 纯类型定义 |
| 5 | 中 | 11 个文件依赖 ControlPlane；instance.rs 与 ControlPlane 强耦合 |
| 6 | 中-高 | eBPF verifier 调用深度、inline(never) 栈预算、cross-file inlining |
| 7 | 低 | 仅 agent 依赖 core |
| 8 | 中 | eBPF 目标，但纯类型拆分风险可控 |

## 明确不拆分的文件

| 文件 | 行数 | 原因 |
|------|------|------|
| `core/src/common.rs` | 875 | 纯类型定义，类型与常量交织，不值得拆 |
| `ebpf/src/common.rs` | 944 | 纯类型定义，eBPF 编译风险不值得 |
| `ebpf/src/ssl.rs` | 1,204 | refactor-guardrails §5.3 标记高风险，verifier 敏感 |
| `agent/src/instance.rs` | 1,178 | 与 ControlPlane 强耦合，职责边界不够清晰 |
