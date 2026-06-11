//! Overlay apply failures.

use alloc::string::String;

/// Error while applying an overlay mutation to the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    InventoryUnavailable { message: String },
}
