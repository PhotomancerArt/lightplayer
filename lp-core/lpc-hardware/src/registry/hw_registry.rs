use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use core::cell::RefCell;

use crate::{
    HwAddress, HwCapability, HwClaim, HwEndpointStatus, HwError,
    HardwareLease, HwLeaseId, HwManifest,
};

#[derive(Debug)]
pub struct HwRegistry {
    manifest: HwManifest,
    state: RefCell<HwRegistryState>,
}

#[derive(Debug, Clone)]
struct ActiveClaim {
    claimant: String,
}

#[derive(Debug, Clone)]
struct HwRegistryState {
    next_lease_id: u64,
    active_by_address: BTreeMap<HwAddress, ActiveClaim>,
    addresses_by_lease: BTreeMap<HwLeaseId, BTreeSet<HwAddress>>,
}

impl HwRegistry {
    pub fn new(manifest: HwManifest) -> Self {
        Self {
            manifest,
            state: RefCell::new(HwRegistryState {
                next_lease_id: 1,
                active_by_address: BTreeMap::new(),
                addresses_by_lease: BTreeMap::new(),
            }),
        }
    }

    pub fn manifest(&self) -> &HwManifest {
        &self.manifest
    }

    pub fn claim_bundle(&self, claim: HwClaim) -> Result<HardwareLease, HwError> {
        self.validate_claim(&claim)?;

        let mut state = self.state.borrow_mut();
        let lease_id = HwLeaseId::new(state.next_lease_id);
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

    pub fn release(&self, lease: &HardwareLease) -> Result<(), HwError> {
        let mut state = self.state.borrow_mut();
        let addresses =
            state
                .addresses_by_lease
                .remove(&lease.id())
                .ok_or(HwError::UnknownLease {
                    lease_id: lease.id(),
                })?;

        for address in addresses {
            state.active_by_address.remove(&address);
        }
        Ok(())
    }

    pub fn is_claimed(&self, address: &HwAddress) -> bool {
        self.state.borrow().active_by_address.contains_key(address)
    }

    pub fn claimant_for(&self, address: &HwAddress) -> Option<String> {
        self.state
            .borrow()
            .active_by_address
            .get(address)
            .map(|claim| claim.claimant.clone())
    }

    pub fn endpoint_status_for(&self, address: &HwAddress) -> HwEndpointStatus {
        match self.manifest.resource(address) {
            Some(resource) => {
                if let Some(reason) = resource.reserved_reason() {
                    HwEndpointStatus::Reserved {
                        reason: reason.into(),
                    }
                } else if let Some(claimant) = self.claimant_for(address) {
                    HwEndpointStatus::InUse { claimant }
                } else {
                    HwEndpointStatus::Available
                }
            }
            None => HwEndpointStatus::Unavailable {
                reason: alloc::format!("unknown hardware resource: {address}"),
            },
        }
    }

    pub fn ensure_capability(
        &self,
        address: &HwAddress,
        capability: HwCapability,
    ) -> Result<(), HwError> {
        let resource =
            self.manifest
                .resource(address)
                .ok_or_else(|| HwError::UnknownResource {
                    address: address.clone(),
                })?;
        if !resource.supports(capability) {
            return Err(HwError::UnsupportedCapability {
                address: address.clone(),
                capability,
            });
        }
        Ok(())
    }

    fn validate_claim(&self, claim: &HwClaim) -> Result<(), HwError> {
        if claim.addresses().is_empty() {
            return Err(HwError::EmptyClaim);
        }

        let mut seen = BTreeSet::new();
        let state = self.state.borrow();
        for address in claim.addresses() {
            if !seen.insert(address.clone()) {
                return Err(HwError::DuplicateAddressInClaim {
                    address: address.clone(),
                });
            }

            let resource =
                self.manifest
                    .resource(address)
                    .ok_or_else(|| HwError::UnknownResource {
                        address: address.clone(),
                    })?;
            if let Some(reason) = resource.reserved_reason() {
                return Err(HwError::ReservedResource {
                    address: address.clone(),
                    reason: reason.into(),
                });
            }

            if let Some(active) = state.active_by_address.get(address) {
                return Err(HwError::ResourceAlreadyClaimed {
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
    use crate::HwResource;
    use alloc::vec;

    #[test]
    fn claim_bundle_claims_and_releases_resources() {
        let registry = registry();
        let lease = registry
            .claim_bundle(HwClaim::new(
                "output",
                vec![HwAddress::gpio(18), HwAddress::rmt_ws281x(0)],
            ))
            .unwrap();

        assert!(registry.is_claimed(&HwAddress::gpio(18)));
        assert!(registry.is_claimed(&HwAddress::rmt_ws281x(0)));

        registry.release(&lease).unwrap();

        assert!(!registry.is_claimed(&HwAddress::gpio(18)));
        assert!(!registry.is_claimed(&HwAddress::rmt_ws281x(0)));
    }

    #[test]
    fn claim_bundle_is_atomic_when_later_resource_is_claimed() {
        let registry = registry();
        let rmt_lease = registry
            .claim_bundle(HwClaim::new(
                "output-a",
                vec![HwAddress::rmt_ws281x(0)],
            ))
            .unwrap();

        let result = registry.claim_bundle(HwClaim::new(
            "output-b",
            vec![HwAddress::gpio(18), HwAddress::rmt_ws281x(0)],
        ));

        assert!(matches!(
            result,
            Err(HwError::ResourceAlreadyClaimed { .. })
        ));
        assert!(!registry.is_claimed(&HwAddress::gpio(18)));
        assert!(registry.is_claimed(&HwAddress::rmt_ws281x(0)));

        registry.release(&rmt_lease).unwrap();
    }

    #[test]
    fn duplicate_address_in_claim_fails() {
        let registry = registry();
        let result = registry.claim_bundle(HwClaim::new(
            "output",
            vec![HwAddress::gpio(18), HwAddress::gpio(18)],
        ));

        assert!(matches!(
            result,
            Err(HwError::DuplicateAddressInClaim { .. })
        ));
    }

    #[test]
    fn reserved_resource_fails() {
        let manifest = HwManifest::new(
            "board",
            "Board",
            [HwResource::new(
                HwAddress::gpio(12),
                [HwCapability::GpioOutput],
                "GPIO12",
            )
            .reserved("crashes during GPIO scan")],
        );
        let registry = HwRegistry::new(manifest);

        let result = registry.claim_bundle(HwClaim::new(
            "output",
            vec![HwAddress::gpio(12)],
        ));

        assert!(matches!(
            result,
            Err(HwError::ReservedResource { .. })
        ));
    }

    #[test]
    fn unsupported_capability_fails() {
        let registry = registry();

        let result =
            registry.ensure_capability(&HwAddress::gpio(18), HwCapability::Radio);

        assert!(matches!(
            result,
            Err(HwError::UnsupportedCapability { .. })
        ));
    }

    fn registry() -> HwRegistry {
        HwRegistry::new(HwManifest::new(
            "board",
            "Board",
            [
                HwResource::new(
                    HwAddress::gpio(18),
                    [
                        HwCapability::GpioOutput,
                        HwCapability::GpioInput,
                    ],
                    "D6",
                ),
                HwResource::new(
                    HwAddress::rmt_ws281x(0),
                    [HwCapability::Rmt, HwCapability::Ws281xOutput],
                    "RMT0",
                ),
            ],
        ))
    }
}
