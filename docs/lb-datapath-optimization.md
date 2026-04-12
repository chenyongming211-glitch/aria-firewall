# L4 LB 数据面性能分析与优化方案

## 当前架构概览

L4 LB 数据面由以下核心组件构成：

| 组件 | eBPF Map | 类型 | 容量 |
|------|----------|------|------|
| Frontend VIP 查表 | `SVC_FRONTEND_MAP` | HashMap | 4096 |
| Backend 成员查表 | `SVC_BACKEND_MAP` | HashMap | 16384 |
| RevNat 反向映射 | `SVC_REVNAT_MAP` | HashMap | 4096 |
| Session Affinity | `SVC_AFFINITY_MAP` | LruHashMap | 65536 |
| Maglev 一致性哈希表 | `SVC_MAGLEV_MAP` | HashMap | 65536 |
| LB 统计 | `SVC_LB_STATS` | PerCpuHashMap | 8192 |
| 统计攒批缓存 | `LB_STATS_CACHE` | PerCpuArray | 1 |
| 连接跟踪 | `CT_TABLE_V4` | LruHashMap | 262144 |
| 连接跟踪 | `CT_TABLE_V6` | LruHashMap | 65536 |

## 数据包处理路径分析

### TC Ingress — LB 首包（CT miss）

```
load_runtime_ctx_tc          → 1 lookup (IFACE_CTX_MAP)
load_feature_flags_tc        → 5 lookup (TAP_CONFIG_MAP × 5, 同一 key!)
port_identity                → 1 lookup
sg_check                     → 1-2 lookup
phase_lb_ingress_v4:
  svc_frontend_lookup_v4     → 1 lookup (SVC_FRONTEND_MAP)
  affinity_lookup (可选)     → 1 lookup (SVC_AFFINITY_MAP)
  maglev_select (可选)       → 1 lookup (SVC_MAGLEV_MAP)
  SVC_BACKEND_MAP.get        → 1 lookup
  svc_dnat_v4                → 2-4 bpf_skb_store_bytes + 2-3 csum_replace
  update_lb_stats            → 1 PerCpuArray access
  affinity_write (可选)      → 1 map insert
phase_ct_v4:
  CT_TABLE_V4 lookup         → 2 lookup (forward + reverse)
  ct_create_v4               → 1 map insert
─────────────────────────────────────────────────
总计: ~12-15 map operations + 4-7 bpf helper calls
```

### TC Ingress — CT Fast-Path（后续包，命中 established）

```
load_runtime_ctx_tc          → 1 lookup
load_feature_flags_tc        → 5 lookup (TAP_CONFIG_MAP × 5)
phase_ct_v4:
  CT_TABLE_V4 lookup         → 1-2 lookup (forward hit)
phase_ct_fastpath_tc_ingress_v4:
  apply_dnat_v4_raw          → 2-4 bpf_skb_store_bytes + 2-3 csum_replace
  update_lb_stats            → 1 PerCpuArray access
─────────────────────────────────────────────────
总计: 7-8 map operations + 4-7 bpf helper calls
```

CT fast-path 已将 LB 首包的全部查表（frontend/affinity/maglev/backend）跳过，直接从 CT entry 读取缓存的 backend 地址完成 DNAT。

### TC Egress — RevNat（后端回包）

```
load_runtime_ctx_tc          → 1 lookup
load_feature_flags_tc        → 5 lookup (TAP_CONFIG_MAP × 5)
phase_lb_egress_v4:
  svc_revnat_lookup_v4       → 1 lookup (SVC_REVNAT_MAP)
  svc_snat_v4                → 2 bpf_skb_store_bytes + 3 csum_replace
phase_ct_v4:
  CT_TABLE_V4 lookup         → 1-2 lookup
─────────────────────────────────────────────────
总计: 8-9 map operations + 5 bpf helper calls
```

## 已完成的最优设计

| 设计点 | 说明 |
|--------|------|
| CT 缓存 LB 决策 | 后续包跳过全部 LB 查表（frontend/affinity/maglev/backend），从 ~15 次降到 ~8 次 map 操作 |
| DNAT 增量校验和 | `apply_dnat_v4_raw` 直接改 skb + 增量 csum，无 clone/copy |
| RevNat 独立 map | Egress 只需 1 次 SVC_REVNAT_MAP lookup |
| Affinity LRU | 65536 条目，自动淘汰过期会话 |
| Stats PerCpu 攒批 | 阈值 64，长连接 TCP 有效聚合 |
| Maglev 预计算表 | 查表 O(1)，数据面不做排列计算 |
| PipelineCtx 缓存 | LB 结果写入 Per-CPU PipelineCtx，下游 CT 直接读取 |
| Stats 缓存初始化 | 首包和新 key 切换时完整初始化 cache 所有字段 |

## 优化方案

### 优化 1：合并 `load_feature_flags_tc` 重复 lookup（优先级：高）

**现状：**

