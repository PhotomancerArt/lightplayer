use alloc::string::String;
use alloc::vec::Vec;

use super::HardwareAddress;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HardwareLeaseId(u64);

impl HardwareLeaseId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareLease {
    id: HardwareLeaseId,
    claimant: String,
    addresses: Vec<HardwareAddress>,
}

impl HardwareLease {
    pub fn new(
        id: HardwareLeaseId,
        claimant: impl Into<String>,
        addresses: impl Into<Vec<HardwareAddress>>,
    ) -> Self {
        Self {
            id,
            claimant: claimant.into(),
            addresses: addresses.into(),
        }
    }

    pub fn id(&self) -> HardwareLeaseId {
        self.id
    }

    pub fn claimant(&self) -> &str {
        &self.claimant
    }

    pub fn addresses(&self) -> &[HardwareAddress] {
        &self.addresses
    }
}
