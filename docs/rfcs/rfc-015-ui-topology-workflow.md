# RFC-015：UI / Topology / Workflow Model v1

状态：Draft  
阶段：Phase 5 / Phase 8  
上游文档：[IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md)

## 1. 目标

定义 Aria 平台的 UI、拓扑视图与关键工作流模型，确保未来界面建设不是零散页面堆叠，而是服务于平台对象、诊断流程和运维闭环。

## 2. 非目标

本 RFC 不直接定义：

- 最终前端技术栈
- 最终视觉设计语言
- 每个页面的像素级交互稿

## 3. 设计原则

### 3.1 UI 必须围绕对象和工作流组织

UI 不应围绕“命令菜单”组织，而应围绕：

- 资源管理
- 观测查询
- 诊断
- 发布与回滚

### 3.2 拓扑不是装饰图，而是诊断入口

Topology 的意义不是展示漂亮的线，而是作为：

- Service / Chain 关系入口
- 故障定位入口
- Observe / Diagnose 的可视过滤入口

### 3.3 实时与历史视图必须区分

UI 应区分：

- 实时 Observe
- 近期 Diagnose
- 历史分析

### 3.4 高风险操作必须工作流化

安全组、路由、NAT、Service 等高风险对象修改应走明确 workflow，而不是一个立即生效的裸表单。

## 4. 顶层导航模型

建议顶层至少划分：

- `Overview`
- `Tenants`
- `Networks`
- `Ports / Instances`
- `Security`
- `Routing & NAT`
- `Services & Chains`
- `Observe`
- `Diagnose`
- `Rollouts`
- `Audit`
- `Nodes`
- `Settings`

## 5. Overview

Overview 负责提供平台级摘要：

- node health
- active tenants
- service health
- recent critical drops
- active rollout
- recent diagnose verdicts

## 6. 资源管理视图

### 6.1 Tenant / Network / Segment

重点展示：

- 对象状态
- 关联资源数
- 最近变更

### 6.2 Port / Instance

重点展示：

- attachment
- fixed IPs
- security groups
- floating IPs
- recent flow summary

### 6.3 Security

重点展示：

- security groups
- rules
- audit / shadow 结果
- 最近命中统计

### 6.4 Routing & NAT

重点展示：

- route tables
- next hops
- floating IP
- nat gateway
- route/nat decision summary

### 6.5 Services & Chains

重点展示：

- service VIP
- backend health
- chain hops
- recent lb decisions

## 7. Observe 模型

Observe 页面建议至少支持：

- 实时流列表
- 事件过滤
- 多维标签过滤
- 节点 / 租户 / 服务 / 实例切片

### 7.1 实时流

应支持：

- 时间滚动
- verdict 高亮
- 按 service / port / instance 快速钻取

### 7.2 事件详情

事件详情应能展开：

- route decision
- nat decision
- backend selection
- policy match
- trace path

## 8. Topology 模型

拓扑建议至少提供三种视角：

- `network topology`
- `service topology`
- `diagnose path`

### 8.1 Network Topology

重点看：

- Network / Segment
- Node
- Port / Attachment
- 跨节点 reachability

### 8.2 Service Topology

重点看：

- Service
- BackendSet
- Backend
- Chain

### 8.3 Diagnose Path

重点看：

- 某次 Diagnose 关联的关键 hop
- verdict 变化点
- 主要证据源

## 9. Diagnose 工作流

Diagnose 工作流建议为：

1. 选择目标
2. 选择时间窗口
3. 发起 Diagnose
4. 查看 verdict / evidence / candidate causes
5. 跳转到相关事件或对象

Diagnose 结果页应能直接链接到：

- service
- port
- route
- security rule
- rollout

## 10. Rollout 工作流

高风险变更建议统一 workflow：

1. 创建变更
2. validate
3. audit / shadow
4. canary
5. enforce
6. rollback（如需）

UI 不应把这些阶段藏到日志里，而应作为明确状态流展示。

## 11. 节点与能力视图

Nodes 页面应至少展示：

- node health
- capability profile
- attached ports
- active generation
- degraded reasons

这样运维可以快速区分：

- 真 bug
- capability gap
- rollout 风险

## 12. 审计视图

Audit 页面应支持：

- actor
- action
- resource
- result
- time window

并能关联到：

- rollout
- diagnose
- object detail

## 13. 与 RFC 的关系

UI / workflow 必须建立在以下 RFC 之上：

- `RFC-001` 资源模型
- `RFC-002` 事件模型
- `RFC-006` Diagnose / Relay
- `RFC-007` 权限与审计
- `RFC-008` Service 模型
- `RFC-012` Rollout / Audit / Shadow

## 14. 当前代码映射与缺口

当前仓库已有：

- OpenAPI 文档
- 节点本地 metrics
- 节点本地 diagnose 命令

当前缺口：

- 没有平台级 UI 模型
- 没有 topology 视图模型
- 没有 rollout 工作流模型
- 没有对象详情页和诊断联动设计

## 15. 验收标准

UI / topology / workflow model v1 的验收标准：

- 能支撑平台对象管理
- 能支撑 Observe、Diagnose、Rollout 三大核心流程
- 能把 topology 作为诊断与运营入口，而不是独立装饰页

## 16. 后续拆分建议

- `RFC-015A` overview and object pages
- `RFC-015B` observe and diagnose UI
- `RFC-015C` rollout workflow UI
