//! Client-side property view over [`lpc_model::WireValue`] (no `lps-shared`).

mod wire_prop_access;

pub use wire_prop_access::{WirePropAccess, WirePropsMap};
