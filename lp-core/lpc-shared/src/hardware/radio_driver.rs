use alloc::boxed::Box;
use alloc::vec::Vec;

use super::{HardwareDriver, HardwareEndpoint, HardwareEndpointError, HardwareEndpointId};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioPacket {
    peer: [u8; 6],
    payload: Vec<u8>,
}

impl RadioPacket {
    pub fn new(peer: [u8; 6], payload: impl Into<Vec<u8>>) -> Self {
        Self {
            peer,
            payload: payload.into(),
        }
    }

    pub fn peer(&self) -> [u8; 6] {
        self.peer
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

pub trait RadioDevice {
    fn send(&mut self, peer: [u8; 6], payload: &[u8]) -> Result<(), HardwareEndpointError>;

    fn receive(&mut self) -> Result<Option<RadioPacket>, HardwareEndpointError>;
}

pub trait RadioDriver: HardwareDriver {
    fn endpoints(&self) -> Vec<HardwareEndpoint>;

    fn open(
        &self,
        endpoint_id: &HardwareEndpointId,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError>;
}
