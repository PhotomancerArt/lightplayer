//! Syscall-based OutputProvider implementation
//!
//! Uses emulator syscalls to send LED output data to the host.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;
use core::cell::RefCell;

use lp_riscv_emu_guest::println;
use lpc_shared::OutputError;
use lpc_shared::hardware::{
    HardwareAddress, HardwareCapability, HardwareClaim, HardwareLease, HardwareManifest,
    HardwareRegistry,
};
use lpc_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

/// Syscall-based OutputProvider implementation
///
/// For now, uses print logging to indicate output changes.
/// Output syscalls will be added later if needed.
pub struct SyscallOutputProvider {
    hardware_registry: HardwareRegistry,
    channels: RefCell<BTreeMap<OutputChannelHandle, HardwareLease>>,
    next_handle: RefCell<u32>,
}

impl SyscallOutputProvider {
    /// Create a new syscall-based OutputProvider instance
    pub fn new() -> Self {
        Self {
            hardware_registry: HardwareRegistry::new(
                HardwareManifest::virtual_single_rmt_gpio_board(),
            ),
            channels: RefCell::new(BTreeMap::new()),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for SyscallOutputProvider {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
        options: Option<lpc_shared::output::OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, OutputError> {
        let _ = options;
        if byte_count == 0 {
            return Err(OutputError::InvalidConfig {
                reason: format!("byte_count must be > 0, got {byte_count}"),
            });
        }
        if format != OutputFormat::Ws2811 {
            return Err(OutputError::InvalidConfig {
                reason: format!("unsupported output format: {format:?}"),
            });
        }

        let lease = self.claim_ws281x_output(pin)?;
        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id as i32);
        self.channels.borrow_mut().insert(handle, lease);

        println!(
            "[output] open: pin={}, bytes={}, format={:?}, handle={:?}",
            pin, byte_count, format, handle
        );

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
        if !self.channels.borrow().contains_key(&handle) {
            return Err(OutputError::InvalidHandle {
                handle: handle.as_i32(),
            });
        }
        println!("[output] write: handle={:?}, len={}", handle, data.len());
        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let lease = self.channels.borrow_mut().remove(&handle).ok_or_else(|| {
            OutputError::InvalidHandle {
                handle: handle.as_i32(),
            }
        })?;
        self.hardware_registry
            .release(&lease)
            .map_err(|error| OutputError::Hardware { error })?;
        println!("[output] close: handle={:?}", handle);
        Ok(())
    }
}

impl SyscallOutputProvider {
    fn claim_ws281x_output(&self, pin: u32) -> Result<HardwareLease, OutputError> {
        let gpio = HardwareAddress::gpio(pin);
        let rmt = HardwareAddress::rmt_ws281x(0);
        self.hardware_registry
            .ensure_capability(&gpio, HardwareCapability::GpioOutput)
            .map_err(|error| OutputError::Hardware { error })?;
        self.hardware_registry
            .ensure_capability(&rmt, HardwareCapability::Rmt)
            .map_err(|error| OutputError::Hardware { error })?;
        self.hardware_registry
            .ensure_capability(&rmt, HardwareCapability::Ws281xOutput)
            .map_err(|error| OutputError::Hardware { error })?;
        self.hardware_registry
            .claim_bundle(HardwareClaim::new("fw-emu-output", Vec::from([gpio, rmt])))
            .map_err(|error| OutputError::Hardware { error })
    }
}
