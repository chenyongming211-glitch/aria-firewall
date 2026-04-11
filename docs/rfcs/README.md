# Aria RFC Index

本文档索引用于承接 [IaaS 网络架构原则与演进路线](../iaas-architecture-roadmap.md) 中 Phase 0 的正式设计细化。所有 RFC 都必须服从总纲，不允许绕开总纲单独演进。

## 使用规则

- 总纲定义长期方向和分阶段路线
- RFC 定义可执行的阶段性设计
- 如果 RFC 与总纲冲突，优先修订总纲，再修订 RFC
- 如果实现与 RFC 冲突，优先修订 RFC，再修改代码
- 新增平台级对象、事件主模型或 southbound 协议时，必须新增或更新对应 RFC

## 当前 RFC

- [RFC-001 资源模型 v1](rfc-001-resource-model.md)
- [RFC-002 统一事件模型 v1](rfc-002-event-model.md)
- [RFC-003 Controller-Agent Southbound 协议 v1](rfc-003-southbound-protocol.md)
- [RFC-004 Node datapath 编译模型 v1](rfc-004-node-datapath-compiler.md)
- [RFC-005 Routing / NAT 基础数据面 v1](rfc-005-routing-nat-datapath.md)
- [RFC-005A 单节点 IaaS Map Schema v1](rfc-005a-single-node-iaas-map-schema.md)
- [RFC-006 Diagnose 服务端化与 Relay v1](rfc-006-diagnose-relay.md)
- [RFC-007 权限、租户与审计模型 v1](rfc-007-tenant-authz-audit.md)
- [RFC-008 Service / Backend / HealthCheck 模型 v1](rfc-008-service-backend-healthcheck.md)
- [RFC-009 Multi-node Overlay / Native Routing v1](rfc-009-multi-node-networking.md)
- [RFC-010 Northbound API v1](rfc-010-northbound-api.md)
- [RFC-011 Datapath Capability Matrix v1](rfc-011-datapath-capability-matrix.md)
- [RFC-012 Rollout / Audit / Shadow Mode v1](rfc-012-rollout-audit-shadow.md)
- [RFC-013 Controller Deployment Topology v1](rfc-013-controller-topology.md)
- [RFC-014 Persistence and Storage Model v1](rfc-014-persistence-storage-model.md)
- [RFC-015 UI / Topology / Workflow Model v1](rfc-015-ui-topology-workflow.md)
- [RFC-016 Node-local Debug API Boundary v1](rfc-016-node-local-debug-api.md)
- [RFC-017 Multi-cluster / Region Model v1](rfc-017-multi-cluster-region.md)
- [RFC-018 Upgrade and Migration Model v1](rfc-018-upgrade-migration.md)

## 当前状态

当前规划内的主 RFC 已全部落地。`RFC-005A` 作为 Phase 3 的补充设计，用于冻结单节点 IaaS map schema 与 Mode B NAT 设计预留。
