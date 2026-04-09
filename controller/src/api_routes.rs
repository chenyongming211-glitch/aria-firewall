use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{api_handlers, store::PlatformStore};

pub fn build_router(store: Arc<PlatformStore>) -> Router {
    Router::new()
        .merge(SwaggerUi::new("/docs").url("/openapi.json", crate::openapi::ApiDoc::openapi()))
        .route("/api/v1/health", get(api_handlers::health))
        .route(
            "/api/v1/tenants",
            get(api_handlers::list_tenants).post(api_handlers::create_tenant),
        )
        .route(
            "/api/v1/tenants/{id}",
            get(api_handlers::get_tenant)
                .put(api_handlers::update_tenant)
                .delete(api_handlers::delete_tenant),
        )
        .route(
            "/api/v1/nodes",
            get(api_handlers::list_nodes).post(api_handlers::create_node),
        )
        .route(
            "/api/v1/nodes/{id}",
            get(api_handlers::get_node)
                .put(api_handlers::update_node)
                .delete(api_handlers::delete_node),
        )
        .route(
            "/api/v1/networks",
            get(api_handlers::list_networks).post(api_handlers::create_network),
        )
        .route(
            "/api/v1/networks/{id}",
            get(api_handlers::get_network)
                .put(api_handlers::update_network)
                .delete(api_handlers::delete_network),
        )
        .route(
            "/api/v1/ports",
            get(api_handlers::list_ports).post(api_handlers::create_port),
        )
        .route(
            "/api/v1/ports/{id}",
            get(api_handlers::get_port)
                .put(api_handlers::update_port)
                .delete(api_handlers::delete_port),
        )
        .route(
            "/api/v1/security-groups",
            get(api_handlers::list_security_groups).post(api_handlers::create_security_group),
        )
        .route(
            "/api/v1/security-groups/{id}",
            get(api_handlers::get_security_group)
                .put(api_handlers::update_security_group)
                .delete(api_handlers::delete_security_group),
        )
        .route(
            "/api/v1/route-tables",
            get(api_handlers::list_route_tables).post(api_handlers::create_route_table),
        )
        .route(
            "/api/v1/route-tables/{id}",
            get(api_handlers::get_route_table)
                .put(api_handlers::update_route_table)
                .delete(api_handlers::delete_route_table),
        )
        .with_state(store)
}
