//! Reference model for tracking expected database state
//!
//! The reference model maintains an in-memory copy of expected database state
//! to verify correctness after crash recovery.
//!
//! # Example
//!
//! ```ignore
//! use strata_storage::testing::ReferenceModel;
//!
//! let mut model = ReferenceModel::new();
//! model.kv_put("run1", "key1", b"value1".to_vec());
//! model.checkpoint();
//!
//! // After recovery, compare actual state to reference
//! let mismatches = model.compare_kv("run1", actual_kv_pairs);
//! assert!(mismatches.is_empty());
//! ```

use std::collections::HashMap;

/// Reference model tracking expected database state
///
/// Maintains expected state for crash testing verification.
/// Operations are tracked to compare against actual database
/// state after recovery.
pub struct ReferenceModel {
    /// KV state per run: run_name -> (key -> value)
    kv_state: HashMap<String, HashMap<String, Vec<u8>>>,
    /// Event state per run: run_name -> events
    event_state: HashMap<String, Vec<Vec<u8>>>,
    /// State values per run: run_name -> (key -> value)
    state_values: HashMap<String, HashMap<String, Vec<u8>>>,
    /// Committed operations in order
    committed_ops: Vec<Operation>,
    /// Last checkpoint operation index
    last_checkpoint: Option<usize>,
    /// Total operations since last checkpoint
    ops_since_checkpoint: usize,
}

/// Operation recorded in reference model
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// KV put operation
    KvPut {
        /// Run name
        run: String,
        /// Key
        key: String,
        /// Value
        value: Vec<u8>,
    },
    /// KV delete operation
    KvDelete {
        /// Run name
        run: String,
        /// Key
        key: String,
    },
    /// Event append operation
    EventAppend {
        /// Run name
        run: String,
        /// Event payload
        payload: Vec<u8>,
    },
    /// State value set operation
    StateSet {
        /// Run name
        run: String,
        /// State key
        key: String,
        /// State value
        value: Vec<u8>,
    },
    /// Checkpoint operation
    Checkpoint,
}

impl ReferenceModel {
    /// Create a new empty reference model
    pub fn new() -> Self {
        ReferenceModel {
            kv_state: HashMap::new(),
            event_state: HashMap::new(),
            state_values: HashMap::new(),
            committed_ops: Vec::new(),
            last_checkpoint: None,
            ops_since_checkpoint: 0,
        }
    }

    /// Record a KV put operation
    pub fn kv_put(&mut self, run: &str, key: &str, value: Vec<u8>) {
        self.kv_state
            .entry(run.to_string())
            .or_default()
            .insert(key.to_string(), value.clone());

        self.committed_ops.push(Operation::KvPut {
            run: run.to_string(),
            key: key.to_string(),
            value,
        });
        self.ops_since_checkpoint += 1;
    }

    /// Record a KV delete operation
    pub fn kv_delete(&mut self, run: &str, key: &str) {
        if let Some(run_state) = self.kv_state.get_mut(run) {
            run_state.remove(key);
        }

        self.committed_ops.push(Operation::KvDelete {
            run: run.to_string(),
            key: key.to_string(),
        });
        self.ops_since_checkpoint += 1;
    }

    /// Record an event append operation
    pub fn event_append(&mut self, run: &str, payload: Vec<u8>) {
        self.event_state
            .entry(run.to_string())
            .or_default()
            .push(payload.clone());

        self.committed_ops.push(Operation::EventAppend {
            run: run.to_string(),
            payload,
        });
        self.ops_since_checkpoint += 1;
    }

    /// Record a state value set operation
    pub fn state_set(&mut self, run: &str, key: &str, value: Vec<u8>) {
        self.state_values
            .entry(run.to_string())
            .or_default()
            .insert(key.to_string(), value.clone());

        self.committed_ops.push(Operation::StateSet {
            run: run.to_string(),
            key: key.to_string(),
            value,
        });
        self.ops_since_checkpoint += 1;
    }

    /// Record a checkpoint operation
    pub fn checkpoint(&mut self) {
        self.last_checkpoint = Some(self.committed_ops.len());
        self.committed_ops.push(Operation::Checkpoint);
        self.ops_since_checkpoint = 0;
    }

    /// Get expected KV value for a run/key
    pub fn get_kv(&self, run: &str, key: &str) -> Option<&Vec<u8>> {
        self.kv_state.get(run)?.get(key)
    }

    /// Get all expected KV pairs for a run
    pub fn get_kv_all(&self, run: &str) -> Option<&HashMap<String, Vec<u8>>> {
        self.kv_state.get(run)
    }

    /// Get expected events for a run
    pub fn get_events(&self, run: &str) -> Option<&Vec<Vec<u8>>> {
        self.event_state.get(run)
    }

    /// Get expected state value for a run/key
    pub fn get_state(&self, run: &str, key: &str) -> Option<&Vec<u8>> {
        self.state_values.get(run)?.get(key)
    }

    /// Get all run names in the model
    pub fn run_names(&self) -> impl Iterator<Item = &String> {
        self.kv_state
            .keys()
            .chain(self.event_state.keys())
            .chain(self.state_values.keys())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
    }

