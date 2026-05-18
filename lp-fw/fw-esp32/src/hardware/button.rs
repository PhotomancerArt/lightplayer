extern crate alloc;

use alloc::rc::Rc;
use alloc::vec;

use esp_hal::gpio::{Input, InputConfig, Pull};
use lpc_shared::hardware::{
    ButtonDebouncer, ButtonEvent, HardwareAddress, HardwareCapability, HardwareClaim,
    HardwareError, HardwareLease, HardwareRegistry,
};

pub struct ButtonConfig {
    stable_ms: u64,
}

impl ButtonConfig {
    pub fn new(stable_ms: u64) -> Self {
        Self { stable_ms }
    }

    pub fn stable_ms(&self) -> u64 {
        self.stable_ms
    }
}

impl Default for ButtonConfig {
    fn default() -> Self {
        Self::new(ButtonDebouncer::DEFAULT_STABLE_MS)
    }
}

pub struct Esp32ButtonInput {
    registry: Rc<HardwareRegistry>,
    source: HardwareAddress,
    lease: Option<HardwareLease>,
    input: Input<'static>,
    debouncer: ButtonDebouncer,
}

impl Esp32ButtonInput {
    pub fn open_gpio4(
        registry: Rc<HardwareRegistry>,
        pin: esp_hal::peripherals::GPIO4<'static>,
        config: ButtonConfig,
    ) -> Result<Self, HardwareError> {
        let source = HardwareAddress::gpio(4);
        registry.ensure_capability(&source, HardwareCapability::GpioInput)?;
        let lease =
            registry.claim_bundle(HardwareClaim::new("esp32-button", vec![source.clone()]))?;
        let input = Input::new(pin, InputConfig::default().with_pull(Pull::Up));
        Ok(Self {
            registry,
            source: source.clone(),
            lease: Some(lease),
            input,
            debouncer: ButtonDebouncer::new(source, config.stable_ms()),
        })
    }

    pub fn source(&self) -> &HardwareAddress {
        &self.source
    }

    pub fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        self.debouncer.sample(now_ms, self.input.is_low())
    }

    pub fn close(&mut self) -> Result<(), HardwareError> {
        if let Some(lease) = self.lease.take() {
            self.registry.release(&lease)?;
        }
        Ok(())
    }
}

impl Drop for Esp32ButtonInput {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
