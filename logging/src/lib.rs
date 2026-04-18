use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::EnvFilter;

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

pub fn init_dual_tracing(
    log_format: &str,
    log_filter: &str,
    log_file_path: &str,
) -> Result<(), String> {
    match log_format.to_ascii_lowercase().as_str() {
        "text" => tracing_subscriber::fmt()
            .compact()
            .with_env_filter(build_env_filter(log_filter)?)
            .with_target(true)
            .with_thread_names(true)
            .with_writer(build_log_writer(log_file_path))
            .try_init()
            .map_err(|e| format!("failed to initialize text logger: {}", e)),
        "json" => tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_env_filter(build_env_filter(log_filter)?)
            .with_target(true)
            .with_thread_names(true)
            .with_writer(build_log_writer(log_file_path))
            .try_init()
            .map_err(|e| format!("failed to initialize json logger: {}", e)),
        other => Err(format!(
            "unsupported log_format '{}': expected 'text' or 'json'",
            other
        )),
    }
}
