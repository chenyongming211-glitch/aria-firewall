# Known Issues

## v0.10.0 — Controller Store Trait Abstraction

| ID | Severity | Component | Description | Status |
|----|----------|-----------|-------------|--------|
| BUG-1 | Medium | store | `southbound_status_inner` 曾对未注册 southbound 的节点返回 `last_seen_at = now()`，产生虚假时间戳。现已改为在没有 southbound 观察记录时返回 `None`。 | Fixed (2026-04-09) |
| BUG-2 | Low | openapi | `register_node` 的 utoipa responses 声明顺序已整理为先成功响应、再错误响应，同时补齐了 500 声明。 | Fixed (2026-04-09) |
| BUG-3 | Low | store | 早期 `bump_generation` 接口曾暴露在 trait 上且返回值未被使用；现已改为由 store 内部在资源 mutation 时统一推进 generation。 | Fixed (2026-04-09) |
| BUG-4 | Low | store | `desired_state_for_node_inner` 连续 5 次独立 `list()` 无全局快照，中间可能插入 CUD 导致返回不一致状态。Phase 0 单进程内存 store 下竞态窗口极小。 | Deferred (Phase 1) |
| BUG-5 | Info | store | `InMemoryControllerStore` 的资源字段（tenants/nodes/...）仍为 `pub`，handler 已全走 trait 方法，不再需要外部直接访问。 | Won't fix |
| BUG-6 | Medium | store | `FileBackedControllerStore::run_persisted` 在文件写失败时会把内存态恢复到 `before` 快照；由于读路径不走 `mutation_lock`，并发读者可能短暂观察到回滚前的未持久化中间态。当前更像一致性窗口问题，而不是已提交数据丢失。 | Open |
| BUG-7 | Info | store | `node` 资源方法未复用宏，而是手工展开，因为 `delete_node` 需要额外清理 southbound runtime。建议保留实现并补注释，不单独作为代码 bug 处理。 | Accepted |
| BUG-11 | High | store | file-backed store 早期会把 southbound `registration / apply-status / heartbeat` 等高频运行态也纳入整份 controller JSON 快照；节点心跳频繁时会放大序列化和磁盘写入成本。现已将 southbound runtime 改为内存态，仅持久化 publish 摘要。 | Fixed (2026-04-09) |
| BUG-12 | High | store | 一阶引用校验早期在 handler 层完成，写入在 store 层完成，存在 TOCTOU 窗口：引用对象可能在校验后、写入前被并发删除。现已把引用完整性校验和依赖删除保护下沉到 store 锁范围内。 | Fixed (2026-04-09) |
| BUG-13 | Medium | store | `pending_object_counts / changed_kinds / has_deletes` 曾直接使用上一轮 publish 摘要，即使其 generation 已落后于 controller 当前 `desired_generation`，会在状态面暴露旧摘要。现已只在 publish generation 命中当前 desired generation 时使用摘要。 | Fixed (2026-04-09) |
| BUG-14 | Medium | store | `pending_object_counts` 曾在 `partial / failed` apply 报告下也用 `compiled_objects` 抵扣 desired counts，导致待 reconcile 数量被低估。现已仅在当前 generation 且 `status = applied` 时扣减，否则保守返回当前 publish 摘要。 | Fixed (2026-04-09) |
| BUG-15 | Medium | controller/store | `NetworkSpec.route_mode` 曾作为自由字符串被 controller 接受，agent 对未知值又会静默归类为 native handoff。现已在 store 准入层限制为 `native / overlay / hybrid`。 | Fixed (2026-04-10) |
| BUG-16 | Medium | controller/store | `BackendSet.backends[].target_ref` 曾不会在 controller 侧校验；`target_type = port_ref` 时拼错的 `Port` 引用会混入 shadow 状态。现已在 store 准入层校验 `port_ref` 必须存在，且 tenant/network 必须对齐。 | Fixed (2026-04-10) |
| BUG-17 | Medium | controller/store | `ServiceSpec.ports = []` 曾被接受，生成无 frontend 的无效 service。现已要求 service 至少定义一个 listener port。 | Fixed (2026-04-10) |
| BUG-18 | Medium | agent/compiler | `service_revnat_map / service_affinity_map / service_maglev_map` 曾按 service_program 计数，低估多端口 service 的 runtime-family 预算。现已改为按 frontend listener 粒度统计。 | Fixed (2026-04-10) |
| BUG-19 | Medium | agent/compiler | agent 曾在 `supports_encap = false` 时仍把 overlay remote backend 仅标记为普通 shadow overlay handoff，没有显式 degraded reason。现已将该场景显式标记为 `overlay_encap_unsupported` 并下沉到 services 域降级语义。 | Fixed (2026-04-10) |
| BUG-9 | Low | store | 读操作（尤其 `desired_state_for_node_inner` 和 `southbound_status_inner`）与 CUD 并发时可能读到 mixed-time 视图。Phase 0 原型可接受，Phase 1 应结合快照/事务边界统一处理。 | Deferred (Phase 1) |
| STYLE-1 | Info | store | 5 个 helper 方法（`list_resource` / `get_resource` 等）只是转发到 `ResourceStore` 同名方法，宏可直接调用 `self.$field.xxx()`。约 50 行冗余。 | Won't fix |
| STYLE-2 | Info | store | southbound 方法采用 `_inner` + trait impl 委托模式，代码量翻倍。是 `async_trait` 的合理 workaround。 | Won't fix |

