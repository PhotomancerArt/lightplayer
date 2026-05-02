//! Compatibility snapshot state for legacy wire and view clients during M4.
//!
//! M4 keeps compatibility through snapshot-style state where needed. M4.1
//! replaces this with buffer- and render-product-aware sync, refs, and client
//! cache behavior.

/// Placeholder for projected compatibility/wire state until M4.1 buffer sync.
///
/// Constructed with [`Self::new`]; behavior is added in later M4 work.
#[derive(Debug, Default)]
pub struct CompatibilityProjection;

impl CompatibilityProjection {
    pub fn new() -> Self {
        Self
    }
}
