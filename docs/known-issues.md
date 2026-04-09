# Known Issues

## v0.10.0 — Controller Store Trait Abstraction

| ID | Severity | Component | Description | Status |
|----|----------|-----------|-------------|--------|
| BUG-1 | Medium | store | `southbound_status_inner` 曾对未注册 southbound 的节点返回 `last_seen_at = now()`，产生虚假时间戳。现已改为在没有 southbound 观察记录时返回 `None`。 | Fixed (2026-04-09) |
| BUG-2 | Low | openapi | `register_node` 的 utoipa responses 声明顺序已整理为先成功响应、再错误响应，同时补齐了 500 声明。 | Fixed (2026-04-09) |
| BUG-3 | Low | store | 早期 `bump_generation` 接口曾暴露在 trait 上且返回值未被使用；现已改为由 store 内部在资源 mutation 时统一推进 generation。 | Fixed (2026-04-09) |
| BUG-4 | Low | store | `desired_state_for_node_inner` 连续 5 次独立 `list()` 无全局快照，中间可能插入 CUD 导致返回不一致状态。Phase 0 单进程内存 store 下竞态窗口极小。 | Deferred (Phase 1) |
| BUG-5 | Info | store | `InMemoryControllerStore` 的资源字段（tenants/nodes/...）仍为 `pub`，handler 已全走 trait 方法，不再需要外部直接访问。 | Won't fix |
| STYLE-1 | Info | store | 5 个 helper 方法（`list_resource` / `get_resource` 等）只是转发到 `ResourceStore` 同名方法，宏可直接调用 `self.$field.xxx()`。约 50 行冗余。 | Won't fix |
| STYLE-2 | Info | store | southbound 方法采用 `_inner` + trait impl 委托模式，代码量翻倍。是 `async_trait` 的合理 workaround。 | Won't fix |
