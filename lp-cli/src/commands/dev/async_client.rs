//! Re-export async client from client module
//!
//! This module re-exports AsyncLpClient and related functions from the client module
//! for backwards compatibility with existing code.

// Re-exports for backwards compatibility (may be used by external code)
#[allow(unused_imports, reason = "Backwards compatibility re-exports")]
pub use crate::client::{LpClient, serializable_response_to_project_response};
