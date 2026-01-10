//! Core types for in-mem database
//!
//! This module defines the foundational types:
//! - RunId: Unique identifier for agent runs
//! - Namespace: Hierarchical namespace (tenant/app/agent/run)

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for an agent run
///
/// A RunId is a wrapper around a UUID v4, providing unique identification
/// for each agent execution run. RunIds are used throughout the system
/// to scope data and enable run-specific queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);

impl RunId {
    /// Create a new random RunId using UUID v4
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a RunId from raw bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// Get the raw bytes of this RunId
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Hierarchical namespace: tenant → app → agent → run
///
/// Namespaces provide multi-tenant isolation and hierarchical organization
/// of data. The hierarchy enables efficient querying and access control.
///
/// Format: "tenant/app/agent/run_id"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Namespace {
    /// Tenant identifier (top-level isolation)
    pub tenant: String,
    /// Application identifier
    pub app: String,
    /// Agent identifier
    pub agent: String,
    /// Run identifier
    pub run_id: RunId,
}

impl Namespace {
    /// Create a new namespace
    pub fn new(tenant: String, app: String, agent: String, run_id: RunId) -> Self {
        Self {
            tenant,
            app,
            agent,
            run_id,
        }
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}/{}",
            self.tenant, self.app, self.agent, self.run_id
        )
    }
}

// Ord implementation for BTreeMap key ordering
// Orders by: tenant → app → agent → run_id
impl Ord for Namespace {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tenant
            .cmp(&other.tenant)
            .then(self.app.cmp(&other.app))
            .then(self.agent.cmp(&other.agent))
            .then(self.run_id.0.cmp(&other.run_id.0))
    }
}

impl PartialOrd for Namespace {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // RunId Tests
    // ========================================

    #[test]
    fn test_run_id_creation_uniqueness() {
        let id1 = RunId::new();
        let id2 = RunId::new();
        assert_ne!(id1, id2, "RunIds should be unique");
    }

    #[test]
    fn test_run_id_serialization_roundtrip() {
        let id = RunId::new();
        let bytes = id.as_bytes();
        let restored = RunId::from_bytes(*bytes);
        assert_eq!(id, restored, "RunId should roundtrip through bytes");
    }

    #[test]
    fn test_run_id_display() {
        let id = RunId::new();
        let s = format!("{}", id);
        assert!(!s.is_empty(), "Display should produce non-empty string");
        assert_eq!(
            s.len(),
            36,
            "UUID v4 should format as 36 characters with hyphens"
        );
    }

    #[test]
    fn test_run_id_hash_consistency() {
        use std::collections::HashSet;

        let id1 = RunId::new();
        let id2 = id1; // Copy

        let mut set = HashSet::new();
        set.insert(id1);

        assert!(
            set.contains(&id2),
            "Hash should be consistent for copied RunId"
        );

        let id3 = RunId::new();
        set.insert(id3);

        assert_eq!(
            set.len(),
            2,
            "Different RunIds should have different hashes"
        );
    }

    #[test]
    fn test_run_id_default() {
        let id1 = RunId::default();
        let id2 = RunId::default();
        assert_ne!(id1, id2, "Default RunIds should be unique");
    }

    #[test]
    fn test_run_id_clone() {
        let id1 = RunId::new();
        let id2 = id1.clone();
        assert_eq!(id1, id2, "Cloned RunId should equal original");
    }

    #[test]
    fn test_run_id_debug() {
        let id = RunId::new();
        let debug_str = format!("{:?}", id);
        assert!(
            debug_str.contains("RunId"),
            "Debug should include type name"
        );
    }

    // ========================================
    // Namespace Tests
    // ========================================

