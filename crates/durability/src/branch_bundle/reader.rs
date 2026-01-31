//! RunBundle archive reader (v2)
//!
//! Reads .runbundle.tar.zst archives and validates their contents.

use crate::branch_bundle::error::{RunBundleError, RunBundleResult};
use crate::branch_bundle::types::{
    paths, xxh3_hex, BundleManifest, BundleRunInfo, BundleVerifyInfo, RUNBUNDLE_FORMAT_VERSION,
};
use crate::branch_bundle::wal_log::{RunlogPayload, WalLogReader};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use tar::Archive;

/// Reader for RunBundle archives (v2)
///
/// Reads and validates .runbundle.tar.zst files.
pub struct RunBundleReader;

impl RunBundleReader {
    /// Validate a bundle's integrity without fully parsing WAL entries
    ///
    /// Checks:
    /// - Archive can be decompressed
    /// - Required files exist (MANIFEST.json, RUN.json, WAL.runlog)
    /// - Checksums match manifest
    /// - WAL.runlog header is valid
    pub fn validate(path: &Path) -> RunBundleResult<BundleVerifyInfo> {
        let files = Self::extract_all_files(path)?;

        // Check required files
        let manifest_data = files
            .get("MANIFEST.json")
            .ok_or_else(|| RunBundleError::missing_file("MANIFEST.json"))?;
        let run_data = files
            .get("RUN.json")
            .ok_or_else(|| RunBundleError::missing_file("RUN.json"))?;
        let wal_data = files
            .get("WAL.runlog")
            .ok_or_else(|| RunBundleError::missing_file("WAL.runlog"))?;

        // Parse manifest
        let manifest: BundleManifest = serde_json::from_slice(manifest_data)?;

        // Validate format version
        if manifest.format_version != RUNBUNDLE_FORMAT_VERSION {
            return Err(RunBundleError::UnsupportedVersion {
                version: manifest.format_version,
            });
        }

        // Validate checksums
        let mut checksums_valid = true;

        if let Some(expected) = manifest.checksums.get("RUN.json") {
            let actual = xxh3_hex(run_data);
            if expected != &actual {
                checksums_valid = false;
            }
        }

        if let Some(expected) = manifest.checksums.get("WAL.runlog") {
            let actual = xxh3_hex(wal_data);
            if expected != &actual {
                checksums_valid = false;
            }
        }

        // Validate WAL header (without parsing entries)
        WalLogReader::validate(std::io::Cursor::new(wal_data))?;

        // Parse run info for branch_id
        let run_info: BundleRunInfo = serde_json::from_slice(run_data)?;

        Ok(BundleVerifyInfo {
            branch_id: run_info.branch_id,
            format_version: manifest.format_version,
            wal_entry_count: manifest.contents.wal_entry_count,
            checksums_valid,
        })
    }

    /// Read and parse the manifest
    pub fn read_manifest(path: &Path) -> RunBundleResult<BundleManifest> {
        let data = Self::extract_file(path, "MANIFEST.json")?;
        let manifest: BundleManifest = serde_json::from_slice(&data)?;

        if manifest.format_version != RUNBUNDLE_FORMAT_VERSION {
            return Err(RunBundleError::UnsupportedVersion {
                version: manifest.format_version,
            });
        }

        Ok(manifest)
    }

    /// Read and parse the run info
    pub fn read_run_info(path: &Path) -> RunBundleResult<BundleRunInfo> {
        let data = Self::extract_file(path, "RUN.json")?;
        let run_info: BundleRunInfo = serde_json::from_slice(&data)?;
        Ok(run_info)
    }

    /// Read and parse WAL payloads
    pub fn read_wal_entries(path: &Path) -> RunBundleResult<Vec<RunlogPayload>> {
        let data = Self::extract_file(path, "WAL.runlog")?;
        WalLogReader::read_from_slice(&data)
    }

    /// Read and parse WAL payloads with checksum validation
    pub fn read_wal_entries_validated(path: &Path) -> RunBundleResult<Vec<RunlogPayload>> {
        let files = Self::extract_all_files(path)?;

        let manifest_data = files
            .get("MANIFEST.json")
            .ok_or_else(|| RunBundleError::missing_file("MANIFEST.json"))?;
        let wal_data = files
            .get("WAL.runlog")
            .ok_or_else(|| RunBundleError::missing_file("WAL.runlog"))?;

        let manifest: BundleManifest = serde_json::from_slice(manifest_data)?;

        // Validate WAL checksum
        if let Some(expected) = manifest.checksums.get("WAL.runlog") {
            let actual = xxh3_hex(wal_data);
            if expected != &actual {
                return Err(RunBundleError::ChecksumMismatch {
                    file: "WAL.runlog".to_string(),
                    expected: expected.clone(),
                    actual,
                });
            }
        }

        WalLogReader::read_from_slice(wal_data)
    }

