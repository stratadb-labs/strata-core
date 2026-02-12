//! Shared inference runtime foundation.
//!
//! Provides tensor operations, weight loading, and compute backend abstraction
//! used by all inference backends.

pub mod backend;
pub mod cpu_backend;
pub mod safetensors;
pub mod tensor;

#[cfg(feature = "embed")]
pub mod dl;

#[cfg(feature = "embed-cuda")]
pub mod cuda;

#[cfg(all(feature = "embed-metal", target_os = "macos"))]
pub mod metal;
