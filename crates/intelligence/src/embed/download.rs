//! Model download utility for MiniLM-L6-v2.
//!
//! Downloads model files from a GitHub Release as a zstd-compressed tarball
//! into a system-wide directory (`~/.stratadb/models/minilm-l6-v2/`).
//!
//! All four delivery surfaces (CLI, MCP, Python, Node) call [`ensure_model`]
//! to guarantee model files are present before embedding.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// GitHub Release URL for the model tarball.
const MODEL_URL: &str =
    "https://github.com/stratadb-labs/strata-core/releases/download/models-v1/minilm-l6-v2.tar.zst";

/// Maximum time to wait for a concurrent download (seconds).
const DOWNLOAD_WAIT_TIMEOUT_SECS: u64 = 120;

/// Poll interval while waiting for a concurrent download (seconds).
const DOWNLOAD_POLL_INTERVAL_SECS: u64 = 2;

/// Maximum age of a `.downloading` sentinel before we consider it stale (seconds).
const LOCK_STALE_SECS: u64 = 300;

/// Maximum bytes we'll read from the network (100 MB safety cap).
const MAX_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024;

/// Returns the system-wide model directory: `~/.stratadb/models/minilm-l6-v2/`.
pub fn system_model_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".stratadb/models/minilm-l6-v2")
}

/// Returns `true` if both `model.safetensors` and `vocab.txt` exist in `dir`.
pub fn model_files_present(dir: &Path) -> bool {
    dir.join("model.safetensors").exists() && dir.join("vocab.txt").exists()
}

/// Download the model tarball from GitHub, decompress with zstd, and extract
/// into `target_dir`. Creates parent directories as needed.
///
/// Uses a `.downloading` sentinel file for coarse concurrency control:
/// if another process is already downloading, this function waits for it
/// to finish (up to 120 seconds).
pub fn download_model(target_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|e| {
        format!(
            "Failed to create model directory '{}': {}",
            target_dir.display(),
            e
        )
    })?;

    let lock_path = target_dir.join(".downloading");

    // If another process is downloading (and the lock isn't stale), wait for it.
    if lock_path.exists() && !is_lock_stale(&lock_path) {
        return wait_for_download(target_dir, &lock_path);
    }

    // Remove stale lock if present.
    if lock_path.exists() {
        let _ = fs::remove_file(&lock_path);
    }

    // Create sentinel — best-effort; races are handled by the final check.
    if let Err(e) = fs::write(&lock_path, std::process::id().to_string()) {
        // If we can't write the lock, another process may have beaten us.
        if model_files_present(target_dir) {
            return Ok(());
        }
        return Err(format!(
            "Failed to create lock file '{}': {}",
            lock_path.display(),
            e
        ));
    }

    let result = do_download(target_dir);

    // Always clean up the sentinel.
    let _ = fs::remove_file(&lock_path);

    result?;

    if !model_files_present(target_dir) {
        return Err(format!(
            "Download completed but model files are missing in '{}'",
            target_dir.display()
        ));
    }

    Ok(())
}

/// Ensure model files are present, downloading if necessary.
///
/// This is the main entry point for all surfaces. Returns the path where
/// model files are located.
pub fn ensure_model() -> Result<PathBuf, String> {
    let dir = system_model_dir();
    if model_files_present(&dir) {
        return Ok(dir);
    }
    download_model(&dir)?;
    Ok(dir)
}

// =========================================================================
// Internal helpers
// =========================================================================

/// Check if a lock file is stale (older than `LOCK_STALE_SECS`).
fn is_lock_stale(lock_path: &Path) -> bool {
    lock_path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .map(|age| age.as_secs() > LOCK_STALE_SECS)
        .unwrap_or(true) // Can't determine age → treat as stale
}

