use alloc::string::String;
use alloc::vec::Vec;

use super::HardwareAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareClaim {
    claimant: String,
    addresses: Vec<HardwareAddress>,
}

impl HardwareClaim {
    pub fn new(claimant: impl Into<String>, addresses: impl Into<Vec<HardwareAddress>>) -> Self {
        Self {
            claimant: claimant.into(),
            addresses: addresses.into(),
        }
    }

    pub fn claimant(&self) -> &str {
        &self.claimant
    }

    pub fn addresses(&self) -> &[HardwareAddress] {
        &self.addresses
    }
}
