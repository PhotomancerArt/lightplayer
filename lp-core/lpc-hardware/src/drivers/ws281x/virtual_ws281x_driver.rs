use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::OutputError;

use crate::{
    HwAddress, HwCapability, HwClaim, HwDriver, HwEndpoint,
    HardwareEndpointError, HwEndpointId, HwEndpointKind, HwEndpointSpec,
    HwEndpointStatus, HardwareLease, HwRegistry, Ws281xConfig, Ws281xDriver,
    Ws281xOutput,
};

pub struct VirtualWs281xDriver {
    registry: Rc<HwRegistry>,
    driver_id: String,
    display_label: String,
    timing_address: HwAddress,
}

impl VirtualWs281xDriver {
    pub fn new(registry: Rc<HwRegistry>, rmt_channel: u8) -> Self {
        let timing_address = HwAddress::rmt_ws281x(rmt_channel);
        Self {
            registry,
            driver_id: format!("virtual-ws281x-rmt{rmt_channel}"),
            display_label: format!("Virtual WS281x RMT {rmt_channel}"),
            timing_address,
        }
    }

    fn endpoint_id(&self, spec: &HwEndpointSpec) -> HwEndpointId {
        HwEndpointId::for_driver_spec(self.driver_id(), spec)
    }

    fn endpoint_status(&self, gpio: &HwAddress) -> HwEndpointStatus {
        let gpio_status = self.registry.endpoint_status_for(gpio);
        if !gpio_status.is_available() {
            return gpio_status;
        }

        match self.registry.endpoint_status_for(&self.timing_address) {
            HwEndpointStatus::Available => HwEndpointStatus::Available,
            HwEndpointStatus::Reserved { reason } => HwEndpointStatus::Unavailable {
                reason: format!("WS281x timing resource is reserved: {reason}"),
            },
            HwEndpointStatus::InUse { claimant } => HwEndpointStatus::Unavailable {
                reason: format!("WS281x timing resource is in use by {claimant}"),
            },
            HwEndpointStatus::Unavailable { reason } => {
                HwEndpointStatus::Unavailable { reason }
            }
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
        let timing_supported = self
            .registry
            .ensure_capability(&self.timing_address, HwCapability::Rmt)
            .is_ok()
            && self
                .registry
                .ensure_capability(&self.timing_address, HwCapability::Ws281xOutput)
                .is_ok();
        if !timing_supported {
            return endpoints;
        }

        for resource in self.registry.manifest().resources() {
            if !resource.supports(HwCapability::GpioOutput) {
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
        validate_ws281x_byte_count(config.byte_count())?;
        let gpio = self.gpio_for_endpoint(endpoint_id)?;
        self.registry
            .ensure_capability(&gpio, HwCapability::GpioOutput)?;
        self.registry
            .ensure_capability(&self.timing_address, HwCapability::Rmt)?;
        self.registry
            .ensure_capability(&self.timing_address, HwCapability::Ws281xOutput)?;
        let lease = self.registry.claim_bundle(HwClaim::new(
            self.driver_id(),
            vec![gpio, self.timing_address.clone()],
        ))?;
        Ok(Box::new(VirtualWs281xOutput::new(
            Rc::clone(&self.registry),
            lease,
            config.byte_count(),
        )))
    }
}

pub struct VirtualWs281xOutput {
    registry: Rc<HwRegistry>,
    lease: Option<HardwareLease>,
    byte_count: u32,
    data: Vec<u16>,
}

impl VirtualWs281xOutput {
    fn new(registry: Rc<HwRegistry>, lease: HardwareLease, byte_count: u32) -> Self {
        let data_len = u16_len_for_byte_count(byte_count);
        Self {
            registry,
            lease: Some(lease),
            byte_count,
            data: vec![0; data_len],
        }
    }

    pub fn data(&self) -> &[u16] {
        &self.data
    }

    fn close(&mut self) {
        if let Some(lease) = self.lease.take() {
            let _ = self.registry.release(&lease);
        }
    }
}

impl Ws281xOutput for VirtualWs281xOutput {
    fn write(&mut self, data: &[u16]) -> Result<(), OutputError> {
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
        self.data.resize(u16_len_for_byte_count(self.byte_count), 0);
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

fn u16_len_for_byte_count(byte_count: u32) -> usize {
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

fn ws281x_rmt_spec(config: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(format!("ws281x:rmt:{config}"))
        .expect("manifest display label should form a valid endpoint spec")
}
