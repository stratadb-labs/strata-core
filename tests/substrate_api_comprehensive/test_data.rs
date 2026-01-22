//! Test Data Loader
//!
//! Provides utilities for loading test data from JSONL files.
//! All test data should be loaded from files rather than generated at runtime.

use serde::Deserialize;
use serde_json::Value as JsonValue;
use strata_core::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

// =============================================================================
// DATA STRUCTURES
// =============================================================================

/// Header information from the JSONL file
#[derive(Debug, Deserialize)]
pub struct TestDataHeader {
    pub description: String,
    #[serde(default)]
    pub total_runs: usize,
    #[serde(default)]
    pub entries_per_run: usize,
    #[serde(default)]
    pub total_entries: usize,
}

/// A single KV test entry
#[derive(Debug, Clone)]
pub struct KvTestEntry {
    pub run_index: usize,
    pub run_id_type: String,
    pub key: String,
    pub value_type: String,
    pub value: Value,
}

/// Edge case test entry
#[derive(Debug, Clone, Deserialize)]
pub struct EdgeCaseEntry {
    pub category: String,
    pub test: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: JsonValue,
    #[serde(default)]
    pub expected: String,
    #[serde(default)]
    pub entries: Vec<JsonValue>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub entry_count: usize,
    #[serde(default)]
    pub key_count: usize,
    #[serde(default)]
    pub operation_count: usize,
}

/// Loaded test data
pub struct TestData {
    pub header: TestDataHeader,
    pub entries: Vec<KvTestEntry>,
    pub entries_by_run: HashMap<usize, Vec<KvTestEntry>>,
    pub entries_by_type: HashMap<String, Vec<KvTestEntry>>,
}

/// Loaded edge case data
pub struct EdgeCaseData {
    pub entries: Vec<EdgeCaseEntry>,
    pub by_category: HashMap<String, Vec<EdgeCaseEntry>>,
}

// =============================================================================
// LOADING FUNCTIONS
// =============================================================================

/// Get the path to the testdata directory
pub fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("substrate_api_comprehensive")
        .join("testdata")
}

/// Load the main KV test data from JSONL
pub fn load_kv_test_data() -> TestData {
    let path = testdata_dir().join("kv_test_data.jsonl");
    let file = File::open(&path).expect(&format!("Failed to open {:?}", path));
    let reader = BufReader::new(file);

    let mut header: Option<TestDataHeader> = None;
    let mut entries = Vec::new();
    let mut entries_by_run: HashMap<usize, Vec<KvTestEntry>> = HashMap::new();
    let mut entries_by_type: HashMap<String, Vec<KvTestEntry>> = HashMap::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line.expect(&format!("Failed to read line {}", line_num));
        let json: JsonValue = serde_json::from_str(&line)
            .expect(&format!("Failed to parse line {}: {}", line_num, line));

        // First line is header
        if line_num == 0 {
            if json.get("type").and_then(|v| v.as_str()) == Some("header") {
                header = Some(serde_json::from_value(json).expect("Failed to parse header"));
                continue;
            }
        }

        // Parse entry
        let entry = parse_kv_entry(&json);

        entries_by_run
            .entry(entry.run_index)
            .or_default()
            .push(entry.clone());

        entries_by_type
            .entry(entry.value_type.clone())
            .or_default()
            .push(entry.clone());

        entries.push(entry);
    }

    TestData {
        header: header.expect("No header found in test data"),
        entries,
        entries_by_run,
        entries_by_type,
    }
}

/// Load edge case test data from JSONL
pub fn load_edge_case_data() -> EdgeCaseData {
    let path = testdata_dir().join("kv_edge_cases.jsonl");
    let file = File::open(&path).expect(&format!("Failed to open {:?}", path));
    let reader = BufReader::new(file);

    let mut entries = Vec::new();
    let mut by_category: HashMap<String, Vec<EdgeCaseEntry>> = HashMap::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line.expect(&format!("Failed to read line {}", line_num));
        let json: JsonValue = serde_json::from_str(&line)
            .expect(&format!("Failed to parse line {}: {}", line_num, line));

        // Skip header
        if json.get("type").and_then(|v| v.as_str()) == Some("header") {
            continue;
        }

        let entry: EdgeCaseEntry = serde_json::from_value(json)
            .expect(&format!("Failed to parse edge case at line {}", line_num));

        by_category
            .entry(entry.category.clone())
            .or_default()
            .push(entry.clone());

        entries.push(entry);
    }

    EdgeCaseData { entries, by_category }
}

// =============================================================================
// PARSING HELPERS
// =============================================================================

fn parse_kv_entry(json: &JsonValue) -> KvTestEntry {
    let run_index = json["run_index"].as_u64().unwrap_or(0) as usize;
    let run_id_type = json["run_id_type"].as_str().unwrap_or("default").to_string();
    let key = json["key"].as_str().unwrap_or("").to_string();
    let value_type = json["type"].as_str().unwrap_or("null").to_string();

    let value = parse_value(&value_type, json);

    KvTestEntry {
        run_index,
        run_id_type,
        key,
        value_type,
        value,
    }
}

