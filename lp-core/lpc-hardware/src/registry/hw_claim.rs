use alloc::string::String;
use alloc::vec::Vec;

use crate::HwAddress;

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
