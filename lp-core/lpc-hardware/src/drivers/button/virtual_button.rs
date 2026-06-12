use alloc::rc::Rc;
use alloc::vec;

use crate::{
    ButtonDebouncer, ButtonEvent, HwAddress, HwCapability, HwClaim,
    HwError, HardwareLease, HwRegistry,
};

pub struct VirtualButton {
    registry: Rc<HwRegistry>,
    source: HwAddress,
    lease: Option<HardwareLease>,
    debouncer: ButtonDebouncer,
}

impl VirtualButton {
    pub fn open_gpio(
        registry: Rc<HwRegistry>,
        pin: u32,
        stable_ms: u64,
    ) -> Result<Self, HwError> {
        let source = HwAddress::gpio(pin);
        registry.ensure_capability(&source, HwCapability::GpioInput)?;
        let lease =
            registry.claim_bundle(HwClaim::new("virtual-button", vec![source.clone()]))?;
        Ok(Self {
            registry,
            source: source.clone(),
            lease: Some(lease),
            debouncer: ButtonDebouncer::new(source, stable_ms),
        })
    }

    pub fn source(&self) -> &HwAddress {
        &self.source
    }

    pub fn sample(&mut self, now_ms: u64, pressed: bool) -> Option<ButtonEvent> {
        self.debouncer.sample(now_ms, pressed)
    }

    pub fn close(&mut self) -> Result<(), HwError> {
        if let Some(lease) = self.lease.take() {
            self.registry.release(&lease)?;
        }
        Ok(())
    }
}

impl Drop for VirtualButton {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HardwareEndpointError, HwEndpointSpec, HwManifest, HwResource,
        HardwareSystem, Ws281xConfig,
    };

    #[test]
    fn button_claim_blocks_output_on_same_gpio() {
        let registry = Rc::new(HwRegistry::new(test_manifest()));
        let _button = VirtualButton::open_gpio(Rc::clone(&registry), 4, 30).unwrap();
        let system = HardwareSystem::with_virtual_drivers(registry);
        let endpoint = endpoint("ws281x:rmt:GPIO4");

        let result = system.open_ws281x_by_spec(&endpoint, Ws281xConfig::new(3, None));

        assert!(matches!(
            result,
            Err(HardwareEndpointError::Hardware {
                error: HwError::ResourceAlreadyClaimed { .. }
            })
        ));
    }

    #[test]
    fn output_claim_blocks_button_on_same_gpio() {
        let registry = Rc::new(HwRegistry::new(test_manifest()));
        let system = HardwareSystem::with_virtual_drivers(Rc::clone(&registry));
        let _output = system
            .open_ws281x_by_spec(&endpoint("ws281x:rmt:GPIO4"), Ws281xConfig::new(3, None))
            .unwrap();

        let result = VirtualButton::open_gpio(registry, 4, 30);

        assert!(matches!(
            result,
            Err(HwError::ResourceAlreadyClaimed { .. })
        ));
    }

    #[test]
    fn output_and_button_can_use_different_resources() {
        let registry = Rc::new(HwRegistry::new(test_manifest()));
        let system = HardwareSystem::with_virtual_drivers(Rc::clone(&registry));
        let _output = system
            .open_ws281x_by_spec(&endpoint("ws281x:rmt:GPIO18"), Ws281xConfig::new(3, None))
            .unwrap();

        let button = VirtualButton::open_gpio(Rc::clone(&registry), 4, 30).unwrap();

        assert_eq!(button.source(), &HwAddress::gpio(4));
        assert!(registry.is_claimed(&HwAddress::gpio(18)));
        assert!(registry.is_claimed(&HwAddress::gpio(4)));
    }

    #[test]
    fn reserved_gpio_cannot_be_claimed_for_button() {
        let registry = Rc::new(HwRegistry::new(test_manifest()));

        let result = VirtualButton::open_gpio(registry, 12, 30);

        assert!(matches!(
            result,
            Err(HwError::ReservedResource { .. })
        ));
    }

    #[test]
    fn close_releases_button_gpio() {
        let registry = Rc::new(HwRegistry::new(test_manifest()));
        let mut button = VirtualButton::open_gpio(Rc::clone(&registry), 4, 30).unwrap();

        button.close().unwrap();

        assert!(!registry.is_claimed(&HwAddress::gpio(4)));
    }

    fn test_manifest() -> HwManifest {
        HwManifest::new(
            "test",
            "Test Board",
            [
                HwResource::new(
                    HwAddress::gpio(4),
                    [
                        HwCapability::GpioOutput,
                        HwCapability::GpioInput,
                    ],
                    "GPIO4",
                ),
                HwResource::new(
                    HwAddress::gpio(12),
                    [
                        HwCapability::GpioOutput,
                        HwCapability::GpioInput,
                    ],
                    "GPIO12",
                )
                .reserved("reserved for test"),
                HwResource::new(
                    HwAddress::gpio(18),
                    [
                        HwCapability::GpioOutput,
                        HwCapability::GpioInput,
                    ],
                    "GPIO18",
                ),
                HwResource::new(
                    HwAddress::rmt_ws281x(0),
                    [HwCapability::Rmt, HwCapability::Ws281xOutput],
                    "RMT WS281x 0",
                ),
            ],
        )
    }

    fn endpoint(spec: &'static str) -> HwEndpointSpec {
        HwEndpointSpec::from_static(spec)
    }
}