    /// Get total committed operations
    pub fn total_operations(&self) -> usize {
        self.committed_ops.len()
    }

    /// Get operations since last checkpoint
    pub fn operations_since_checkpoint(&self) -> usize {
        self.ops_since_checkpoint
    }

    /// Get last checkpoint operation index
    pub fn last_checkpoint_index(&self) -> Option<usize> {
        self.last_checkpoint
    }

    /// Compare expected KV state against actual
    ///
    /// Returns list of mismatches found.
    pub fn compare_kv(&self, run: &str, actual: &HashMap<String, Vec<u8>>) -> Vec<StateMismatch> {
        let mut mismatches = Vec::new();

        let expected = self.kv_state.get(run);

        // Check for missing or different values
        if let Some(expected_kv) = expected {
            for (key, expected_value) in expected_kv {
                match actual.get(key) {
                    Some(actual_value) if actual_value == expected_value => {
                        // Match - ok
                    }
                    Some(actual_value) => {
                        mismatches.push(StateMismatch {
                            entity: format!("kv:{}:{}", run, key),
                            expected: format!("{:?}", expected_value),
                            actual: format!("{:?}", actual_value),
                        });
                    }
                    None => {
                        mismatches.push(StateMismatch {
                            entity: format!("kv:{}:{}", run, key),
                            expected: format!("{:?}", expected_value),
                            actual: "not found".to_string(),
                        });
                    }
                }
            }
        }

        // Check for unexpected values in actual
        for key in actual.keys() {
            if expected.map_or(true, |e| !e.contains_key(key)) {
                mismatches.push(StateMismatch {
                    entity: format!("kv:{}:{}", run, key),
                    expected: "not present".to_string(),
                    actual: "found".to_string(),
                });
            }
        }

        mismatches
    }

    /// Compare expected events against actual
    pub fn compare_events(&self, run: &str, actual: &[Vec<u8>]) -> Vec<StateMismatch> {
        let mut mismatches = Vec::new();

        let expected = self
            .event_state
            .get(run)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        // Check counts
        if expected.len() != actual.len() {
            mismatches.push(StateMismatch {
                entity: format!("events:{}:count", run),
                expected: expected.len().to_string(),
                actual: actual.len().to_string(),
            });
        }

        // Check individual events
        for (i, (exp, act)) in expected.iter().zip(actual.iter()).enumerate() {
            if exp != act {
                mismatches.push(StateMismatch {
                    entity: format!("events:{}:{}", run, i),
                    expected: format!("{:?}", exp),
                    actual: format!("{:?}", act),
                });
            }
        }

        mismatches
    }

    /// Compare expected state values against actual
    pub fn compare_state(
        &self,
        run: &str,
        actual: &HashMap<String, Vec<u8>>,
    ) -> Vec<StateMismatch> {
        let mut mismatches = Vec::new();

        let expected = self.state_values.get(run);

        if let Some(expected_state) = expected {
            for (key, expected_value) in expected_state {
                match actual.get(key) {
                    Some(actual_value) if actual_value == expected_value => {
                        // Match - ok
                    }
                    Some(actual_value) => {
                        mismatches.push(StateMismatch {
                            entity: format!("state:{}:{}", run, key),
                            expected: format!("{:?}", expected_value),
                            actual: format!("{:?}", actual_value),
                        });
                    }
                    None => {
                        mismatches.push(StateMismatch {
                            entity: format!("state:{}:{}", run, key),
                            expected: format!("{:?}", expected_value),
                            actual: "not found".to_string(),
                        });
                    }
                }
            }
        }

        mismatches
    }

    /// Reset model to empty state
    pub fn reset(&mut self) {
        self.kv_state.clear();
        self.event_state.clear();
        self.state_values.clear();
        self.committed_ops.clear();
        self.last_checkpoint = None;
        self.ops_since_checkpoint = 0;
    }

    /// Get committed operations
    pub fn operations(&self) -> &[Operation] {
        &self.committed_ops
    }
}

impl Default for ReferenceModel {
    fn default() -> Self {
        Self::new()
    }
}

/// State mismatch found during comparison
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateMismatch {
    /// Entity identifier (e.g., "kv:run:key")
    pub entity: String,
    /// Expected value
    pub expected: String,
    /// Actual value
    pub actual: String,
}

