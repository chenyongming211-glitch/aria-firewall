# Aria eBPF 实现约束

本文档是 Aria 在进入代码实现前的 eBPF verifier / datapath 编码约束清单。  
它不替代架构 RFC，而是作为所有 eBPF 代码落地时必须遵守的实现门槛。

适用范围：

- `ebpf/src/*`
- 与 eBPF map / attach / helper 语义直接耦合的用户态编译与加载逻辑

## 1. 目标

约束的目标不是“写出能跑的 eBPF”，而是：

- 降低 verifier 拒绝风险
- 控制 stack 和调用复杂度
- 保持跨内核版本的可移植性
- 避免 datapath 功能膨胀后失控

## 2. 当前代码基线

当前代码扫描后，有几个现状可以作为后续基线：

- 主数据面已经通过 `PKT_SCRATCH` / `PIPE_SCRATCH` 等 per-CPU scratch map 避免在主路径堆大栈对象，见 [ebpf/src/lib.rs](/Users/chen/code/aria-firewall/ebpf/src/lib.rs#L52)
- 当前仓库没有使用 tail call
- IPv6 扩展头解析已经使用有上界循环，当前上界为 `4`，见 [ebpf/src/parser.rs](/Users/chen/code/aria-firewall/ebpf/src/parser.rs#L204)
- `ssl` 路径已经把大块 HTTP scratch 放在 map-backed scratch 中，但局部仍有小数组和较长辅助调用链，后续新增逻辑应格外保守，见 [ebpf/src/ssl.rs](/Users/chen/code/aria-firewall/ebpf/src/ssl.rs#L96)

## 3. 硬约束

### 3.1 Stack 硬上限

每个 eBPF 函数的 stack 使用不得超过 `512` 字节。

这是内核 verifier 的核心限制之一。函数调用也有独立栈帧，但每一帧都必须满足该限制。  
官方资料也指出：普通函数调用场景下每个函数有 `512` 字节 stack；如果把 tail call 和普通函数调用混用，可用 stack 会下降到 `256` 字节。  
来源：

- [Functions - eBPF Docs](https://docs.ebpf.io/linux/concepts/functions/)

### 3.2 调用深度硬上限

eBPF 的普通函数调用最大深度按 `8` 处理。

来源：

- [Functions - eBPF Docs](https://docs.ebpf.io/linux/concepts/functions/)

### 3.3 禁止递归

任何形式的递归都禁止。

### 3.4 循环必须有 verifier 可证明的上界

所有循环都必须：

- 有固定小上界
- 或使用 verifier 可接受的 bounded loop
- 或改写为 unroll / helper / iterator

来源：

- [Verifier - eBPF Docs](https://docs.ebpf.io/linux/concepts/verifier/)
- [Loops - eBPF Docs](https://docs.ebpf.io/linux/concepts/loops/)

## 4. Aria 项目级软约束

这些不是内核硬限制，但在 Aria 中应视为默认开发规范。

### 4.1 主路径 stack 软上限

对 XDP / TC 主路径函数，建议把单函数 stack 控制在：

- 常规目标：`<= 256` 字节
- 审慎上限：`<= 320` 字节

`512` 只作为绝对硬上限，不作为常规设计目标。

### 4.2 禁止在 datapath 中引入 tail call

当前项目在正式立项和单独评审前，不允许在主数据面引入 tail call。

原因：

- 会把普通函数调用场景下的可用 stack 从 `512` 压到 `256`
- 会提高架构复杂度和调试难度
- 当前代码并未使用 tail call，没必要在 Phase 1/2 就引入新不稳定因子

如果未来确实要使用，必须先新增专门 RFC 或实现评审说明。

### 4.3 大对象一律不上栈

以下对象不允许直接放到主路径函数栈上：

- 大于 `64` 字节的临时数组
- 事件大 payload
- 需要跨多个 helper / 分支复用的 scratch 结构

这类数据应优先放到：

- per-CPU scratch map
- map-backed staging buffer

### 4.4 热路径 helper 一律小而内联

热路径 leaf helper 默认要求：

- `#[inline(always)]`
- 单一职责
- 无大局部对象
- 无复杂循环

### 4.5 顶层 phase 函数允许 `#[inline(never)]`

像 parser 入口、XDP/TC 顶层 phase 函数这类“控制 verifier 复杂度”的函数，可以保留 `#[inline(never)]`。  
但必须保持：

- 参数简单
- 局部栈小
- 不嵌套过深调用链

### 4.6 eBPF datapath 中尽量避免新增 global function 语义风险

数据面热路径优先使用：

- 小型静态 helper
- 显式内联 helper

避免为了“代码好看”把 verifier 难验证的复杂逻辑拆成过多跨函数边界的公共函数。

## 5. 调用链约束

### 5.1 主路径函数调用层数

对 XDP/TC 主数据面，建议把“从程序入口到最深叶子”的普通调用链控制在：

- 常规目标：`<= 5`
- 审慎上限：`<= 6`

虽然硬上限按 `8` 处理，但不建议把调用链写到 verifier 边缘。

### 5.2 每次新增调用链都要回答 3 个问题

- 这个 helper 必须单独成函数吗？
- 这个 helper 会不会引入新的大栈对象？
- 这个 helper 会不会把 verifier 看到的状态空间显著放大？

### 5.3 跨函数传参规则

优先传：

- 标量
- 小结构引用
- scratch map 指针

避免：

- 大结构按值传递
- 对 packet data 的复杂跨函数推导
- 多层转发同一批局部缓冲区

## 6. 循环约束

### 6.1 默认策略

默认优先级：

1. 固定小上界循环
2. 编译期可展开循环
3. bounded loop
4. `bpf_loop` / iterator

### 6.2 禁止的循环模式

- 上界来自未经收缩的报文字段大范围值
- 迭代次数和分支数都高的复杂 bounded loop
- 热路径中大范围扫描

### 6.3 解析类循环规则

像协议解析这种场景，循环上界必须明确写死。  
当前 IPv6 扩展头最多迭代 `4` 次，这类写法应继续保持，[ebpf/src/parser.rs](/Users/chen/code/aria-firewall/ebpf/src/parser.rs#L204)。

## 7. Packet 与 helper 使用约束

### 7.1 任何报文读取都必须先做 bounds check

不允许依赖“前面某处大概检查过”。  
每段读取都必须有本地可证明的边界关系。

### 7.2 helper 调用后要重新审视上下文有效性

新增代码时必须显式考虑：

- helper 是否可能改变 verifier 对寄存器/指针的认识
- 是否需要重新装载 packet 指针或上下文字段
- 是否需要重新做 bounds 证明

### 7.3 helper 选择必须考虑 program type

同一个 helper 不是所有 program type 都能调用。  
新增 helper 调用前，必须确认它对：

- XDP
- TC classifier
- uprobe / uretprobe

分别是否合法。

## 8. 事件与观测约束

### 8.1 事件 payload 不能直接把栈打满

凡是事件结构较大、字段较多，优先：

- map-backed scratch
- staged write
- 分离 header 与 payload

### 8.2 不要为观测牺牲主路径可验证性

观测很重要，但新增 trace / event / metrics 时不能让主路径 stack 和 verifier 复杂度失控。

### 8.3 大字符串/大 header 采样统一走 scratch

像 SSL/HTTP 这类大块 payload，继续沿用 scratch map 路线，不回退到栈对象，[ebpf/src/ssl.rs](/Users/chen/code/aria-firewall/ebpf/src/ssl.rs#L785)。

## 9. 代码审查清单

后续每次修改 `ebpf/src/*` 时，至少要做下面这些自查：

- 是否新增了大于 `64` 字节的局部数组？
- 是否新增了更深的普通函数调用链？
- 是否引入了 tail call？
- 是否新增了 verifier 难证明的循环？
- 是否新增了 program type 不兼容的 helper？
- 是否把大事件 payload 放回了栈上？
- 是否破坏了当前 scratch-map 规避大栈的模式？

## 10. 当前实施结论

在真正开始 `Phase 1` 代码落地前，Aria 后续 eBPF 代码统一遵循以下门槛：

- 单函数 stack 绝不超过 `512` 字节
- XDP/TC 主路径按 `256` 字节软目标设计
- 普通调用深度按 `8` 为硬上限，主路径按 `5-6` 为软上限
- 当前阶段禁止引入 tail call
- 所有大 scratch / 大 payload 一律 map-backed
- 所有循环必须 verifier 可证明且上界清晰

这份约束文档是后续 datapath 改造的前置门槛，不是可选建议。
