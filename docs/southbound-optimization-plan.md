# Southbound 下发优化方案

> 基于 RFC-003（Controller-Agent Southbound 协议）

## 0. 当前问题

| 问题 | 影响 |
|------|------|
| 每次轮询拉全量快照 | Controller CPU + 带宽浪费，节点多时成瓶颈 |
| 没有增量协议 | 永远 `full_sync: true`，`deletes` 永远为空 |
| generation 是全局的 | 节点 A 的变更触发节点 B 无意义重编译 |
| `desired_state_for_node` 每次 N 次 list + filter | 资源量大时 Controller 延迟高 |
| 没有 ETag / If-None-Match | 无变化时也传输完整 payload |
| materialize 全量清除 + 全量写入 | 改 1 条规则要删写 2000 次 map 操作 |

## 1. 优化 1：ETag + 304 Not Modified

优先级：P0（最高）  
工作量：~50 行  
收益：generation 没变时零序列化、零传输

### 1.1 实现

Controller `desired_state` handler：
- 读请求的 `If-None-Match` header
- 如果值等于当前 node-local generation（优化 2 实施前用全局 generation），返回 304 空 body + `ETag` header
- 否则正常返回 200 + 完整 envelope + `ETag: {generation}` header

Agent `desired_state` 方法：
- 缓存上一次成功响应的 ETag 值
- 后续请求带 `If-None-Match: {cached_etag}` header
- 收到 304：跳过 compile，但不跳过 heartbeat
- 收到 200：正常处理，更新缓存的 ETag
- 响应没有 ETag header（旧版 Controller）：fallback 到全量比较

### 1.2 注意事项

- 当前 Controller 是单副本，全局 generation（AtomicU64）可直接作为 ETag
- 多副本部署时，ETag 必须来自共享存储（etcd revision / DB sequence），否则不同副本返回不同 ETag 导致缓存失效
- 304 响应时 Agent 仍需发送 heartbeat，避免被 Controller 判定失联
- Agent 需更新本地 `last_successful_fetch` 时间

### 1.3 改动文件

- `controller/src/southbound_handlers.rs`：desired_state handler 加 ETag 逻辑
- `agent/src/platform_agent/southbound_client.rs`：desired_state 方法加 If-None-Match
- `agent/src/platform_agent/mod.rs`：run 循环处理 304 分支

## 2. 优化 2：per-node generation

优先级：P1  
工作量：~80 行  
收益：节点无关变更不触发重编译

### 2.1 实现

在 `desired_state_for_node_inner` 里，对投影到该节点的所有资源的 `(resource_type, id, resource_version)` 三元组做 hash，得到 node-local generation。

```rust
fn compute_node_generation(
    tenants: &[TenantResource],
    networks: &[NetworkResource],
    ports: &[PortResource],
    // ... 所有投影资源
) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for t in tenants { hasher.write(t.metadata.resource_version.as_bytes()); }
    for n in networks { hasher.write(n.metadata.resource_version.as_bytes()); }
    // ... 所有资源类型
    format!("{:016x}", hasher.finish())
}
```

用这个 hash 替代全局 generation 作为 envelope 的 generation 字段和 ETag 值。

### 2.2 传递依赖

资源引用关系：
```
Port → Network → Tenant
Port → SecurityGroup
Port → Node
Network → RouteTable / IpGroup / NetworkPolicy / QosPolicy / MirrorPolicy / ServiceChain
Network → BackendSet → HealthCheck
Network → Service → BackendSet
Node → NodeConfig
```

当 SecurityGroup 规则变了，SecurityGroup 的 `resource_version` 递增。由于 `desired_state_for_node_inner` 已经把所有相关资源都投影出来了（通过 network_ids + security_group_ids 过滤），hash 输入自然包含 SecurityGroup 的新版本。传递依赖通过"投影所有相关资源"自然覆盖，不需要额外的依赖图计算。

### 2.3 注意事项

