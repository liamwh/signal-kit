//! File-based logging for the signal-kit crate.
//!
//! This module provides file-based logging capabilities as a composable
//! layer for the tracing subscriber. It supports time-based log rotation
//! and automatic cleanup of old log files.

mod config;
#[cfg(feature = "file-logging")]
mod rotation;

pub use config::{FileLoggingConfig, FileLoggingConfigBuilder, RotationConfig, RotationStrategy};

use std::io;
#[cfg(feature = "file-logging")]
use std::path::Path;
#[cfg(feature = "file-logging")]
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};

#[cfg(feature = "file-logging")]
use rotation::{cleanup_old_logs, create_rolling_appender, split_dir_and_prefix};

/// Errors that can occur during file logging initialization.
///
/// Note: `Disabled` is not an error - use `Ok(None)` to indicate disabled.
#[derive(Debug, thiserror::Error)]
pub enum FileLoggingError {
    /// Invalid log file path provided.
    #[error("invalid log file path: {0}")]
    InvalidPath(String),

    /// IO error occurred during file logging operations.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

/// Builds a file logging writer and guard.
///
/// This returns the non-blocking writer and worker guard, allowing the
/// fmt layer to be constructed inline with proper type inference.
///
/// # Arguments
///
/// * `config` - File logging configuration
///
/// # Returns
///
/// - `Ok(Some((writer, guard)))` - File logging enabled
/// - `Ok(None)` - File logging disabled
/// - `Err(e)` - Configuration error
#[cfg(feature = "file-logging")]
pub fn build_file_writer(
    config: &FileLoggingConfig,
) -> Result<Option<(NonBlocking, WorkerGuard)>, FileLoggingError> {
    if !config.enabled {
        return Ok(None);
    }

    if config.file_path.is_empty() {
        return Err(FileLoggingError::InvalidPath(
            "file_path must be set when file logging is enabled".to_string(),
        ));
    }

    let file_path = Path::new(&config.file_path);
    let appender = create_rolling_appender(file_path, config.rotation.strategy)?;
    let (dir, prefix) = split_dir_and_prefix(file_path)?;

    // Cleanup old logs
    let _ = cleanup_old_logs(
        &dir,
        &prefix,
        config.rotation.strategy,
        config.rotation.retention_days,
    );

    let (non_blocking, guard) = tracing_appender::non_blocking(appender);

    Ok(Some((non_blocking, guard)))
}

#[cfg(all(test, feature = "file-logging"))]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_build_file_writer_returns_none_when_disabled() {
        let config = FileLoggingConfig {
            enabled: false,
            ..Default::default()
        };

        let result = build_file_writer(&config).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_build_file_writer_errors_with_empty_path() {
        let config = FileLoggingConfig {
            enabled: true,
            file_path: String::new(),
            ..Default::default()
        };

        let result = build_file_writer(&config);
        assert!(result.is_err());
        assert!(matches!(result, Err(FileLoggingError::InvalidPath(_))));
    }

    #[test]
    fn test_build_file_writer_creates_writer() {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.log");

        let config = FileLoggingConfig {
            enabled: true,
            file_path: log_path.to_str().unwrap().to_string(),
            rotation: RotationConfig {
                strategy: RotationStrategy::Never,
                retention_days: 0,
            },
        };

        let result = build_file_writer(&config);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        // The file should be created when the layer starts writing
        // (This is a basic check that the writer was created)
    }

    #[test]
    fn test_cleanup_old_logs_safe_pattern_matching() {
        let temp_dir = TempDir::new().unwrap();

        // Create various files to test pattern matching safety
        fs::write(temp_dir.path().join("service.2026-01-17"), "log1").unwrap();
        fs::write(temp_dir.path().join("service.2026-01-16"), "log2").unwrap();
        fs::write(temp_dir.path().join("service.2026-01-15"), "log3").unwrap();

        // Create files that should NOT be deleted
        fs::write(temp_dir.path().join("service.2026-01-17.backup"), "backup").unwrap();
        fs::write(temp_dir.path().join("service.2026-01-17.gz"), "gz").unwrap();
        fs::write(temp_dir.path().join("other.2026-01-17"), "other").unwrap();
        fs::write(temp_dir.path().join("service_bak.log"), "bak").unwrap();

        // Run cleanup with 1 day retention (should delete files older than 1 day)
        // We need to make the old files actually old by setting their modification time
        // For this test, we'll just verify the pattern matching logic

        let entries = fs::read_dir(temp_dir.path()).unwrap();
        let matching_files: Vec<_> = entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| rotation::matches_log_pattern_strict(n, "service", RotationStrategy::Daily))
            .collect();

        // Should only match the exact daily rotation files
        assert_eq!(matching_files.len(), 3);
        assert!(matching_files.contains(&"service.2026-01-17".to_string()));
        assert!(matching_files.contains(&"service.2026-01-16".to_string()));
        assert!(matching_files.contains(&"service.2026-01-15".to_string()));
    }

    #[test]
    fn test_build_file_writer_disabled_returns_none() {
        let config = FileLoggingConfig {
            enabled: false,
            ..Default::default()
        };

        let result = build_file_writer(&config).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_build_file_writer_empty_path_errors() {
        let config = FileLoggingConfig {
            enabled: true,
            file_path: String::new(),
            ..Default::default()
        };

        let result = build_file_writer(&config);
        assert!(result.is_err());
        assert!(matches!(result, Err(FileLoggingError::InvalidPath(_))));
    }

    #[test]
    fn test_build_file_writer_returns_writer_and_guard() {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.log");

        let config = FileLoggingConfig {
            enabled: true,
            file_path: log_path.to_str().unwrap().to_string(),
            rotation: RotationConfig {
                strategy: RotationStrategy::Never,
                retention_days: 0,
            },
        };

        let result = build_file_writer(&config);
        assert!(result.is_ok());
        let option = result.unwrap();
        assert!(option.is_some());
        let (_writer, _guard) = option.unwrap();

        // The guard keeps the appender alive
        // Dropping the guard would flush logs
    }
}
