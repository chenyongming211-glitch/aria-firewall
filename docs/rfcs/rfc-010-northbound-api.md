# RFC-010：Northbound API v1

状态：Draft  
阶段：Phase 0 / Phase 1 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 平台的 northbound API 边界与风格，作为以下能力的统一入口：

- 资源 CRUD
- 状态查询
- Diagnose
- Observe / 查询
- 审计与平台运维接口

本 RFC 的目标不是一次性穷举所有 API，而是先冻结 API 设计原则、资源路径模式、版本策略和异步操作模型。

## 2. 非目标

本 RFC 不直接定义：

- southbound 协议
- 节点本地调试 API
- 最终 SDK 代码生成细节
- UI 交互细节

## 3. 设计原则

### 3.1 Northbound API 面向平台对象，不面向节点功能菜单

平台 API 必须围绕以下对象组织：

- Tenant
- Network
- Segment
- Port
- Instance
- SecurityGroup
- RouteTable
- FloatingIP
- NatGateway
- Service
- Chain

不应沿用节点本地的 `group/policy/qos/mirror` 风格作为长期 northbound 顶层模型。

### 3.2 Node-local API 与 Platform API 必须分层

当前节点本地 API 可以继续保留，但其定位应调整为：

- debug
- break-glass 运维
- 节点侧诊断

长期主控制面必须走 platform northbound API。

### 3.3 REST + OpenAPI 是默认 northbound 形态

建议长期保持：

- REST 资源导向路径
- OpenAPI 规范
- 清晰的 schema 与 error model

### 3.4 所有写操作都必须有版本、请求标识和审计语义

northbound 写操作至少应支持：

- request id
- idempotency key
- actor context
- audit trail

### 3.5 状态与期望状态必须分离表达

资源应区分：

- `spec`
- `status`

控制面写入的是 `spec`，平台返回和聚合的是 `status`。

## 4. API 层次

### 4.1 Resource API

用于对象 CRUD。

### 4.2 Action API

用于少量需要显式动作触发的操作，例如：

- rollout
- diagnose run
- replay
- drain

### 4.3 Observe API

用于流查询、事件检索、实时订阅和聚合视图。

### 4.4 Admin API

用于平台运维、节点状态和审计查询。

## 5. 路径设计

### 5.1 顶层路径

建议统一为：

- `/api/v1/tenants`
- `/api/v1/nodes`
- `/api/v1/networks`
- `/api/v1/segments`
- `/api/v1/ports`
- `/api/v1/instances`
- `/api/v1/security-groups`
- `/api/v1/route-tables`
- `/api/v1/routes`
- `/api/v1/floating-ips`
- `/api/v1/nat-gateways`
- `/api/v1/services`
- `/api/v1/chains`
- `/api/v1/diagnose`
- `/api/v1/observe`
- `/api/v1/audit`

### 5.2 资源内子路径

建议支持：

- `/api/v1/services/{id}`
- `/api/v1/services/{id}/status`
- `/api/v1/services/{id}/backends`
- `/api/v1/ports/{id}/attachments`
- `/api/v1/networks/{id}/route-tables`

### 5.3 避免的路径风格

不建议长期保留：

- 以节点实例名为主命名空间的控制面接口
- 以功能菜单为中心的顶层 API
- 混合“资源 + 动词”且无统一规则的路径

## 6. 资源表示

### 6.1 建议响应结构

建议所有对象遵循统一外层：

```json
{
  "metadata": {
    "id": "svc-123",
    "resource_version": "42",
    "created_at": "...",
    "updated_at": "..."
  },
  "spec": {},
  "status": {}
}
```

### 6.2 列表响应

建议列表响应统一包含：

- `items`
- `next_page_token`
- `total_count`（可选）

### 6.3 错误响应

建议统一错误模型：

- `code`
- `message`
- `request_id`
- `details`

## 7. 写操作模型

### 7.1 同步写

适用于轻量级控制面对象创建与更新。

### 7.2 异步写

适用于以下场景：

- 跨节点 rollout
- 大规模发布
- Diagnose 长时会话
- 回放或修复任务

