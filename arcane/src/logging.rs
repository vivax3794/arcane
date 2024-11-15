//! Setups panic hooks for errors and contains logging widget

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

use crate::prelude::*;
use crate::project_dirs;

/// How many bytes of log data to keep in memory
const LOGGER_MEMORY_MAX: usize = 1_000_000;

/// The in memory logger
pub type Logger = Arc<Mutex<Vec<u8>>>;

/// A writer that writes to a vector behind a mutex
struct MemoryLogger {
    /// The vector to write to
    logger: Logger,
}

impl std::io::Write for MemoryLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let Ok(mut logger) = self.logger.lock() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Mutex Posioned",
            ));
        };

        let result = logger.write(buf);
        if logger.len() > LOGGER_MEMORY_MAX {
            let drain_end = logger.len().saturating_sub(LOGGER_MEMORY_MAX);
            logger.drain(0..drain_end);
        }
        result
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let Ok(mut logger) = self.logger.lock() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Mutex Posioned",
            ));
        };

        logger.flush()
    }
}

/// A `MakeWriter` that clones the arc within it
struct MemoryLoggerFactory {
    /// The vector to write to
    logger: Logger,
}

impl<'a> MakeWriter<'a> for MemoryLoggerFactory {
    type Writer = MemoryLogger;

    fn make_writer(&'a self) -> Self::Writer {
        MemoryLogger {
            logger: Arc::clone(&self.logger),
        }
    }
}

/// Setup error panic hooks and tracing
///
/// Returns a reference to the log output
#[errors(color_eyre::Report, std::io::Error)]
pub fn setup() -> Result<Logger> {
    color_eyre::install()?;

    let log_path = if let Ok(log_path) = std::env::var("ARCANE_LOG") {
        PathBuf::from(log_path)
    } else if let Some(dirs) = project_dirs() {
        dirs.data_dir().join("log.txt")
    } else {
        PathBuf::from("./log.txt")
    };
    if let Some(parent_dir) = log_path.parent() {
        std::fs::create_dir_all(parent_dir)?;
    }
    let log_file = std::fs::File::create(log_path)?;

    let in_app_logs = Arc::default();

    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_ansi(cfg!(feature = "ansi_log_file"))
        .with_writer(log_file);

    let in_app_log_subscriber = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .without_time()
        .with_writer(MemoryLoggerFactory {
            logger: Arc::clone(&in_app_logs),
        })
        .with_filter(LevelFilter::from_level(Level::DEBUG));
    let error_subscriber = tracing_error::ErrorLayer::default();
    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(in_app_log_subscriber)
        .with(error_subscriber)
        .init();

    event!(Level::INFO, "Installed error panic hook and tracing logs.");

    Ok(in_app_logs)
}
