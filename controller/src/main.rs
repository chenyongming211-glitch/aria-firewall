mod api_handlers;
mod api_routes;
mod openapi;
mod request_id;
mod southbound_handlers;
mod store;

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::EnvFilter;

fn default_log_file_path() -> String {
    "/var/log/aria-controller/aria-controller.log".to_string()
}

fn default_log_filter() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "text".to_string()
}

#[derive(Clone)]
struct DualMakeWriter {
    file: Option<Arc<Mutex<File>>>,
}

struct DualWriter {
    stdout: io::Stdout,
    file: Option<Arc<Mutex<File>>>,
}

impl<'a> MakeWriter<'a> for DualMakeWriter {
    type Writer = DualWriter;

    fn make_writer(&'a self) -> Self::Writer {
        DualWriter {
            stdout: io::stdout(),
            file: self.file.clone(),
        }
    }
}

impl Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdout.write_all(buf)?;
        if let Some(file) = &self.file {
            let mut file = file
                .lock()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "log file mutex poisoned"))?;
            file.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()?;
        if let Some(file) = &self.file {
            let mut file = file
                .lock()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "log file mutex poisoned"))?;
            file.flush()?;
        }
        Ok(())
    }
}

fn build_env_filter(log_filter: &str) -> Result<EnvFilter, String> {
    EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_filter))
        .map_err(|e| format!("failed to build log filter: {}", e))
}

fn build_log_writer(log_file_path: &str) -> DualMakeWriter {
    let path = log_file_path.trim();
    if path.is_empty() {
        return DualMakeWriter { file: None };
    }

    let log_path = PathBuf::from(path);
    let parent = log_path.parent().unwrap_or_else(|| Path::new("."));
    if let Err(e) = std::fs::create_dir_all(parent) {
        eprintln!(
            "Warning: failed to create log directory {:?}: {}; file logging disabled",
            parent, e
        );
        return DualMakeWriter { file: None };
    }

    let file = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!(
                "Warning: failed to open log file {:?}: {}; file logging disabled",
                log_path, e
            );
            return DualMakeWriter { file: None };
        }
    };

    DualMakeWriter {
        file: Some(Arc::new(Mutex::new(file))),
    }
}

fn init_tracing() -> Result<String, String> {
    let log_format =
        std::env::var("ARIA_CONTROLLER_LOG_FORMAT").unwrap_or_else(|_| default_log_format());
    let log_filter =
        std::env::var("ARIA_CONTROLLER_LOG_FILTER").unwrap_or_else(|_| default_log_filter());
    let log_file_path =
        std::env::var("ARIA_CONTROLLER_LOG_FILE_PATH").unwrap_or_else(|_| default_log_file_path());

    match log_format.to_ascii_lowercase().as_str() {
        "text" => tracing_subscriber::fmt()
            .compact()
            .with_env_filter(build_env_filter(&log_filter)?)
            .with_target(true)
            .with_thread_names(true)
            .with_writer(build_log_writer(&log_file_path))
            .try_init()
            .map_err(|e| format!("failed to initialize text logger: {}", e))?,
        "json" => tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_env_filter(build_env_filter(&log_filter)?)
            .with_target(true)
            .with_thread_names(true)
            .with_writer(build_log_writer(&log_file_path))
            .try_init()
            .map_err(|e| format!("failed to initialize json logger: {}", e))?,
        other => {
            return Err(format!(
                "unsupported ARIA_CONTROLLER_LOG_FORMAT '{}': expected 'text' or 'json'",
                other
            ));
        }
    }

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