## v0.10.0 — Phase 3 Platform Migration (2026-04-18)

| ID | Severity | Component | Description | Status |
|----|----------|-----------|-------------|--------|
| BUG-20 | High | controller | `std::net::IpNet` 在 `validation.rs` 中被使用，但标准库没有这个类型，仓库也没有 `ipnet` 依赖。Controller 从未在 CI 上编译过。现已替换为手写 `is_valid_cidr` 函数，并把 controller 加入 CI。 | Fixed (2026-04-18) |
| BUG-21 | High | controller | Controller 有 236 个历史编译错误：`PersistedResourceStore` 的 `Default` derive 要求 `T: Default`、`FileBackedControllerStore` 缺 Service 方法、utoipa `__path_xxx` 路径解析失败、`as_deref()` 用在 `String` 上、`run_mutation` async closure lifetime 问题。现已全部修复。 | Fixed (2026-04-18) |
| BUG-22 | Medium | controller | `api_routes.rs` 中 `ip-groups` 和 `network-policies` 路由重复注册，Axum 启动时会 panic。现已删除重复路由。 | Fixed (2026-04-18) |
| BUG-23 | Medium | controller | `DesiredStateEnvelope` 构造中 `ip_groups` 和 `network_policies` 字段重复赋值（Rust 编译错误）。`desired_state_object_counts` 也有 `_ip_groups_dup` / `_network_policies_dup` 冗余参数。现已清理。 | Fixed (2026-04-18) |
| BUG-24 | Medium | api | `dataplane::service_chain` 和 `platform::service_chain` 导出了同名的 `ServiceChainListResponse` / `CreateServiceChainRequest`，glob re-export 导致 controller 引用到错误的类型。现已将 dataplane 版本重命名为 `DataplaneServiceChainListResponse` / `DataplaneCreateServiceChainRequest`。 | Fixed (2026-04-18) |
| BUG-25 | High | agent | NodeConfig 的 `qos_enabled: false` 被 QoS materialize 后的 `update_runtime_config(Some(has_any_qos_rule))` 无条件覆盖，导致节点级禁用开关失效。Mirror 和 LB 同理。现已改为 `feature_enabled_when_present(configured, has_objects)` 模式，NodeConfig 显式 false 优先。 | Fixed (2026-04-18) |
| BUG-26 | Medium | agent | NodeConfig CT timeout 部分更新会用硬编码默认值重置未指定字段。现已改为先读当前 pinned CT_CONFIG 值，缺省字段沿用当前值。同时 `write_ct_config_pinned` 和 `update_runtime_config` 的错误不再被 `let _ =` 吞掉。 | Fixed (2026-04-18) |
| BUG-27 | Medium | agent/compiler | `compiled_objects` 缺少 `ip_groups` / `network_policies` / `service_chains` / `node_configs` 四个 key，导致 controller 的 pending_object_counts 计算永远认为这些资源未处理。现已补齐。 | Fixed (2026-04-18) |
