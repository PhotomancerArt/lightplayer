use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::OutputError;

use crate::{
    HardwareEndpointError, HardwareLease, HwAddress, HwCapability, HwClaim, HwDriver, HwEndpoint,
    HwEndpointId, HwEndpointKind, HwEndpointSpec, HwEndpointStatus, HwRegistry, Ws281xConfig,
    Ws281xDriver, Ws281xOutput,
};

/// Manifest-backed virtual WS281x driver for tests and emulation.
///
/// The driver exposes WS281x endpoints for GPIO output resources when the
/// manifest also declares at least one RMT/WS281x timing resource.
///
/// # Why the timing resource is chosen at `open`
///
/// Boards differ in how many WS281x channels they can drive at once — the
/// virtual single-RMT board declares `/rmt/ws281x0` alone, the XIAO S3 Plus
/// declares four — and the count is the manifest's to state, not this type's.
/// Pinning one address at construction made every open contend for
/// `/rmt/ws281x0`, so on a four-channel board the second output failed with
/// "already claimed" no matter what the manifest said. The channel is now
/// taken from the free list at open and released with the output, mirroring
/// `Esp32S3RmtWs281xDriver`, so host runs and silicon agree on how many
/// concurrent channels a board offers.
pub struct VirtualWs281xDriver {
    registry: Rc<HwRegistry>,
    driver_id: String,
    display_label: String,
    /// Every `/rmt/ws281xK` the manifest declares with WS281x timing
    /// capability, in manifest order. Empty on a board with no such resource,
    /// which offers no endpoints at all.
    timing_addresses: Vec<HwAddress>,
}

impl VirtualWs281xDriver {
    pub fn new(registry: Rc<HwRegistry>) -> Self {
        let timing_addresses = registry
            .manifest()
            .resources()
            .iter()
            .filter(|resource| {
                resource.supports(HwCapability::Rmt)
                    && resource.supports(HwCapability::Ws281xOutput)
            })
            .map(|resource| resource.address().clone())
            .collect();
        Self {
            registry,
            driver_id: String::from("virtual-ws281x-rmt"),
            display_label: String::from("Virtual WS281x RMT"),
            timing_addresses,
        }
    }

    fn endpoint_id(&self, spec: &HwEndpointSpec) -> HwEndpointId {
        HwEndpointId::for_driver_spec(self.driver_id(), spec)
    }

    /// The first declared timing resource the registry still reports as free.
    fn free_timing_address(&self) -> Option<&HwAddress> {
        self.timing_addresses
            .iter()
            .find(|address| self.registry.endpoint_status_for(address).is_available())
    }

    fn endpoint_status(&self, gpio: &HwAddress) -> HwEndpointStatus {
        let gpio_status = self.registry.endpoint_status_for(gpio);
        if !gpio_status.is_available() {
            return gpio_status;
        }

        if self.free_timing_address().is_some() {
            return HwEndpointStatus::Available;
        }

        // Nothing is free: report why the last-numbered channel is blocked
        // rather than a bare count, since with one declared channel that is
        // the whole story and with several it still names a live claimant.
        let Some(last) = self.timing_addresses.last() else {
            return HwEndpointStatus::Unavailable {
                reason: String::from("board declares no WS281x timing resource"),
            };
        };
        match self.registry.endpoint_status_for(last) {
            HwEndpointStatus::Available => HwEndpointStatus::Available,
            HwEndpointStatus::Reserved { reason } => HwEndpointStatus::Unavailable {
                reason: format!("WS281x timing resource is reserved: {reason}"),
            },
            HwEndpointStatus::InUse { claimant } => HwEndpointStatus::Unavailable {
                reason: format!("WS281x timing resource is in use by {claimant}"),
            },
            HwEndpointStatus::Unavailable { reason } => HwEndpointStatus::Unavailable { reason },
        }
    }

