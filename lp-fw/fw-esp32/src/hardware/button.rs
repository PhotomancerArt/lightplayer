extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use esp_hal::gpio::{Input, InputConfig, Pull};
use lpc_model::HardwareEndpointSpec;
use lpc_shared::hardware::{
    ButtonConfig, ButtonDebouncer, ButtonDriver, ButtonEvent, ButtonInput, HardwareAddress,
    HardwareCapability, HardwareClaim, HardwareDriver, HardwareEndpoint, HardwareEndpointError,
    HardwareEndpointId, HardwareEndpointKind, HardwareError, HardwareLease, HardwareRegistry,
};

const DRIVER_ID: &str = "esp32-gpio-button";
const GPIO20_SPEC: &str = "button:gpio:D9";

pub struct Esp32Gpio20ButtonDriver {
    registry: Rc<HardwareRegistry>,
    pin: RefCell<Option<esp_hal::peripherals::GPIO20<'static>>>,
}

impl Esp32Gpio20ButtonDriver {
    pub fn new(registry: Rc<HardwareRegistry>, pin: esp_hal::peripherals::GPIO20<'static>) -> Self {
        Self {
            registry,
            pin: RefCell::new(Some(pin)),
        }
    }

    fn source() -> HardwareAddress {
        HardwareAddress::gpio(20)
    }

    fn endpoint_id() -> HardwareEndpointId {
        HardwareEndpointId::for_driver_address(DRIVER_ID, &Self::source())
    }
}

impl HardwareDriver for Esp32Gpio20ButtonDriver {
    fn driver_id(&self) -> &str {
        DRIVER_ID
    }

    fn display_label(&self) -> &str {
        "ESP32 GPIO Button"
    }
}

impl ButtonDriver for Esp32Gpio20ButtonDriver {
    fn endpoints(&self) -> Vec<HardwareEndpoint> {
        let source = Self::source();
        vec![HardwareEndpoint::new(
            Self::endpoint_id(),
            HardwareEndpointSpec::from_static(GPIO20_SPEC),
            HardwareEndpointKind::Button,
            DRIVER_ID,
            source.clone(),
            "D9",
            self.registry.endpoint_status_for(&source),
        )]
    }

    fn open(
        &self,
        endpoint_id: &HardwareEndpointId,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError> {
        if endpoint_id != &Self::endpoint_id() {
            return Err(HardwareEndpointError::UnknownEndpoint {
                kind: HardwareEndpointKind::Button,
                endpoint_id: endpoint_id.clone(),
            });
        }

        let Some(pin) = self.pin.borrow_mut().take() else {
            return Err(HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: String::from("GPIO20 is already open"),
            });
        };

        Esp32ButtonInput::open_gpio20(Rc::clone(&self.registry), pin, config)
            .map(|input| Box::new(input) as Box<dyn ButtonInput>)
            .map_err(HardwareEndpointError::from)
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
    pub fn open_gpio20(
        registry: Rc<HardwareRegistry>,
        pin: esp_hal::peripherals::GPIO20<'static>,
        config: ButtonConfig,
    ) -> Result<Self, HardwareError> {
        let source = HardwareAddress::gpio(20);
        registry.ensure_capability(&source, HardwareCapability::GpioInput)?;
        let lease = registry.claim_bundle(HardwareClaim::new(DRIVER_ID, vec![source.clone()]))?;
        let input = Input::new(pin, InputConfig::default().with_pull(Pull::Up));
        Ok(Self {
            registry,
            source: source.clone(),
            lease: Some(lease),
            input,
            debouncer: ButtonDebouncer::new(source, config.stable_ms()),
        })
    }

    pub fn close(&mut self) -> Result<(), HardwareError> {
        if let Some(lease) = self.lease.take() {
            self.registry.release(&lease)?;
        }
        Ok(())
    }
}

impl ButtonInput for Esp32ButtonInput {
    fn source(&self) -> &HardwareAddress {
        &self.source
    }

    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        self.debouncer.sample(now_ms, self.input.is_low())
    }
}

impl Drop for Esp32ButtonInput {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
