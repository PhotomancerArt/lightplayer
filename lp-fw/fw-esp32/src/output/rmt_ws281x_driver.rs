//! ESP32 RMT-backed WS281x hardware driver.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use esp_hal::Blocking;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::rmt::{ConfigError as RmtConfigError, Rmt};
use lpc_hardware::{
    HwAddress, HwCapability, HwClaim, HwDriver, HwEndpoint,
    HardwareEndpointError, HwEndpointId, HwEndpointKind, HwEndpointSpec,
    HwEndpointStatus, HardwareLease, HwRegistry, Ws281xConfig, Ws281xDriver,
    Ws281xOutput,
};
use lpc_shared::output::OutputDriverOptions;
use lpc_shared::{DisplayPipeline, OutputError};

use crate::output::{LedChannel, LedTransaction};

const DRIVER_ID: &str = "esp32-rmt-ws281x0";
const DISPLAY_LABEL: &str = "ESP32 RMT WS281x 0";
const OUTPUT_GPIO: u32 = 18;
const MAX_LEDS: usize = 256;
const ENDPOINT_SPEC: &str = "ws281x:rmt:D10";

// Unsafe static to store the currently initialized GPIO18-backed LED channel.
// This is needed because LedChannel has lifetime constraints that do not fit the
// trait object owned by the root hardware system.
static mut LED_CHANNEL: Option<LedChannel<'static>> = None;
static mut CURRENT_TRANSACTION: Option<LedTransaction<'static>> = None;

pub struct Esp32RmtWs281xDriver {
    registry: Rc<HwRegistry>,
    gpio_address: HwAddress,
    timing_address: HwAddress,
}

impl Esp32RmtWs281xDriver {
    pub fn new(registry: Rc<HwRegistry>) -> Self {
        Self {
            registry,
            gpio_address: HwAddress::gpio(OUTPUT_GPIO),
            timing_address: HwAddress::rmt_ws281x(0),
        }
    }

    pub fn init_rmt<O>(
        rmt: Rmt<'static, Blocking>,
        pin: O,
        num_leds: usize,
    ) -> Result<(), RmtConfigError>
    where
        O: PeripheralOutput<'static>,
    {
        unsafe {
            let channel_ptr = core::ptr::addr_of_mut!(LED_CHANNEL);
            if (*channel_ptr).is_some() {
                return Ok(());
            }
            let channel = LedChannel::new(rmt, pin, num_leds)?;
            (*channel_ptr) = Some(core::mem::transmute(channel));
        }
        Ok(())
    }

    fn endpoint_id(&self) -> HwEndpointId {
        HwEndpointId::for_driver_spec(self.driver_id(), &endpoint_spec())
    }

    fn endpoint_status(&self) -> HwEndpointStatus {
        let gpio_status = self.registry.endpoint_status_for(&self.gpio_address);
        if !gpio_status.is_available() {
            return gpio_status;
        }

        match self.registry.endpoint_status_for(&self.timing_address) {
            HwEndpointStatus::Available => {
                if rmt_channel_is_initialized() {
                    HwEndpointStatus::Available
                } else {
                    HwEndpointStatus::Unavailable {
                        reason: "RMT channel is not initialized".into(),
                    }
                }
            }
            HwEndpointStatus::Reserved { reason } => HwEndpointStatus::Unavailable {
                reason: format!("RMT timing resource is reserved: {reason}"),
            },
            HwEndpointStatus::InUse { claimant } => HwEndpointStatus::Unavailable {
                reason: format!("RMT timing resource is in use by {claimant}"),
            },
            HwEndpointStatus::Unavailable { reason } => {
                HwEndpointStatus::Unavailable { reason }
            }
        }
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
        let Some(resource) = self.registry.manifest().resource(&self.gpio_address) else {
            return Vec::new();
        };
        if !resource.supports(HwCapability::GpioOutput)
            || self
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

        vec![HwEndpoint::new(
            self.endpoint_id(),
            endpoint_spec(),
            HwEndpointKind::Ws281x,
            self.driver_id(),
            self.gpio_address.clone(),
            resource.display_label(),
            self.endpoint_status(),
        )]
    }

    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
        if endpoint_id != &self.endpoint_id() {
            return Err(HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Ws281x,
                endpoint_id: endpoint_id.clone(),
            });
        }
        validate_byte_count(config.byte_count())?;

        let endpoint = self.endpoints().into_iter().next().ok_or_else(|| {
            HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Ws281x,
                endpoint_id: endpoint_id.clone(),
            }
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
            .ensure_capability(&self.gpio_address, HwCapability::GpioOutput)?;
        self.registry
            .ensure_capability(&self.timing_address, HwCapability::Rmt)?;
        self.registry
            .ensure_capability(&self.timing_address, HwCapability::Ws281xOutput)?;
        let lease = self.registry.claim_bundle(HwClaim::new(
            self.driver_id(),
            vec![self.gpio_address.clone(), self.timing_address.clone()],
        ))?;

        let options = config.display_options_cloned().unwrap_or_default();
        let pipeline =
            DisplayPipeline::new(config.byte_count() / 3, options.clone()).map_err(|error| {
                let _ = self.registry.release(&lease);
                HardwareEndpointError::Other {
                    message: format!("DisplayPipeline allocation failed: {error}"),
                }
            })?;

        Ok(Box::new(Esp32RmtWs281xOutput {
            registry: Rc::clone(&self.registry),
            lease: Some(lease),
            byte_count: config.byte_count(),
            pipeline,
            options,
        }))
    }
}

pub struct Esp32RmtWs281xOutput {
    registry: Rc<HwRegistry>,
    lease: Option<HardwareLease>,
    byte_count: u32,
    pipeline: DisplayPipeline,
    options: OutputDriverOptions,
}

impl Ws281xOutput for Esp32RmtWs281xOutput {
    fn write(&mut self, data: &[u16]) -> Result<(), OutputError> {
        let mut num_leds = (self.byte_count / 3) as usize;
        let expected_len = num_leds * 3;

        if data.len() > expected_len {
            let new_byte_count = capped_byte_count_for_len(data.len());
            self.resize(Ws281xConfig::new(
                new_byte_count,
                Some(self.options.clone()),
            ))?;
            num_leds = (self.byte_count / 3) as usize;
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        let mut rmt_buffer = Vec::with_capacity(num_leds * 3);
        rmt_buffer.resize(num_leds * 3, 0);

        self.pipeline.write_frame(0, data);
        self.pipeline.write_frame(16667, data);
        self.pipeline.tick(8333, &mut rmt_buffer);

        transmit_rmt_buffer(&rmt_buffer)
    }

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError> {
        validate_byte_count(config.byte_count()).map_err(endpoint_error_to_output_error)?;
        let byte_count = capped_byte_count(config.byte_count());
        let num_leds = byte_count / 3;
        if let Some(options) = config.display_options_cloned() {
            self.options = options;
        }
        self.pipeline.resize(num_leds);
        self.byte_count = byte_count;
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

fn rmt_channel_is_initialized() -> bool {
    unsafe {
        let channel_ptr = core::ptr::addr_of!(LED_CHANNEL);
        (*channel_ptr).is_some()
    }
}

fn endpoint_spec() -> HwEndpointSpec {
    HwEndpointSpec::from_static(ENDPOINT_SPEC)
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

fn capped_byte_count_for_len(data_len: usize) -> u32 {
    capped_byte_count(((data_len / 3) * 3) as u32)
}

fn capped_byte_count(byte_count: u32) -> u32 {
    let max_byte_count = (MAX_LEDS * 3) as u32;
    byte_count.min(max_byte_count)
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}
