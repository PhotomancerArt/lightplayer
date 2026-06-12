use alloc::string::String;
use alloc::vec::Vec;

use crate::HwAddress;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HwLeaseId(u64);

impl HwLeaseId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareLease {
    id: HwLeaseId,
    claimant: String,
    addresses: Vec<HwAddress>,
}

impl HardwareLease {
    pub fn new(
        id: HwLeaseId,
        claimant: impl Into<String>,
        addresses: impl Into<Vec<HwAddress>>,
    ) -> Self {
        Self {
            id,
            claimant: claimant.into(),
            addresses: addresses.into(),
        }
    }

    pub fn id(&self) -> HwLeaseId {
        self.id
    }

    pub fn claimant(&self) -> &str {
        &self.claimant
    }

    pub fn addresses(&self) -> &[HwAddress] {
        &self.addresses
    }
}
