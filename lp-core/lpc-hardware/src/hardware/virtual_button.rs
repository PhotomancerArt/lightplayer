use alloc::rc::Rc;
use alloc::vec;

use super::{
    ButtonDebouncer, ButtonEvent, HardwareAddress, HardwareCapability, HardwareClaim,
    HardwareError, HardwareLease, HardwareRegistry,
};

pub struct VirtualButton {
    registry: Rc<HardwareRegistry>,
    source: HardwareAddress,
    lease: Option<HardwareLease>,
    debouncer: ButtonDebouncer,
}

impl VirtualButton {
    pub fn open_gpio(
        registry: Rc<HardwareRegistry>,
        pin: u32,
        stable_ms: u64,
    ) -> Result<Self, HardwareError> {
        let source = HardwareAddress::gpio(pin);
        registry.ensure_capability(&source, HardwareCapability::GpioInput)?;
        let lease =
            registry.claim_bundle(HardwareClaim::new("virtual-button", vec![source.clone()]))?;
        Ok(Self {
            registry,
            source: source.clone(),
            lease: Some(lease),
            debouncer: ButtonDebouncer::new(source, stable_ms),
        })
    }

    pub fn source(&self) -> &HardwareAddress {
        &self.source
    }

    pub fn sample(&mut self, now_ms: u64, pressed: bool) -> Option<ButtonEvent> {
        self.debouncer.sample(now_ms, pressed)
    }

    pub fn close(&mut self) -> Result<(), HardwareError> {
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
    use crate::output::{MemoryOutputProvider, OutputFormat, OutputProvider};

    use super::*;
    use crate::hardware::{HardwareEndpointSpec, HardwareManifest, HardwareResource};

    #[test]
    fn button_claim_blocks_output_on_same_gpio() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let _button = VirtualButton::open_gpio(Rc::clone(&registry), 4, 30).unwrap();
        let output = MemoryOutputProvider::with_hardware_registry(registry);
        let endpoint = endpoint("ws281x:rmt:GPIO4");

        let result = output.open(&endpoint, 3, OutputFormat::Ws2811, None);

        assert!(matches!(
            result,
            Err(crate::OutputError::Hardware {
                error: HardwareError::ResourceAlreadyClaimed { .. }
            })
        ));
    }

    #[test]
    fn output_claim_blocks_button_on_same_gpio() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let output = MemoryOutputProvider::with_hardware_registry(Rc::clone(&registry));
        let handle = output
            .open(&endpoint("ws281x:rmt:GPIO4"), 3, OutputFormat::Ws2811, None)
            .unwrap();

        let result = VirtualButton::open_gpio(registry, 4, 30);

        assert!(matches!(
            result,
            Err(HardwareError::ResourceAlreadyClaimed { .. })
        ));
        output.close(handle).unwrap();
    }

    #[test]
    fn output_and_button_can_use_different_resources() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let output = MemoryOutputProvider::with_hardware_registry(Rc::clone(&registry));
        let handle = output
            .open(
                &endpoint("ws281x:rmt:GPIO18"),
                3,
                OutputFormat::Ws2811,
                None,
            )
            .unwrap();

        let button = VirtualButton::open_gpio(Rc::clone(&registry), 4, 30).unwrap();

        assert_eq!(button.source(), &HardwareAddress::gpio(4));
        assert!(registry.is_claimed(&HardwareAddress::gpio(18)));
        assert!(registry.is_claimed(&HardwareAddress::gpio(4)));
        output.close(handle).unwrap();
    }

    #[test]
    fn reserved_gpio_cannot_be_claimed_for_button() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));

        let result = VirtualButton::open_gpio(registry, 12, 30);

        assert!(matches!(
            result,
            Err(HardwareError::ReservedResource { .. })
        ));
    }

    #[test]
    fn close_releases_button_gpio() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let mut button = VirtualButton::open_gpio(Rc::clone(&registry), 4, 30).unwrap();

        button.close().unwrap();

        assert!(!registry.is_claimed(&HardwareAddress::gpio(4)));
    }

    fn test_manifest() -> HardwareManifest {
        HardwareManifest::new(
            "test",
            "Test Board",
            [
                HardwareResource::new(
                    HardwareAddress::gpio(4),
                    [
                        HardwareCapability::GpioOutput,
                        HardwareCapability::GpioInput,
                    ],
                    "GPIO4",
                ),
                HardwareResource::new(
                    HardwareAddress::gpio(12),
                    [
                        HardwareCapability::GpioOutput,
                        HardwareCapability::GpioInput,
                    ],
                    "GPIO12",
                )
                .reserved("reserved for test"),
                HardwareResource::new(
                    HardwareAddress::gpio(18),
                    [
                        HardwareCapability::GpioOutput,
                        HardwareCapability::GpioInput,
                    ],
                    "GPIO18",
                ),
                HardwareResource::new(
                    HardwareAddress::rmt_ws281x(0),
                    [HardwareCapability::Rmt, HardwareCapability::Ws281xOutput],
                    "RMT WS281x 0",
                ),
            ],
        )
    }

    fn endpoint(spec: &'static str) -> HardwareEndpointSpec {
        HardwareEndpointSpec::from_static(spec)
    }
}
