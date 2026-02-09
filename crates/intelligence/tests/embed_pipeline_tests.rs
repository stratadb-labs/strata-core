//! Integration tests for the extract → tokenize → embed pipeline.
//!
//! These test cross-module behaviors through the public API.

#![cfg(feature = "embed")]

use strata_core::types::BranchId;
use strata_core::{PrimitiveType, Value};
use strata_intelligence::embed::extract::extract_text;
use strata_intelligence::embed::tokenizer::WordPieceTokenizer;
use strata_intelligence::runtime::safetensors::SafeTensors;
use strata_intelligence::runtime::tensor::Tensor;
use strata_intelligence::{Fuser, RRFFuser, SimpleFuser};

use strata_engine::search::{EntityRef, SearchHit, SearchResponse, SearchStats};

use std::collections::HashMap;

fn minimal_vocab() -> String {
    let mut lines = vec!["[PAD]".to_string(); 103];
    lines[0] = "[PAD]".into();
    lines[100] = "[UNK]".into();
    lines[101] = "[CLS]".into();
    lines[102] = "[SEP]".into();
    lines.push("hello".into()); // 103
    lines.push("world".into()); // 104
    lines.push("test".into()); // 105
    lines.join("\n")
}

#[test]
fn test_extract_then_tokenize_roundtrip() {
    let text = extract_text(&Value::String("hello world".into())).unwrap();
    let vocab = minimal_vocab();
    let tok = WordPieceTokenizer::from_vocab(&vocab);
    let result = tok.tokenize(&text);
    assert!(result.input_ids.len() >= 3); // CLS + at least 1 token + SEP
    assert_eq!(result.input_ids[0], 101); // CLS
    assert_eq!(*result.input_ids.last().unwrap(), 102); // SEP
}

#[test]
fn test_extract_complex_value_tokenizable() {
    let mut map = HashMap::new();
    map.insert("name".to_string(), Value::String("Alice".into()));
    map.insert(
        "scores".to_string(),
        Value::Array(vec![Value::Int(10), Value::Int(20)]),
    );
    let nested = Value::Object(map);

    let text = extract_text(&nested).unwrap();
    let vocab = minimal_vocab();
    let tok = WordPieceTokenizer::from_vocab(&vocab);
    // Should not panic
    let result = tok.tokenize(&text);
    assert!(result.input_ids.len() >= 3);
}

#[test]
fn test_tokenizer_output_contract() {
    let vocab = minimal_vocab();
    let tok = WordPieceTokenizer::from_vocab(&vocab);

    let inputs = ["", "hello", "hello world", &"test ".repeat(200)];
    for input in &inputs {
        let result = tok.tokenize(input);
        // Length invariant
        assert_eq!(
            result.input_ids.len(),
            result.attention_mask.len(),
            "input_ids and attention_mask length mismatch for input: {:?}",
            input
        );
        assert_eq!(
            result.input_ids.len(),
            result.token_type_ids.len(),
            "input_ids and token_type_ids length mismatch for input: {:?}",
            input
        );
        // CLS/SEP invariant
        assert_eq!(
            result.input_ids[0], 101,
            "missing CLS for input: {:?}",
            input
        );
        assert_eq!(
            *result.input_ids.last().unwrap(),
            102,
            "missing SEP for input: {:?}",
            input
        );
    }
}

#[test]
fn test_safetensors_to_tensor_pipeline() {
    // Build synthetic SafeTensors → parse → extract → matmul
    let header = r#"{"weight":{"dtype":"F32","shape":[2,3],"data_offsets":[0,24]}}"#;
    let header_bytes = header.as_bytes();
    let header_len = header_bytes.len() as u64;

    let mut buf = Vec::new();
    buf.extend_from_slice(&header_len.to_le_bytes());
    buf.extend_from_slice(header_bytes);
    for &v in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0] {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    let st = SafeTensors::from_bytes(&buf).unwrap();
    let weight = st.tensor("weight").unwrap();

    // Multiply an input vector through the extracted weight
    let input = Tensor::from_slice(&[2.0, 3.0, 4.0], 1, 3);
    let result = input.matmul_transpose(&weight);
    assert_eq!(result.rows, 1);
    assert_eq!(result.cols, 2);
    // [2*1+3*0+4*0, 2*0+3*1+4*0] = [2.0, 3.0]
    assert!((result.data[0] - 2.0).abs() < 1e-6);
    assert!((result.data[1] - 3.0).abs() < 1e-6);
}

#[test]
fn test_extract_returns_none_for_non_embeddable() {
    assert!(extract_text(&Value::Null).is_none());
    assert!(extract_text(&Value::Bytes(vec![1, 2, 3])).is_none());
    assert!(extract_text(&Value::String("".into())).is_none());
    assert!(extract_text(&Value::Array(vec![Value::Null, Value::Null])).is_none());
}

#[test]
fn test_fuser_handles_realistic_search_results() {
    let branch_id = BranchId::new();

    // Build 25 hits across 3 primitives
    let make_kv_hit = |key: &str, score: f32, rank: u32| SearchHit {
        doc_ref: EntityRef::Kv {
            branch_id: branch_id.clone(),
            key: key.to_string(),
        },
        score,
        rank,
        snippet: None,
    };

    let kv_hits: Vec<SearchHit> = (0..10)
        .map(|i| make_kv_hit(&format!("kv_{}", i), 1.0 - i as f32 * 0.05, (i + 1) as u32))
        .collect();

    let json_hits: Vec<SearchHit> = (0..10)
        .map(|i| SearchHit {
            doc_ref: EntityRef::Json {
                branch_id: branch_id.clone(),
                doc_id: format!("json_{}", i),
            },
            score: 0.9 - i as f32 * 0.05,
            rank: (i + 1) as u32,
            snippet: None,
        })
        .collect();

    let event_hits: Vec<SearchHit> = (0..5)
        .map(|i| SearchHit {
            doc_ref: EntityRef::Event {
                branch_id: branch_id.clone(),
                sequence: i as u64,
            },
            score: 0.8 - i as f32 * 0.1,
            rank: (i + 1) as u32,
            snippet: None,
        })
        .collect();

    let make_response = |hits: Vec<SearchHit>| SearchResponse {
        hits,
        truncated: false,
        stats: SearchStats::new(0, 0),
    };

    // Test SimpleFuser
    let simple = SimpleFuser::new();
    let simple_results = vec![
        (PrimitiveType::Kv, make_response(kv_hits.clone())),
        (PrimitiveType::Json, make_response(json_hits.clone())),
        (PrimitiveType::Event, make_response(event_hits.clone())),
    ];
    let simple_fused = simple.fuse(simple_results, 10);
    assert_eq!(simple_fused.hits.len(), 10);
    assert!(simple_fused.truncated);
    // Scores should be non-increasing
    for w in simple_fused.hits.windows(2) {
        assert!(w[0].score >= w[1].score);
    }

    // Test RRFFuser
    let rrf = RRFFuser::default();
    let rrf_results = vec![
        (PrimitiveType::Kv, make_response(kv_hits)),
        (PrimitiveType::Json, make_response(json_hits)),
        (PrimitiveType::Event, make_response(event_hits)),
    ];
    let rrf_fused = rrf.fuse(rrf_results, 10);
    assert_eq!(rrf_fused.hits.len(), 10);
    assert!(rrf_fused.truncated);
    // RRF scores should be non-increasing
    for w in rrf_fused.hits.windows(2) {
        assert!(w[0].score >= w[1].score);
    }
}
