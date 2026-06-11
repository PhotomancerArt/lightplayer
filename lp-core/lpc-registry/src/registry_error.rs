//! Errors returned by project registry helpers.

use alloc::string::String;

/// Shared registry error for artifact/reference resolution during rebuild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    InvalidPath { message: String },
    SpecifierResolution { message: String },
}
