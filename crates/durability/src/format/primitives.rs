//! Primitive serialization for snapshots
//!
//! Each primitive has a defined binary format for snapshot storage.
//! All values pass through the codec for encoding/decoding.
//!
//! # Binary Format
//!
//! All sections start with a 4-byte count of entries, followed by the entries.
//! Strings are length-prefixed (4-byte length + bytes).
//! All integers are little-endian.

use crate::codec::StorageCodec;

/// Snapshot entry for KV primitive
///
/// Format: key_len(4) + key + value_len(4) + value + version(8) + timestamp(8)
#[derive(Debug, Clone, PartialEq)]
pub struct KvSnapshotEntry {
    /// Key string
    pub key: String,
    /// Value bytes (pre-codec)
    pub value: Vec<u8>,
    /// Version counter
    pub version: u64,
    /// Timestamp (microseconds since epoch)
    pub timestamp: u64,
}

/// Snapshot entry for Event primitive
///
/// Format: sequence(8) + payload_len(4) + payload + timestamp(8)
#[derive(Debug, Clone, PartialEq)]
pub struct EventSnapshotEntry {
    /// Event sequence number
    pub sequence: u64,
    /// Event payload bytes (pre-codec)
    pub payload: Vec<u8>,
    /// Timestamp (microseconds since epoch)
    pub timestamp: u64,
}

/// Snapshot entry for State primitive
///
/// Format: name_len(4) + name + value_len(4) + value + counter(8) + timestamp(8)
#[derive(Debug, Clone, PartialEq)]
pub struct StateSnapshotEntry {
    /// State cell name
    pub name: String,
    /// Value bytes (pre-codec)
    pub value: Vec<u8>,
    /// CAS counter
    pub counter: u64,
    /// Timestamp (microseconds since epoch)
    pub timestamp: u64,
}

/// Snapshot entry for Run primitive
///
/// Format: run_id(16 bytes UUID) + name_len(4) + name + created_at(8) + metadata_len(4) + metadata
#[derive(Debug, Clone, PartialEq)]
pub struct RunSnapshotEntry {
    /// Run identifier (UUID bytes)
    pub run_id: [u8; 16],
    /// Run name
    pub name: String,
    /// Creation timestamp (microseconds)
    pub created_at: u64,
    /// Metadata as serialized bytes
    pub metadata: Vec<u8>,
}

/// Snapshot entry for Json primitive
///
/// Format: doc_id_len(4) + doc_id + content_len(4) + content + version(8) + timestamp(8)
#[derive(Debug, Clone, PartialEq)]
pub struct JsonSnapshotEntry {
    /// Document identifier
    pub doc_id: String,
    /// JSON content bytes (pre-codec)
    pub content: Vec<u8>,
    /// Version counter
    pub version: u64,
    /// Timestamp (microseconds)
    pub timestamp: u64,
}

/// Snapshot entry for Vector primitive (collection level)
///
/// Format: collection_name_len(4) + name + config_len(4) + config + vectors_count(4) + [vectors...]
#[derive(Debug, Clone, PartialEq)]
pub struct VectorCollectionSnapshotEntry {
    /// Collection name
    pub name: String,
    /// Collection configuration as serialized bytes
    pub config: Vec<u8>,
    /// Vectors in this collection
    pub vectors: Vec<VectorSnapshotEntry>,
}

/// Individual vector within a collection
///
/// Format: key_len(4) + key + vector_id(8) + dimensions(4) + [f32...] + metadata_len(4) + metadata
#[derive(Debug, Clone, PartialEq)]
pub struct VectorSnapshotEntry {
    /// Vector key
    pub key: String,
    /// Internal vector ID
    pub vector_id: u64,
    /// Embedding values
    pub embedding: Vec<f32>,
    /// Optional metadata as serialized bytes
    pub metadata: Vec<u8>,
}

/// Serializer for snapshot primitive data
pub struct SnapshotSerializer {
    codec: Box<dyn StorageCodec>,
}

impl SnapshotSerializer {
    /// Create a new serializer with the given codec
    pub fn new(codec: Box<dyn StorageCodec>) -> Self {
        SnapshotSerializer { codec }
    }

