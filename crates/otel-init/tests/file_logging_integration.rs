//! Integration tests for file-based logging.
//!
//! This test verifies that the file logging layer actually writes
//! log entries to disk, addressing the issue where log files were
//! created but remained 0 bytes.

#![cfg(feature = "file-logging")]

use otel_init::file_logging::{FileLoggingConfig, RotationConfig, build_file_writer};
use std::fs;
use tempfile::TempDir;
use tracing_subscriber::{fmt, prelude::*};

#[test]
fn test_file_logging_writes_bytes_to_disk() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("test.log");

    let config = FileLoggingConfig::builder()
        .enabled(true)
        .file_path(log_path.to_string_lossy())
        .rotation(RotationConfig::default())
        .build()
        .unwrap();

    // Build the file writer
    let (writer, guard) = build_file_writer(&config)
        .unwrap()
        .expect("File logging should be enabled");

    // Build a minimal subscriber with just the file layer
    let subscriber = tracing_subscriber::registry()
        .with(fmt::layer().with_writer(writer).json().with_target(false));

    // Initialize the subscriber for this test
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");

    // Emit a test log
    tracing::info!("test message from integration test");

    // Drop the guard to ensure logs are flushed to disk
    drop(guard);

    // Give the background thread a moment to flush
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Check if any files were created in the temp directory
    let entries: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    // If no files found, the test should fail
    if entries.is_empty() {
        panic!("No log files were created in {:?}", temp_dir.path());
    }

    // Find the actual log file (may have date suffix due to rotation)
    let log_file = entries.iter().find(|e| {
        e.path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("test"))
            .unwrap_or(false)
    });

    let log_file = match log_file {
        Some(f) => f,
        None => {
            let file_names: Vec<_> = entries
                .iter()
                .filter_map(|e| {
                    e.path()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(String::from)
                })
                .collect();
            panic!("No test.log file found. Entries: {:?}", file_names);
        }
    };

    // Verify bytes hit disk
    let log_contents =
        fs::read_to_string(log_file.path()).expect("Log file should exist and be readable");

    assert!(!log_contents.is_empty(), "Log file should contain data");
    assert!(
        log_contents.contains("test message from integration test"),
        "Log file should contain our test message. Contents: {}",
        log_contents
    );
}
