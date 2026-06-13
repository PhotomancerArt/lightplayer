//! ESP32 RMT-backed WS281x hardware driver.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use esp_hal::Blocking;
use esp_hal::gpio::AnyPin;
use esp_hal::rmt::Rmt;
use lpc_hardware::{
    HardwareEndpointError, HardwareLease, HwAddress, HwCapability, HwClaim, HwDriver, HwEndpoint,
    HwEndpointId, HwEndpointKind, HwEndpointSpec, HwEndpointStatus, HwRegistry, OutputError,
    Ws281xConfig, Ws281xDriver, Ws281xOutput,
};

use crate::output::{LedChannel, LedTransaction};

const DRIVER_ID: &str = "esp32-rmt-ws281x0";
const DISPLAY_LABEL: &str = "ESP32 RMT WS281x 0";
const MAX_LEDS: usize = 256;
const MAX_ESP32C6_GPIO: u8 = 30;

// Unsafe static to store the currently initialized LED channel.
// This is needed because LedChannel has lifetime constraints that do not fit the
// trait object owned by the root hardware system.
static mut LED_CHANNEL: Option<LedChannel<'static>> = None;
static mut LED_GPIO: Option<u32> = None;
static mut CURRENT_TRANSACTION: Option<LedTransaction<'static>> = None;

pub struct Esp32RmtWs281xDriver {
    registry: Rc<HwRegistry>,
    timing_address: HwAddress,
    rmt: Rc<RefCell<Option<Rmt<'static, Blocking>>>>,
}

impl Esp32RmtWs281xDriver {
    pub fn new(registry: Rc<HwRegistry>, rmt: Rmt<'static, Blocking>) -> Self {
        Self {
            registry,
            timing_address: HwAddress::rmt_ws281x(0),
            rmt: Rc::new(RefCell::new(Some(rmt))),
        }
    }

    fn endpoint_id(&self, spec: &HwEndpointSpec) -> HwEndpointId {
        HwEndpointId::for_driver_spec(self.driver_id(), spec)
    }

    fn endpoint_status(&self, gpio_address: &HwAddress) -> HwEndpointStatus {
        let gpio_status = self.registry.endpoint_status_for(gpio_address);
        if !gpio_status.is_available() {
            return gpio_status;
        }

        match self.registry.endpoint_status_for(&self.timing_address) {
            HwEndpointStatus::Available => {
                if rmt_channel_is_available_for(gpio_address) {
                    HwEndpointStatus::Available
                } else {
                    HwEndpointStatus::Unavailable {
                        reason: "RMT channel is already initialized for another GPIO".into(),
                    }
                }
            }
            HwEndpointStatus::Reserved { reason } => HwEndpointStatus::Unavailable {
                reason: format!("RMT timing resource is reserved: {reason}"),
            },
            HwEndpointStatus::InUse { claimant } => HwEndpointStatus::Unavailable {
                reason: format!("RMT timing resource is in use by {claimant}"),
            },
            HwEndpointStatus::Unavailable { reason } => HwEndpointStatus::Unavailable { reason },
        }
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
            kind: HwEndpointKind::Ws281x,
            endpoint_id: endpoint_id.clone(),
        })
    }

    fn ensure_rmt_initialized(
        &self,
        gpio_address: &HwAddress,
        num_leds: usize,
    ) -> Result<(), HardwareEndpointError> {
        let gpio = u32::from(gpio_number(gpio_address)?);
        unsafe {
            let channel_ptr = core::ptr::addr_of_mut!(LED_CHANNEL);
            let gpio_ptr = core::ptr::addr_of_mut!(LED_GPIO);
            if (*channel_ptr).is_some() {
                if (*gpio_ptr) == Some(gpio) {
                    log::info!(
                        "ensure_rmt_initialized: reusing existing RMT channel on GPIO{gpio} ({num_leds} LEDs)"
                    );
                    return Ok(());
                }
                log::warn!(
                    "ensure_rmt_initialized: RMT channel already bound to GPIO{:?}, cannot reinit on GPIO{gpio}",
                    *gpio_ptr
                );
                return Err(HardwareEndpointError::EndpointUnavailable {
                    endpoint_id: HwEndpointId::new(gpio_address.as_str()),
                    reason: "RMT channel is already initialized for another GPIO".into(),
                });
            }

            let Some(rmt) = self.rmt.borrow_mut().take() else {
                log::warn!(
                    "ensure_rmt_initialized: RMT peripheral already taken; cannot init GPIO{gpio}"
                );
                return Err(HardwareEndpointError::EndpointUnavailable {
                    endpoint_id: HwEndpointId::new(gpio_address.as_str()),
                    reason: "RMT peripheral is already in use".into(),
                });
            };
            log::info!(
                "ensure_rmt_initialized: initializing RMT WS281x channel on GPIO{gpio} ({num_leds} LEDs)"
            );
            // Board init drops the concrete HAL GPIO token after startup. The hardware registry
            // owns logical exclusivity, so the driver recreates the erased pin after claiming it.
            let pin = AnyPin::steal(gpio as u8);
            let channel = LedChannel::new(rmt, pin, num_leds).map_err(|error| {
                log::error!("ensure_rmt_initialized: LedChannel::new failed on GPIO{gpio}: {error:?}");
                HardwareEndpointError::Other {
                    message: format!("RMT channel init failed: {error:?}"),
                }
            })?;
            (*channel_ptr) = Some(core::mem::transmute(channel));
            (*gpio_ptr) = Some(gpio);
            log::info!("ensure_rmt_initialized: RMT WS281x channel ready on GPIO{gpio}");
        }
        Ok(())
    }
}

