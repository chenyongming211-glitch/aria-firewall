# RFC-016：Node-local Debug API Boundary v1

状态：Draft  
阶段：Phase 1 / Phase 8 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 节点本地 API 的长期边界，明确哪些能力应该保留在 node-local API，哪些能力必须迁移到平台 northbound API。

本 RFC 的目标是避免后续平台化过程中继续把 node-local API 误用为主控制面入口。

## 2. 非目标

本 RFC 不直接定义：

- 最终 node-local API 全量路径清单
- 最终 systemd / CLI 权限模型
- 最终 northbound API schema

## 3. 设计原则

### 3.1 node-local API 是调试面，不是平台主控制面

node-local API 应主要服务于：

- 本机运维
- break-glass 故障处理
- 节点侧观测与调试

不应作为长期主配置接口。

### 3.2 平台对象写入必须走 northbound API

以下对象的主写入路径必须走 Controller：

- Tenant
- Network
- Port
- SecurityGroup
- RouteTable
- FloatingIP
- Service
- Chain

### 3.3 node-local API 应尽量读多写少

长期建议：

- 读操作保留更多
- 写操作仅保留少量 break-glass 能力

### 3.4 node-local API 必须有更强的安全默认值

默认应满足：

- 仅绑定 loopback
- 本机 root 或系统级权限隔离
- 开启远程访问时必须强认证

## 4. node-local API 的职责

建议保留以下职责：

- 节点健康检查
- 当前 generation / capability / degraded reason 查看
- 节点实时观测查询
- 本地 Diagnose
- emergency stop / disable
- runtime inventory 查看

## 5. 应迁移出 node-local API 的职责

以下职责不应长期以 node-local API 为主：

- 平台对象 CRUD
- 多租户资源管理
- 大规模 rollout 发布
- 跨节点查询
- 平台级权限管理

## 6. 推荐分类

### 6.1 Read-only Debug API

建议保留：

- `/health`
- `/node/status`
- `/node/capabilities`
- `/observe/*`
- `/diagnose/*`
- `/metrics`

### 6.2 Break-glass Action API

建议谨慎保留：

- `drain local attachments`
- `disable local feature`
- `force reconcile`
- `reload local runtime`

### 6.3 Legacy Compatibility API

当前已有的功能菜单型接口可暂时保留为兼容层，但应明确标记：

- `legacy`
- `node-local`
- `not platform primary`

## 7. 与 CLI 的关系

CLI 长期应优先调用 northbound API。  
仅在以下场景下降级为 node-local：

- 平台控制面不可达
- 本机 break-glass
- 调试实验环境

## 8. 安全要求

### 8.1 默认绑定

建议默认：

- `127.0.0.1`
- 或 unix socket

### 8.2 授权模型

即使 node-local API 默认只在本地暴露，也应区分：

- read debug
- dangerous action

### 8.3 审计

高风险 node-local action 仍应写入结构化审计。

## 9. 与现有代码的关系

当前仓库中的：

- `agent/src/api_routes.rs`

主要就是 node-local API 雏形。

未来演进方向应为：

- 保留其调试价值
- 不再扩张为平台主 API
- 逐步把功能写入迁移到 Controller

## 10. 当前缺口

当前缺口：

- node-local 与 platform API 边界尚未制度化
- 兼容接口未明确标记 legacy 边界
- break-glass action 语义未正式定义

## 11. 验收标准

Node-local debug API boundary v1 的验收标准：

- 平台团队能明确区分 node-local 与 northbound 的职责
- 后续新增 API 可以判断应该落在哪一层
- legacy 功能菜单 API 不再被误认为长期平台主接口

## 12. 后续拆分建议

- `RFC-016A` node-local read-only endpoints
- `RFC-016B` break-glass actions
- `RFC-016C` legacy API deprecation plan
