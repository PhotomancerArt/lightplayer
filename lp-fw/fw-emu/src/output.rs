//! Syscall-based OutputProvider implementation
//!
//! Uses emulator syscalls to send LED output data to the host.

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use core::cell::RefCell;

use lp_riscv_emu_guest::println;
use lpc_hardware::OutputError;
use lpc_hardware::{
    HardwareEndpointError, HardwareEndpointSpec, HardwareRegistry, HardwareSystem, Ws281xConfig,
    Ws281xOutput,
};
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};

/// Syscall-based OutputProvider implementation
///
/// For now, uses print logging to indicate output changes.
/// Output syscalls will be added later if needed.
pub struct SyscallOutputProvider {
    hardware_system: Rc<HardwareSystem>,
    channels: RefCell<BTreeMap<OutputChannelHandle, Box<dyn Ws281xOutput>>>,
    next_handle: RefCell<u32>,
}

impl SyscallOutputProvider {
    #[allow(
        dead_code,
        reason = "kept for tests and older callers that construct only a registry"
    )]
    pub fn new_with_hardware_registry(hardware_registry: Rc<HardwareRegistry>) -> Self {
        Self::new_with_hardware_system(Rc::new(HardwareSystem::with_virtual_drivers(
            hardware_registry,
        )))
    }

    pub fn new_with_hardware_system(hardware_system: Rc<HardwareSystem>) -> Self {
        Self {
            hardware_system,
            channels: RefCell::new(BTreeMap::new()),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for SyscallOutputProvider {
    fn open(
        &self,
        endpoint: &HardwareEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
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

        let output = self.open_ws281x_output(endpoint, byte_count, options)?;
        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id as i32);
        self.channels.borrow_mut().insert(handle, output);

        println!(
            "[output] open: endpoint={}, bytes={}, format={:?}, handle={:?}",
            endpoint, byte_count, format, handle
        );

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
        let mut channels = self.channels.borrow_mut();
        let output = channels
            .get_mut(&handle)
            .ok_or_else(|| OutputError::InvalidHandle {
                handle: handle.as_i32(),
            })?;
        output.write(data)?;
        println!("[output] write: handle={:?}, len={}", handle, data.len());
        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        self.channels
            .borrow_mut()
            .remove(&handle)
            .ok_or_else(|| OutputError::InvalidHandle {
                handle: handle.as_i32(),
            })?;
        println!("[output] close: handle={:?}", handle);
        Ok(())
    }
}

impl SyscallOutputProvider {
    fn open_ws281x_output(
        &self,
        endpoint: &HardwareEndpointSpec,
        byte_count: u32,
        options: Option<OutputDriverOptions>,
    ) -> Result<Box<dyn Ws281xOutput>, OutputError> {
        self.hardware_system
            .open_ws281x_by_spec(endpoint, Ws281xConfig::new(byte_count, options))
            .map_err(endpoint_error_to_output_error)
    }
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}