impl std::fmt::Display for StateMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: expected {}, got {}",
            self.entity, self.expected, self.actual
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_model_is_empty() {
        let model = ReferenceModel::new();
        assert_eq!(model.total_operations(), 0);
        assert!(model.last_checkpoint_index().is_none());
    }

    #[test]
    fn test_kv_put() {
        let mut model = ReferenceModel::new();
        model.kv_put("run1", "key1", b"value1".to_vec());

        assert_eq!(model.get_kv("run1", "key1"), Some(&b"value1".to_vec()));
        assert_eq!(model.total_operations(), 1);
    }

    #[test]
    fn test_kv_delete() {
        let mut model = ReferenceModel::new();
        model.kv_put("run1", "key1", b"value1".to_vec());
        model.kv_delete("run1", "key1");

        assert!(model.get_kv("run1", "key1").is_none());
        assert_eq!(model.total_operations(), 2);
    }

    #[test]
    fn test_event_append() {
        let mut model = ReferenceModel::new();
        model.event_append("run1", b"event1".to_vec());
        model.event_append("run1", b"event2".to_vec());

        let events = model.get_events("run1").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], b"event1".to_vec());
        assert_eq!(events[1], b"event2".to_vec());
    }

    #[test]
    fn test_state_set() {
        let mut model = ReferenceModel::new();
        model.state_set("run1", "status", b"active".to_vec());

        assert_eq!(model.get_state("run1", "status"), Some(&b"active".to_vec()));
    }

    #[test]
    fn test_checkpoint() {
        let mut model = ReferenceModel::new();
        model.kv_put("run1", "key1", b"value1".to_vec());
        model.kv_put("run1", "key2", b"value2".to_vec());
        model.checkpoint();

        assert_eq!(model.last_checkpoint_index(), Some(2));
        assert_eq!(model.operations_since_checkpoint(), 0);

        model.kv_put("run1", "key3", b"value3".to_vec());
        assert_eq!(model.operations_since_checkpoint(), 1);
    }

    #[test]
    fn test_compare_kv_match() {
        let mut model = ReferenceModel::new();
        model.kv_put("run1", "key1", b"value1".to_vec());
        model.kv_put("run1", "key2", b"value2".to_vec());

        let actual: HashMap<String, Vec<u8>> = [
            ("key1".to_string(), b"value1".to_vec()),
            ("key2".to_string(), b"value2".to_vec()),
        ]
        .into_iter()
        .collect();

        let mismatches = model.compare_kv("run1", &actual);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn test_compare_kv_missing() {
        let mut model = ReferenceModel::new();
        model.kv_put("run1", "key1", b"value1".to_vec());
        model.kv_put("run1", "key2", b"value2".to_vec());

        let actual: HashMap<String, Vec<u8>> = [("key1".to_string(), b"value1".to_vec())]
            .into_iter()
            .collect();

        let mismatches = model.compare_kv("run1", &actual);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].entity, "kv:run1:key2");
    }

    #[test]
    fn test_compare_kv_different() {
        let mut model = ReferenceModel::new();
        model.kv_put("run1", "key1", b"value1".to_vec());

        let actual: HashMap<String, Vec<u8>> = [("key1".to_string(), b"different".to_vec())]
            .into_iter()
            .collect();

        let mismatches = model.compare_kv("run1", &actual);
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].entity.contains("key1"));
    }

    #[test]
    fn test_compare_kv_extra() {
        let model = ReferenceModel::new();

        let actual: HashMap<String, Vec<u8>> = [("unexpected".to_string(), b"value".to_vec())]
            .into_iter()
            .collect();

        let mismatches = model.compare_kv("run1", &actual);
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].entity.contains("unexpected"));
    }

    #[test]
    fn test_compare_events_match() {
        let mut model = ReferenceModel::new();
        model.event_append("run1", b"e1".to_vec());
        model.event_append("run1", b"e2".to_vec());

        let actual = vec![b"e1".to_vec(), b"e2".to_vec()];
        let mismatches = model.compare_events("run1", &actual);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn test_compare_events_count_mismatch() {
        let mut model = ReferenceModel::new();
        model.event_append("run1", b"e1".to_vec());
        model.event_append("run1", b"e2".to_vec());

        let actual = vec![b"e1".to_vec()];
        let mismatches = model.compare_events("run1", &actual);
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].entity.contains("count"));
    }

    #[test]
    fn test_reset() {
        let mut model = ReferenceModel::new();
        model.kv_put("run1", "key1", b"value1".to_vec());
        model.checkpoint();

        model.reset();

        assert_eq!(model.total_operations(), 0);
        assert!(model.get_kv("run1", "key1").is_none());
        assert!(model.last_checkpoint_index().is_none());
    }

    #[test]
    fn test_operations_list() {
        let mut model = ReferenceModel::new();
        model.kv_put("run1", "key1", b"value1".to_vec());
        model.event_append("run1", b"event".to_vec());
        model.checkpoint();

        let ops = model.operations();
        assert_eq!(ops.len(), 3);
        assert!(matches!(ops[0], Operation::KvPut { .. }));
        assert!(matches!(ops[1], Operation::EventAppend { .. }));
        assert!(matches!(ops[2], Operation::Checkpoint));
    }

    #[test]
    fn test_default() {
        let model = ReferenceModel::default();
        assert_eq!(model.total_operations(), 0);
    }

    #[test]
    fn test_state_mismatch_display() {
        let mismatch = StateMismatch {
            entity: "kv:run1:key1".to_string(),
            expected: "value1".to_string(),
            actual: "value2".to_string(),
        };

        let display = format!("{}", mismatch);
        assert!(display.contains("kv:run1:key1"));
        assert!(display.contains("value1"));
        assert!(display.contains("value2"));
    }
}
