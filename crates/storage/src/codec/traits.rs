//! Storage codec trait definitions.

/// Storage codec trait.
///
/// All bytes passing through the storage layer go through the codec.
/// This provides a seam for future encryption-at-rest and compression.
///
/// # Thread Safety
///
/// Codecs must be `Send + Sync` to allow concurrent encoding/decoding
/// from multiple threads.
///
/// # Codec Identity
///
/// Each codec has a unique identifier that is stored in the MANIFEST.
/// This allows the database to verify it's using the correct codec
/// when reopening.
pub trait StorageCodec: Send + Sync {
    /// Encode bytes for storage.
    ///
    /// The returned bytes are what gets written to disk.
    /// For IdentityCodec, this is a no-op.
    fn encode(&self, data: &[u8]) -> Vec<u8>;

    /// Decode bytes from storage.
    ///
    /// Reverses the encode operation. Returns an error if the data
    /// cannot be decoded (e.g., decryption failure, corruption).
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, CodecError>;

    /// Unique codec identifier.
    ///
    /// This is stored in the MANIFEST to ensure the correct codec
    /// is used when reopening a database.
    fn codec_id(&self) -> &str;
}

/// Codec errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    /// Decoding failed (e.g., decryption failure, invalid format).
    #[error("Decode error: {0}")]
    DecodeError(String),

    /// Unknown codec identifier.
    #[error("Unknown codec: {0}")]
    UnknownCodec(String),

    /// Codec mismatch (database was created with different codec).
    #[error("Codec mismatch: expected {expected}, got {actual}")]
    CodecMismatch {
        /// Expected codec ID from MANIFEST
        expected: String,
        /// Actual codec ID being used
        actual: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::IdentityCodec;

    // Test that trait is object-safe
    fn _accepts_box_dyn_codec(_codec: Box<dyn StorageCodec>) {}

    #[test]
    fn test_codec_trait_object_safe() {
        // Verify we can create and use a boxed trait object
        let codec: Box<dyn StorageCodec> = Box::new(IdentityCodec);

        // Test encode/decode through trait object
        let data = b"test data";
        let encoded = codec.encode(data);
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(decoded, data);
    }

    #[test]
    fn test_codec_trait_codec_id() {
        let codec: Box<dyn StorageCodec> = Box::new(IdentityCodec);
        assert_eq!(codec.codec_id(), "identity");
    }

    #[test]
    fn test_codec_error_display() {
        let err = CodecError::DecodeError("test error".to_string());
        assert!(err.to_string().contains("test error"));

        let err = CodecError::UnknownCodec("mystery".to_string());
        assert!(err.to_string().contains("mystery"));

        let err = CodecError::CodecMismatch {
            expected: "aes256".to_string(),
            actual: "identity".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("aes256"));
        assert!(msg.contains("identity"));
    }

    #[test]
    fn test_codec_error_equality() {
        let err1 = CodecError::DecodeError("error".to_string());
        let err2 = CodecError::DecodeError("error".to_string());
        let err3 = CodecError::DecodeError("different".to_string());

        assert_eq!(err1, err2);
        assert_ne!(err1, err3);
    }

    #[test]
    fn test_codec_roundtrip_empty_data() {
        let codec: Box<dyn StorageCodec> = Box::new(IdentityCodec);

        let data = b"";
        let encoded = codec.encode(data);
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(decoded, data);
    }

    #[test]
    fn test_codec_roundtrip_large_data() {
        let codec: Box<dyn StorageCodec> = Box::new(IdentityCodec);

        // 1MB of data
        let data: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();
        let encoded = codec.encode(&data);
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(decoded, data);
    }

    #[test]
    fn test_codec_roundtrip_binary_data() {
        let codec: Box<dyn StorageCodec> = Box::new(IdentityCodec);

        // Data with all byte values including null bytes
        let data: Vec<u8> = (0..=255).collect();
        let encoded = codec.encode(&data);
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(decoded, data);
    }
}
