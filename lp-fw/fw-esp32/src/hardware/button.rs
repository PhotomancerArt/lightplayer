extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;

use esp_hal::gpio::{AnyPin, Input, InputConfig, Pull};
use lpc_hardware::{
    ButtonConfig, ButtonDebouncer, ButtonDriver, ButtonEvent, ButtonInput, HardwareEndpointError,
    HardwareLease, HwAddress, HwCapability, HwClaim, HwDriver, HwEndpoint, HwEndpointId,
    HwEndpointKind, HwError, HwRegistry,
};
use lpc_model::HwEndpointSpec;

const DRIVER_ID: &str = "esp32-gpio-button";
const DISPLAY_LABEL: &str = "ESP32 GPIO Button";
const MAX_ESP32C6_GPIO: u8 = 30;

pub struct Esp32GpioButtonDriver {
    registry: Rc<HwRegistry>,
}

impl Esp32GpioButtonDriver {
    pub fn new(registry: Rc<HwRegistry>) -> Self {
        Self { registry }
    }

    fn endpoint_id(address: &HwAddress) -> HwEndpointId {
        HwEndpointId::for_driver_address(DRIVER_ID, address)
    }

    fn gpio_for_endpoint(
        &self,
        endpoint_id: &HwEndpointId,
    ) -> Result<HwAddress, HardwareEndpointError> {
        for endpoint in self.endpoints() {
            if endpoint.id() == endpoint_id {
                return Ok(endpoint.address().clone());
            }
        }

        Err(HardwareEndpointError::UnknownEndpoint {
            kind: HwEndpointKind::Button,
            endpoint_id: endpoint_id.clone(),
        })
    }
}

impl HwDriver for Esp32GpioButtonDriver {
    fn driver_id(&self) -> &str {
        DRIVER_ID
    }

    fn display_label(&self) -> &str {
        DISPLAY_LABEL
    }
}

impl ButtonDriver for Esp32GpioButtonDriver {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        let mut endpoints = Vec::new();
        for resource in self.registry.manifest().resources() {
            if !resource.supports(HwCapability::GpioInput) {
                continue;
            }
            if !has_board_assigned_label(resource.address(), resource.display_label()) {
                continue;
            }
            let address = resource.address().clone();
            let spec = button_gpio_spec(resource.display_label());
            endpoints.push(HwEndpoint::new(
                Self::endpoint_id(&address),
                spec,
                HwEndpointKind::Button,
                DRIVER_ID,
                address,
                resource.display_label(),
                self.registry.endpoint_status_for(resource.address()),
            ));
        }
        endpoints
    }

    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError> {
        let source = self.gpio_for_endpoint(endpoint_id)?;
        let gpio = gpio_number(&source)?;
        self.registry
            .ensure_capability(&source, HwCapability::GpioInput)?;
        let lease = self
            .registry
            .claim_bundle(HwClaim::new(DRIVER_ID, vec![source.clone()]))?;

        let input = Input::new(
            // Board init drops the concrete HAL GPIO token after startup. The hardware registry
            // owns logical exclusivity, so the driver recreates the erased pin after claiming it.
            unsafe { AnyPin::steal(gpio) },
            InputConfig::default().with_pull(Pull::Up),
        );

        Ok(Box::new(Esp32ButtonInput::new(
            Rc::clone(&self.registry),
            source,
            lease,
            input,
            config,
        )))
    }
}

pub struct Esp32ButtonInput {
    registry: Rc<HwRegistry>,
    source: HwAddress,
    lease: Option<HardwareLease>,
    input: Option<Input<'static>>,
    debouncer: ButtonDebouncer,
}

impl Esp32ButtonInput {
    fn new(
        registry: Rc<HwRegistry>,
        source: HwAddress,
        lease: HardwareLease,
        input: Input<'static>,
        config: ButtonConfig,
    ) -> Self {
        Self {
            registry,
            source: source.clone(),
            lease: Some(lease),
            input: Some(input),
            debouncer: ButtonDebouncer::new(source, config.stable_ms()),
        }
    }

    pub fn close(&mut self) -> Result<(), HwError> {
        let release_result = if let Some(lease) = self.lease.take() {
            self.registry.release(&lease)
        } else {
            Ok(())
        };
        let _ = self.input.take();
        release_result
    }
}

impl ButtonInput for Esp32ButtonInput {
    fn source(&self) -> &HwAddress {
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

fn gpio_number(address: &HwAddress) -> Result<u8, HardwareEndpointError> {
    let Some(raw) = address.as_str().strip_prefix("/gpio/") else {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("button endpoint address is not a GPIO: {address}"),
        });
    };
    let gpio = raw
        .parse::<u8>()
        .map_err(|_| HardwareEndpointError::UnsupportedConfig {
            reason: format!("invalid ESP32 GPIO address: {address}"),
        })?;
    if gpio > MAX_ESP32C6_GPIO {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("ESP32-C6 GPIO {gpio} is outside the supported range"),
        });
    }
    Ok(gpio)
}

fn button_gpio_spec(config: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(alloc::format!("button:gpio:{config}"))
        .expect("manifest display label should form a valid endpoint spec")
}

fn has_board_assigned_label(address: &HwAddress, display_label: &str) -> bool {
    let Ok(gpio) = gpio_number(address) else {
        return false;
    };
    !display_label.eq_ignore_ascii_case(&format!("GPIO{gpio}"))
}
