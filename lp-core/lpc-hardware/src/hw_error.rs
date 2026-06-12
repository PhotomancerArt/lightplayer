use alloc::string::String;
use core::fmt;

use crate::{HwAddress, HwCapability, HwLeaseId};

/// Resource-level hardware errors.
///
/// These errors come from address validation, manifest lookup, and registry
/// claim/release operations. Endpoint-opening code wraps them in
/// [`crate::HardwareEndpointError`] when appropriate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HwError {
    /// Address path is malformed.
    InvalidAddress { address: String },
    /// Address is not present in the manifest.
    UnknownResource { address: HwAddress },
    /// Resource is deliberately disabled in the manifest.
    ReservedResource { address: HwAddress, reason: String },
    /// Resource exists but does not advertise the requested capability.
    UnsupportedCapability {
        address: HwAddress,
        capability: HwCapability,
    },
    /// Resource is already held by another active lease.
    ResourceAlreadyClaimed {
        address: HwAddress,
        claimant: String,
    },
    /// One claim listed the same address more than once.
    DuplicateAddressInClaim { address: HwAddress },
    /// Claims must reserve at least one resource.
    EmptyClaim,
    /// Attempted to release a lease the registry no longer knows about.
    UnknownLease { lease_id: HwLeaseId },
}

impl fmt::Display for HwError {
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
