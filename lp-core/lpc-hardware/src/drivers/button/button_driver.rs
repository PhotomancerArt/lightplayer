use alloc::boxed::Box;

use crate::{
    ButtonDebouncer, ButtonEvent, HardwareAddress, HardwareDriver, HardwareEndpoint,
    HardwareEndpointError, HardwareEndpointId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub trait ButtonInput {
    fn source(&self) -> &HardwareAddress;

    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent>;
}

pub trait ButtonDriver: HardwareDriver {
    fn endpoints(&self) -> alloc::vec::Vec<HardwareEndpoint>;

    fn open(
        &self,
        endpoint_id: &HardwareEndpointId,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError>;
}