    /// Read all components from the bundle
    pub fn read_all(path: &Path) -> RunBundleResult<BundleContents> {
        let files = Self::extract_all_files(path)?;

        let manifest_data = files
            .get("MANIFEST.json")
            .ok_or_else(|| RunBundleError::missing_file("MANIFEST.json"))?;
        let run_data = files
            .get("RUN.json")
            .ok_or_else(|| RunBundleError::missing_file("RUN.json"))?;
        let wal_data = files
            .get("WAL.runlog")
            .ok_or_else(|| RunBundleError::missing_file("WAL.runlog"))?;

        let manifest: BundleManifest = serde_json::from_slice(manifest_data)?;
        let run_info: BundleRunInfo = serde_json::from_slice(run_data)?;
        let payloads = WalLogReader::read_from_slice(wal_data)?;

        Ok(BundleContents {
            manifest,
            run_info,
            payloads,
        })
    }

    /// Extract a single file from the archive
    fn extract_file(path: &Path, file_name: &str) -> RunBundleResult<Vec<u8>> {
        let file = File::open(path)?;
        let buf_reader = BufReader::new(file);
        let decoder = zstd::Decoder::new(buf_reader)
            .map_err(|e| RunBundleError::compression(format!("zstd decode: {}", e)))?;

        let mut archive = Archive::new(decoder);
        let target_path = format!("{}/{}", paths::ROOT, file_name);

        for entry in archive.entries().map_err(|e| RunBundleError::archive(e.to_string()))? {
            let mut entry = entry.map_err(|e| RunBundleError::archive(e.to_string()))?;
            let entry_path = entry
                .path()
                .map_err(|e| RunBundleError::archive(e.to_string()))?
                .to_string_lossy()
                .to_string();

            if entry_path == target_path {
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .map_err(|e| RunBundleError::archive(format!("read {}: {}", file_name, e)))?;
                return Ok(data);
            }
        }

        Err(RunBundleError::missing_file(file_name))
    }

    /// Extract all files from the archive into a HashMap
    fn extract_all_files(path: &Path) -> RunBundleResult<HashMap<String, Vec<u8>>> {
        let file = File::open(path)?;
        let buf_reader = BufReader::new(file);
        let decoder = zstd::Decoder::new(buf_reader)
            .map_err(|e| RunBundleError::compression(format!("zstd decode: {}", e)))?;

        let mut archive = Archive::new(decoder);
        let mut files = HashMap::new();
        let prefix = format!("{}/", paths::ROOT);

        for entry in archive.entries().map_err(|e| RunBundleError::archive(e.to_string()))? {
            let mut entry = entry.map_err(|e| RunBundleError::archive(e.to_string()))?;
            let entry_path = entry
                .path()
                .map_err(|e| RunBundleError::archive(e.to_string()))?
                .to_string_lossy()
                .to_string();

            // Strip prefix to get relative file name
            if let Some(name) = entry_path.strip_prefix(&prefix) {
                if !name.is_empty() {
                    let mut data = Vec::new();
                    entry
                        .read_to_end(&mut data)
                        .map_err(|e| RunBundleError::archive(format!("read {}: {}", name, e)))?;
                    files.insert(name.to_string(), data);
                }
            }
        }

        Ok(files)
    }

    /// Read from a byte slice (for testing)
    pub fn read_manifest_from_bytes(data: &[u8]) -> RunBundleResult<BundleManifest> {
        let decoder = zstd::Decoder::new(data)
            .map_err(|e| RunBundleError::compression(format!("zstd decode: {}", e)))?;

        let mut archive = Archive::new(decoder);
        let target_path = paths::MANIFEST;

        for entry in archive.entries().map_err(|e| RunBundleError::archive(e.to_string()))? {
            let mut entry = entry.map_err(|e| RunBundleError::archive(e.to_string()))?;
            let entry_path = entry
                .path()
                .map_err(|e| RunBundleError::archive(e.to_string()))?
                .to_string_lossy()
                .to_string();

            if entry_path == target_path {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                let manifest: BundleManifest = serde_json::from_slice(&data)?;
                return Ok(manifest);
            }
        }

        Err(RunBundleError::missing_file("MANIFEST.json"))
    }

    /// Read run info from a byte slice (for testing)
    pub fn read_run_info_from_bytes(data: &[u8]) -> RunBundleResult<BundleRunInfo> {
        let decoder = zstd::Decoder::new(data)
            .map_err(|e| RunBundleError::compression(format!("zstd decode: {}", e)))?;

        let mut archive = Archive::new(decoder);
        let target_path = paths::RUN;

        for entry in archive.entries().map_err(|e| RunBundleError::archive(e.to_string()))? {
            let mut entry = entry.map_err(|e| RunBundleError::archive(e.to_string()))?;
            let entry_path = entry
                .path()
                .map_err(|e| RunBundleError::archive(e.to_string()))?
                .to_string_lossy()
                .to_string();

            if entry_path == target_path {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                let run_info: BundleRunInfo = serde_json::from_slice(&data)?;
                return Ok(run_info);
            }
        }

        Err(RunBundleError::missing_file("RUN.json"))
    }

