extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use esp_hal::gpio::{Input, InputConfig, Pull};
use lpc_hardware::{
    ButtonConfig, ButtonDebouncer, ButtonDriver, ButtonEvent, ButtonInput, HardwareAddress,
    HardwareCapability, HardwareClaim, HardwareDriver, HardwareEndpoint, HardwareEndpointError,
    HardwareEndpointId, HardwareEndpointKind, HardwareError, HardwareLease, HardwareRegistry,
};
use lpc_model::HardwareEndpointSpec;

const DRIVER_ID: &str = "esp32-gpio-button";
const GPIO20_SPEC: &str = "button:gpio:D9";

pub struct Esp32Gpio20ButtonDriver {
    registry: Rc<HardwareRegistry>,
    input: Rc<RefCell<Option<Input<'static>>>>,
}

impl Esp32Gpio20ButtonDriver {
    pub fn new(registry: Rc<HardwareRegistry>, pin: esp_hal::peripherals::GPIO20<'static>) -> Self {
        Self {
            registry,
            input: Rc::new(RefCell::new(Some(Input::new(
                pin,
                InputConfig::default().with_pull(Pull::Up),
            )))),
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

        let source = Self::source();
        self.registry
            .ensure_capability(&source, HardwareCapability::GpioInput)?;
        let lease = self
            .registry
            .claim_bundle(HardwareClaim::new(DRIVER_ID, vec![source.clone()]))?;

        let Some(input) = self.input.borrow_mut().take() else {
            self.registry.release(&lease)?;
            return Err(HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: String::from("GPIO20 is already open"),
            });
        };

        Ok(Box::new(Esp32ButtonInput::new_gpio20(
            Rc::clone(&self.registry),
            Rc::clone(&self.input),
            source,
            lease,
            input,
            config,
        )))
    }
}

pub struct Esp32ButtonInput {
    registry: Rc<HardwareRegistry>,
    source: HardwareAddress,
    input_home: Rc<RefCell<Option<Input<'static>>>>,
    lease: Option<HardwareLease>,
    input: Option<Input<'static>>,
    debouncer: ButtonDebouncer,
}

impl Esp32ButtonInput {
    fn new_gpio20(
        registry: Rc<HardwareRegistry>,
        input_home: Rc<RefCell<Option<Input<'static>>>>,
        source: HardwareAddress,
        lease: HardwareLease,
        input: Input<'static>,
        config: ButtonConfig,
    ) -> Self {
        Self {
            registry,
            source: source.clone(),
            input_home,
            lease: Some(lease),
            input: Some(input),
            debouncer: ButtonDebouncer::new(source, config.stable_ms()),
        }
    }

    pub fn close(&mut self) -> Result<(), HardwareError> {
        let release_result = if let Some(lease) = self.lease.take() {
            self.registry.release(&lease)
        } else {
            Ok(())
        };
        if let Some(input) = self.input.take() {
            *self.input_home.borrow_mut() = Some(input);
        }
        release_result
    }
}

impl ButtonInput for Esp32ButtonInput {
    fn source(&self) -> &HardwareAddress {
        &self.source
    }

    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        let input = self.input.as_mut()?;
        self.debouncer.sample(now_ms, input.is_low())
    }
}

impl Drop for Esp32ButtonInput {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
