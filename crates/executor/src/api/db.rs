//! Database operations: ping, info, flush, compact, configuration.

use super::Strata;
use crate::output::EmbedStatusInfo;
use crate::types::*;
use crate::{Command, Error, Output, Result};
use strata_engine::{ModelConfig, StrataConfig};

impl Strata {
    // =========================================================================
    // Database Operations (4)
    // =========================================================================

    /// Ping the database.
    pub fn ping(&self) -> Result<String> {
        match self.executor.execute(Command::Ping)? {
            Output::Pong { version } => Ok(version),
            _ => Err(Error::Internal {
                reason: "Unexpected output for Ping".into(),
            }),
        }
    }

    /// Get database info.
    pub fn info(&self) -> Result<DatabaseInfo> {
        match self.executor.execute(Command::Info)? {
            Output::DatabaseInfo(info) => Ok(info),
            _ => Err(Error::Internal {
                reason: "Unexpected output for Info".into(),
            }),
        }
    }

    /// Flush the database to disk.
    pub fn flush(&self) -> Result<()> {
        match self.executor.execute(Command::Flush)? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for Flush".into(),
            }),
        }
    }

    /// Compact the database.
    pub fn compact(&self) -> Result<()> {
        match self.executor.execute(Command::Compact)? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for Compact".into(),
            }),
        }
    }

    // =========================================================================
    // Bundle Operations (3)
    // =========================================================================

    /// Export a branch to a .branchbundle.tar.zst archive.
    pub fn branch_export(&self, branch_id: &str, path: &str) -> Result<BranchExportResult> {
        match self.executor.execute(Command::BranchExport {
            branch_id: branch_id.to_string(),
            path: path.to_string(),
        })? {
            Output::BranchExported(result) => Ok(result),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchExport".into(),
            }),
        }
    }

    /// Import a branch from a .branchbundle.tar.zst archive.
    pub fn branch_import(&self, path: &str) -> Result<BranchImportResult> {
        match self.executor.execute(Command::BranchImport {
            path: path.to_string(),
        })? {
            Output::BranchImported(result) => Ok(result),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchImport".into(),
            }),
        }
    }

    /// Validate a .branchbundle.tar.zst archive without importing.
    pub fn branch_validate_bundle(&self, path: &str) -> Result<BundleValidateResult> {
        match self.executor.execute(Command::BranchBundleValidate {
            path: path.to_string(),
        })? {
            Output::BundleValidated(result) => Ok(result),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchBundleValidate".into(),
            }),
        }
    }

    // =========================================================================
    // Configuration (4)
    // =========================================================================

    /// Get the current database configuration.
    ///
    /// Returns a snapshot of the unified config (durability, auto_embed, model).
    pub fn config(&self) -> StrataConfig {
        self.executor.primitives().db.config()
    }

    /// Configure an external LLM model for query expansion and re-ranking.
    ///
    /// The configuration is persisted to `strata.toml` for disk-backed databases.
    ///
    /// # Example
    ///
    /// ```text
    /// db.configure_model("http://localhost:11434/v1", "qwen3:1.7b", None, None)?;
    /// ```
    pub fn configure_model(
        &self,
        endpoint: &str,
        model: &str,
        api_key: Option<&str>,
        timeout_ms: Option<u64>,
    ) -> Result<()> {
        self.executor
            .primitives()
            .db
            .update_config(|cfg| {
                cfg.model = Some(ModelConfig {
                    endpoint: endpoint.to_string(),
                    model: model.to_string(),
                    api_key: api_key.map(|s| s.to_string()),
                    timeout_ms: timeout_ms.unwrap_or(5000),
                });
            })
            .map_err(Error::from)
    }

    /// Get a snapshot of the embedding pipeline status.
    pub fn embed_status(&self) -> Result<EmbedStatusInfo> {
        match self.executor.execute(Command::EmbedStatus)? {
            Output::EmbedStatus(info) => Ok(info),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EmbedStatus".into(),
            }),
        }
    }

    /// Check whether auto-embedding is enabled.
    pub fn auto_embed_enabled(&self) -> bool {
        self.executor.primitives().db.auto_embed_enabled()
    }

    /// Enable or disable auto-embedding of text values.
    ///
    /// Persisted to `strata.toml` for disk-backed databases.
    pub fn set_auto_embed(&self, enabled: bool) -> Result<()> {
        self.executor
            .primitives()
            .db
            .update_config(|cfg| {
                cfg.auto_embed = enabled;
            })
            .map_err(Error::from)
    }
}
