//! Optional inverted index for fast keyword search
//!
//! This module provides:
//! - InvertedIndex with posting lists
//! - Enable/disable functionality
//! - Synchronous index updates on commit
//! - Version watermark for consistency
//!
//! See `docs/architecture/M6_ARCHITECTURE.md` for authoritative specification.
//!
//! # Architectural Rules
//!
//! - Rule 1: No Data Movement - index stores DocRef only, not content
//! - Rule 5: Zero Overhead When Disabled - NOOP when disabled
//!
//! # Usage
//!
//! Indexing is OPTIONAL. Search works without it (via full scan).
//! When enabled, search uses the index for candidate lookup.

use crate::tokenizer::tokenize;
use dashmap::DashMap;
use strata_core::search_types::DocRef;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

// ============================================================================
// PostingEntry
// ============================================================================

/// Entry in a posting list
///
/// Contains the DocRef and term statistics for scoring.
#[derive(Debug, Clone)]
pub struct PostingEntry {
    /// Reference to the source document
    pub doc_ref: DocRef,
    /// Term frequency in this document
    pub tf: u32,
    /// Document length in tokens
    pub doc_len: u32,
    /// Timestamp in microseconds (for recency scoring)
    pub ts_micros: Option<u64>,
}

impl PostingEntry {
    /// Create a new posting entry
    pub fn new(doc_ref: DocRef, tf: u32, doc_len: u32, ts_micros: Option<u64>) -> Self {
        PostingEntry {
            doc_ref,
            tf,
            doc_len,
            ts_micros,
        }
    }
}

// ============================================================================
// PostingList
// ============================================================================

/// List of documents containing a term
#[derive(Debug, Clone, Default)]
pub struct PostingList {
    /// Document entries
    pub entries: Vec<PostingEntry>,
}

impl PostingList {
    /// Create a new empty posting list
    pub fn new() -> Self {
        PostingList { entries: vec![] }
    }

    /// Add an entry to the posting list
    pub fn add(&mut self, entry: PostingEntry) {
        self.entries.push(entry);
    }

    /// Remove entries matching a DocRef
    pub fn remove(&mut self, doc_ref: &DocRef) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| &e.doc_ref != doc_ref);
        before - self.entries.len()
    }

    /// Number of documents containing this term
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if posting list is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ============================================================================
// InvertedIndex
// ============================================================================

/// Inverted index for fast keyword search
///
/// CRITICAL: This is OPTIONAL. Search works without it (via scan).
/// When disabled, all operations are NOOP (zero overhead).
///
/// # Thread Safety
///
/// Uses DashMap for concurrent access. Multiple readers/writers supported.
///
/// # Version Watermark
///
/// The version field tracks index state for consistency checking.
/// Incremented on every update operation.
pub struct InvertedIndex {
    /// Term -> PostingList mapping
    postings: DashMap<String, PostingList>,

    /// Term -> document frequency
    doc_freqs: DashMap<String, usize>,

    /// Total documents indexed
    total_docs: AtomicUsize,

    /// Whether index is enabled
    enabled: AtomicBool,

    /// Version watermark for consistency
    version: AtomicU64,

    /// Sum of all document lengths (for average calculation)
    total_doc_len: AtomicUsize,

    /// DocRef -> document length mapping for proper removal tracking
    /// Fixes #608 (total_doc_len drift) and #609 (double-counting)
    doc_lengths: DashMap<DocRef, u32>,
}

impl Default for InvertedIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl InvertedIndex {
    /// Create a new disabled index
    pub fn new() -> Self {
        InvertedIndex {
            postings: DashMap::new(),
            doc_freqs: DashMap::new(),
            total_docs: AtomicUsize::new(0),
            enabled: AtomicBool::new(false),
            version: AtomicU64::new(0),
            total_doc_len: AtomicUsize::new(0),
            doc_lengths: DashMap::new(),
        }
    }

    // ========================================================================
    // Enable/Disable
    // ========================================================================

