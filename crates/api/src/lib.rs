//! Public API layer for Strata
//!
//! This crate provides the public interface to the database:
//! - Embedded: In-process library API (M1-M5)
//! - RPC: Network server (M7)
//! - MCP: Model Context Protocol integration (M8)
//!
//! For MVP, only the embedded API is implemented.

// Module declarations (will be implemented across milestones)
// pub mod embedded;  // M5
// pub mod rpc;       // M7
// pub mod mcp;       // M8

#![warn(missing_docs)]
#![warn(clippy::all)]

/// Placeholder for API functionality
pub fn placeholder() {
    // This crate will contain the public API
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder() {
        placeholder();
    }
}
