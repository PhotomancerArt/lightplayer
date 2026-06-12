use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::{
    HardwareDriver, HardwareEndpoint, HardwareEndpointError, HardwareEndpointId, RadioChannelId,
    RadioDrainReport, RadioMessage, RadioMessageKind,
};

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

pub trait RadioDevice {
    fn subscribe_channel(&mut self, channel: RadioChannelId) -> Result<(), HardwareEndpointError>;

    fn unsubscribe_channel(&mut self, channel: RadioChannelId)
    -> Result<(), HardwareEndpointError>;

    fn send_channel(
        &mut self,
        channel: RadioChannelId,
        kind: RadioMessageKind,
        payload: &[u8],
    ) -> Result<(), HardwareEndpointError>;

    fn drain_channel(
        &mut self,
        channel: RadioChannelId,
        out: &mut Vec<RadioMessage>,
    ) -> Result<RadioDrainReport, HardwareEndpointError>;
}

pub trait RadioDriver: HardwareDriver {
    fn endpoints(&self) -> Vec<HardwareEndpoint>;

    fn open(
        &self,
        endpoint_id: &HardwareEndpointId,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError>;
}