    /// Read WAL payloads from a byte slice (for testing)
    pub fn read_wal_entries_from_bytes(data: &[u8]) -> RunBundleResult<Vec<RunlogPayload>> {
        let decoder = zstd::Decoder::new(data)
            .map_err(|e| RunBundleError::compression(format!("zstd decode: {}", e)))?;

        let mut archive = Archive::new(decoder);
        let target_path = paths::WAL;

        for entry in archive.entries().map_err(|e| RunBundleError::archive(e.to_string()))? {
            let mut entry = entry.map_err(|e| RunBundleError::archive(e.to_string()))?;
            let entry_path = entry
                .path()
                .map_err(|e| RunBundleError::archive(e.to_string()))?
                .to_string_lossy()
                .to_string();

            if entry_path == target_path {
                let mut wal_data = Vec::new();
                entry.read_to_end(&mut wal_data)?;
                return WalLogReader::read_from_slice(&wal_data);
            }
        }

        Err(RunBundleError::missing_file("WAL.runlog"))
    }
}

/// Complete bundle contents after reading (v2)
#[derive(Debug)]
pub struct BundleContents {
    /// Bundle manifest
    pub manifest: BundleManifest,
    /// Run metadata
    pub run_info: BundleRunInfo,
    /// Transaction payloads
    pub payloads: Vec<RunlogPayload>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_bundle::writer::RunBundleWriter;
    use crate::branch_bundle::wal_log::RunlogPayload;
    use crate::branch_bundle::types::ExportOptions;
    use strata_core::types::{Key, Namespace, BranchId, TypeTag};
    use strata_core::value::Value;
    use tempfile::tempdir;

    fn make_test_run_info() -> BundleRunInfo {
        BundleRunInfo {
            branch_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            name: "test-run".to_string(),
            state: "completed".to_string(),
            created_at: "2025-01-24T10:00:00Z".to_string(),
            closed_at: "2025-01-24T11:00:00Z".to_string(),
            parent_run_id: None,
            tags: vec!["test".to_string()],
            metadata: serde_json::json!({"key": "value"}),
            error: None,
        }
    }

    fn make_test_payloads() -> Vec<RunlogPayload> {
        let branch_id = BranchId::new();
        let ns = Namespace::for_branch(branch_id);
        vec![
            RunlogPayload {
                branch_id: branch_id.to_string(),
                version: 1,
                puts: vec![(
                    Key::new(ns.clone(), TypeTag::KV, b"key1".to_vec()),
                    Value::String("value1".to_string()),
                )],
                deletes: vec![],
            },
            RunlogPayload {
                branch_id: branch_id.to_string(),
                version: 2,
                puts: vec![],
                deletes: vec![Key::new(ns, TypeTag::KV, b"key1".to_vec())],
            },
        ]
    }

    fn create_test_bundle() -> (Vec<u8>, BundleRunInfo, Vec<RunlogPayload>) {
        let writer = RunBundleWriter::new(&ExportOptions::default());
        let run_info = make_test_run_info();
        let payloads = make_test_payloads();
        let (data, _) = writer.write_to_vec(&run_info, &payloads).unwrap();
        (data, run_info, payloads)
    }

    #[test]
    fn test_read_manifest_from_bytes() {
        let (data, _, _) = create_test_bundle();

        let manifest = RunBundleReader::read_manifest_from_bytes(&data).unwrap();

        assert_eq!(manifest.format_version, RUNBUNDLE_FORMAT_VERSION);
        assert!(manifest.checksums.contains_key("RUN.json"));
        assert!(manifest.checksums.contains_key("WAL.runlog"));
    }

    #[test]
    fn test_read_run_info_from_bytes() {
        let (data, expected_run_info, _) = create_test_bundle();

        let run_info = RunBundleReader::read_run_info_from_bytes(&data).unwrap();

        assert_eq!(run_info.branch_id, expected_run_info.branch_id);
        assert_eq!(run_info.name, expected_run_info.name);
        assert_eq!(run_info.state, expected_run_info.state);
    }

    #[test]
    fn test_read_wal_entries_from_bytes() {
        let (data, _, expected_payloads) = create_test_bundle();

        let payloads = RunBundleReader::read_wal_entries_from_bytes(&data).unwrap();

        assert_eq!(payloads.len(), expected_payloads.len());
    }

    #[test]
    fn test_validate_from_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.runbundle.tar.zst");

