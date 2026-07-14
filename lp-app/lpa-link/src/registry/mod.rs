//! Provider registry, metadata, and built-in provider keys.
//!
//! The registry layer answers "which providers exist in this build?" and owns
//! the compiled-in provider instances keyed by `LinkProviderKind`. It keeps the
//! feature/target matrix in `lpa-link`, so applications can enumerate providers
//! and construct the default registry without duplicating conditional logic.
//!
//! `LinkEnv` is the application-supplied construction input for resources that
//! cannot live inside the crate, such as browser asset paths or host serial
//! options. `LinkProviderInstance` is the enum-dispatched storage type used
//! because `LinkProvider` has async methods and is not object-safe.

pub mod descriptor;
pub mod env;
pub mod instance;
pub mod kind;
pub mod registry;
