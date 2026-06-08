//! File logging configuration.
//!
//! This module provides configuration structures for file-based logging
//! with rotation support.

use serde::{Deserialize, Serialize};

/// Configuration for file-based logging.
///
/// # Note
///
/// File logging is primarily intended for development and debugging.
/// In production, ensure logs do not contain PII or sensitive data.
///
/// # Error Behavior
///
/// If file logging is enabled and cannot be initialized (e.g., invalid path,
/// permission denied, unwritable directory), the application will **panic**
/// with a clear error message. This fail-fast behavior ensures that logging
/// issues are detected immediately rather than silently losing logs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct FileLoggingConfig {
    /// Whether file logging is enabled.
    ///
    /// Defaults to `false` - requires explicit opt-in.
    #[serde(default)]
    pub enabled: bool,

    /// Full path to the log file (e.g., `/var/log/myapp/service.log`).
    ///
    /// Note: With `rolling::daily` or `rolling::hourly`, the actual log files
    /// will have date suffixes (e.g., `service.2026-01-17`), not the exact
    /// path specified here. This path is used to derive the directory and
    /// filename prefix.
    #[serde(default)]
    pub file_path: String,

    /// Log rotation settings.
    #[serde(default)]
    pub rotation: RotationConfig,
}

/// Log rotation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RotationConfig {
    /// Rotation strategy.
    #[serde(default)]
    pub strategy: RotationStrategy,

    /// Number of days to retain rotated log files.
    ///
    /// Only files matching the log filename pattern will be deleted.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

const fn default_retention_days() -> u32 {
    7
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            strategy: RotationStrategy::default(),
            retention_days: default_retention_days(),
        }
    }
}

impl RotationConfig {
    /// Creates a daily rotation config with default retention.
    pub const fn daily() -> Self {
        Self {
            strategy: RotationStrategy::Daily,
            retention_days: default_retention_days(),
        }
    }

    /// Creates an hourly rotation config with default retention.
    pub const fn hourly() -> Self {
        Self {
            strategy: RotationStrategy::Hourly,
            retention_days: default_retention_days(),
        }
    }

    /// Creates a no-rotation config with default retention.
    pub const fn never() -> Self {
        Self {
            strategy: RotationStrategy::Never,
            retention_days: default_retention_days(),
        }
    }
}

/// Rotation strategy for log files.
///
/// `tracing_appender::rolling` creates files with date suffixes:
/// - `Daily`: `service.2026-01-17`, `service.2026-01-16`, ...
/// - `Hourly`: `service.2026-01-17.14`, `service.2026-01-17.13`, ...
/// - `Never`: `service` (no rotation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum RotationStrategy {
    /// Rotate logs daily (creates `service.YYYY-MM-DD` files)
    #[default]
    Daily,
    /// Rotate logs hourly (creates `service.YYYY-MM-DD.HH` files)
    Hourly,
    /// Do not rotate logs (creates a single `service` file)
    Never,
}

impl RotationStrategy {
    /// Returns the string representation of the strategy.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Hourly => "hourly",
            Self::Never => "never",
        }
    }
}

impl FileLoggingConfig {
    /// Returns a builder for constructing a `FileLoggingConfig`.
    pub fn builder() -> FileLoggingConfigBuilder {
        FileLoggingConfigBuilder::default()
    }

    /// Applies environment variable overrides to the configuration.
    ///
    /// Environment variables take precedence over values in the config struct.
    ///
    /// Supported environment variables:
    /// - `OTEL_INIT_FILE_ENABLED`: "true", "false", "1", or "0"
    /// - `OTEL_INIT_FILE_PATH`: Full path to the log file
    /// - `OTEL_INIT_FILE_ROTATION`: "daily", "hourly", or "never"
    /// - `OTEL_INIT_FILE_RETENTION_DAYS`: Number of days (u32)
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(val) = std::env::var("OTEL_INIT_FILE_ENABLED") {
            self.enabled = parse_bool(&val).unwrap_or(self.enabled);
        }
        if let Ok(val) = std::env::var("OTEL_INIT_FILE_PATH")
            && !val.trim().is_empty()
        {
            self.file_path = val;
        }
        if let Ok(val) = std::env::var("OTEL_INIT_FILE_ROTATION") {
            self.rotation.strategy = match val.to_lowercase().as_str() {
                "daily" => RotationStrategy::Daily,
                "hourly" => RotationStrategy::Hourly,
                "never" => RotationStrategy::Never,
                _ => self.rotation.strategy, // Keep existing if invalid
            };
        }
        if let Ok(val) = std::env::var("OTEL_INIT_FILE_RETENTION_DAYS") {
            self.rotation.retention_days = val.parse().unwrap_or(self.rotation.retention_days);
        }
        self
    }
}

