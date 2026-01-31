//! Database operations: ping, info, flush, compact.

use super::Strata;
use crate::{Command, Error, Output, Result};
use crate::types::*;

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

    /// Export a run to a .runbundle.tar.zst archive.
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

    /// Import a run from a .runbundle.tar.zst archive.
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

    /// Validate a .runbundle.tar.zst archive without importing.
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
}
