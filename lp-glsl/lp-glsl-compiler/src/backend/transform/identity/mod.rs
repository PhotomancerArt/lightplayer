//! Identity transform - copies functions exactly without modification
//!
//! This transform is useful for:
//! - Testing the transform framework
//! - Validating that copying preserves all function structure
//! - As a base for other transforms

mod transform;

pub use transform::IdentityTransform;