    /// Serialize KV entries to bytes
    pub fn serialize_kv(&self, entries: &[KvSnapshotEntry]) -> Vec<u8> {
        let mut data = Vec::new();

        // Entry count
        data.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        for entry in entries {
            // Key
            let key_bytes = entry.key.as_bytes();
            data.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(key_bytes);

            // Value (through codec)
            let value_bytes = self.codec.encode(&entry.value);
            data.extend_from_slice(&(value_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&value_bytes);

            // Version and timestamp
            data.extend_from_slice(&entry.version.to_le_bytes());
            data.extend_from_slice(&entry.timestamp.to_le_bytes());
        }

        data
    }

    /// Deserialize KV entries from bytes
    pub fn deserialize_kv(&self, data: &[u8]) -> Result<Vec<KvSnapshotEntry>, PrimitiveSerializeError> {
        let mut cursor = 0;

        if data.len() < 4 {
            return Err(PrimitiveSerializeError::UnexpectedEof);
        }

        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            // Key
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let key_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + key_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let key = String::from_utf8(data[cursor..cursor + key_len].to_vec())
                .map_err(|_| PrimitiveSerializeError::InvalidUtf8)?;
            cursor += key_len;

            // Value
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let value_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + value_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let encoded_value = &data[cursor..cursor + value_len];
            let value = self.codec.decode(encoded_value)?;
            cursor += value_len;

            // Version and timestamp
            if cursor + 16 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let version = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;
            let timestamp = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;

            entries.push(KvSnapshotEntry {
                key,
                value,
                version,
                timestamp,
            });
        }

        Ok(entries)
    }

    /// Serialize Event entries to bytes
    pub fn serialize_events(&self, entries: &[EventSnapshotEntry]) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        for entry in entries {
            data.extend_from_slice(&entry.sequence.to_le_bytes());

            let payload_bytes = self.codec.encode(&entry.payload);
            data.extend_from_slice(&(payload_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&payload_bytes);

            data.extend_from_slice(&entry.timestamp.to_le_bytes());
        }

        data
    }

    /// Deserialize Event entries from bytes
    pub fn deserialize_events(&self, data: &[u8]) -> Result<Vec<EventSnapshotEntry>, PrimitiveSerializeError> {
        let mut cursor = 0;

        if data.len() < 4 {
            return Err(PrimitiveSerializeError::UnexpectedEof);
        }

        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            if cursor + 8 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let sequence = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;

            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let payload_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + payload_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let encoded_payload = &data[cursor..cursor + payload_len];
            let payload = self.codec.decode(encoded_payload)?;
            cursor += payload_len;

            if cursor + 8 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let timestamp = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;

            entries.push(EventSnapshotEntry {
                sequence,
                payload,
                timestamp,
            });
        }

