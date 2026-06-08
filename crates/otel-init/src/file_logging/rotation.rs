//! Log rotation and retention cleanup.
//!
//! This module provides utilities for creating rolling file appenders
//! and safely cleaning up old log files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tracing_appender::rolling;

use super::config::RotationStrategy;

/// Cleans up log files older than the retention period.
///
/// # Safety
///
/// This function ONLY deletes files that exactly match the pattern produced
/// by `tracing_appender::rolling`. This prevents accidental deletion of
/// unrelated files that happen to share a prefix.
///
/// # Pattern Matching
///
/// - For `Daily` rotation: matches `prefix.YYYY-MM-DD` exactly (11 chars suffix)
/// - For `Hourly` rotation: matches `prefix.YYYY-MM-DD.HH` exactly (14 chars suffix)
/// - For `Never` rotation: matches exactly `prefix`
///
/// Examples of what gets deleted with prefix "service":
/// - Daily: `service.2026-01-17` ✓
/// - Hourly: `service.2026-01-17.14` ✓
/// - Never: `service` ✓
///
/// Examples of what is NOT deleted:
/// - `service.2026-01-17.backup` ✗ (wrong suffix)
/// - `service.2026-01-17.gz` ✗ (wrong suffix)
/// - `service_bak.log` ✗ (doesn't match pattern)
/// - `other-service.2026-01-17` ✗ (different prefix)
pub fn cleanup_old_logs(
    dir: &Path,
    filename_prefix: &str,
    strategy: RotationStrategy,
    retention_days: u32,
) -> io::Result<()> {
    if retention_days == 0 || filename_prefix.is_empty() {
        return Ok(());
    }

    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(86_400 * retention_days as u64))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let entries = fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name_str = match file_name.to_str() {
            Some(s) => s,
            None => continue,
        };

        // Strict pattern matching - only delete exact matches
        if !matches_log_pattern_strict(file_name_str, filename_prefix, strategy) {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if !meta.is_file() {
            continue;
        }

        let modified = match meta.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };

        if modified < cutoff {
            // Best effort - ignore errors during cleanup
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// Checks if a filename matches the expected log pattern for the rotation strategy.
///
/// This performs strict validation to prevent accidental deletion of unrelated files.
/// Only patterns exactly matching what `tracing_appender::rolling` produces are accepted.
///
/// This function is public to allow testing of the pattern matching logic, which is
/// critical for safety in the cleanup functionality.
pub fn matches_log_pattern_strict(
    filename: &str,
    prefix: &str,
    strategy: RotationStrategy,
) -> bool {
    match strategy {
        RotationStrategy::Never => {
            // Exactly the prefix, nothing more
            filename == prefix
        }
        RotationStrategy::Daily => {
            // prefix.YYYY-MM-DD exactly
            // Length: prefix.len() + 11 (dot + 10 chars date)
            let expected_len = prefix.len() + 11;
            if filename.len() != expected_len {
                return false;
            }
            if !filename.starts_with(prefix) {
                return false;
            }
            let suffix = &filename[prefix.len()..];
            // Check format: .YYYY-MM-DD
            // Positions: .0123456789
            //            .YYYY-MM-DD
            if suffix.len() != 11 {
                return false;
            }
            if suffix.as_bytes()[0] != b'.' {
                return false;
            }
            if suffix.as_bytes()[5] != b'-' {
                return false;
            }
            if suffix.as_bytes()[8] != b'-' {
                return false;
            }
            if !suffix[1..5].bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            if !suffix[6..8].bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            if !suffix[9..].bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            true
        }
        RotationStrategy::Hourly => {
            // prefix.YYYY-MM-DD.HH exactly
            // Length: prefix.len() + 14 (dot + 10 chars date + dot + 2 chars hour)
            let expected_len = prefix.len() + 14;
            if filename.len() != expected_len {
                return false;
            }
            if !filename.starts_with(prefix) {
                return false;
            }
            let suffix = &filename[prefix.len()..];
            // Check format: .YYYY-MM-DD.HH
            // Positions: .01234567890123
            //            .YYYY-MM-DD.HH
            if suffix.len() != 14 {
                return false;
            }
            if suffix.as_bytes()[0] != b'.' {
                return false;
            }
            if suffix.as_bytes()[5] != b'-' {
                return false;
            }
            if suffix.as_bytes()[8] != b'-' {
                return false;
            }
            if suffix.as_bytes()[11] != b'.' {
                return false;
            }
            if !suffix[1..5].bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            if !suffix[6..8].bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            if !suffix[9..11].bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            if !suffix[12..].bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            true
        }
    }
}

/// Creates a rolling file appender based on the rotation strategy.
pub fn create_rolling_appender(
    file_path: &Path,
    strategy: RotationStrategy,
) -> io::Result<rolling::RollingFileAppender> {
    let (dir, prefix) = split_dir_and_prefix(file_path)?;

    // Ensure directory exists
    fs::create_dir_all(&dir)?;

    match strategy {
        RotationStrategy::Daily => Ok(rolling::daily(&dir, prefix)),
        RotationStrategy::Hourly => Ok(rolling::hourly(&dir, prefix)),
        RotationStrategy::Never => Ok(rolling::never(&dir, prefix)),
    }
}

/// Splits a file path into directory and filename prefix.
///
/// For `/var/log/myapp/service.log`, returns (`/var/log/myapp`, `service`).
pub fn split_dir_and_prefix(path: &Path) -> io::Result<(PathBuf, String)> {
    // Check if the path ends with '/' (indicates a directory, not a file)
    if let Some(s) = path.to_str()
        && s.ends_with('/')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path ends with '/', expected a file path not a directory",
        ));
    }

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid log file path"))?;

    // Remove .log extension if present for the prefix
    let prefix = file_name.strip_suffix(".log").unwrap_or(file_name);

    if prefix.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "log file prefix cannot be empty",
        ));
    }

    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    Ok((dir, prefix.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_log_pattern_strict_never() {
        assert!(matches_log_pattern_strict(
            "service",
            "service",
            RotationStrategy::Never
        ));
        assert!(!matches_log_pattern_strict(
            "service.log",
            "service",
            RotationStrategy::Never
        ));
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17",
            "service",
            RotationStrategy::Never
        ));
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17.14",
            "service",
            RotationStrategy::Never
        ));
    }

    #[test]
    fn test_matches_log_pattern_strict_daily() {
        // Valid daily patterns
        assert!(matches_log_pattern_strict(
            "service.2026-01-17",
            "service",
            RotationStrategy::Daily
        ));
        assert!(matches_log_pattern_strict(
            "app.1999-12-31",
            "app",
            RotationStrategy::Daily
        ));
        assert!(matches_log_pattern_strict(
            "my-service.2026-01-17",
            "my-service",
            RotationStrategy::Daily
        ));

        // Invalid patterns - wrong length
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17.backup",
            "service",
            RotationStrategy::Daily
        ));
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17.gz",
            "service",
            RotationStrategy::Daily
        ));
        assert!(!matches_log_pattern_strict(
            "service.2026-1-17",
            "service",
            RotationStrategy::Daily
        )); // Missing leading zero

        // Invalid patterns - wrong prefix
        assert!(!matches_log_pattern_strict(
            "other.2026-01-17",
            "service",
            RotationStrategy::Daily
        ));

        // Invalid patterns - wrong format
        assert!(!matches_log_pattern_strict(
            "service.2026/01/17",
            "service",
            RotationStrategy::Daily
        ));
        assert!(!matches_log_pattern_strict(
            "service.20260117",
            "service",
            RotationStrategy::Daily
        ));
    }

    #[test]
    fn test_matches_log_pattern_strict_hourly() {
        // Valid hourly patterns
        assert!(matches_log_pattern_strict(
            "service.2026-01-17.14",
            "service",
            RotationStrategy::Hourly
        ));
        assert!(matches_log_pattern_strict(
            "app.1999-12-31.23",
            "app",
            RotationStrategy::Hourly
        ));
        assert!(matches_log_pattern_strict(
            "my-service.2026-01-17.00",
            "my-service",
            RotationStrategy::Hourly
        ));

        // Invalid patterns - wrong length
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17.14.backup",
            "service",
            RotationStrategy::Hourly
        ));
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17.1",
            "service",
            RotationStrategy::Hourly
        )); // Hour should be 2 digits
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17.141",
            "service",
            RotationStrategy::Hourly
        )); // Hour should be 2 digits

        // Invalid patterns - wrong format
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17-14",
            "service",
            RotationStrategy::Hourly
        ));
        assert!(!matches_log_pattern_strict(
            "service.2026-01-17:14",
            "service",
            RotationStrategy::Hourly
        ));
    }

    #[test]
    fn test_split_dir_and_prefix() {
        // Test with .log extension
        let (dir, prefix) = split_dir_and_prefix(Path::new("/var/log/myapp/service.log")).unwrap();
        assert_eq!(dir, PathBuf::from("/var/log/myapp"));
        assert_eq!(prefix, "service");

        // Test without .log extension
        let (dir, prefix) = split_dir_and_prefix(Path::new("/var/log/myapp/service")).unwrap();
        assert_eq!(dir, PathBuf::from("/var/log/myapp"));
        assert_eq!(prefix, "service");

        // Test with relative path
        let (dir, prefix) = split_dir_and_prefix(Path::new("./logs/app.log")).unwrap();
        assert_eq!(dir, PathBuf::from("./logs"));
        assert_eq!(prefix, "app");

        // Test with just filename
        let (dir, prefix) = split_dir_and_prefix(Path::new("service.log")).unwrap();
        assert_eq!(dir, PathBuf::from("."));
        assert_eq!(prefix, "service");

        // Test with empty prefix after stripping .log
        let result = split_dir_and_prefix(Path::new("/var/log/.log"));
        assert!(result.is_err());

        // Test with no filename
        let result = split_dir_and_prefix(Path::new("/var/log/"));
        assert!(result.is_err());
    }

    #[test]
    fn test_split_dir_and_prefix_multiple_extensions() {
        // Test with multiple extensions - only .log should be stripped
        let (dir, prefix) =
            split_dir_and_prefix(Path::new("/var/log/myapp/service.tar.log")).unwrap();
        assert_eq!(dir, PathBuf::from("/var/log/myapp"));
        assert_eq!(prefix, "service.tar");
    }
}
