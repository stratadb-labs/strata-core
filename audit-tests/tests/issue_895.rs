//! Audit test for issue #895: Codec decode errors not diagnosable
//! Verdict: CONFIRMED BUG
//!
//! All codec.decode() failures in primitives.rs produce the same opaque error
//! (PrimitiveSerializeError::Codec) with no context about which primitive type,
//! entry index, or field triggered the failure. When non-identity codecs are used
//! (e.g., encryption or compression), decode failures will be extremely hard to diagnose.

use strata_durability::codec::{CodecError, StorageCodec};
use strata_durability::format::primitives::{
    KvSnapshotEntry, PrimitiveSerializeError, SnapshotSerializer,
};

/// A codec that fails on decode after a configured number of successful decodes.
/// This simulates a codec that can decode some fields but fails on others
/// (e.g., a corrupted encryption key for a specific data segment).
struct FailingCodec {
    /// Number of successful decodes before failure
    fail_after: std::sync::atomic::AtomicUsize,
}

impl FailingCodec {
    fn new(fail_after: usize) -> Self {
        Self {
            fail_after: std::sync::atomic::AtomicUsize::new(fail_after),
        }
    }
}

impl StorageCodec for FailingCodec {
    fn encode(&self, data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }

    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, CodecError> {
        let remaining = self
            .fail_after
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        if remaining == 0 {
            Err(CodecError::decode(
                "simulated decryption failure",
                self.codec_id(),
                data.len(),
            ))
        } else {
            Ok(data.to_vec())
        }
    }

    fn codec_id(&self) -> &str {
        "failing-test-codec"
    }
}

#[test]
fn issue_895_kv_decode_error_has_no_field_context() {
    // Create valid serialized KV data using identity codec
    let identity_serializer =
        SnapshotSerializer::new(Box::new(strata_durability::codec::IdentityCodec));

    let entries = vec![
        KvSnapshotEntry {
            key: "key1".to_string(),
            value: b"value1".to_vec(),
            version: 1,
            timestamp: 1000,
        },
        KvSnapshotEntry {
            key: "key2".to_string(),
            value: b"value2".to_vec(),
            version: 2,
            timestamp: 2000,
        },
    ];

    let serialized = identity_serializer.serialize_kv(&entries);

    // Now try to deserialize with a codec that fails on the second decode call.
    // The first entry's value decodes successfully, the second entry's value fails.
    let failing_serializer = SnapshotSerializer::new(Box::new(FailingCodec::new(1)));

    let result = failing_serializer.deserialize_kv(&serialized);

    // BUG: The error gives no indication of WHICH entry or WHICH field failed.
    // Was it key1's value? key2's value? Some metadata field?
    let err = result.unwrap_err();

    match &err {
        PrimitiveSerializeError::Codec(codec_err) => {
            let msg = codec_err.to_string();
            // The error message only says "Decode error: simulated decryption failure"
            // It does NOT include:
            // - The primitive type being deserialized (KV)
            // - The entry index (1, i.e., "key2")
            // - The field name ("value")
            assert!(
                msg.contains("simulated decryption failure"),
                "Error message should contain the codec error: {}",
                msg
            );
            assert!(
                !msg.contains("key2"),
                "BUG CONFIRMED: Error does not identify which key failed"
            );
            assert!(
                !msg.contains("entry"),
                "BUG CONFIRMED: Error does not identify which entry index failed"
            );
            assert!(
                !msg.contains("KV") && !msg.contains("value field"),
                "BUG CONFIRMED: Error does not identify which field or primitive type failed"
            );
        }
        other => {
            panic!("Expected PrimitiveSerializeError::Codec, got: {:?}", other);
        }
    }
}

#[test]
fn issue_895_all_primitive_decode_errors_look_identical() {
    // Demonstrate that codec decode errors from different primitive types
    // produce identical error structures — you cannot tell them apart.
    let identity_serializer =
        SnapshotSerializer::new(Box::new(strata_durability::codec::IdentityCodec));

    // Serialize KV entries
    let kv_entries = vec![KvSnapshotEntry {
        key: "k".to_string(),
        value: b"v".to_vec(),
        version: 1,
        timestamp: 0,
    }];
    let kv_data = identity_serializer.serialize_kv(&kv_entries);

    // Serialize Event entries
    let event_entries = vec![strata_durability::format::primitives::EventSnapshotEntry {
        sequence: 1,
        payload: b"p".to_vec(),
        timestamp: 0,
    }];
    let event_data = identity_serializer.serialize_events(&event_entries);

    // Deserialize both with failing codecs
    let kv_serializer = SnapshotSerializer::new(Box::new(FailingCodec::new(0)));
    let event_serializer = SnapshotSerializer::new(Box::new(FailingCodec::new(0)));

    let kv_err = kv_serializer.deserialize_kv(&kv_data).unwrap_err();
    let event_err = event_serializer
        .deserialize_events(&event_data)
        .unwrap_err();

    // BUG: Both errors produce identical messages — you cannot distinguish
    // a KV decode failure from an Event decode failure
    let kv_msg = kv_err.to_string();
    let event_msg = event_err.to_string();

    assert_eq!(
        kv_msg, event_msg,
        "BUG CONFIRMED: KV and Event codec errors are indistinguishable: '{}' vs '{}'",
        kv_msg, event_msg
    );
}