        Ok(entries)
    }

    /// Serialize State entries to bytes
    pub fn serialize_states(&self, entries: &[StateSnapshotEntry]) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        for entry in entries {
            let name_bytes = entry.name.as_bytes();
            data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(name_bytes);

            let value_bytes = self.codec.encode(&entry.value);
            data.extend_from_slice(&(value_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&value_bytes);

            data.extend_from_slice(&entry.counter.to_le_bytes());
            data.extend_from_slice(&entry.timestamp.to_le_bytes());
        }

        data
    }

    /// Deserialize State entries from bytes
    pub fn deserialize_states(&self, data: &[u8]) -> Result<Vec<StateSnapshotEntry>, PrimitiveSerializeError> {
        let mut cursor = 0;

        if data.len() < 4 {
            return Err(PrimitiveSerializeError::UnexpectedEof);
        }

        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let name_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + name_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let name = String::from_utf8(data[cursor..cursor + name_len].to_vec())
                .map_err(|_| PrimitiveSerializeError::InvalidUtf8)?;
            cursor += name_len;

            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let value_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + value_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let value = self.codec.decode(&data[cursor..cursor + value_len])?;
            cursor += value_len;

            if cursor + 16 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let counter = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;
            let timestamp = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;

            entries.push(StateSnapshotEntry {
                name,
                value,
                counter,
                timestamp,
            });
        }

        Ok(entries)
    }

    /// Serialize Run entries to bytes
    pub fn serialize_runs(&self, entries: &[RunSnapshotEntry]) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        for entry in entries {
            data.extend_from_slice(&entry.run_id);

            let name_bytes = entry.name.as_bytes();
            data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(name_bytes);

            data.extend_from_slice(&entry.created_at.to_le_bytes());

            let metadata_bytes = self.codec.encode(&entry.metadata);
            data.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&metadata_bytes);
        }

        data
    }

    /// Deserialize Run entries from bytes
    pub fn deserialize_runs(&self, data: &[u8]) -> Result<Vec<RunSnapshotEntry>, PrimitiveSerializeError> {
        let mut cursor = 0;

        if data.len() < 4 {
            return Err(PrimitiveSerializeError::UnexpectedEof);
        }

        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            // Run ID (16 bytes)
            if cursor + 16 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let run_id: [u8; 16] = data[cursor..cursor + 16].try_into().unwrap();
            cursor += 16;

            // Name
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let name_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + name_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let name = String::from_utf8(data[cursor..cursor + name_len].to_vec())
                .map_err(|_| PrimitiveSerializeError::InvalidUtf8)?;
            cursor += name_len;

            // Created at
            if cursor + 8 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let created_at = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;

            // Metadata
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let metadata_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + metadata_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let metadata = self.codec.decode(&data[cursor..cursor + metadata_len])?;
            cursor += metadata_len;

            entries.push(RunSnapshotEntry {
                run_id,
                name,
                created_at,
                metadata,
            });
        }

        Ok(entries)
    }

    /// Serialize Json entries to bytes
    pub fn serialize_json(&self, entries: &[JsonSnapshotEntry]) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        for entry in entries {
            let doc_id_bytes = entry.doc_id.as_bytes();
            data.extend_from_slice(&(doc_id_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(doc_id_bytes);

            let content_bytes = self.codec.encode(&entry.content);
            data.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&content_bytes);

            data.extend_from_slice(&entry.version.to_le_bytes());
            data.extend_from_slice(&entry.timestamp.to_le_bytes());
        }

        data
    }

    /// Deserialize Json entries from bytes
    pub fn deserialize_json(&self, data: &[u8]) -> Result<Vec<JsonSnapshotEntry>, PrimitiveSerializeError> {
        let mut cursor = 0;

        if data.len() < 4 {
            return Err(PrimitiveSerializeError::UnexpectedEof);
        }

        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            // Doc ID
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let doc_id_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + doc_id_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let doc_id = String::from_utf8(data[cursor..cursor + doc_id_len].to_vec())
                .map_err(|_| PrimitiveSerializeError::InvalidUtf8)?;
            cursor += doc_id_len;

            // Content
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let content_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + content_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let content = self.codec.decode(&data[cursor..cursor + content_len])?;
            cursor += content_len;

            // Version and timestamp
            if cursor + 16 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let version = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;
            let timestamp = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;

            entries.push(JsonSnapshotEntry {
                doc_id,
                content,
                version,
                timestamp,
            });
        }

        Ok(entries)
    }

    /// Serialize Vector collections to bytes
    pub fn serialize_vectors(&self, collections: &[VectorCollectionSnapshotEntry]) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(&(collections.len() as u32).to_le_bytes());

        for collection in collections {
            // Collection name
            let name_bytes = collection.name.as_bytes();
            data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(name_bytes);

            // Config (through codec)
            let config_bytes = self.codec.encode(&collection.config);
            data.extend_from_slice(&(config_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&config_bytes);

            // Vectors
            data.extend_from_slice(&(collection.vectors.len() as u32).to_le_bytes());
            for vector in &collection.vectors {
                // Key
                let key_bytes = vector.key.as_bytes();
                data.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
                data.extend_from_slice(key_bytes);

                // Vector ID
                data.extend_from_slice(&vector.vector_id.to_le_bytes());

                // Embedding dimensions and values
                data.extend_from_slice(&(vector.embedding.len() as u32).to_le_bytes());
                for &value in &vector.embedding {
                    data.extend_from_slice(&value.to_le_bytes());
                }

                // Metadata (through codec)
                let metadata_bytes = self.codec.encode(&vector.metadata);
                data.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
                data.extend_from_slice(&metadata_bytes);
            }
        }

        data
    }

    /// Deserialize Vector collections from bytes
    pub fn deserialize_vectors(&self, data: &[u8]) -> Result<Vec<VectorCollectionSnapshotEntry>, PrimitiveSerializeError> {
        let mut cursor = 0;

        if data.len() < 4 {
            return Err(PrimitiveSerializeError::UnexpectedEof);
        }

        let collections_count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut collections = Vec::with_capacity(collections_count);

        for _ in 0..collections_count {
            // Collection name
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let name_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + name_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let name = String::from_utf8(data[cursor..cursor + name_len].to_vec())
                .map_err(|_| PrimitiveSerializeError::InvalidUtf8)?;
            cursor += name_len;

            // Config
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let config_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            if cursor + config_len > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let config = self.codec.decode(&data[cursor..cursor + config_len])?;
            cursor += config_len;

            // Vectors
            if cursor + 4 > data.len() {
                return Err(PrimitiveSerializeError::UnexpectedEof);
            }
            let vectors_count = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            let mut vectors = Vec::with_capacity(vectors_count);
            for _ in 0..vectors_count {
                // Key
                if cursor + 4 > data.len() {
                    return Err(PrimitiveSerializeError::UnexpectedEof);
                }
                let key_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
                cursor += 4;

                if cursor + key_len > data.len() {
                    return Err(PrimitiveSerializeError::UnexpectedEof);
                }
                let key = String::from_utf8(data[cursor..cursor + key_len].to_vec())
                    .map_err(|_| PrimitiveSerializeError::InvalidUtf8)?;
                cursor += key_len;

                // Vector ID
                if cursor + 8 > data.len() {
                    return Err(PrimitiveSerializeError::UnexpectedEof);
                }
                let vector_id = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap());
                cursor += 8;

                // Embedding
                if cursor + 4 > data.len() {
                    return Err(PrimitiveSerializeError::UnexpectedEof);
                }
                let dims = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
                cursor += 4;

                if cursor + dims * 4 > data.len() {
                    return Err(PrimitiveSerializeError::UnexpectedEof);
                }
                let mut embedding = Vec::with_capacity(dims);
                for _ in 0..dims {
                    let value = f32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap());
                    cursor += 4;
                    embedding.push(value);
                }

                // Metadata
                if cursor + 4 > data.len() {
                    return Err(PrimitiveSerializeError::UnexpectedEof);
                }
                let metadata_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
                cursor += 4;

                if cursor + metadata_len > data.len() {
                    return Err(PrimitiveSerializeError::UnexpectedEof);
                }
                let metadata = self.codec.decode(&data[cursor..cursor + metadata_len])?;
                cursor += metadata_len;

                vectors.push(VectorSnapshotEntry {
                    key,
                    vector_id,
                    embedding,
                    metadata,
                });
            }

            collections.push(VectorCollectionSnapshotEntry {
                name,
                config,
                vectors,
            });
        }

        Ok(collections)
    }
}

