//! Storage codec abstraction.
//!
//! The codec seam provides a hook point for future encryption-at-rest and
//! compression. All bytes passing through the storage layer go through the
//! codec for encode/decode operations.
//!
//!
//!
//! Uses `IdentityCodec` which performs no transformation. This establishes
//! the codec seam without adding complexity. Future milestones can add:
//!
//! - `AesGcmCodec`: AES-256-GCM encryption at rest
//! - `Lz4Codec`: LZ4 compression
//! - `ChainedCodec`: Compression + encryption pipeline
//!
//! # Usage
//!
//! ```ignore
//! use strata_storage::codec::{StorageCodec, IdentityCodec};
//!
//! let codec = IdentityCodec;
//! let data = b"hello world";
//!
//! let encoded = codec.encode(data);
//! let decoded = codec.decode(&encoded)?;
//!
//! assert_eq!(data.as_slice(), decoded.as_slice());
//! ```

mod identity;
mod traits;

pub use identity::IdentityCodec;
pub use traits::{CodecError, StorageCodec};

/// Get a codec by its identifier.
///
/// Returns the codec if recognized, or an error for unknown codec IDs.
///
/// # Known Codecs
///
/// - `"identity"`: No-op codec (pass-through)
///
/// # Future Codecs
///
/// - `"aes-gcm-256"`: AES-256-GCM encryption
/// - `"lz4"`: LZ4 compression
pub fn get_codec(codec_id: &str) -> Result<Box<dyn StorageCodec>, CodecError> {
    match codec_id {
        "identity" => Ok(Box::new(IdentityCodec)),
        _ => Err(CodecError::UnknownCodec(codec_id.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_identity_codec() {
        let codec = get_codec("identity").unwrap();
        assert_eq!(codec.codec_id(), "identity");
    }

    #[test]
    fn test_get_unknown_codec() {
        let result = get_codec("unknown");
        assert!(matches!(result, Err(CodecError::UnknownCodec(_))));
    }
}
