use alloc::string::{String, ToString};
use core::cell::{Cell, RefCell};
use lp_collection::{VecMap, VecSet};

use crate::{
    HardwareLease, HwAddress, HwCapability, HwClaim, HwEndpointStatus, HwError, HwLeaseId,
    HwManifest,
};

/// Live ownership registry for a hardware manifest.
///
/// The registry validates capabilities against the [`HwManifest`], tracks active
/// [`HardwareLease`]s, and reports endpoint status for drivers. It uses interior
/// mutability so shared driver handles can coordinate claims in `no_std` code.
///
/// # Generation
///
/// [`HwRegistry::generation`] is the registry's change signal: it changes
/// whenever a claim succeeds or a lease is released, which is exactly when an
/// endpoint that could not be opened might now open (or the reverse). Consumers
/// compare it for *inequality* only — that it currently counts upward is an
/// implementation detail, not a promise.
///
/// Only ownership needs signalling. Reserved status comes from the manifest,
/// which is immutable after construction, and capability support with it, so
/// neither can change under a holder of this registry.
#[derive(Debug)]
pub struct HwRegistry {
    manifest: HwManifest,
    state: RefCell<HwRegistryState>,
    generation: Cell<u64>,
}

#[derive(Debug, Clone)]
struct ActiveClaim {
    claimant: String,
}

#[derive(Debug, Clone)]
struct HwRegistryState {
    next_lease_id: u64,
    active_by_address: VecMap<HwAddress, ActiveClaim>,
    addresses_by_lease: VecMap<HwLeaseId, VecSet<HwAddress>>,
}

impl HwRegistry {
    pub fn new(manifest: HwManifest) -> Self {
        Self {
            manifest,
            state: RefCell::new(HwRegistryState {
                next_lease_id: 1,
                active_by_address: VecMap::new(),
                addresses_by_lease: VecMap::new(),
            }),
            // Starts nonzero so a consumer whose "last seen" value defaults to
            // zero sees an initial change rather than mistaking a fresh
            // registry for one it has already observed.
            generation: Cell::new(1),
        }
    }

    pub fn manifest(&self) -> &HwManifest {
        &self.manifest
    }

    /// Change signal for hardware ownership; see the type-level docs.
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// Bump after a claim or release actually changed ownership.
    ///
    /// Only successful mutations bump. A *failed* claim changes nothing
    /// observable, and signalling it would let one permanently-failing open
    /// retrigger every parked consumer on every attempt — the retry storm this
    /// signal exists to end.
    fn bump_generation(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
    }

    pub fn claim_bundle(&self, claim: HwClaim) -> Result<HardwareLease, HwError> {
        self.validate_claim(&claim)?;

        let mut state = self.state.borrow_mut();
        let lease_id = HwLeaseId::new(state.next_lease_id);
        state.next_lease_id += 1;

        let mut addresses = VecSet::new();
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
        drop(state);
        self.bump_generation();

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
        drop(state);
        self.bump_generation();
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
        let resource = self
            .manifest
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

        let mut seen = VecSet::new();
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
            .claim_bundle(HwClaim::new("output-a", vec![HwAddress::rmt_ws281x(0)]))
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
            [
                HwResource::new(HwAddress::gpio(12), [HwCapability::GpioOutput], "GPIO12")
                    .reserved("crashes during GPIO scan"),
            ],
        );
        let registry = HwRegistry::new(manifest);

        let result = registry.claim_bundle(HwClaim::new("output", vec![HwAddress::gpio(12)]));

        assert!(matches!(result, Err(HwError::ReservedResource { .. })));
    }

    /// Reads must not look like changes: the generation is what parks a
    /// consumer that could not open an endpoint, so a registry nobody has
    /// mutated must keep answering with the same value no matter how often it
    /// is interrogated.
    #[test]
    fn reads_do_not_change_the_generation() {
        let registry = registry();
        let before = registry.generation();

        registry.endpoint_status_for(&HwAddress::gpio(18));
        registry.endpoint_status_for(&HwAddress::gpio(999));
        registry.is_claimed(&HwAddress::gpio(18));
        registry.claimant_for(&HwAddress::gpio(18));
        registry
            .ensure_capability(&HwAddress::gpio(18), HwCapability::GpioOutput)
            .unwrap();

        assert_eq!(registry.generation(), before);
    }

    #[test]
    fn successful_claim_and_release_each_change_the_generation() {
        let registry = registry();
        let fresh = registry.generation();

        let lease = registry
            .claim_bundle(HwClaim::new("output", vec![HwAddress::gpio(18)]))
            .unwrap();
        let claimed = registry.generation();
        assert_ne!(claimed, fresh, "claim should signal a change");

        registry.release(&lease).unwrap();
        let released = registry.generation();
        assert_ne!(released, claimed, "release should signal a change");
        assert_ne!(released, fresh);
    }

    /// A failed claim changes nothing, so it must signal nothing. If it did,
    /// a permanently unopenable endpoint would unpark every other waiting
    /// consumer on each attempt and the per-frame retry storm would return.
    #[test]
    fn failed_claims_and_releases_leave_the_generation_alone() {
        let manifest = HwManifest::new(
            "board",
            "Board",
            [
                HwResource::new(HwAddress::gpio(18), [HwCapability::GpioOutput], "D6"),
                HwResource::new(HwAddress::gpio(12), [HwCapability::GpioOutput], "GPIO12")
                    .reserved("crashes during GPIO scan"),
            ],
        );
        let registry = HwRegistry::new(manifest);
        let held = registry
            .claim_bundle(HwClaim::new("holder", vec![HwAddress::gpio(18)]))
            .unwrap();
        let before = registry.generation();

        assert!(
            registry
                .claim_bundle(HwClaim::new("empty", vec![]))
                .is_err()
        );
        assert!(
            registry
                .claim_bundle(HwClaim::new("reserved", vec![HwAddress::gpio(12)]))
                .is_err()
        );
        assert!(
            registry
                .claim_bundle(HwClaim::new("taken", vec![HwAddress::gpio(18)]))
                .is_err()
        );
        assert!(
            registry
                .claim_bundle(HwClaim::new(
                    "dupe",
                    vec![HwAddress::gpio(12), HwAddress::gpio(12)],
                ))
                .is_err()
        );
        assert!(
            registry
                .claim_bundle(HwClaim::new("unknown", vec![HwAddress::gpio(200)]))
                .is_err()
        );

        assert_eq!(registry.generation(), before);

        // A lease released twice: the second call finds no lease and must not
        // signal either.
        registry.release(&held).unwrap();
        let after_release = registry.generation();
        assert!(registry.release(&held).is_err());
        assert_eq!(registry.generation(), after_release);
    }

    #[test]
    fn unsupported_capability_fails() {
        let registry = registry();

        let result = registry.ensure_capability(&HwAddress::gpio(18), HwCapability::Radio);

        assert!(matches!(result, Err(HwError::UnsupportedCapability { .. })));
    }

    fn registry() -> HwRegistry {
        HwRegistry::new(HwManifest::new(
            "board",
            "Board",
            [
                HwResource::new(
                    HwAddress::gpio(18),
                    [HwCapability::GpioOutput, HwCapability::GpioInput],
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