/// Errors that can occur during primitive serialization
#[derive(Debug, thiserror::Error)]
pub enum PrimitiveSerializeError {
    /// Unexpected end of data
    #[error("Unexpected end of data")]
    UnexpectedEof,
    /// Invalid UTF-8 string
    #[error("Invalid UTF-8 string")]
    InvalidUtf8,
    /// Codec error
    #[error("Codec error: {0}")]
    Codec(#[from] crate::codec::CodecError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::IdentityCodec;

    fn test_serializer() -> SnapshotSerializer {
        SnapshotSerializer::new(Box::new(IdentityCodec))
    }

    #[test]
    fn test_kv_roundtrip() {
        let serializer = test_serializer();

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

        let data = serializer.serialize_kv(&entries);
        let parsed = serializer.deserialize_kv(&data).unwrap();

        assert_eq!(entries, parsed);
    }

    #[test]
    fn test_kv_empty() {
        let serializer = test_serializer();

        let entries: Vec<KvSnapshotEntry> = vec![];
        let data = serializer.serialize_kv(&entries);
        let parsed = serializer.deserialize_kv(&data).unwrap();

        assert!(parsed.is_empty());
    }

    #[test]
    fn test_kv_unicode() {
        let serializer = test_serializer();

        let entries = vec![KvSnapshotEntry {
            key: "key_\u{1F600}_emoji".to_string(),
            value: "value_\u{4E2D}\u{6587}_chinese".as_bytes().to_vec(),
            version: 42,
            timestamp: 9999,
        }];

        let data = serializer.serialize_kv(&entries);
        let parsed = serializer.deserialize_kv(&data).unwrap();

        assert_eq!(entries, parsed);
    }

    #[test]
    fn test_events_roundtrip() {
        let serializer = test_serializer();

        let entries = vec![
            EventSnapshotEntry {
                sequence: 1,
                payload: b"event1".to_vec(),
                timestamp: 1000,
            },
            EventSnapshotEntry {
                sequence: 2,
                payload: b"event2".to_vec(),
                timestamp: 2000,
            },
        ];

        let data = serializer.serialize_events(&entries);
        let parsed = serializer.deserialize_events(&data).unwrap();

        assert_eq!(entries, parsed);
    }

    #[test]
    fn test_states_roundtrip() {
        let serializer = test_serializer();

        let entries = vec![
            StateSnapshotEntry {
                name: "state1".to_string(),
                value: b"value1".to_vec(),
                counter: 10,
                timestamp: 1000,
            },
            StateSnapshotEntry {
                name: "state2".to_string(),
                value: b"value2".to_vec(),
                counter: 20,
                timestamp: 2000,
            },
        ];

        let data = serializer.serialize_states(&entries);
        let parsed = serializer.deserialize_states(&data).unwrap();

        assert_eq!(entries, parsed);
    }

    #[test]
    fn test_runs_roundtrip() {
        let serializer = test_serializer();

        let entries = vec![RunSnapshotEntry {
            run_id: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            name: "my-run".to_string(),
            created_at: 1000,
            metadata: b"{}".to_vec(),
        }];

        let data = serializer.serialize_runs(&entries);
        let parsed = serializer.deserialize_runs(&data).unwrap();

        assert_eq!(entries, parsed);
    }

    #[test]
    fn test_json_roundtrip() {
        let serializer = test_serializer();

        let entries = vec![
            JsonSnapshotEntry {
                doc_id: "doc1".to_string(),
                content: b"{\"name\":\"test\"}".to_vec(),
                version: 1,
                timestamp: 1000,
            },
            JsonSnapshotEntry {
                doc_id: "doc2".to_string(),
                content: b"{\"value\":42}".to_vec(),
                version: 2,
                timestamp: 2000,
            },
        ];

        let data = serializer.serialize_json(&entries);
        let parsed = serializer.deserialize_json(&data).unwrap();

        assert_eq!(entries, parsed);
    }

    #[test]
    fn test_vectors_roundtrip() {
        let serializer = test_serializer();

        let collections = vec![VectorCollectionSnapshotEntry {
            name: "embeddings".to_string(),
            config: b"{\"dimensions\":384}".to_vec(),
            vectors: vec![
                VectorSnapshotEntry {
                    key: "vec1".to_string(),
                    vector_id: 1,
                    embedding: vec![0.1, 0.2, 0.3],
                    metadata: b"{}".to_vec(),
                },
                VectorSnapshotEntry {
                    key: "vec2".to_string(),
                    vector_id: 2,
                    embedding: vec![0.4, 0.5, 0.6],
                    metadata: b"{\"label\":\"test\"}".to_vec(),
                },
            ],
        }];

        let data = serializer.serialize_vectors(&collections);
        let parsed = serializer.deserialize_vectors(&data).unwrap();

        assert_eq!(collections, parsed);
    }

    #[test]
    fn test_vectors_high_dimension() {
        let serializer = test_serializer();

        // Test with 384 dimensions (MiniLM)
        let embedding: Vec<f32> = (0..384).map(|i| i as f32 * 0.001).collect();

        let collections = vec![VectorCollectionSnapshotEntry {
            name: "minilm".to_string(),
            config: b"{\"dimensions\":384}".to_vec(),
            vectors: vec![VectorSnapshotEntry {
                key: "high-dim".to_string(),
                vector_id: 1,
                embedding: embedding.clone(),
                metadata: vec![],
            }],
        }];

        let data = serializer.serialize_vectors(&collections);
        let parsed = serializer.deserialize_vectors(&data).unwrap();

        assert_eq!(parsed[0].vectors[0].embedding.len(), 384);
        assert_eq!(parsed[0].vectors[0].embedding, embedding);
    }

    #[test]
    fn test_deserialize_truncated_data() {
        let serializer = test_serializer();

        // Truncated KV data
        let result = serializer.deserialize_kv(&[0, 0, 0, 1]); // Says 1 entry but no data
        assert!(matches!(result, Err(PrimitiveSerializeError::UnexpectedEof)));

        // Too short
        let result = serializer.deserialize_kv(&[0, 0]);
        assert!(matches!(result, Err(PrimitiveSerializeError::UnexpectedEof)));
    }
}