        let writer = RunBundleWriter::new(&ExportOptions::default());
        let run_info = make_test_run_info();
        let payloads = make_test_payloads();
        writer.write(&run_info, &payloads, &path).unwrap();

        let verify_info = RunBundleReader::validate(&path).unwrap();

        assert_eq!(verify_info.branch_id, run_info.branch_id);
        assert_eq!(verify_info.format_version, RUNBUNDLE_FORMAT_VERSION);
        assert_eq!(verify_info.wal_entry_count, 2);
        assert!(verify_info.checksums_valid);
    }

    #[test]
    fn test_read_manifest_from_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.runbundle.tar.zst");

        let writer = RunBundleWriter::new(&ExportOptions::default());
        let run_info = make_test_run_info();
        let payloads = make_test_payloads();
        writer.write(&run_info, &payloads, &path).unwrap();

        let manifest = RunBundleReader::read_manifest(&path).unwrap();

        assert_eq!(manifest.format_version, RUNBUNDLE_FORMAT_VERSION);
        assert_eq!(manifest.contents.wal_entry_count, 2);
    }

    #[test]
    fn test_read_run_info_from_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.runbundle.tar.zst");

        let writer = RunBundleWriter::new(&ExportOptions::default());
        let expected_run_info = make_test_run_info();
        let payloads = make_test_payloads();
        writer.write(&expected_run_info, &payloads, &path).unwrap();

        let run_info = RunBundleReader::read_run_info(&path).unwrap();

        assert_eq!(run_info.branch_id, expected_run_info.branch_id);
        assert_eq!(run_info.name, expected_run_info.name);
    }

    #[test]
    fn test_read_wal_entries_from_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.runbundle.tar.zst");

        let writer = RunBundleWriter::new(&ExportOptions::default());
        let run_info = make_test_run_info();
        let payloads = make_test_payloads();
        writer.write(&run_info, &payloads, &path).unwrap();

        let read_payloads = RunBundleReader::read_wal_entries(&path).unwrap();

        assert_eq!(read_payloads.len(), payloads.len());
    }

    #[test]
    fn test_read_wal_entries_validated() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.runbundle.tar.zst");

        let writer = RunBundleWriter::new(&ExportOptions::default());
        let run_info = make_test_run_info();
        let payloads = make_test_payloads();
        writer.write(&run_info, &payloads, &path).unwrap();

        let read_payloads = RunBundleReader::read_wal_entries_validated(&path).unwrap();

        assert_eq!(read_payloads.len(), payloads.len());
    }

    #[test]
    fn test_read_all() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.runbundle.tar.zst");

        let writer = RunBundleWriter::new(&ExportOptions::default());
        let run_info = make_test_run_info();
        let payloads = make_test_payloads();
        writer.write(&run_info, &payloads, &path).unwrap();

        let contents = RunBundleReader::read_all(&path).unwrap();

        assert_eq!(contents.run_info.branch_id, run_info.branch_id);
        assert_eq!(contents.payloads.len(), payloads.len());
        assert_eq!(contents.manifest.contents.wal_entry_count, 2);
    }

    #[test]
    fn test_missing_file_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.runbundle.tar.zst");

        let result = RunBundleReader::validate(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_corrupted_archive() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("corrupted.runbundle.tar.zst");

        // Write garbage
        std::fs::write(&path, b"not a valid archive").unwrap();

        let result = RunBundleReader::validate(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_bundle() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.runbundle.tar.zst");

        let writer = RunBundleWriter::new(&ExportOptions::default());
        let run_info = make_test_run_info();
        let payloads: Vec<RunlogPayload> = vec![];
        writer.write(&run_info, &payloads, &path).unwrap();

        let contents = RunBundleReader::read_all(&path).unwrap();

        assert!(contents.payloads.is_empty());
        assert_eq!(contents.manifest.contents.wal_entry_count, 0);
    }

    #[test]
    fn test_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("roundtrip.runbundle.tar.zst");

        // Write
        let writer = RunBundleWriter::new(&ExportOptions::default());
        let original_run_info = make_test_run_info();
        let original_payloads = make_test_payloads();
        writer
            .write(&original_run_info, &original_payloads, &path)
            .unwrap();

        // Read back
        let contents = RunBundleReader::read_all(&path).unwrap();

        // Verify
        assert_eq!(contents.run_info.branch_id, original_run_info.branch_id);
        assert_eq!(contents.run_info.name, original_run_info.name);
        assert_eq!(contents.run_info.state, original_run_info.state);
        assert_eq!(contents.run_info.tags, original_run_info.tags);
        assert_eq!(contents.payloads.len(), original_payloads.len());

        // Verify payloads match
        for (original, read) in original_payloads.iter().zip(contents.payloads.iter()) {
            assert_eq!(original, read);
        }
    }
}
