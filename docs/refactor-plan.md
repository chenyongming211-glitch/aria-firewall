# Code Refactor Plan (Revised)

基于代码实际依赖关系的重构方案。

## 风险评估（修正后）

| 文件 | 风险 | 原因 |
|------|------|------|
| `agent/src/api_handlers.rs` | 低→中 | 路由边界清楚，但有共享 helper 和大 metrics 逻辑 |
| `user/src/main.rs` | 低→中 | trace/tcprt/diagnose 有大量共享渲染和聚合逻辑 |
| `core/src/ebpf_ops.rs` | 中→高 | 核心底座，跨 agent/core 被广泛调用 |
| `agent/src/control_plane.rs` | 高 | 但可以先抽"只读域"，不碰 WAL 和核心写路径 |

## 执行顺序

```
Phase 1: api_handlers.rs   ← 路由已天然分组，边界现成
Phase 2: user/main.rs      ← 只抽复杂工作流，不按命令平铺
Phase 3: control_plane.rs  ← 只抽只读域，不动写路径
Phase 4: ebpf_ops.rs       ← 内部模块化，对外 API 不变
Phase 5: control_plane.rs  ← 写路径和锁粒度（需测试覆盖）
```

---

## Phase 1: agent/src/api_handlers.rs

**风险**：低→中

**原因**：
- 路由边界清楚（见 api_routes.rs）
- 但有共享 helper（err_response, legacy_drop_headers）
- metrics 逻辑特别大，需要单独文件

**目标结构**：
```
agent/src/api_handlers/
├── mod.rs           # re-exports
├── common.rs        # err_response, legacy_drop_headers, 字符串转换 helpers
├── metrics.rs       # /metrics + 它的 helper（单独，因为被多模块引用）
├── health.rs        # health
├── system.rs        # list_instances, system_start, system_stop
├── groups.rs        # add/list/delete_group, list_groups_with_stats
├── policies.rs      # add/delete/list_policies, batch_add_policies
├── qos.rs           # add/delete/list_qos, list_qos_with_stats
├── mirror.rs        # add/delete/list_mirror, list_mirror_with_stats
├── stats.rs         # stats_overview, stats_rules/flows/qos/groups/mirror
├── conntrack.rs     # list/flush_conntrack
├── tcprt.rs         # list/filter/batch_query/flush_tcprt, histogram, states
├── trace.rs         # start/stop/list/flush_trace
├── ssl.rs           # list/flush_ssl*, list/flush_ssl_http*, errors
├── drops.rs         # list/flush_drops, list/flush_kernel_drops
└── chains.rs        # create/list/get/delete_chain
```

**关键点**：
- `common.rs` 放所有共享 helper
- `metrics.rs` 必须单拎，否则所有模块都会引用它的 helper

**实现步骤**：
1. 创建 `api_handlers/` 目录
2. 先搬运 `common.rs` 和 `metrics.rs`
3. 按域搬运其他 handlers
4. 更新 `api_routes.rs` 的 imports
5. 保持 `aria_api::*` 类型引用不变

---

## Phase 2: user/src/main.rs

**风险**：低→中

**原因**：
- 不是所有命令都独立
- trace、tcprt、diagnose 有大量共享渲染和聚合逻辑
- clap enum 本身不复杂，复杂的是渲染逻辑

**目标结构**：
```
user/src/
├── main.rs          # 初始化、client 创建、顶层 dispatch（~150 行）
├── cli.rs           # Cli + 所有 Commands enum 定义
├── commands/
│   ├── mod.rs
│   ├── trace.rs     # trace 命令逻辑
│   ├── tcprt.rs     # tcprt 命令逻辑
│   └── diagnose.rs  # diagnose 命令逻辑
└── render/
    ├── mod.rs
    ├── trace.rs     # trace 渲染/聚合逻辑
    └── tcprt.rs     # tcprt 渲染/聚合逻辑
```

**关键点**：
- 不按子命令一比一拆文件
- "命令定义"和"复杂工作流/渲染"分开
- 简单命令（group/policy/qos）留在 main.rs 或简单拆分

**实现步骤**：
1. 抽取 `cli.rs`（Commands enums）
2. 抽取 `render/` 目录（trace/tcprt 渲染逻辑）
3. 抽取 `commands/` 目录（复杂命令的工作流）
4. 简化 main.rs 为 dispatch 入口

---

## Phase 3: agent/src/control_plane.rs（只读域）

**风险**：中（只抽只读域）

**原因**：
- 文件已经按领域分段（见 line 1981 之后）
- 只读/轻写域风险低
- 写路径、WAL、锁结构暂不动

**目标结构**：
```
agent/src/control_plane/
├── mod.rs               # ControlPlane struct + 核心编排
├── instance_access.rs   # get_instance, shared helper, query resolver
├── observability.rs     # stats, kernel drop, metrics summary
├── trace.rs             # trace 相关方法
├── ssl.rs               # SSL 相关方法
└── tcprt.rs             # TCP-RT 相关方法
```

**暂不动**：
- group/policy/qos/mirror 的写路径
- managed registration / unregister
- WAL compact / fallback
- 任何锁结构 redesign

**实现步骤**：
1. 创建 `control_plane/` 目录
2. 先搬运只读方法（stats, observability）
3. 再搬运 trace/ssl/tcprt 域
4. 保持所有 public API 签名不变

---

## Phase 4: core/src/ebpf_ops.rs（内部模块化）

**风险**：中→高

**原因**：
- 核心底座，被广泛调用
- 拆错了会波及面很大
- trace 不是强领域，不应该单独成模块

**目标结构**：
```
core/src/ebpf_ops/
├── mod.rs          # 公共 API re-exports（保持向后兼容）
├── maps.rs         # open helpers, scrub helpers
├── runtime.rs      # iface ctx, tap config, firewall config
├── policy.rs       # parse_ports, add/delete_policy, port set
├── network.rs      # add/delete_network, parse_cidr
├── replay.rs       # validate, replay_state*
├── attach.rs       # load_bpf_with_pin, attach_tc_*, qdisc
└── inventory.rs    # critical maps, scrub, inventory helpers
```

**关键点**：
- 保持 `aria_core::ebpf_ops::*` 对外接口不变
- 只做内部搬运
- `mod.rs` re-export 所有 public 函数

**实现步骤**：
1. 创建 `ebpf_ops/` 目录
2. 按域搬运函数，保持 pub 可见性
3. 在 `mod.rs` 中 re-export 所有 public API
4. 确保 agent 编译通过

---

## Phase 5: control_plane.rs（写路径）

**前提条件**：
- 集成测试覆盖
- 清晰的域边界定义

**可能方向**：
- 按领域服务拆分（GroupService, PolicyService 等）
- 重新设计锁粒度
- WAL 与业务逻辑解耦

**暂不规划具体步骤**。

---

## 验证清单

每个 Phase 完成后：

- [ ] cargo build 通过（CI 验证）
- [ ] 所有 public API 签名不变
- [ ] 服务器冒烟测试通过
- [ ] 文件行数 < 500（目标）

---

## 回滚方案

每个 Phase = 一个 commit，可 `git revert` 独立回滚。
