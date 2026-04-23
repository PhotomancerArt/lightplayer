//! Unifying error type for **domain-level** operations that are not yet split
//! into per-artifact `thiserror` enums. Used today by the [`Node`](crate::node::Node)
//! trait; more variants appear as load and validation grow (M3+,
//! `docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md`).

use alloc::string::String;
use core::fmt;

/// Failure from [`Node::get_property`](crate::node::Node::get_property) or
/// [`Node::set_property`](crate::node::Node::set_property), and other cross-cutting domain checks.
#[derive(Clone, Debug, PartialEq)]
pub enum DomainError {
    /// No property at the given [`crate::types::PropPath`].
    UnknownProperty(String),
    /// A value with the wrong structural type for the target property. Carries
    /// simple expected/actual names for early diagnostics; richer paths land with real artifact types.
    PropertyTypeMismatch { expected: String, actual: String },
    /// Catch-all until the surface is refactored into finer errors.
    Other(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProperty(p) => write!(f, "unknown property: {p}"),
            Self::PropertyTypeMismatch { expected, actual } => {
                write!(
                    f,
                    "property type mismatch: expected {expected}, got {actual}"
                )
            }
            Self::Other(s) => f.write_str(s),
        }
    }
}

impl core::error::Error for DomainError {}