- 用资源版本 hash 而不是 object_counts hash，因为 object_counts 只记录数量，不感知内容变化
- hash 算法不需要密码学强度，DefaultHasher（SipHash）或 fnv 即可
- node-local generation 是确定性的：相同的资源集合 + 相同的版本 = 相同的 hash

### 2.4 改动文件

- `controller/src/store/southbound.rs`：`desired_state_for_node_inner` 加 hash 计算
- `controller/src/store/southbound.rs`：envelope 的 generation 改为 node-local hash

## 3. 优化 3：增量 desired-state

优先级：P2  
工作量：~300 行  
依赖：Phase 1 持久化（etcd/DB）或内存快照

### 3.1 实现

Controller 为每个节点存储上一次下发的完整快照。下次请求时：
1. 计算当前投影结果
2. 与上次快照比较，找出新增/修改/删除的资源
3. 只返回 diff（`full_sync: false`，`deletes` 非空）
4. Agent 侧维护本地 desired state cache，apply diff

### 3.2 diff 算法

每个资源有 `(id, resource_version)`。比较两个快照：
- 新快照有、旧快照没有 → 新增
- 两个都有但 resource_version 不同 → 修改
- 旧快照有、新快照没有 → 删除

不需要深度比较嵌套对象，resource_version 比较就够了。

### 3.3 注意事项

- Controller 重启后快照丢失，下次请求退化为 `full_sync: true`
- Agent 收到 `full_sync: true` 时丢弃本地 cache，用完整 envelope 替换
- 内存快照的内存开销：每节点一份，约 10-100KB（取决于资源数量）
- 建议在 Phase 14（持久化）之后再做，当前内存 store 阶段不值得

### 3.4 改动文件

- `controller/src/store/southbound.rs`：存储 per-node 上次快照，计算 diff
- `api/src/southbound.rs`：DesiredStateEnvelope 已有 `full_sync` 和 `deletes` 字段
- `agent/src/platform_agent/mod.rs`：处理增量 envelope，维护本地 cache

## 4. 优化 4：增量 materialize

优先级：P3  
工作量：~500 行  
依赖：增量 desired-state

### 4.1 实现

Agent 的 materialize 从"全量清除 + 全量写入"改成"diff-based 增量写入"：
1. 比较上一轮和当前的 compiled state
2. 只写入变化的 map entry（新增/修改/删除）
3. 不变的 entry 不碰

### 4.2 注意事项

- 当前全量 materialize 在 < 1000 条规则时延迟可接受（毫秒级）
- 增量 materialize 的状态一致性风险高：如果 diff 计算有 bug，map 会处于不一致状态
- 可以考虑 `bpf_map_batch_update`（内核 5.6+）减少 syscall 次数，4.18 fallback 到逐个写入
- 建议等规模真正成为瓶颈时再做

### 4.3 改动文件

- `agent/src/platform_agent/runtime_materialize.rs`：diff 计算 + 增量写入
- `agent/src/platform_agent/ir_types.rs`：可能需要 IR diff 类型

## 5. 其他可选优化

| 优化 | 说明 | 优先级 |
|------|------|--------|
| gzip 压缩 | HTTP 响应启用 gzip，减少传输量 | P2（ETag 之后很少传全量） |
| 批量 map 操作 | `bpf_map_batch_update`（5.6+） | P3（需要能力探测 + fallback） |
| gRPC streaming | 替代 HTTP 轮询，Controller 主动推送 | Phase 5（Relay） |
| Informer 模式 | Agent watch Controller 资源变化事件 | Phase 5（依赖 gRPC streaming） |

## 6. 执行顺序

```
优化 1：ETag + 304              → P0，~50 行，立即可做
优化 2：per-node generation      → P1，~80 行，与 P0 并行或紧接
优化 3：增量 desired-state       → P2，~300 行，Phase 14 持久化之后
优化 4：增量 materialize         → P3，~500 行，规模瓶颈出现时
```

优化 1 + 2 能消除 90% 以上的无效传输和重编译。
