use alloc::string::String;
use core::fmt;

use crate::{HardwareAddress, HardwareCapability, HardwareLeaseId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareError {
    InvalidAddress {
        address: String,
    },
    UnknownResource {
        address: HardwareAddress,
    },
    ReservedResource {
        address: HardwareAddress,
        reason: String,
    },
    UnsupportedCapability {
        address: HardwareAddress,
        capability: HardwareCapability,
    },
    ResourceAlreadyClaimed {
        address: HardwareAddress,
        claimant: String,
    },
    DuplicateAddressInClaim {
        address: HardwareAddress,
    },
    EmptyClaim,
    UnknownLease {
        lease_id: HardwareLeaseId,
    },
}

impl fmt::Display for HardwareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddress { address } => {
                write!(f, "invalid hardware address: {address}")
            }
            Self::UnknownResource { address } => {
                write!(f, "unknown hardware resource: {address}")
            }
            Self::ReservedResource { address, reason } => {
                write!(f, "hardware resource {address} is reserved: {reason}")
            }
            Self::UnsupportedCapability {
                address,
                capability,
            } => {
                write!(
                    f,
                    "hardware resource {address} does not support {capability:?}"
                )
            }
            Self::ResourceAlreadyClaimed { address, claimant } => {
                write!(
                    f,
                    "hardware resource {address} is already claimed by {claimant}"
                )
            }
            Self::DuplicateAddressInClaim { address } => {
                write!(f, "hardware resource {address} appears twice in one claim")
            }
            Self::EmptyClaim => f.write_str("hardware claim must include at least one resource"),
            Self::UnknownLease { lease_id } => {
                write!(f, "unknown hardware lease: {}", lease_id.as_u64())
            }
        }
    }
}
