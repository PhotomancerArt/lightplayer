//! [`DeviceByteStream`] endpoint of the fake device.

use crate::providers::fake_device::fake_device_core::FakeEsp32Device;
use crate::stream::{ByteStreamError, DeviceByteStream};

/// One serial attachment to a [`FakeEsp32Device`].
///
/// The real transport machinery
/// (`lpa_client::transport_serial::create_hardware_serial_transport_pair_with_options`)
/// drives this from its I/O thread exactly as it drives a native serial
/// port — same framing, same reset dance, same polling cadence.
pub struct FakeDeviceByteStream {
    device: FakeEsp32Device,
}

impl FakeDeviceByteStream {
    pub fn new(device: FakeEsp32Device) -> Self {
        Self { device }
    }
}

impl DeviceByteStream for FakeDeviceByteStream {
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
        self.device.lock().serve_read(buf)
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
        self.device.lock().accept_write(bytes)
    }

    fn set_signals(&mut self, dtr: Option<bool>, rts: Option<bool>) -> Result<(), ByteStreamError> {
        self.device.lock().set_signals(dtr, rts);
        Ok(())
    }

    fn reopen(&mut self, _baud_rate: u32) -> Result<(), ByteStreamError> {
        self.device.lock().reopen();
        Ok(())
    }
}
