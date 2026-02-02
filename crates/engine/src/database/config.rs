//! Database configuration via `strata.toml`
//!
//! Replaces the builder pattern with a simple config file in the data directory.
//! On first open, a default `strata.toml` is created. To change settings,
//! edit the file and restart — same model as Redis.

use serde::{Deserialize, Serialize};
use std::path::Path;
use strata_core::{StrataError, StrataResult};
use strata_durability::wal::DurabilityMode;

/// Config file name placed in the database data directory.
pub const CONFIG_FILE_NAME: &str = "strata.toml";

/// Database configuration loaded from `strata.toml`.
///
/// # Example
///
/// ```toml
/// # Durability mode: "standard" (default) or "always"
/// # "standard" = periodic fsync (~100ms), may lose last interval on crash
/// # "always" = fsync every commit, zero data loss
/// durability = "standard"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrataConfig {
    /// Durability mode: `"standard"` or `"always"`.
    #[serde(default = "default_durability_str")]
    pub durability: String,
}

fn default_durability_str() -> String {
    "standard".to_string()
}

impl Default for StrataConfig {
    fn default() -> Self {
        Self {
            durability: default_durability_str(),
        }
    }
}

impl StrataConfig {
    /// Parse the durability string into a `DurabilityMode`.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not `"standard"` or `"always"`.
    pub fn durability_mode(&self) -> StrataResult<DurabilityMode> {
        match self.durability.as_str() {
            "standard" => Ok(DurabilityMode::standard_default()),
            "always" => Ok(DurabilityMode::Always),
            other => Err(StrataError::invalid_input(format!(
                "Invalid durability mode '{}' in strata.toml. Expected \"standard\" or \"always\".",
                other
            ))),
        }
    }

    /// Returns the default config file content with comments.
    pub fn default_toml() -> &'static str {
        r#"# Strata database configuration
#
# Durability mode: "standard" (default) or "always"
#   "standard" = periodic fsync (~100ms), may lose last interval on crash
#   "always"   = fsync every commit, zero data loss
durability = "standard"
"#
    }

    /// Read and parse config from a file path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &Path) -> StrataResult<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            StrataError::internal(format!(
                "Failed to read config file '{}': {}",
                path.display(),
                e
            ))
        })?;
        let config: StrataConfig = toml::from_str(&content).map_err(|e| {
            StrataError::invalid_input(format!(
                "Failed to parse config file '{}': {}",
                path.display(),
                e
            ))
        })?;
        // Validate the durability value eagerly
        config.durability_mode()?;
        Ok(config)
    }

    /// Write the default config file if it does not already exist.
    ///
    /// Returns `Ok(())` whether the file was created or already existed.
    pub fn write_default_if_missing(path: &Path) -> StrataResult<()> {
        if !path.exists() {
            std::fs::write(path, Self::default_toml()).map_err(|e| {
                StrataError::internal(format!(
                    "Failed to write default config file '{}': {}",
                    path.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_config_is_standard() {
        let config = StrataConfig::default();
        assert_eq!(config.durability, "standard");
        let mode = config.durability_mode().unwrap();
        assert!(matches!(mode, DurabilityMode::Standard { .. }));
    }

    #[test]
    fn parse_standard() {
        let config: StrataConfig = toml::from_str("durability = \"standard\"").unwrap();
        assert!(matches!(
            config.durability_mode().unwrap(),
            DurabilityMode::Standard { .. }
        ));
    }

    #[test]
    fn parse_always() {
        let config: StrataConfig = toml::from_str("durability = \"always\"").unwrap();
        assert_eq!(config.durability_mode().unwrap(), DurabilityMode::Always);
    }

    #[test]
    fn parse_invalid_mode_returns_error() {
        let config: StrataConfig = toml::from_str("durability = \"turbo\"").unwrap();
        assert!(config.durability_mode().is_err());
    }

    #[test]
    fn default_toml_parses_correctly() {
        let config: StrataConfig = toml::from_str(StrataConfig::default_toml()).unwrap();
        assert_eq!(config.durability, "standard");
    }

    #[test]
    fn write_default_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        assert!(!path.exists());

        StrataConfig::write_default_if_missing(&path).unwrap();
        assert!(path.exists());

        let config = StrataConfig::from_file(&path).unwrap();
        assert_eq!(config.durability, "standard");
    }

    #[test]
    fn write_default_does_not_overwrite() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);

        // Write custom config
        std::fs::write(&path, "durability = \"always\"\n").unwrap();

        // write_default_if_missing should not overwrite
        StrataConfig::write_default_if_missing(&path).unwrap();

        let config = StrataConfig::from_file(&path).unwrap();
        assert_eq!(config.durability, "always");
    }

    #[test]
    fn from_file_with_missing_field_uses_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);

        // Empty config file — all fields should use defaults
        std::fs::write(&path, "").unwrap();

        let config = StrataConfig::from_file(&path).unwrap();
        assert_eq!(config.durability, "standard");
    }
}
