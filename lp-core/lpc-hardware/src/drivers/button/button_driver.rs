use alloc::boxed::Box;

use crate::{
    ButtonDebouncer, ButtonEvent, HardwareEndpointError, HwAddress, HwDriver, HwEndpoint,
    HwEndpointId,
};

/// Button endpoint configuration.
///
/// `stable_ms` controls how long a raw input level must remain unchanged before
/// [`ButtonDebouncer`] emits a [`ButtonEvent`]. `pull` and `active` describe
/// the wiring: the default is a button to ground (pull-up, active low);
/// a switch that drives the pin high when on is pull-down, active high.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonConfig {
    stable_ms: u64,
    pull: ButtonPull,
    active: ButtonActive,
}

impl ButtonConfig {
    pub fn new(stable_ms: u64) -> Self {
        Self {
            stable_ms,
            pull: ButtonPull::Up,
            active: ButtonActive::Low,
        }
    }

    pub fn with_pull(mut self, pull: ButtonPull) -> Self {
        self.pull = pull;
        self
    }

    pub fn with_active(mut self, active: ButtonActive) -> Self {
        self.active = active;
        self
    }

    pub fn stable_ms(&self) -> u64 {
        self.stable_ms
    }

    pub fn pull(&self) -> ButtonPull {
        self.pull
    }

    pub fn active(&self) -> ButtonActive {
        self.active
    }
}

impl Default for ButtonConfig {
    fn default() -> Self {
        Self::new(ButtonDebouncer::DEFAULT_STABLE_MS)
    }
}

/// Internal pull resistor a button input enables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonPull {
    Up,
    Down,
    None,
}

/// Pin level that means "pressed" (or, for a switch, "on").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonActive {
    Low,
    High,
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