impl HwDriver for Esp32RmtWs281xDriver {
    fn driver_id(&self) -> &str {
        DRIVER_ID
    }

    fn display_label(&self) -> &str {
        DISPLAY_LABEL
    }
}

impl Ws281xDriver for Esp32RmtWs281xDriver {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        if self
            .registry
            .ensure_capability(&self.timing_address, HwCapability::Rmt)
            .is_err()
            || self
                .registry
                .ensure_capability(&self.timing_address, HwCapability::Ws281xOutput)
                .is_err()
        {
            return Vec::new();
        }

        let mut endpoints = Vec::new();
        for resource in self.registry.manifest().resources() {
            if !resource.supports(HwCapability::GpioOutput)
                || !has_board_assigned_label(resource.address(), resource.display_label())
            {
                continue;
            }
            let address = resource.address().clone();
            let spec = ws281x_rmt_spec(resource.display_label());
            endpoints.push(HwEndpoint::new(
                self.endpoint_id(&spec),
                spec,
                HwEndpointKind::Ws281x,
                self.driver_id(),
                address,
                resource.display_label(),
                self.endpoint_status(resource.address()),
            ));
        }
        endpoints
    }

    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
        let gpio_address = self.gpio_for_endpoint(endpoint_id)?;
        validate_byte_count(config.byte_count())?;
        log::info!(
            "Esp32RmtWs281xDriver::open: endpoint={endpoint_id}, gpio={}, byte_count={}",
            gpio_address.as_str(),
            config.byte_count()
        );

        let endpoint = self
            .endpoints()
            .into_iter()
            .find(|endpoint| endpoint.id() == endpoint_id)
            .ok_or_else(|| HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Ws281x,
                endpoint_id: endpoint_id.clone(),
            })?;
        if !endpoint.is_available() {
            return Err(HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: endpoint
                    .status()
                    .unavailable_reason()
                    .unwrap_or("endpoint unavailable")
                    .into(),
            });
        }

        self.registry
            .ensure_capability(&gpio_address, HwCapability::GpioOutput)?;
        self.registry
            .ensure_capability(&self.timing_address, HwCapability::Rmt)?;
        self.registry
            .ensure_capability(&self.timing_address, HwCapability::Ws281xOutput)?;
        let lease = self.registry.claim_bundle(HwClaim::new(
            self.driver_id(),
            vec![gpio_address.clone(), self.timing_address.clone()],
        ))?;

        if let Err(error) = self.ensure_rmt_initialized(
            &gpio_address,
            capped_byte_count(config.byte_count()) as usize / 3,
        ) {
            let _ = self.registry.release(&lease);
            return Err(error);
        }

        Ok(Box::new(Esp32RmtWs281xOutput {
            registry: Rc::clone(&self.registry),
            lease: Some(lease),
            byte_count: config.byte_count(),
        }))
    }
}

