//! Board resources and the capabilities they support.
//!
//! A resource is a concrete thing on the board, identified by
//! [`HwAddress`](crate::HwAddress).
//! Resources are declared in a [`crate::HwManifest`], checked by
//! [`crate::HwRegistry`], and claimed by drivers before an endpoint is opened.

pub mod hw_address;
pub mod hw_capability;
pub mod hw_resource;
