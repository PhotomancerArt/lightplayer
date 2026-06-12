use alloc::boxed::Box;

use crate::{
    ButtonDebouncer, ButtonEvent, HardwareEndpointError, HwAddress, HwDriver, HwEndpoint,
    HwEndpointId,
};

/// Button endpoint configuration.
///
/// `stable_ms` controls how long a raw input level must remain unchanged before
/// [`ButtonDebouncer`] emits a [`ButtonEvent`].
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

/// Opened button input.
///
/// Implementations usually own a GPIO input lease and use a
/// [`ButtonDebouncer`] to turn raw pressed/released samples into events.
pub trait ButtonInput {
    /// Resource address being sampled.
    fn source(&self) -> &HwAddress;

    /// Poll the input and return a debounced event when state changes.
    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent>;
}

/// Driver that exposes GPIO-backed button endpoints.
pub trait ButtonDriver: HwDriver {
    /// List currently known button endpoints.
    fn endpoints(&self) -> alloc::vec::Vec<HwEndpoint>;

    /// Open one endpoint and claim the underlying input resource.
    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError>;
}
