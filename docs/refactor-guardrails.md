# Aria 重构护栏

本文档用于沉淀 Aria 过往几次重大重构和功能 cutover 中暴露出来的高风险模式，作为后续平台化开发阶段的硬约束参考。

它的目的不是限制重构，而是避免重复踩坑。

如果后续实现与本护栏冲突，必须先更新总纲或相关 RFC，再调整实现；不能直接绕过这些约束落代码。

## 1. 适用范围

适用于以下类型的工作：

- 平台对象层重构
- southbound / northbound 协议重构
- eBPF 数据面重构
- trace / SSL / kernel-drop / observability cutover
- 恢复、reconcile、生命周期、cleanup 语义调整

## 2. 核心结论

Aria 过去的大改方向大多是对的，但代价最高的 bug 几乎都不是来自“目标选错”，而是来自：

- 一次改太多层
- 先切默认路径，再补回归
- 只改 happy path，不改 recovery / cleanup
- 代码语义和对外文档/API 语义脱节
- verifier / 内核兼容问题被后置

因此，后续开发必须把“控制 blast radius”放到和“功能正确”同等重要的位置。

## 3. 必须避免的模式

### 3.1 禁止跨层 big-bang 重构

不要在同一个改动周期里同时大改：

- datapath
- runtime recovery
- northbound / southbound 契约
- CLI / API
- 文档语义

原因：

- 问题定位困难
- 回滚困难
- 每一层都可能把别层的 bug 伪装成自己问题

要求：

- 一次只重构一层主边界
- 纯重构与语义变更必须拆开
- PR 主题必须单一

### 3.2 禁止“先 cutover，后补回归”

任何默认路径切换前，必须先具备：

- regression runner
- rollback / gate
- fallback 路径
- 文档化的 rollout 条件

尤其适用于：

- trace backend
- observability pipeline
- runtime attach 模型
- 路由 / NAT 主路径

### 3.3 禁止只实现 happy path

任何新能力在宣称“可用”之前，必须同时设计：

- create
- update
- replay
- recover
- cleanup
- stop / detach / delete

如果这些语义没有闭环，该能力只能算原型，不能算平台能力。

### 3.4 禁止把 node-local API 继续扩张成平台主控制面

节点本地 API 可以保留，但定位只能是：

- debug
- break-glass
- node-local observe

新平台能力的主写入路径必须走：

- northbound API
- Controller
- southbound generation

### 3.5 禁止把 verifier 风险后置

eBPF 相关大改在设计时就必须先经过：

- stack 预算
- 调用深度预算
- helper 合法性检查
- 循环上界检查

不能先把逻辑写出来，再等 verifier 报错后返工。

具体实现门槛见 [eBPF 实现约束](ebpf-implementation-constraints.md)。

### 3.6 禁止代码语义与公开语义脱节

公开能力必须保持以下四者一致：

- 实现
- OpenAPI / API schema
- CLI 行为
- README / 用户手册 / 示例

如果其中任意一个未同步，该能力不应视为“已完成”。

### 3.7 禁止把兼容性当成后补项

所有平台级能力在进入实现阶段前，都要先回答：

- baseline kernel path 是什么
- recommended kernel path 是什么
- degraded mode 是什么
- 不支持时如何显式拒绝

不能默认只按 6.8+ 写完，再考虑 4.18 / 旧环境。

### 3.8 禁止在对象模型未稳定前继续扩张功能菜单

在平台化阶段，原则上不再新增长期的顶层“功能菜单式”模型。

优先级必须是：

1. 资源模型
2. northbound / southbound 契约
3. 编译模型
4. datapath feature

### 3.9 禁止以牺牲性能为代价堆功能

新增能力时，必须同时关注：

- datapath 延迟
- CPU 开销
- 内存与 map 占用
- 事件量与高基数风险
- northbound / southbound 响应体规模

要求：

- 热路径和常用查询路径优先输出摘要、计数和聚合视图
- 大对象明细、高基数差异和逐条诊断默认放到 observe / diagnose 路径，不直接塞进主控制面响应
- 如果一个功能会带来明显性能成本，必须同时提供预算说明、裁剪策略或降级模式

## 4. 这次开发必须坚持的顺序

当前阶段建议严格按以下顺序推进：

1. 先 northbound API 骨架
2. 再资源模型落代码
3. 再 southbound 协议骨架
4. 再 agent 编译模型重构
5. 最后再进入 routing / NAT / Service 等 datapath 主功能

不建议跳过前面的对象层和协议层，直接从 datapath feature 开始。

## 5. 历史高风险区域

从历史提交和文档看，以下区域是高风险区：

### 5.1 Runtime Recovery / Lifecycle

高风险原因：

- attach / detach / pin / cleanup 语义复杂
- system stop / managed stop / crash recovery 差异大

### 5.2 Trace Backend Cutover

高风险原因：

- 用户态与 eBPF 二进制需要原子升级
- first-read / flush / retention 很容易被回归打破

### 5.3 SSL / HTTP Observe

高风险原因：

- verifier 敏感
- scratch / helper / bounds 非常脆弱
- 语义是 host-global，不是 per-instance

### 5.4 Kernel-drop / Old-kernel Compatibility

高风险原因：

- 不同内核字段能力差异大
- pin / link / filter 恢复链路复杂

### 5.5 QoS / qdisc Lifecycle

高风险原因：

- `fq` ownership、reconcile、cleanup 极易遗漏

## 6. 每次重大改动前必须回答的问题

### 6.1 架构问题

- 这次改动主要改变哪一层？
- 有没有跨层改动？
- 如果有，能否拆成两次？

### 6.2 生命周期问题

- 创建后如何更新？
- 删除后如何清理？
- Agent 重启后如何恢复？
- Controller 不可用时如何运行？

### 6.3 兼容性问题

- baseline kernel 行为是什么？
- degraded mode 是什么？
- 是否影响旧 API / 旧 CLI / 旧文档示例？

### 6.4 可观测性问题

- 改动后如何观测？
- 回归脚本如何新增覆盖？
- rollout 失败时如何判定和回滚？

## 7. PR / 实施约束

### 7.1 单个 PR 应满足

- 单一目标
- 单一主层次
- 文档先行或文档同步
- 回归范围明确

### 7.2 禁止的 PR 形态

- “顺手再把 xxx 一起改了”
- “既提取模块又改行为”
- “先把默认值切了，回归之后补”

### 7.3 对外能力完成定义

一个对外能力只有同时满足以下条件，才算完成：

- 契约已定
- 实现可用
- recovery / cleanup 语义闭环
- 文档同步
- regression 覆盖

## 8. 与现有文档的关系

本护栏文档与以下文档配套使用：

- [IaaS 网络架构原则与演进路线](iaas-architecture-roadmap.md)
- [RFC 索引](rfcs/README.md)
- [eBPF 实现约束](ebpf-implementation-constraints.md)

## 9. 当前实施要求

在进入 `Phase 1` 代码落地前，当前项目统一遵循：

- 先对象层，后功能层
- 先契约，后实现
- 先回归与降级，后默认 cutover
- 先恢复与 cleanup 语义，后宣布可用

以上要求与 [IaaS 网络架构原则与演进路线](iaas-architecture-roadmap.md) 及下游 RFC 一起构成当前阶段的实施红线。

这不是建议，而是当前阶段的开发约束。
