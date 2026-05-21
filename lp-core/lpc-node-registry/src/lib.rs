//! Node definition registry and artifact freshness store (parallel stack for M6 cutover).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;

mod change;
mod registry;
mod source;
mod view;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactReadFailure,
    ArtifactReadState, ArtifactStore,
};
