//! Transaction configuration and retry logic
//!
//! Contains RetryConfig for transaction retry behavior and related utilities.

use std::time::Duration;

// ============================================================================
// Retry Configuration
// ============================================================================

/// Configuration for transaction retry behavior
///
/// Per spec Section 4.3: Implicit transactions include automatic retry on conflict.
/// This configuration controls the retry behavior for transactions.
///
/// # Example
/// ```ignore
/// let config = RetryConfig {
///     max_retries: 5,
///     base_delay_ms: 10,
///     max_delay_ms: 200,
/// };
/// db.transaction_with_retry(branch_id, config, |txn| { ... })?;
/// ```
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_retries: usize,
    /// Base delay between retries in milliseconds (exponential backoff)
    pub base_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 10,
            max_delay_ms: 100,
        }
    }
}

impl RetryConfig {
    /// Create a new RetryConfig with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a RetryConfig with no retries
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Set maximum number of retries
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set base delay for exponential backoff
    pub fn with_base_delay_ms(mut self, base_delay_ms: u64) -> Self {
        self.base_delay_ms = base_delay_ms;
        self
    }

    /// Set maximum delay between retries
    pub fn with_max_delay_ms(mut self, max_delay_ms: u64) -> Self {
        self.max_delay_ms = max_delay_ms;
        self
    }

    /// Calculate delay for a given attempt (exponential backoff)
    pub(crate) fn calculate_delay(&self, attempt: usize) -> Duration {
        // Cap the shift to prevent overflow (1 << 63 is the max for u64)
        let shift = attempt.min(63);
        let multiplier = 1u64 << shift;
        let delay_ms = self.base_delay_ms.saturating_mul(multiplier);
        Duration::from_millis(delay_ms.min(self.max_delay_ms))
    }
}
