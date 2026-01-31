//! Retention policy system for version history management
//!
//! This module provides user-configurable retention policies that control
//! how much version history is retained per run.
//!
//! # Overview
//!
//! - Policies are stored as versioned database entries
//! - Default policy is KeepAll (no data loss)
//! - Policies support per-primitive-type overrides
//! - System namespace isolates policy storage from user data
//!
//! # Policy Types
//!
//! - **KeepAll**: Retain all versions forever (default, safest)
//! - **KeepLast(n)**: Keep only the last N versions
//! - **KeepFor(duration)**: Keep versions within time window
//! - **Composite**: Different policies per primitive type
//!
//! # System Namespace
//!
//! Retention policies are stored in a system namespace (`_system/`)
//! that is isolated from user data. Users cannot directly access
//! or modify system keys.

mod policy;

pub use policy::{CompositeBuilder, RetentionPolicy, RetentionPolicyError};

/// System namespace for internal storage
///
/// The system namespace provides isolated storage for internal
/// database metadata that should not be visible to users.
pub mod system_namespace {
    /// Prefix for all system keys
    pub const PREFIX: &str = "_system/";

    /// Prefix for retention policies
    pub const RETENTION_POLICY_PREFIX: &str = "_system/retention_policy/";

    /// Check if a key is in the system namespace
    pub fn is_system_key(key: &str) -> bool {
        key.starts_with(PREFIX)
    }

    /// Check if a key is a retention policy key
    pub fn is_retention_policy_key(key: &str) -> bool {
        key.starts_with(RETENTION_POLICY_PREFIX)
    }

    /// Generate retention policy key for a run
    pub fn retention_policy_key(branch_id: &[u8; 16]) -> String {
        format!("{}{}", RETENTION_POLICY_PREFIX, hex_encode(branch_id))
    }

    /// Extract run ID from retention policy key
    pub fn branch_id_from_retention_key(key: &str) -> Option<[u8; 16]> {
        if !is_retention_policy_key(key) {
            return None;
        }

        let hex = key.strip_prefix(RETENTION_POLICY_PREFIX)?;
        hex_decode(hex)
    }

    /// Encode bytes as hex string
    fn hex_encode(bytes: &[u8; 16]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Decode hex string to bytes
    fn hex_decode(hex: &str) -> Option<[u8; 16]> {
        if hex.len() != 32 {
            return None;
        }

        let mut result = [0u8; 16];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let s = std::str::from_utf8(chunk).ok()?;
            result[i] = u8::from_str_radix(s, 16).ok()?;
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prefix() {
        assert!(system_namespace::is_system_key("_system/foo"));
        assert!(system_namespace::is_system_key(
            "_system/retention_policy/abc"
        ));
        assert!(!system_namespace::is_system_key("user/key"));
        assert!(!system_namespace::is_system_key("_other/key"));
    }

    #[test]
    fn test_retention_policy_key_format() {
        let branch_id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let key = system_namespace::retention_policy_key(&branch_id);

        assert!(system_namespace::is_retention_policy_key(&key));
        assert!(key.starts_with("_system/retention_policy/"));
        assert!(key.contains("0102030405060708090a0b0c0d0e0f10"));
    }

    #[test]
    fn test_run_id_extraction() {
        let branch_id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let key = system_namespace::retention_policy_key(&branch_id);

        let extracted = system_namespace::branch_id_from_retention_key(&key);
        assert_eq!(extracted, Some(branch_id));
    }

    #[test]
    fn test_run_id_extraction_invalid() {
        // Not a retention key
        assert!(system_namespace::branch_id_from_retention_key("user/key").is_none());

        // Invalid hex
        assert!(
            system_namespace::branch_id_from_retention_key("_system/retention_policy/invalid")
                .is_none()
        );

        // Wrong length
        assert!(
            system_namespace::branch_id_from_retention_key("_system/retention_policy/0102").is_none()
        );
    }
}