异步写建议返回：

- `operation_id`
- `status_url`

### 7.3 幂等性

所有 create / action 类接口建议支持：

- `Idempotency-Key`

## 8. 查询与过滤

### 8.1 列表过滤

建议支持：

- `tenant_id`
- `network_id`
- `node_id`
- `status`
- `label_selector`

### 8.2 Observe / Diagnose 过滤

建议支持：

- `time_range`
- `target_type`
- `instance_id`
- `service_id`
- `network_id`
- `tenant_id`

## 9. 版本策略

### 9.1 顶层版本

建议用路径版本：

- `/api/v1/...`

### 9.2 字段兼容

建议：

- 新增字段向后兼容
- 删除或重命名字段需要新版本
- 高风险语义变化需升级 minor 或 major API 版本

## 10. 与治理模型的关系

所有 northbound API 都必须纳入：

- `RFC-007` 认证
- `RFC-007` 授权
- `RFC-007` 审计

平台必须基于调用方作用域自动过滤资源结果。

## 11. 与 Observe / Diagnose 的关系

Observe 与 Diagnose 不是“附加脚本接口”，而是 northbound 一级能力。

建议提供：

- `POST /api/v1/diagnose`
- `GET /api/v1/diagnose/{id}`
- `GET /api/v1/diagnose/{id}/events`
- `GET /api/v1/observe/events`
- `GET /api/v1/observe/flows`

## 12. 与当前代码的关系

当前仓库中的：

- `agent/src/api_routes.rs`

更接近 node-local API，而不是平台 northbound API。

因此未来演进应为：

- 保留 node-local API 作为调试面
- 新增 Controller northbound API 作为平台主入口

## 13. 当前缺口

当前主要缺口：

- 没有平台级资源导向 northbound API
- 没有统一 spec/status 模型
- 没有统一操作异步模型
- 没有平台级 error / pagination / filtering 规范

## 14. 验收标准

Northbound API v1 的验收标准：

- 能覆盖资源模型 v1 的主要对象
- 能与授权、审计、Diagnose、Observe 自洽集成
- 不依赖 node-local 功能菜单 API 作为主控制面

## 15. 后续拆分建议

- `RFC-010A` resource schema catalog
- `RFC-010B` operation / async task model
- `RFC-010C` observe and diagnose API details

## 16. 当前实现状态（2026-04-09）

当前仓库已经新增实验性的 `controller` crate，作为本 RFC 的第一阶段代码落地：

- 已提供 `REST + OpenAPI` 形态的 northbound API 骨架
- 已覆盖首批资源：`Tenant / Node / Network / Port / SecurityGroup / RouteTable`
- 已提供统一 `metadata / spec / status` 外层
- 已提供内存态 CRUD 存储和 `/openapi.json` / `/docs`
- 已提供第一版列表查询骨架：`limit / page_token / label_selector`，并按资源补充 `tenant_id / network_id / node_id / status` 等基础过滤字段
- `Node` 列表已开始支持 `sync_state` 过滤，可直接筛选 `pending / out_of_sync / in_sync / failed / degraded` 节点
- 已提供 `X-Request-Id` 透传/自动生成，错误响应中的 `request_id` 与响应头一致
- 已提供第一版平台关系校验与删除保护：northbound 写入会校验一阶引用，破坏依赖关系的删除会返回 `409 dependency_conflict`
- `Node.status` 已开始镜像 southbound 的关键运行态，直接返回 `desired_generation / last_applied_generation / last_seen_at / last_reconcile_at / last_error / pending_object_counts / sync_status`
- `Node.status` 已开始镜像 `pending_object_counts`，用轻量聚合的方式展示当前 generation 仍待 reconcile 的对象数量
- 当前 `page_token` 仍是简单 offset 语义，`label_selector` 仅支持精确匹配的 `key=value[,key=value...]`

当前仍未实现：

- 持久化与恢复
- 认证、授权与审计
- 异步 operation 模型
- 稳定 cursor / 高级过滤表达式 / 排序
- Observe / Diagnose 北向能力
