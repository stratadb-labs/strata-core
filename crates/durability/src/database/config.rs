//! Database configuration
//!
//! Configuration for database storage behavior including durability mode,
//! WAL settings, and codec selection.

use crate::codec::get_codec;
use crate::wal::DurabilityMode;
use crate::wal::{WalConfig, WalConfigError};

/// Database configuration
///
/// Controls how the database handles persistence and durability.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Durability mode for commits
    pub durability: DurabilityMode,
    /// WAL configuration
    pub wal_config: WalConfig,
    /// Codec identifier (default: "identity")
    pub codec_id: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        DatabaseConfig {
            durability: DurabilityMode::Strict,
            wal_config: WalConfig::default(),
            codec_id: "identity".to_string(),
        }
    }
}

impl DatabaseConfig {
    /// Create config with strict durability (default)
    ///
    /// Every commit is fsynced before returning.
    pub fn strict() -> Self {
        DatabaseConfig {
            durability: DurabilityMode::Strict,
            ..Default::default()
        }
    }

    /// Create config with batched durability
    ///
    /// Commits are batched and fsynced periodically.
    /// Some committed transactions may be lost on crash.
    pub fn batched() -> Self {
        DatabaseConfig {
            durability: DurabilityMode::buffered_default(),
            ..Default::default()
        }
    }

    /// Create config for testing
    ///
    /// Uses small segment sizes for faster tests.
    pub fn for_testing() -> Self {
        DatabaseConfig {
            durability: DurabilityMode::Strict,
            wal_config: WalConfig::for_testing(),
            codec_id: "identity".to_string(),
        }
    }

    /// Set durability mode
    pub fn with_durability(mut self, mode: DurabilityMode) -> Self {
        self.durability = mode;
        self
    }

    /// Set WAL configuration
    pub fn with_wal_config(mut self, config: WalConfig) -> Self {
        self.wal_config = config;
        self
    }

    /// Set WAL segment size
    pub fn with_wal_segment_size(mut self, size: u64) -> Self {
        self.wal_config.segment_size = size;
        self
    }

    /// Set codec identifier
    pub fn with_codec(mut self, codec_id: impl Into<String>) -> Self {
        self.codec_id = codec_id.into();
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.wal_config.validate()?;
        get_codec(&self.codec_id).map_err(|e| ConfigError::InvalidCodec(e.to_string()))?;
        Ok(())
    }
}

/// Configuration validation errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Invalid WAL configuration
    #[error("Invalid WAL config: {0}")]
    InvalidWalConfig(#[from] WalConfigError),

    /// Invalid codec identifier
    #[error("Invalid codec: {0}")]
    InvalidCodec(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DatabaseConfig::default();
        assert!(matches!(config.durability, DurabilityMode::Strict));
        assert_eq!(config.codec_id, "identity");
    }

    #[test]
    fn test_strict_config() {
        let config = DatabaseConfig::strict();
        assert!(matches!(config.durability, DurabilityMode::Strict));
    }

    #[test]
    fn test_batched_config() {
        let config = DatabaseConfig::batched();
        assert!(matches!(config.durability, DurabilityMode::Batched { .. }));
    }

    #[test]
    fn test_builder_pattern() {
        let config = DatabaseConfig::default()
            .with_durability(DurabilityMode::Strict)
            .with_codec("identity")
            .with_wal_segment_size(1024 * 1024);

        assert!(matches!(config.durability, DurabilityMode::Strict));
        assert_eq!(config.codec_id, "identity");
        assert_eq!(config.wal_config.segment_size, 1024 * 1024);
    }

    #[test]
    fn test_validate_valid_config() {
        let config = DatabaseConfig::default();
        let result = config.validate();
        assert!(result.is_ok(), "Default config should be valid");

        // Verify the config has expected values after validation
        assert_eq!(config.codec_id, "identity");
        assert!(matches!(config.durability, DurabilityMode::Strict));

        // Verify WAL config is also valid
        assert!(config.wal_config.segment_size > 0);
        assert!(config.wal_config.buffered_sync_bytes <= config.wal_config.segment_size);
    }

    #[test]
    fn test_validate_valid_config_batched() {
        let config = DatabaseConfig::batched();
        let result = config.validate();
        assert!(result.is_ok(), "Batched config should be valid");
        assert!(matches!(config.durability, DurabilityMode::Batched { .. }));
    }

    #[test]
    fn test_validate_valid_config_custom() {
        // Segment size of 128MB (larger than default buffered_sync_bytes of 4MB)
        let config = DatabaseConfig::default()
            .with_codec("identity")
            .with_wal_segment_size(128 * 1024 * 1024);

        let result = config.validate();
        assert!(result.is_ok(), "Custom config should be valid");
        assert_eq!(config.wal_config.segment_size, 128 * 1024 * 1024);
    }

    #[test]
    fn test_validate_invalid_codec() {
        let config = DatabaseConfig::default().with_codec("nonexistent_codec");
        let result = config.validate();
        assert!(matches!(result, Err(ConfigError::InvalidCodec(_))));

        // Verify error message contains the codec name
        if let Err(ConfigError::InvalidCodec(msg)) = result {
            assert!(!msg.is_empty());
        }
    }

    #[test]
    fn test_validate_invalid_wal_config() {
        // Create config with invalid WAL settings (buffered_sync_bytes > segment_size)
        let mut config = DatabaseConfig::default();
        config.wal_config.segment_size = 1000;
        config.wal_config.buffered_sync_bytes = 2000; // Invalid: sync > segment

        let result = config.validate();
        assert!(matches!(result, Err(ConfigError::InvalidWalConfig(_))));
    }

    #[test]
    fn test_for_testing() {
        let config = DatabaseConfig::for_testing();
        assert!(matches!(config.durability, DurabilityMode::Strict));
        // Testing config should have small segment size
        assert!(config.wal_config.segment_size < 1024 * 1024);
    }
}