    /// The GPIO an endpoint id names, without building the endpoint list.
    ///
    /// Every entry in that list carries a freshly computed status — several
    /// registry lookups each — and none of it is wanted here: the id already
    /// determines the address. Walking the manifest directly answers the same
    /// question for the cost of a spec string per candidate.
    fn gpio_for_endpoint(
        &self,
        endpoint_id: &HwEndpointId,
    ) -> Result<HwAddress, HardwareEndpointError> {
        // A board with no timing resource offers no endpoints at all, so no id
        // can belong to this driver.
        if !self.timing_addresses.is_empty() {
            for resource in self.registry.manifest().resources() {
                if !resource.supports(HwCapability::GpioOutput) {
                    continue;
                }
                if self.endpoint_id(&ws281x_local_spec(resource.display_label())) == *endpoint_id {
                    return Ok(resource.address().clone());
                }
            }
        }

        Err(HardwareEndpointError::UnknownEndpoint {
            kind: HwEndpointKind::Ws281x,
            endpoint_id: endpoint_id.clone(),
        })
    }
}

impl HwDriver for VirtualWs281xDriver {
    fn driver_id(&self) -> &str {
        &self.driver_id
    }

    fn display_label(&self) -> &str {
        &self.display_label
    }
}

impl Ws281xDriver for VirtualWs281xDriver {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        let mut endpoints = Vec::new();
        if self.timing_addresses.is_empty() {
            return endpoints;
        }

        for resource in self.registry.manifest().resources() {
            if !resource.supports(HwCapability::GpioOutput) {
                continue;
            }
            let address = resource.address().clone();
            let spec = ws281x_local_spec(resource.display_label());
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
        validate_ws281x_byte_count(config.byte_count())?;
        let gpio = self.gpio_for_endpoint(endpoint_id)?;
        self.registry
            .ensure_capability(&gpio, HwCapability::GpioOutput)?;

        // Try each declared timing resource in turn rather than picking one up
        // front: `claim_bundle` is atomic, so a GPIO that is already in use
        // fails on every candidate and its error — which names the contended
        // address — is the one worth reporting.
        let mut last_error = None;
        for timing_address in &self.timing_addresses {
            let claim = HwClaim::new(self.driver_id(), vec![gpio.clone(), timing_address.clone()]);
            match self.registry.claim_bundle(claim) {
                Ok(lease) => {
                    return Ok(Box::new(VirtualWs281xOutput::new(
                        Rc::clone(&self.registry),
                        lease,
                        config.byte_count(),
                    )));
                }
                Err(error) => last_error = Some(error),
            }
        }

        Err(match last_error {
            Some(error) => HardwareEndpointError::from(error),
            None => HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: String::from("board declares no WS281x timing resource"),
            },
        })
    }
}

/// In-memory WS281x output used by [`VirtualWs281xDriver`].
///
/// It stores the most recent raw RGB byte frame and releases its hardware lease
/// when dropped.
pub struct VirtualWs281xOutput {
    registry: Rc<HwRegistry>,
    lease: Option<HardwareLease>,
    byte_count: u32,
    data: Vec<u8>,
}

impl VirtualWs281xOutput {
    fn new(registry: Rc<HwRegistry>, lease: HardwareLease, byte_count: u32) -> Self {
        let data_len = byte_len_for_byte_count(byte_count);
        Self {
            registry,
            lease: Some(lease),
            byte_count,
            data: vec![0; data_len],
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    fn close(&mut self) {
        if let Some(lease) = self.lease.take() {
            let _ = self.registry.release(&lease);
        }
    }
}

impl Ws281xOutput for VirtualWs281xOutput {
    fn write(&mut self, data: &[u8]) -> Result<(), OutputError> {
        let expected_len = self.data.len();
        if data.len() > expected_len {
            let new_len = (data.len() / 3) * 3;
            self.data.resize(new_len, 0);
            self.byte_count = new_len as u32;
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        let len = self.data.len();
        self.data.copy_from_slice(&data[..len]);
        Ok(())
    }

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError> {
        validate_ws281x_byte_count(config.byte_count()).map_err(endpoint_error_to_output_error)?;
        self.byte_count = config.byte_count();
        self.data
            .resize(byte_len_for_byte_count(self.byte_count), 0);
        Ok(())
    }
}

impl Drop for VirtualWs281xOutput {
    fn drop(&mut self) {
        self.close();
    }
}

fn validate_ws281x_byte_count(byte_count: u32) -> Result<(), HardwareEndpointError> {
    if byte_count < 3 {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: String::from("WS281x byte_count must be at least 3"),
        });
    }
    Ok(())
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

fn ws281x_local_spec(config: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(format!("ws281x:local:{config}"))
        .expect("manifest display label should form a valid endpoint spec")
}
