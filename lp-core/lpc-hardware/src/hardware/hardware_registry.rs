use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use core::cell::RefCell;

use super::{
    HardwareAddress, HardwareCapability, HardwareClaim, HardwareEndpointStatus, HardwareError,
    HardwareLease, HardwareLeaseId, HardwareManifest,
};

#[derive(Debug, Clone)]
struct ActiveClaim {
    claimant: String,
}

#[derive(Debug, Clone)]
struct HardwareRegistryState {
    next_lease_id: u64,
    active_by_address: BTreeMap<HardwareAddress, ActiveClaim>,
    addresses_by_lease: BTreeMap<HardwareLeaseId, BTreeSet<HardwareAddress>>,
}

#[derive(Debug)]
pub struct HardwareRegistry {
    manifest: HardwareManifest,
    state: RefCell<HardwareRegistryState>,
}

impl HardwareRegistry {
    pub fn new(manifest: HardwareManifest) -> Self {
        Self {
            manifest,
            state: RefCell::new(HardwareRegistryState {
                next_lease_id: 1,
                active_by_address: BTreeMap::new(),
                addresses_by_lease: BTreeMap::new(),
            }),
        }
    }

    pub fn manifest(&self) -> &HardwareManifest {
        &self.manifest
    }

    pub fn claim_bundle(&self, claim: HardwareClaim) -> Result<HardwareLease, HardwareError> {
        self.validate_claim(&claim)?;

        let mut state = self.state.borrow_mut();
        let lease_id = HardwareLeaseId::new(state.next_lease_id);
        state.next_lease_id += 1;

        let mut addresses = BTreeSet::new();
        for address in claim.addresses() {
            state.active_by_address.insert(
                address.clone(),
                ActiveClaim {
                    claimant: claim.claimant().to_string(),
                },
            );
            addresses.insert(address.clone());
        }
        state.addresses_by_lease.insert(lease_id, addresses);

        Ok(HardwareLease::new(
            lease_id,
            claim.claimant().to_string(),
            claim.addresses().to_vec(),
        ))
    }

    pub fn release(&self, lease: &HardwareLease) -> Result<(), HardwareError> {
        let mut state = self.state.borrow_mut();
        let addresses =
            state
                .addresses_by_lease
                .remove(&lease.id())
                .ok_or(HardwareError::UnknownLease {
                    lease_id: lease.id(),
                })?;

        for address in addresses {
            state.active_by_address.remove(&address);
        }
        Ok(())
    }

    pub fn is_claimed(&self, address: &HardwareAddress) -> bool {
        self.state.borrow().active_by_address.contains_key(address)
    }

    pub fn claimant_for(&self, address: &HardwareAddress) -> Option<String> {
        self.state
            .borrow()
            .active_by_address
            .get(address)
            .map(|claim| claim.claimant.clone())
    }

    pub fn endpoint_status_for(&self, address: &HardwareAddress) -> HardwareEndpointStatus {
        match self.manifest.resource(address) {
            Some(resource) => {
                if let Some(reason) = resource.reserved_reason() {
                    HardwareEndpointStatus::Reserved {
                        reason: reason.into(),
                    }
                } else if let Some(claimant) = self.claimant_for(address) {
                    HardwareEndpointStatus::InUse { claimant }
                } else {
                    HardwareEndpointStatus::Available
                }
            }
            None => HardwareEndpointStatus::Unavailable {
                reason: alloc::format!("unknown hardware resource: {address}"),
            },
        }
    }

    pub fn ensure_capability(
        &self,
        address: &HardwareAddress,
        capability: HardwareCapability,
    ) -> Result<(), HardwareError> {
        let resource =
            self.manifest
                .resource(address)
                .ok_or_else(|| HardwareError::UnknownResource {
                    address: address.clone(),
                })?;
        if !resource.supports(capability) {
            return Err(HardwareError::UnsupportedCapability {
                address: address.clone(),
                capability,
            });
        }
        Ok(())
    }

