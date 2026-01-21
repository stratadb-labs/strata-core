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

    // Test that trait is object-safe
    fn _accepts_box_dyn_codec(_codec: Box<dyn StorageCodec>) {}
}