fn parse_value(value_type: &str, json: &JsonValue) -> Value {
    match value_type {
        "null" => Value::Null,
        "bool" => Value::Bool(json["value"].as_bool().unwrap_or(false)),
        "int" => Value::Int(json["value"].as_i64().unwrap_or(0)),
        "float" => {
            let v = &json["value"];
            if v.is_string() {
                match v.as_str().unwrap() {
                    "Infinity" => Value::Float(f64::INFINITY),
                    "-Infinity" => Value::Float(f64::NEG_INFINITY),
                    "NaN" => Value::Float(f64::NAN),
                    s => Value::Float(s.parse().unwrap_or(0.0)),
                }
            } else {
                Value::Float(v.as_f64().unwrap_or(0.0))
            }
        }
        "string" => Value::String(json["value"].as_str().unwrap_or("").to_string()),
        "bytes" => {
            let b64 = json["value_base64"].as_str().unwrap_or("");
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            let bytes = STANDARD.decode(b64).unwrap_or_default();
            Value::Bytes(bytes)
        }
        "array" => {
            let arr = json["value"].as_array().map(|a| {
                a.iter().map(|v| json_to_value(v)).collect()
            }).unwrap_or_default();
            Value::Array(arr)
        }
        "object" => {
            let obj = json["value"].as_object().map(|o| {
                o.iter().map(|(k, v)| (k.clone(), json_to_value(v))).collect()
            }).unwrap_or_default();
            Value::Object(obj)
        }
        _ => Value::Null,
    }
}

fn json_to_value(json: &JsonValue) -> Value {
    match json {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(arr) => {
            Value::Array(arr.iter().map(json_to_value).collect())
        }
        JsonValue::Object(obj) => {
            Value::Object(obj.iter().map(|(k, v)| (k.clone(), json_to_value(v))).collect())
        }
    }
}

// =============================================================================
// VALUE GENERATION FOR EDGE CASES
// =============================================================================

/// Generate a large string of specified size in KB
pub fn generate_large_string(size_kb: usize) -> String {
    let size_bytes = size_kb * 1024;
    "x".repeat(size_bytes)
}

/// Generate large bytes of specified size in KB
pub fn generate_large_bytes(size_kb: usize) -> Vec<u8> {
    let size_bytes = size_kb * 1024;
    vec![0xAB; size_bytes]
}

/// Generate a large array with specified element count
pub fn generate_large_array(count: usize) -> Value {
    Value::Array((0..count).map(|i| Value::Int(i as i64)).collect())
}

/// Generate a large object with specified key count
pub fn generate_large_object(count: usize) -> Value {
    Value::Object(
        (0..count)
            .map(|i| (format!("key_{}", i), Value::Int(i as i64)))
            .collect()
    )
}

/// Generate a deeply nested array
pub fn generate_nested_array(depth: usize) -> Value {
    let mut result = Value::Int(42);
    for _ in 0..depth {
        result = Value::Array(vec![result]);
    }
    result
}

/// Generate a deeply nested object
pub fn generate_nested_object(depth: usize) -> Value {
    let mut result = Value::Int(42);
    for i in 0..depth {
        let mut map = HashMap::new();
        map.insert(format!("level_{}", i), result);
        result = Value::Object(map);
    }
    result
}

// =============================================================================
// TEST HELPERS
// =============================================================================

impl TestData {
    /// Get entries for a specific run
    pub fn get_run(&self, run_index: usize) -> &[KvTestEntry] {
        self.entries_by_run.get(&run_index).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get entries of a specific type
    pub fn get_type(&self, value_type: &str) -> &[KvTestEntry] {
        self.entries_by_type.get(value_type).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get first N entries
    pub fn take(&self, n: usize) -> &[KvTestEntry] {
        &self.entries[..n.min(self.entries.len())]
    }
}

impl EdgeCaseData {
    /// Get entries for a specific category
    pub fn get_category(&self, category: &str) -> &[EdgeCaseEntry] {
        self.by_category.get(category).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get a specific test by name
    pub fn get_test(&self, test_name: &str) -> Option<&EdgeCaseEntry> {
        self.entries.iter().find(|e| e.test == test_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_kv_test_data() {
        let data = load_kv_test_data();
        assert!(data.entries.len() > 0, "Should load entries");
        assert_eq!(data.header.total_runs, 20);
        assert_eq!(data.header.entries_per_run, 100);
    }

    #[test]
    fn test_load_edge_case_data() {
        let data = load_edge_case_data();
        assert!(data.entries.len() > 0, "Should load edge cases");
        assert!(data.by_category.contains_key("key_validation"));
    }
}