pub struct Esp32RmtWs281xOutput {
    registry: Rc<HwRegistry>,
    lease: Option<HardwareLease>,
    byte_count: u32,
}

impl Ws281xOutput for Esp32RmtWs281xOutput {
    fn write(&mut self, data: &[u8]) -> Result<(), OutputError> {
        let expected_len = byte_len_for_byte_count(self.byte_count);
        if data.len() != expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        transmit_rmt_buffer(data)
    }

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError> {
        validate_byte_count(config.byte_count()).map_err(endpoint_error_to_output_error)?;
        self.byte_count = capped_byte_count(config.byte_count());
        Ok(())
    }
}

impl Drop for Esp32RmtWs281xOutput {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.take() {
            if let Err(error) = self.registry.release(&lease) {
                log::warn!("Esp32RmtWs281xOutput: failed to release hardware lease: {error}");
            }
        }
    }
}

fn rmt_channel_is_available_for(gpio_address: &HwAddress) -> bool {
    let Ok(gpio) = gpio_number(gpio_address) else {
        return false;
    };
    unsafe {
        let channel_ptr = core::ptr::addr_of!(LED_CHANNEL);
        let gpio_ptr = core::ptr::addr_of!(LED_GPIO);
        (*channel_ptr).is_none() || (*gpio_ptr) == Some(u32::from(gpio))
    }
}

fn transmit_rmt_buffer(rmt_buffer: &[u8]) -> Result<(), OutputError> {
    unsafe {
        let tx_ptr = core::ptr::addr_of_mut!(CURRENT_TRANSACTION);
        let channel_ptr = core::ptr::addr_of_mut!(LED_CHANNEL);

        if let Some(tx) = (*tx_ptr).take() {
            log::debug!("Esp32RmtWs281xOutput::write: Waiting for previous transaction");
            let ch = tx.wait_complete();
            (*channel_ptr) = Some(ch);
        }

        if let Some(led_channel) = (*channel_ptr).take() {
            log::debug!(
                "Esp32RmtWs281xOutput::write: Starting transmission, {} bytes",
                rmt_buffer.len()
            );
            let tx = led_channel.start_transmission(rmt_buffer);
            log::debug!("Esp32RmtWs281xOutput::write: Waiting for transmission to complete");
            let led_channel = tx.wait_complete();
            (*channel_ptr) = Some(led_channel);
            log::debug!("Esp32RmtWs281xOutput::write: Transmission complete");
            Ok(())
        } else {
            log::error!("Esp32RmtWs281xOutput::write: RMT channel not initialized");
            Err(OutputError::InvalidConfig {
                reason: "RMT channel not initialized".into(),
            })
        }
    }
}

fn validate_byte_count(byte_count: u32) -> Result<(), HardwareEndpointError> {
    if byte_count < 3 {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: "WS281x byte_count must be at least 3".into(),
        });
    }
    Ok(())
}

fn capped_byte_count(byte_count: u32) -> u32 {
    let max_byte_count = (MAX_LEDS * 3) as u32;
    byte_count.min(max_byte_count)
}

fn byte_len_for_byte_count(byte_count: u32) -> usize {
    ((byte_count / 3) as usize) * 3
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}

fn gpio_number(address: &HwAddress) -> Result<u8, HardwareEndpointError> {
    let Some(raw) = address.as_str().strip_prefix("/gpio/") else {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("WS281x endpoint address is not a GPIO: {address}"),
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

fn has_board_assigned_label(address: &HwAddress, display_label: &str) -> bool {
    let Ok(gpio) = gpio_number(address) else {
        return false;
    };
    !display_label.eq_ignore_ascii_case(&format!("GPIO{gpio}"))
}

fn ws281x_rmt_spec(config: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(format!("ws281x:rmt:{config}"))
        .expect("manifest display label should form a valid endpoint spec")
}
