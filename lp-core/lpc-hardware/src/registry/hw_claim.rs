use alloc::string::String;
use alloc::vec::Vec;

use crate::HwAddress;

/// Request to reserve one or more hardware resources atomically.
///
/// Drivers construct claims before opening a device. The registry either turns
/// the whole claim into a [`crate::HardwareLease`] or rejects it without taking
/// any partial ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwClaim {
    claimant: String,
    addresses: Vec<HwAddress>,
}

impl HwClaim {
    pub fn new(claimant: impl Into<String>, addresses: impl Into<Vec<HwAddress>>) -> Self {
        Self {
            claimant: claimant.into(),
            addresses: addresses.into(),
        }
    }

    pub fn claimant(&self) -> &str {
        &self.claimant
    }

    pub fn addresses(&self) -> &[HwAddress] {
        &self.addresses
    }
}
