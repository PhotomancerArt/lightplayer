//! Syscall-based OutputProvider implementation
//!
//! Uses emulator syscalls to send LED output data to the host.

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::cell::RefCell;

use lp_riscv_emu_guest::println;
use lpc_hardware::OutputError;
use lpc_hardware::{
    HardwareEndpointError, HardwareSystem, HwEndpointSpec, HwRegistry, Ws281xConfig, Ws281xOutput,
};
use lpc_shared::output::{OutputDriverOptions, OutputFormat, OutputPortHandle, OutputProvider};

/// Syscall-based OutputProvider implementation
///
/// For now, uses print logging to indicate output changes.
/// Output syscalls will be added later if needed.
pub struct SyscallOutputProvider {
    hardware_system: Rc<HardwareSystem>,
    ports: RefCell<BTreeMap<OutputPortHandle, EmuPort>>,
    next_handle: RefCell<u32>,
}

/// One open port: its virtual output and the 8-bit frame it renders into.
///
/// The frame buffer is kept per port, like the ESP32 provider's
/// `PortState.frame`, so a steady-state write allocates nothing — a fresh
/// 3 B/lamp `Vec` per write was the emulator's whole steady `frame`
/// transient, which the device never paid.
struct EmuPort {
    output: Box<dyn Ws281xOutput>,
    frame: Vec<u8>,
}

impl SyscallOutputProvider {
    #[allow(
        dead_code,
        reason = "kept for tests and older callers that construct only a registry"
    )]
    pub fn new_with_hardware_registry(hardware_registry: Rc<HwRegistry>) -> Self {
        Self::new_with_hardware_system(Rc::new(HardwareSystem::with_virtual_drivers(
            hardware_registry,
        )))
    }

    pub fn new_with_hardware_system(hardware_system: Rc<HardwareSystem>) -> Self {
        Self {
            hardware_system,
            ports: RefCell::new(BTreeMap::new()),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for SyscallOutputProvider {
    fn open(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputPortHandle, OutputError> {
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
        let handle = OutputPortHandle::new(handle_id as i32);
        self.ports.borrow_mut().insert(
            handle,
            EmuPort {
                output,
                frame: Vec::with_capacity(byte_count as usize),
            },
        );

        println!(
            "[output] open: endpoint={}, bytes={}, format={:?}, handle={:?}",
            endpoint, byte_count, format, handle
        );

        Ok(handle)
    }

    fn write(&self, handle: OutputPortHandle, data: &[u16]) -> Result<(), OutputError> {
        let mut ports = self.ports.borrow_mut();
        let port = ports
            .get_mut(&handle)
            .ok_or_else(|| OutputError::InvalidHandle {
                handle: handle.as_i32(),
            })?;
        render_rgb8(data, &mut port.frame);
        port.output.write(&port.frame)?;
        println!("[output] write: handle={:?}, len={}", handle, data.len());
        Ok(())
    }

    fn close(&self, handle: OutputPortHandle) -> Result<(), OutputError> {
        self.ports
            .borrow_mut()
            .remove(&handle)
            .ok_or_else(|| OutputError::InvalidHandle {
                handle: handle.as_i32(),
            })?;
        println!("[output] close: handle={:?}", handle);
        Ok(())
    }

    fn hardware_generation(&self) -> u64 {
        self.hardware_system.registry().generation()
    }
}

impl SyscallOutputProvider {
    fn open_ws281x_output(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        options: Option<OutputDriverOptions>,
    ) -> Result<Box<dyn Ws281xOutput>, OutputError> {
        let _ = options;
        self.hardware_system
            .open_ws281x_by_spec(endpoint, Ws281xConfig::new(byte_count))
            .map_err(endpoint_error_to_output_error)
    }
}

fn render_rgb8(data: &[u16], out: &mut Vec<u8>) {
    out.clear();
    out.extend(data.iter().map(|sample| (sample >> 8) as u8));
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}
