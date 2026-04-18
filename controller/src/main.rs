mod api_handlers;
mod api_routes;
mod openapi;
mod request_id;
mod southbound_handlers;
mod store;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;

fn default_log_file_path() -> String {
    "/var/log/aria-controller/aria-controller.log".to_string()
}

fn default_log_filter() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "text".to_string()
}

fn init_tracing() -> Result<String, String> {
    let log_format =
        std::env::var("ARIA_CONTROLLER_LOG_FORMAT").unwrap_or_else(|_| default_log_format());
    let log_filter =
        std::env::var("ARIA_CONTROLLER_LOG_FILTER").unwrap_or_else(|_| default_log_filter());
    let log_file_path =
        std::env::var("ARIA_CONTROLLER_LOG_FILE_PATH").unwrap_or_else(|_| default_log_file_path());

    aria_logging::init_dual_tracing(&log_format, &log_filter, &log_file_path)
        .map_err(|e| e.replace("log_format", "ARIA_CONTROLLER_LOG_FORMAT"))?;

    Ok(log_file_path)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_file_path = init_tracing().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    info!(log_file_path = %log_file_path, "aria-controller logging initialized");

    let bind =
        std::env::var("ARIA_CONTROLLER_BIND").unwrap_or_else(|_| "127.0.0.1:8180".to_string());
    let addr: SocketAddr = bind.parse()?;

    let store: store::SharedStore = match std::env::var("ARIA_CONTROLLER_STATE_PATH") {
        Ok(path) => {
            info!("aria-controller using file-backed state at {}", path);
            Arc::new(store::FileBackedControllerStore::open(path).await?)
        }
        Err(_) => {
            info!("aria-controller using in-memory state backend");
            Arc::new(store::InMemoryControllerStore::new())
        }
    };
    let app = api_routes::build_router(store).layer(TraceLayer::new_for_http());
    let listener = TcpListener::bind(addr).await?;

    info!("aria-controller listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
