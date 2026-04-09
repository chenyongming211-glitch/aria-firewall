mod api_handlers;
mod api_routes;
mod openapi;
mod request_id;
mod southbound_handlers;
mod store;

use std::{net::SocketAddr, sync::Arc};

use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

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
