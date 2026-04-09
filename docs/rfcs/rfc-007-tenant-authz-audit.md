# RFC-007：权限、租户与审计模型 v1

状态：Draft  
阶段：Phase 1 / Phase 8 过渡  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 平台级的治理模型，为以下能力提供统一基础：

- 多租户隔离
- 身份认证
- 细粒度授权
- 配额控制
- 审计追踪

本 RFC 的目标不是一步做完企业级 IAM，而是先冻结平台的治理骨架，避免后续 northbound API、Relay、Diagnose、Controller 与 Agent 的身份模型分裂。

## 2. 非目标

本 RFC 不直接定义：

- 最终 SSO / IdP 对接方案
- UI 登录体验
- 最终数据库权限表结构
- southbound 证书协议的详细编码

## 3. 设计原则

### 3.1 多租户是平台基本属性，不是后加标签

除少数基础设施级对象外，平台对象必须天然带租户语义。

### 3.2 认证与授权必须解耦

“你是谁”和“你能做什么”是两件不同的事。

平台至少应分为：

- Authentication
- Authorization
- Audit

### 3.3 控制面和观测面都必须纳入权限模型

权限不能只保护配置写入。以下接口都必须考虑授权：

- 资源 CRUD
- Diagnose
- Relay observe
- 历史查询
- 节点级调试入口

### 3.4 审计必须结构化

审计日志不应只是文本日志，而应有结构化字段，支持检索、追溯和合规导出。

## 4. 主体与作用域

### 4.1 主体类型

建议支持以下主体：

- `platform_admin`
- `tenant_admin`
- `tenant_operator`
- `tenant_viewer`
- `service_account`
- `node_agent`

### 4.2 作用域层级

建议作用域层级：

- `platform`
- `tenant`
- `project`（可选）
- `network`
- `resource`

### 4.3 基础设施主体

`node_agent` 是特殊主体：

- 不走人类用户登录模型
- 使用 mTLS 或等效节点身份
- 权限仅限 southbound 协议和必要状态回报

## 5. 身份认证模型

### 5.1 控制面 northbound 认证

建议长期支持：

- OIDC / SSO
- API token
- service account token

### 5.2 southbound 认证

Controller 与 Agent 之间建议使用：

- mTLS
- node identity
- certificate rotation

### 5.3 本地调试入口

节点本地调试入口可保留更轻量模型，但必须满足以下条件之一：

- 默认仅绑定本地 loopback
- 通过系统级权限隔离访问
- 显式开启远程访问时强制认证

## 6. 授权模型

### 6.1 角色模型

建议采用 RBAC 为主、资源作用域约束为辅的模型。

基础角色建议：

- `platform_admin`
- `tenant_admin`
- `tenant_operator`
- `tenant_viewer`
- `readonly_auditor`
- `service_account_runtime`

### 6.2 权限动作

权限动作建议标准化为：

- `read`
- `create`
- `update`
- `delete`
- `bind`
- `observe`
- `diagnose`
- `replay`
- `admin`

### 6.3 资源类别

资源类别至少覆盖：

- `tenant`
- `node`
- `network`
- `segment`
- `port`
- `instance`
- `security_group`
- `route_table`
- `floating_ip`
- `nat_gateway`
- `service`
- `chain`
- `diagnose_session`
- `observe_stream`

### 6.4 授权规则示例

示例：

- `tenant_viewer` 可 `read` 本租户对象，可 `observe` 本租户流量，不可修改资源
- `tenant_operator` 可管理本租户网络、端口、安全组、Service，不可管理平台级 Node
- `readonly_auditor` 可查看审计和事件，但不可变更资源，且默认不授予 `diagnose`
- `node_agent` 仅可访问分配给自身的 southbound generation 和状态回报接口

## 7. 配额模型

租户配额建议至少覆盖：

- networks
- segments
- ports
- floating_ips
- nat_gateways
- services
- backends
- chains
- diagnose sessions

配额控制应在 northbound 写入路径提前判定，而不是等到 southbound 或 datapath 才失败。

## 8. 审计模型

### 8.1 审计事件类型

建议至少覆盖：

- `auth_success`
- `auth_failure`
- `resource_create`
- `resource_update`
- `resource_delete`
- `binding_change`
- `policy_publish`
- `diagnose_run`
- `observe_subscribe`
- `credential_rotate`
- `node_register`

### 8.2 审计字段

建议字段：

- `audit_id`
- `timestamp`
- `actor_type`
- `actor_id`
- `tenant_id`
- `action`
- `resource_type`
- `resource_id`
- `request_id`
- `result`
- `reason`
- `source_ip`
- `user_agent`

### 8.3 审计保留原则

- 安全相关审计必须长期保留
- 审计不可与普通 debug log 混用
- 审计应支持导出和检索

## 9. Observe 与 Diagnose 的权限约束

### 9.1 Observe

Observe 访问应至少按以下维度过滤：

- tenant
- network
- resource scope

### 9.2 Diagnose

Diagnose 属于高敏感操作，因为它可能暴露：

- 路由决策
- 安全策略结果
- TLS / HTTP 元信息
- 后端拓扑

因此 Diagnose 不应默认开放给所有只读用户。

建议默认策略：

- `tenant_viewer`：默认无 `diagnose`
- `readonly_auditor`：默认无 `diagnose`
- `tenant_operator`：可按租户范围显式授予
- `tenant_admin` / `platform_admin`：可按作用域授予

后续应在 `RFC-007A` 中补充完整的角色-动作-资源示例矩阵，尤其明确 `observe` 与 `diagnose` 的差异化授权。

## 10. 节点与平台信任边界

必须明确：

- Node Agent 不应拥有平台管理员权限
- Agent 只能访问分配给本节点的 desired state
- Relay 不应默认获得全平台配置写权限

## 11. API 设计约束

Northbound API 应支持：

- token 中携带主体与租户信息
- 服务端基于作用域做资源过滤
- 审计自动记录写操作与高敏感读操作

Southbound API 应支持：

- 双向身份确认
- generation 级别的授权上下文

## 12. 当前代码映射与缺口

当前仓库中：

- 节点本地 API 默认主要跑在 `127.0.0.1`
- 尚未形成平台级租户、RBAC 和审计体系

当前缺口：

- 缺少 Tenant 级模型
- 缺少平台级认证与授权
- 缺少结构化审计
- 缺少 Observe / Diagnose 的授权模型

## 13. 验收标准

治理模型 v1 的验收标准：

- 能支持多租户资源隔离
- 能支持 northbound 基础 RBAC
- 能支持 southbound 节点身份
- 能对高敏感读取和所有写操作产出结构化审计

## 14. 后续拆分建议

- `RFC-007A` OIDC / token 与角色权限示例矩阵
- `RFC-007B` southbound mTLS 与 node identity
- `RFC-007C` audit event schema
