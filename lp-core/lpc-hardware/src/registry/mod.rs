//! Runtime ownership for hardware resources.
//!
//! Drivers submit a [`HwClaim`](hw_claim::HwClaim) for one or more addresses.
//! The [`HwRegistry`](hw_registry::HwRegistry) validates the claim against the
//! manifest and returns a [`HardwareLease`](hw_lease::HardwareLease) that keeps
//! those resources reserved until it is released or the owning handle is dropped.

pub mod hw_claim;
pub mod hw_lease;
pub mod hw_registry;