    #[test]
    fn test_namespace_construction() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        assert_eq!(ns.tenant, "acme");
        assert_eq!(ns.app, "chatbot");
        assert_eq!(ns.agent, "agent-42");
        assert_eq!(ns.run_id, run_id);
    }

    #[test]
    fn test_namespace_display_format() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        let display_str = format!("{}", ns);
        let expected = format!("acme/chatbot/agent-42/{}", run_id);
        assert_eq!(
            display_str, expected,
            "Namespace should format as tenant/app/agent/run_id"
        );
    }

    #[test]
    fn test_namespace_equality() {
        let run_id1 = RunId::new();
        let run_id2 = RunId::new();

        let ns1 = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id1,
        );

        let ns2 = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id1,
        );

        let ns3 = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id2,
        );

        assert_eq!(ns1, ns2, "Namespaces with same values should be equal");
        assert_ne!(
            ns1, ns3,
            "Namespaces with different run_ids should not be equal"
        );
    }

    #[test]
    fn test_namespace_clone() {
        let run_id = RunId::new();
        let ns1 = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        let ns2 = ns1.clone();
        assert_eq!(ns1, ns2, "Cloned namespace should equal original");
    }

    #[test]
    fn test_namespace_debug() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        let debug_str = format!("{:?}", ns);
        assert!(
            debug_str.contains("Namespace"),
            "Debug should include type name"
        );
        assert!(debug_str.contains("acme"), "Debug should include tenant");
    }

    #[test]
    fn test_namespace_with_special_characters() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant-1".to_string(),
            "my_app".to_string(),
            "agent.42".to_string(),
            run_id,
        );

        let display = format!("{}", ns);
        assert!(display.contains("tenant-1"));
        assert!(display.contains("my_app"));
        assert!(display.contains("agent.42"));
    }

    #[test]
    fn test_namespace_with_empty_strings() {
        let run_id = RunId::new();
        let ns = Namespace::new("".to_string(), "".to_string(), "".to_string(), run_id);

        // Should still construct, even if semantically invalid
        assert_eq!(ns.tenant, "");
        assert_eq!(ns.app, "");
        assert_eq!(ns.agent, "");
    }

    #[test]
    fn test_namespace_ordering() {
        let run1 = RunId::new();
        let run2 = RunId::new();

        let ns1 = Namespace::new(
            "tenant1".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns2 = Namespace::new(
            "tenant1".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run2,
        );
        let ns3 = Namespace::new(
            "tenant2".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns4 = Namespace::new(
            "tenant1".to_string(),
            "app2".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns5 = Namespace::new(
            "tenant1".to_string(),
            "app1".to_string(),
            "agent2".to_string(),
            run1,
        );

        // Same tenant/app/agent, different run_id - order depends on UUID
        assert_ne!(ns1, ns2);

        // Different tenant should sort differently
        assert!(ns1 < ns3, "tenant1 should be less than tenant2");

        // Different app within same tenant
        assert!(ns1 < ns4, "app1 should be less than app2");

        // Different agent within same tenant/app
        assert!(ns5 > ns1, "agent2 should be greater than agent1");
    }

    #[test]
    fn test_namespace_serialization() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "acme".to_string(),
            "myapp".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        let json = serde_json::to_string(&ns).unwrap();
        let ns2: Namespace = serde_json::from_str(&json).unwrap();

        assert_eq!(ns, ns2, "Namespace should roundtrip through JSON");
    }

    #[test]
    fn test_namespace_btreemap_ordering() {
        use std::collections::BTreeMap;

        let run1 = RunId::new();
        let run2 = RunId::new();

        let ns1 = Namespace::new(
            "acme".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns2 = Namespace::new(
            "acme".to_string(),
            "app1".to_string(),
            "agent2".to_string(),
            run2,
        );
        let ns3 = Namespace::new(
            "acme".to_string(),
            "app2".to_string(),
            "agent1".to_string(),
            run1,
        );

        let mut map = BTreeMap::new();
        map.insert(ns3.clone(), "value3");
        map.insert(ns1.clone(), "value1");
        map.insert(ns2.clone(), "value2");

        // Collect keys in order
        let keys: Vec<_> = map.keys().cloned().collect();

        // Should be ordered: ns1 (app1/agent1) < ns2 (app1/agent2) < ns3 (app2/agent1)
        assert_eq!(keys[0], ns1);
        assert_eq!(keys[1], ns2);
        assert_eq!(keys[2], ns3);
    }
}
