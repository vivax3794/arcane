//! Setups panic hooks for errors and contains logging widget

use std::path::PathBuf;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::prelude::*;
use crate::project_dirs;

/// Setup error panic hooks and tracing
pub fn setup() -> Result<()> {
    color_eyre::install()?;

    let log_path = if let Ok(log_path) = std::env::var("ARCANE_LOG") {
        PathBuf::from(log_path)
    } else {
        let dirs = project_dirs()?;
        dirs.data_dir().join("log.txt")
    };
    if let Some(parent_dir) = log_path.parent() {
        std::fs::create_dir_all(parent_dir)?;
    }
    let log_file = std::fs::File::create(log_path)?;

    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_writer(log_file);
    let error_subscriber = tracing_error::ErrorLayer::default();
    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(error_subscriber)
        .with(tui_logger::TuiTracingSubscriberLayer)
        .init();
    tui_logger::init_logger(log::LevelFilter::Info)?;

    event!(Level::INFO, "Installed error panic hook and tracing logs.");

    Ok(())
}