`ebpf/src/lib.rs:484-502` 的 `load_feature_flags_tc` 调用 5 个独立的 `runtime::xxx_enabled()` 函数，每个都独立查询 `TAP_CONFIG_MAP`，key 完全相同：

```rust
// 当前: 5 × TAP_CONFIG_MAP.get(&tap_id)
if qos::qos_enabled(p.tap_id) { ... }     // lookup 1
if tcprt::tcprt_enabled(p.tap_id) { ... }  // lookup 2
if policy::acl_enabled(p.tap_id) { ... }   // lookup 3
if mirror::mirror_enabled(p.tap_id) { ... } // lookup 4
if runtime::lb_enabled(p.tap_id) { ... }   // lookup 5
```

**影响范围：** TC ingress、TC egress、XDP 的每个数据包都会执行。Fast-path 也不例外。

**优化方案：** 一次 lookup 取出 `TapConfig`，在调用侧提取所有标志位：

```rust
#[inline(always)]
unsafe fn load_feature_flags_tc(p: &mut PipelineCtx, info: &parser::PacketInfo) {
    if p.tap_id == TAP_ID_UNASSIGNED {
        return;
    }
    if let Some(cfg) = maps::TAP_CONFIG_MAP.get(&p.tap_id) {
        if cfg.qos_enabled != 0 { p.flags |= FLAG_QOS_ON; }
        if cfg.tcprt_enabled != 0 { p.flags |= FLAG_TCPRT_ON; }
        if cfg.acl_enabled != 0 { p.flags |= FLAG_ACL_ON; }
        if cfg.mirror_enabled != 0 { p.flags |= FLAG_MIRROR_ON; }
        if cfg.lb_enabled != 0 { p.flags |= FLAG_LB_ON; }
    }
    if trace::should_trace(p.tap_id, info) {
        p.flags |= FLAG_TRACING;
    }
}
```

**预估收益：**
- Fast-path 从 7-8 次 map 操作降到 3-4 次
- 每个 TC 入口包省 4 次 hash map lookup（约 ~200ns @ 10Gbps）
- 全局性改进，LB 和非 LB 路径均受益

**风险：** 低。`TAP_CONFIG_MAP` 是全局只读 map，运行时不会并发修改。

### 优化 2：Egress CT fast-path 缓存 RevNat 结果（优先级：低）

**现状：**

TC egress 每包都做 `SVC_REVNAT_MAP` lookup，即使 CT 已 established。RevNat 结果（VIP 地址+端口）没有缓存在 CT entry 中。

**优化方案：** 在 `CtValue` 中加 `lb_revnat_ip: [u8; 16]` + `lb_revnat_port: u16` 字段。Egress fast-path 直接从 CT 读 RevNat 映射，跳过 SVC_REVNAT_MAP lookup。

**预估收益：** 每个后端回包省 1 次 hash map lookup（~50ns）。但 egress 路径通常不是瓶颈（后端回包带宽远小于入口 VIP 带宽）。

**代价：** `CtValue` 增加 18 字节（从 ~80B 增到 ~98B），CT_TABLE_V4 的 262144 条目多占 ~4.5MB。需评估内存开销是否可接受。

**建议：** 暂不实施。除非 egress RevNat lookup 被 profiling 确认为瓶颈。

### 优化 3：Maglev Entry padding（优先级：极低）

**现状：**

`SvcMaglevEntry { backend_slot: u16, pad: [u8; 2] }` = 4 字节，50% 是 padding。65536 条目浪费 128KB。

**优化方案：** 改为 `backend_slot: u16`（2 字节 value）。BPF HashMap 支持非 4 字节对齐的 value。

**预估收益：** 省内存但几乎无性能影响。内核 hash table 通常按 8 字节对齐 bucket，实际可能不省。

**建议：** 不值得单独实施。

### 优化 4：XDP 卸载 LB（优先级：远期）

**现状：** LB 完全在 TC 层执行。XDP 层不做 LB。

**优化方案：** 将 frontend lookup + 直接转发搬到 XDP 层，绕过 skb 分配开销。需要 `bpf_xdp_adjust_tail` + `bpf_csum_diff` 替代 TC 的 `bpf_skb_store_bytes`。

**预估收益：** 架构级变更，XDP 可达 ~20Mpps/核 vs TC 的 ~10Mpps/核。但引入复杂度：XDP 没有 CT map，需要独立的 XDP CT 管理。

**建议：** 远期考虑。当前 TC 性能已足够覆盖大多数场景。

## 结论

修复已知 bug 后，L4 LB 数据面的核心架构已接近最优。CT 缓存 LB 决策是最高效的设计——首包 ~15 次 map 操作，fast-path ~8 次。

**唯一有实际意义的近期优化是优化 1（合并 `load_feature_flags_tc` 重复 lookup）。** 它同时优化 LB 和非 LB 路径，改动小、风险低、收益确定。

其余优化收益极小，不建议近期投入。