/// Builder for `FileLoggingConfig`.
#[derive(Debug, Default)]
pub struct FileLoggingConfigBuilder {
    enabled: bool,
    file_path: Option<String>,
    rotation: RotationConfig,
}

impl FileLoggingConfigBuilder {
    /// Sets whether file logging is enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the log file path.
    pub fn file_path(mut self, file_path: impl Into<String>) -> Self {
        self.file_path = Some(file_path.into());
        self
    }

    /// Sets the rotation configuration.
    pub fn rotation(mut self, rotation: RotationConfig) -> Self {
        self.rotation = rotation;
        self
    }

    /// Builds the configuration, returning an error if enabled without a path.
    pub fn build(self) -> Result<FileLoggingConfig, crate::file_logging::FileLoggingError> {
        let file_path = self.file_path.unwrap_or_default();
        if self.enabled && file_path.trim().is_empty() {
            return Err(crate::file_logging::FileLoggingError::InvalidPath(
                "file_path must be set when file logging is enabled".to_string(),
            ));
        }

        Ok(FileLoggingConfig {
            enabled: self.enabled,
            file_path,
            rotation: self.rotation,
        })
    }
}

/// Parses a boolean value from string, accepting "true"/"false" and "1"/"0".
fn parse_bool(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_file_logging_config() {
        let config = FileLoggingConfig::default();
        assert!(!config.enabled);
        assert!(config.file_path.is_empty());
        assert_eq!(config.rotation.strategy, RotationStrategy::Daily);
        assert_eq!(config.rotation.retention_days, 7);
    }

    #[test]
    fn test_default_rotation_config() {
        let config = RotationConfig::default();
        assert_eq!(config.strategy, RotationStrategy::Daily);
        assert_eq!(config.retention_days, 7);
    }

    #[test]
    fn test_rotation_strategy_as_str() {
        assert_eq!(RotationStrategy::Daily.as_str(), "daily");
        assert_eq!(RotationStrategy::Hourly.as_str(), "hourly");
        assert_eq!(RotationStrategy::Never.as_str(), "never");
    }

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("FALSE"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("invalid"), None);
        assert_eq!(parse_bool("2"), None);
    }

    #[test]
    fn test_file_logging_config_with_env_overrides() {
        // Test with no env vars set - should use defaults
        let config = FileLoggingConfig::default();
        let overridden = config.with_env_overrides();
        assert!(!overridden.enabled);
        assert!(overridden.file_path.is_empty());
    }

    #[test]
    fn test_rotation_strategy_serde_roundtrip() {
        let strategies = [
            RotationStrategy::Daily,
            RotationStrategy::Hourly,
            RotationStrategy::Never,
        ];

        for strategy in strategies {
            let serialized = serde_json::to_string(&strategy).unwrap();
            let deserialized: RotationStrategy = serde_json::from_str(&serialized).unwrap();
            assert_eq!(strategy, deserialized);
        }
    }

    #[test]
    fn test_file_logging_config_serde_roundtrip() {
        let config = FileLoggingConfig {
            enabled: true,
            file_path: "/var/log/test.log".to_string(),
            rotation: RotationConfig {
                strategy: RotationStrategy::Hourly,
                retention_days: 14,
            },
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: FileLoggingConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.enabled, config.enabled);
        assert_eq!(deserialized.file_path, config.file_path);
        assert_eq!(deserialized.rotation.strategy, config.rotation.strategy);
        assert_eq!(
            deserialized.rotation.retention_days,
            config.rotation.retention_days
        );
    }
}
