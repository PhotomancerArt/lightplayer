//! Provider catalog, connector factory, metadata, and built-in provider keys.
//!
//! The registry layer answers "which providers exist in this build?" and
//! builds owned connectors on demand. `LinkProviderRegistry` keeps the
//! feature/target matrix in `lpa-link` as a catalog of `LinkProviderKind`s
//! (descriptors for picker UI) plus a factory (`create_connector`) that
//! constructs a provider from stored per-kind options; live provider state
//! belongs to the created `LinkConnector`, owned by the connection owner.
//!
//! `LinkEnv` is the application-supplied construction input for resources that
//! cannot live inside the crate, such as browser asset paths or host serial
//! options. `LinkConnector` is the enum-dispatched owned handle used because
//! `LinkProvider` has async methods and is not object-safe.

pub mod connector;
pub mod descriptor;
pub mod env;
pub mod kind;
pub mod registry;