fn wait_for_download(target_dir: &Path, lock_path: &Path) -> Result<(), String> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(DOWNLOAD_WAIT_TIMEOUT_SECS);
    let poll = std::time::Duration::from_secs(DOWNLOAD_POLL_INTERVAL_SECS);

    while lock_path.exists() && start.elapsed() < timeout {
        std::thread::sleep(poll);
    }

    if model_files_present(target_dir) {
        return Ok(());
    }

    if lock_path.exists() {
        Err(format!(
            "Timed out waiting for another process to finish downloading model files to '{}'",
            target_dir.display()
        ))
    } else {
        // Lock disappeared but files aren't present — the other download may
        // have failed. Try downloading ourselves.
        download_model(target_dir)
    }
}

fn do_download(target_dir: &Path) -> Result<(), String> {
    let response = ureq::get(MODEL_URL)
        .call()
        .map_err(|e| format!("Failed to download model from {}: {}", MODEL_URL, e))?;

    let reader = response.into_body().into_reader().take(MAX_DOWNLOAD_BYTES);

    let decoder = zstd::Decoder::new(reader)
        .map_err(|e| format!("Failed to initialize zstd decoder: {}", e))?;

    let mut archive = tar::Archive::new(decoder);

    // Extract to temporary files first, then rename atomically.
    // This prevents partial files from being treated as complete.
    let tmp_safetensors = target_dir.join("model.safetensors.tmp");
    let tmp_vocab = target_dir.join("vocab.txt.tmp");
    let mut wrote_safetensors = false;
    let mut wrote_vocab = false;

    // Clean up any leftover temp files from a previous failed attempt.
    let _ = fs::remove_file(&tmp_safetensors);
    let _ = fs::remove_file(&tmp_vocab);

    for entry in archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("Failed to read tar entry: {}", e))?;
        let path = entry
            .path()
            .map_err(|e| format!("Failed to read entry path: {}", e))?
            .into_owned();

        // Only extract known files to prevent path traversal.
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let tmp_dest = match file_name {
            "model.safetensors" => &tmp_safetensors,
            "vocab.txt" => &tmp_vocab,
            _ => continue,
        };

        let mut out = fs::File::create(tmp_dest)
            .map_err(|e| format!("Failed to create '{}': {}", tmp_dest.display(), e))?;

        std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("Failed to write '{}': {}", tmp_dest.display(), e))?;

        match file_name {
            "model.safetensors" => wrote_safetensors = true,
            "vocab.txt" => wrote_vocab = true,
            _ => {}
        }
    }

    if !wrote_safetensors || !wrote_vocab {
        // Clean up temp files on failure.
        let _ = fs::remove_file(&tmp_safetensors);
        let _ = fs::remove_file(&tmp_vocab);
        return Err(format!(
            "Archive missing expected files (safetensors={}, vocab={})",
            wrote_safetensors, wrote_vocab
        ));
    }

    // Atomic rename: both files appear only after fully written.
    fs::rename(&tmp_safetensors, target_dir.join("model.safetensors"))
        .map_err(|e| format!("Failed to finalize model.safetensors: {}", e))?;
    fs::rename(&tmp_vocab, target_dir.join("vocab.txt"))
        .map_err(|e| format!("Failed to finalize vocab.txt: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_model_dir_under_home() {
        let dir = system_model_dir();
        let s = dir.to_string_lossy();
        assert!(
            s.contains(".stratadb/models/minilm-l6-v2"),
            "expected .stratadb/models/minilm-l6-v2 in path, got: {}",
            s
        );
    }

    #[test]
    fn test_model_files_present_false_on_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!model_files_present(tmp.path()));
    }

    #[test]
    fn test_model_files_present_true_when_both_exist() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("model.safetensors"), b"fake").unwrap();
        fs::write(tmp.path().join("vocab.txt"), b"fake").unwrap();
        assert!(model_files_present(tmp.path()));
    }

    #[test]
    fn test_model_files_present_false_when_partial() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("model.safetensors"), b"fake").unwrap();
        assert!(!model_files_present(tmp.path()));
    }

    #[test]
    fn test_stale_lock_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let lock = tmp.path().join(".downloading");

        // Fresh lock should not be stale.
        fs::write(&lock, "12345").unwrap();
        assert!(!is_lock_stale(&lock));

        // Non-existent lock is treated as stale.
        let missing = tmp.path().join(".nonexistent");
        assert!(is_lock_stale(&missing));
    }
}
