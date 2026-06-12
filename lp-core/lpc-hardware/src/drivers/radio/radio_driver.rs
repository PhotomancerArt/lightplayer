use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::{
    HardwareEndpointError, HwDriver, HwEndpoint, HwEndpointId, RadioChannelId, RadioDrainReport,
    RadioMessage, RadioMessageKind,
};

/// Radio endpoint configuration.
///
/// The optional channel is target-specific setup metadata; logical
/// subscriptions still happen through [`RadioDevice::subscribe_channel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioConfig {
    channel: Option<u8>,
}

impl RadioConfig {
    pub fn new(channel: Option<u8>) -> Self {
        Self { channel }
    }

    pub fn channel(&self) -> Option<u8> {
        self.channel
    }
}

impl Default for RadioConfig {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Opened packet-radio device.
pub trait RadioDevice {
    /// Start receiving messages for a logical channel.
    fn subscribe_channel(&mut self, channel: RadioChannelId) -> Result<(), HardwareEndpointError>;

    /// Stop receiving messages for a logical channel.
    fn unsubscribe_channel(&mut self, channel: RadioChannelId)
    -> Result<(), HardwareEndpointError>;

    /// Send a message on a logical channel.
    fn send_channel(
        &mut self,
        channel: RadioChannelId,
        kind: RadioMessageKind,
        payload: &[u8],
    ) -> Result<(), HardwareEndpointError>;

    /// Drain received messages for a channel into `out`.
    fn drain_channel(
        &mut self,
        channel: RadioChannelId,
        out: &mut Vec<RadioMessage>,
    ) -> Result<RadioDrainReport, HardwareEndpointError>;
}

/// Driver that exposes radio endpoints.
pub trait RadioDriver: HwDriver {
    /// List currently known radio endpoints.
    fn endpoints(&self) -> Vec<HwEndpoint>;

    /// Open one endpoint and claim the underlying radio resource.
    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError>;
}