    fn validate_claim(&self, claim: &HardwareClaim) -> Result<(), HardwareError> {
        if claim.addresses().is_empty() {
            return Err(HardwareError::EmptyClaim);
        }

        let mut seen = BTreeSet::new();
        let state = self.state.borrow();
        for address in claim.addresses() {
            if !seen.insert(address.clone()) {
                return Err(HardwareError::DuplicateAddressInClaim {
                    address: address.clone(),
                });
            }

            let resource =
                self.manifest
                    .resource(address)
                    .ok_or_else(|| HardwareError::UnknownResource {
                        address: address.clone(),
                    })?;
            if let Some(reason) = resource.reserved_reason() {
                return Err(HardwareError::ReservedResource {
                    address: address.clone(),
                    reason: reason.into(),
                });
            }

            if let Some(active) = state.active_by_address.get(address) {
                return Err(HardwareError::ResourceAlreadyClaimed {
                    address: address.clone(),
                    claimant: active.claimant.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::HardwareResource;
    use alloc::vec;

    #[test]
    fn claim_bundle_claims_and_releases_resources() {
        let registry = registry();
        let lease = registry
            .claim_bundle(HardwareClaim::new(
                "output",
                vec![HardwareAddress::gpio(18), HardwareAddress::rmt_ws281x(0)],
            ))
            .unwrap();

        assert!(registry.is_claimed(&HardwareAddress::gpio(18)));
        assert!(registry.is_claimed(&HardwareAddress::rmt_ws281x(0)));

        registry.release(&lease).unwrap();

        assert!(!registry.is_claimed(&HardwareAddress::gpio(18)));
        assert!(!registry.is_claimed(&HardwareAddress::rmt_ws281x(0)));
    }

    #[test]
    fn claim_bundle_is_atomic_when_later_resource_is_claimed() {
        let registry = registry();
        let rmt_lease = registry
            .claim_bundle(HardwareClaim::new(
                "output-a",
                vec![HardwareAddress::rmt_ws281x(0)],
            ))
            .unwrap();

        let result = registry.claim_bundle(HardwareClaim::new(
            "output-b",
            vec![HardwareAddress::gpio(18), HardwareAddress::rmt_ws281x(0)],
        ));

        assert!(matches!(
            result,
            Err(HardwareError::ResourceAlreadyClaimed { .. })
        ));
        assert!(!registry.is_claimed(&HardwareAddress::gpio(18)));
        assert!(registry.is_claimed(&HardwareAddress::rmt_ws281x(0)));

        registry.release(&rmt_lease).unwrap();
    }

    #[test]
    fn duplicate_address_in_claim_fails() {
        let registry = registry();
        let result = registry.claim_bundle(HardwareClaim::new(
            "output",
            vec![HardwareAddress::gpio(18), HardwareAddress::gpio(18)],
        ));

        assert!(matches!(
            result,
            Err(HardwareError::DuplicateAddressInClaim { .. })
        ));
    }

    #[test]
    fn reserved_resource_fails() {
        let manifest = HardwareManifest::new(
            "board",
            "Board",
            [HardwareResource::new(
                HardwareAddress::gpio(12),
                [HardwareCapability::GpioOutput],
                "GPIO12",
            )
            .reserved("crashes during GPIO scan")],
        );
        let registry = HardwareRegistry::new(manifest);

        let result = registry.claim_bundle(HardwareClaim::new(
            "output",
            vec![HardwareAddress::gpio(12)],
        ));

        assert!(matches!(
            result,
            Err(HardwareError::ReservedResource { .. })
        ));
    }

    #[test]
    fn unsupported_capability_fails() {
        let registry = registry();

        let result =
            registry.ensure_capability(&HardwareAddress::gpio(18), HardwareCapability::Radio);

        assert!(matches!(
            result,
            Err(HardwareError::UnsupportedCapability { .. })
        ));
    }

    fn registry() -> HardwareRegistry {
        HardwareRegistry::new(HardwareManifest::new(
            "board",
            "Board",
            [
                HardwareResource::new(
                    HardwareAddress::gpio(18),
                    [
                        HardwareCapability::GpioOutput,
                        HardwareCapability::GpioInput,
                    ],
                    "D6",
                ),
                HardwareResource::new(
                    HardwareAddress::rmt_ws281x(0),
                    [HardwareCapability::Rmt, HardwareCapability::Ws281xOutput],
                    "RMT0",
                ),
            ],
        ))
    }
}