    /// Check if index is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// Enable the index
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    /// Disable the index
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }

    /// Clear all index data
    ///
    /// Does NOT change enabled state.
    pub fn clear(&self) {
        self.postings.clear();
        self.doc_freqs.clear();
        self.doc_lengths.clear();
        self.total_docs.store(0, Ordering::Relaxed);
        self.total_doc_len.store(0, Ordering::Relaxed);
        self.version.fetch_add(1, Ordering::Release);
    }

    // ========================================================================
    // Version Watermark
    // ========================================================================

    /// Get current version
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Check if index is at least at given version
    pub fn is_at_version(&self, min_version: u64) -> bool {
        self.version.load(Ordering::Acquire) >= min_version
    }

    /// Wait for index to reach a version (with timeout)
    ///
    /// Returns true if version was reached, false on timeout.
    pub fn wait_for_version(&self, version: u64, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            if self.version.load(Ordering::Acquire) >= version {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::yield_now();
        }
    }

    // ========================================================================
    // Statistics
    // ========================================================================

    /// Get total number of indexed documents
    ///
    /// Uses Acquire ordering to ensure visibility of updates from other threads.
    pub fn total_docs(&self) -> usize {
        self.total_docs.load(Ordering::Acquire)
    }

    /// Get document frequency for a term
    pub fn doc_freq(&self, term: &str) -> usize {
        self.doc_freqs.get(term).map(|r| *r).unwrap_or(0)
    }

    /// Get average document length
    ///
    /// Uses Acquire ordering to ensure consistent visibility of both counters.
    pub fn avg_doc_len(&self) -> f32 {
        let total = self.total_docs.load(Ordering::Acquire);
        if total == 0 {
            return 0.0;
        }
        self.total_doc_len.load(Ordering::Acquire) as f32 / total as f32
    }

    /// Compute IDF for a term
    ///
    /// Uses standard IDF formula with smoothing:
    /// IDF(t) = ln((N - df + 0.5) / (df + 0.5) + 1)
    ///
    /// Uses Acquire ordering to ensure visibility of document count updates.
    pub fn compute_idf(&self, term: &str) -> f32 {
        let n = self.total_docs.load(Ordering::Acquire) as f32;
        let df = self.doc_freq(term) as f32;
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    // ========================================================================
    // Index Updates
    // ========================================================================

    /// Index a document
    ///
    /// NOOP if index is disabled.
    /// If document is already indexed, removes old version first (fixes #609).
    pub fn index_document(&self, doc_ref: &DocRef, text: &str, ts_micros: Option<u64>) {
        if !self.is_enabled() {
            return; // Zero overhead when disabled
        }

        // Fix #609: Check if document already indexed, remove first to prevent double-counting
        if self.doc_lengths.contains_key(doc_ref) {
            self.remove_document(doc_ref);
        }

        let tokens = tokenize(text);
        let doc_len = tokens.len() as u32;

        // Count term frequencies
        let mut tf_map: HashMap<String, u32> = HashMap::new();
        for token in &tokens {
            *tf_map.entry(token.clone()).or_insert(0) += 1;
        }

        // Update posting lists
        for (term, tf) in tf_map {
            let entry = PostingEntry::new(doc_ref.clone(), tf, doc_len, ts_micros);

            self.postings.entry(term.clone()).or_default().add(entry);

            self.doc_freqs
                .entry(term)
                .and_modify(|c| *c += 1)
                .or_insert(1);
        }

        // Track document length for proper removal (fixes #608)
        self.doc_lengths.insert(doc_ref.clone(), doc_len);

        self.total_docs.fetch_add(1, Ordering::Relaxed);
        self.total_doc_len
            .fetch_add(doc_len as usize, Ordering::Relaxed);
        self.version.fetch_add(1, Ordering::Release);
    }

    /// Remove a document from the index
    ///
    /// NOOP if index is disabled.
    /// Properly decrements total_doc_len using tracked document length (fixes #608).
    pub fn remove_document(&self, doc_ref: &DocRef) {
        if !self.is_enabled() {
            return;
        }

        // Fix #608: Get document length before removal for proper total_doc_len update
        let doc_len = self.doc_lengths.remove(doc_ref).map(|(_, len)| len);

        let mut removed = false;

        for mut entry in self.postings.iter_mut() {
            let count = entry.remove(doc_ref);
            if count > 0 {
                removed = true;
                let term = entry.key().clone();
                self.doc_freqs
                    .entry(term)
                    .and_modify(|c| *c = c.saturating_sub(count));
            }
        }

        if removed || doc_len.is_some() {
            self.total_docs.fetch_sub(1, Ordering::Relaxed);
            // Fix #608: Properly decrement total_doc_len using tracked length
            if let Some(len) = doc_len {
                self.total_doc_len
                    .fetch_sub(len as usize, Ordering::Relaxed);
            }
            self.version.fetch_add(1, Ordering::Release);
        }
    }

    // ========================================================================
    // Query
    // ========================================================================

    /// Lookup documents containing a term
    ///
    /// Returns None if term not found or index disabled.
    pub fn lookup(&self, term: &str) -> Option<PostingList> {
        if !self.is_enabled() {
            return None;
        }
        self.postings.get(term).map(|r| r.clone())
    }

    /// Get all terms in the index
    pub fn terms(&self) -> Vec<String> {
        self.postings.iter().map(|r| r.key().clone()).collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::RunId;

    fn test_doc_ref(name: &str) -> DocRef {
        let run_id = RunId::new();
        DocRef::Kv {
            run_id,
            key: name.to_string(),
        }
    }

    #[test]
    fn test_index_disabled_by_default() {
        let index = InvertedIndex::new();
        assert!(!index.is_enabled());
    }

    #[test]
    fn test_index_enable_disable() {
        let index = InvertedIndex::new();

        index.enable();
        assert!(index.is_enabled());

        index.disable();
        assert!(!index.is_enabled());
    }

    #[test]
    fn test_index_noop_when_disabled() {
        let index = InvertedIndex::new();
        let doc_ref = test_doc_ref("test");

        // Should be NOOP when disabled
        index.index_document(&doc_ref, "hello world", None);

        assert_eq!(index.total_docs(), 0);
        assert!(index.lookup("hello").is_none());
    }

    #[test]
    fn test_index_document_when_enabled() {
        let index = InvertedIndex::new();
        index.enable();

        let doc_ref = test_doc_ref("test");
        index.index_document(&doc_ref, "hello world test", None);

        assert_eq!(index.total_docs(), 1);
        assert_eq!(index.doc_freq("hello"), 1);
        assert_eq!(index.doc_freq("world"), 1);
        assert_eq!(index.doc_freq("test"), 1);

        let postings = index.lookup("hello").unwrap();
        assert_eq!(postings.len(), 1);
        assert_eq!(postings.entries[0].tf, 1);
        assert_eq!(postings.entries[0].doc_len, 3);
    }

    #[test]
    fn test_index_multiple_documents() {
        let index = InvertedIndex::new();
        index.enable();

        let doc1 = test_doc_ref("doc1");
        let doc2 = test_doc_ref("doc2");

        index.index_document(&doc1, "hello world", None);
        index.index_document(&doc2, "hello there", None);

        assert_eq!(index.total_docs(), 2);
        assert_eq!(index.doc_freq("hello"), 2); // In both docs
        assert_eq!(index.doc_freq("world"), 1); // Only in doc1
        assert_eq!(index.doc_freq("there"), 1); // Only in doc2

        let postings = index.lookup("hello").unwrap();
        assert_eq!(postings.len(), 2);
    }

    #[test]
    fn test_index_term_frequency() {
        let index = InvertedIndex::new();
        index.enable();

        let doc_ref = test_doc_ref("test");
        index.index_document(&doc_ref, "hello hello hello world", None);

        let postings = index.lookup("hello").unwrap();
        assert_eq!(postings.entries[0].tf, 3); // "hello" appears 3 times

        let postings = index.lookup("world").unwrap();
        assert_eq!(postings.entries[0].tf, 1);
    }

    #[test]
    fn test_remove_document() {
        let index = InvertedIndex::new();
        index.enable();

        let doc1 = test_doc_ref("doc1");
        let doc2 = test_doc_ref("doc2");

        index.index_document(&doc1, "hello world", None);
        index.index_document(&doc2, "hello there", None);

        assert_eq!(index.total_docs(), 2);

        index.remove_document(&doc1);

        assert_eq!(index.total_docs(), 1);
        assert_eq!(index.doc_freq("hello"), 1);
        assert_eq!(index.doc_freq("world"), 0);
    }

    #[test]
    fn test_clear() {
        let index = InvertedIndex::new();
        index.enable();

        let doc_ref = test_doc_ref("test");
        index.index_document(&doc_ref, "hello world", None);

        let v1 = index.version();
        index.clear();
        let v2 = index.version();

        assert_eq!(index.total_docs(), 0);
        assert!(index.lookup("hello").is_none());
        assert!(v2 > v1); // Version incremented
    }

    #[test]
    fn test_version_increment() {
        let index = InvertedIndex::new();
        index.enable();

        let v0 = index.version();

        let doc_ref = test_doc_ref("test");
        index.index_document(&doc_ref, "hello", None);
        let v1 = index.version();

        index.remove_document(&doc_ref);
        let v2 = index.version();

        assert!(v1 > v0);
        assert!(v2 > v1);
    }

    #[test]
    fn test_compute_idf() {
        let index = InvertedIndex::new();
        index.enable();

        // Add 10 documents, "common" in all, "rare" in 1
        for i in 0..10 {
            let doc_ref = test_doc_ref(&format!("doc{}", i));
            if i == 0 {
                index.index_document(&doc_ref, "common rare", None);
            } else {
                index.index_document(&doc_ref, "common", None);
            }
        }

        let idf_common = index.compute_idf("common");
        let idf_rare = index.compute_idf("rare");

        // Rare terms should have higher IDF
        assert!(idf_rare > idf_common);
    }

    #[test]
    fn test_avg_doc_len() {
        let index = InvertedIndex::new();
        index.enable();

        let doc1 = test_doc_ref("doc1");
        let doc2 = test_doc_ref("doc2");

        index.index_document(&doc1, "one two", None); // 2 tokens
        index.index_document(&doc2, "one two three four", None); // 4 tokens

        // Average: (2 + 4) / 2 = 3.0
        assert!((index.avg_doc_len() - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_wait_for_version() {
        use std::thread;

        let index = std::sync::Arc::new(InvertedIndex::new());
        index.enable();

        let index_clone = index.clone();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            let doc_ref = test_doc_ref("test");
            index_clone.index_document(&doc_ref, "hello", None);
        });

        // Wait for version to increment
        let result = index.wait_for_version(1, Duration::from_secs(1));
        handle.join().unwrap();

        assert!(result);
        assert!(index.version() >= 1);
    }

    #[test]
    fn test_wait_for_version_timeout() {
        let index = InvertedIndex::new();

        // Version is 0, waiting for 100 should timeout
        let result = index.wait_for_version(100, Duration::from_millis(10));
        assert!(!result);
    }

    #[test]
    fn test_posting_list() {
        let mut list = PostingList::new();
        assert!(list.is_empty());

        let doc_ref = test_doc_ref("test");
        list.add(PostingEntry::new(doc_ref.clone(), 1, 10, None));

        assert_eq!(list.len(), 1);
        assert!(!list.is_empty());

        let removed = list.remove(&doc_ref);
        assert_eq!(removed, 1);
        assert!(list.is_empty());
    }
}
