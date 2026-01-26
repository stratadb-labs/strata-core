//! Error conversion from internal error types.
//!
//! This module provides conversions from internal Strata errors to
//! the executor's [`Error`] type.

use crate::Error;

// TODO: Implement From<strata_core::Error> for Error in Phase 3.2
//
// The conversion should:
// 1. Map each internal error variant to the appropriate executor Error variant
// 2. Preserve all error details (key names, version numbers, etc.)
// 3. Not lose any information in the conversion
//
// Example:
//
// impl From<strata_core::Error> for Error {
//     fn from(err: strata_core::Error) -> Self {
//         match err {
//             strata_core::Error::NotFound { key } => Error::KeyNotFound { key },
//             strata_core::Error::WrongType { expected, actual } => {
//                 Error::WrongType { expected, actual }
//             }
//             // ... other variants
//         }
//     }
// }
