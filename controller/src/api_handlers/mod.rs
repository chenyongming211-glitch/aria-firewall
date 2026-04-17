use aria_api::PlatformApiError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::{
    request_id,
    store::{SharedStore, StoreError},
};

mod backend_set;
mod health;
mod health_check;
pub(crate) mod helpers;
mod ip_group;
mod network;
mod network_policy;
mod node;
mod port;
mod qos_policy;
mod mirror_policy;
mod route_table;
mod security_group;
mod service;
mod tenant;

use self::helpers::{dependency_conflict_details, error_details, invalid_reference_details};

pub use self::{
    backend_set::{
        create_backend_set, delete_backend_set, get_backend_set, list_backend_sets,
        update_backend_set,
    },
    health::health,
    health_check::{
        create_health_check, delete_health_check, get_health_check, list_health_checks,
        update_health_check,
    },
    ip_group::{create_ip_group, delete_ip_group, get_ip_group, list_ip_groups, update_ip_group},
    network::{create_network, delete_network, get_network, list_networks, update_network},
    network_policy::{
        create_network_policy, delete_network_policy, get_network_policy, list_network_policies,
        update_network_policy,
    },
    node::{create_node, delete_node, get_node, list_nodes, update_node},
    port::{create_port, delete_port, get_port, list_ports, update_port},
    qos_policy::{
        create_qos_policy, delete_qos_policy, get_qos_policy, list_qos_policies,
        update_qos_policy,
    },
    mirror_policy::{
        create_mirror_policy, delete_mirror_policy, get_mirror_policy, list_mirror_policies,
        update_mirror_policy,
    },
    route_table::{
        create_route_table, delete_route_table, get_route_table, list_route_tables,
        update_route_table,
    },
    security_group::{
        create_security_group, delete_security_group, get_security_group, list_security_groups,
        update_security_group,
    },
    service::{create_service, delete_service, get_service, list_services, update_service},
    tenant::{create_tenant, delete_tenant, get_tenant, list_tenants, update_tenant},
};

pub(crate) type AppState = SharedStore;

#[derive(Debug)]
pub(crate) enum ControllerError {
    BadRequest(String),
    Conflict {
        resource: &'static str,
        id: String,
    },
    DependencyConflict {
        resource: &'static str,
        id: String,
        dependent_resource: &'static str,
        dependent_id: String,
    },
    Internal(String),
    InvalidReference {
        resource: &'static str,
        field: &'static str,
        value: String,
        referenced_resource: &'static str,
    },
    NotFound {
        resource: &'static str,
        id: String,
    },
}

impl From<StoreError> for ControllerError {
    fn from(value: StoreError) -> Self {
        match value {
            StoreError::AlreadyExists { resource, id } => Self::Conflict { resource, id },
            StoreError::BadRequest(message) => Self::BadRequest(message),
            StoreError::DependencyConflict {
                resource,
                id,
                dependent_resource,
                dependent_id,
            } => Self::DependencyConflict {
                resource,
                id,
                dependent_resource,
                dependent_id,
            },
            StoreError::Internal(message) => Self::Internal(message),
            StoreError::InvalidReference {
                resource,
                field,
                value,
                referenced_resource,
            } => Self::InvalidReference {
                resource,
                field,
                value,
                referenced_resource,
            },
            StoreError::NotFound { resource, id } => Self::NotFound { resource, id },
        }
    }
}

impl IntoResponse for ControllerError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self {
            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                "invalid_request".to_string(),
                message,
                None,
            ),
            Self::Conflict { resource, id } => (
                StatusCode::CONFLICT,
                "resource_conflict".to_string(),
                format!("{resource} '{id}' already exists"),
                Some(error_details(resource, &id)),
            ),
            Self::DependencyConflict {
                resource,
                id,
                dependent_resource,
                dependent_id,
            } => (
                StatusCode::CONFLICT,
                "dependency_conflict".to_string(),
                format!(
                    "{resource} '{id}' is still referenced by {dependent_resource} '{dependent_id}'"
                ),
                Some(dependency_conflict_details(
                    resource,
                    &id,
                    dependent_resource,
                    &dependent_id,
                )),
            ),
            Self::Internal(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error".to_string(),
                message,
                None,
            ),
            Self::InvalidReference {
                resource,
                field,
                value,
                referenced_resource,
            } => (
                StatusCode::BAD_REQUEST,
                "invalid_reference".to_string(),
                format!(
                    "{resource} field '{field}' references missing {referenced_resource} '{value}'"
                ),
                Some(invalid_reference_details(
                    resource,
                    field,
                    &value,
                    referenced_resource,
                )),
            ),
            Self::NotFound { resource, id } => (
                StatusCode::NOT_FOUND,
                "resource_not_found".to_string(),
                format!("{resource} '{id}' was not found"),
                Some(error_details(resource, &id)),
            ),
        };

        (
            status,
            Json(PlatformApiError {
                code,
                message,
                request_id: request_id::current_request_id(),
                details,
            }),
        )
            .into_response()
    }
}
