//! Persistence-level artifact changes.

use crate::{ArtifactLocation, ChangeSummary};

/// Artifact writes/deletes performed against durable storage.
pub type ArtifactChangeSummary = ChangeSummary<ArtifactLocation>;
