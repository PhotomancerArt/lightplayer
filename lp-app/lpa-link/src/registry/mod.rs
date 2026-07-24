//! Provider catalog, connector factory, metadata, and built-in provider keys.
//!
//! The registry layer answers "which providers exist in this build?" and
//! hands out owned connectors on demand. `LinkProviderRegistry` keeps the
//! feature/target matrix in `lpa-link` as a catalog of `LinkProviderKind`s
//! (descriptors for picker UI) plus a factory (`create_connector`) that
//! constructs a provider from stored per-kind options and memoizes it, so
//! every flow for a kind shares ONE `LinkConnector` instance — providers
//! accumulate endpoint state (granted browser-serial ports, for example)
//! that later flows must still see.
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
