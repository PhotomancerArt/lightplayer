use super::host_serial::{HostSerial, SerialError};
use alloc::rc::Rc;
use core::cell::RefCell;
use lp_riscv_shared::{GuestSerial, SerialSyscall};

/// Test implementation of SerialSyscall that uses HostSerial directly
/// This allows testing GuestSerial with HostSerial without the emulator
/// Uses Rc<RefCell<HostSerial>> to share the HostSerial between test code and GuestSerial
pub struct HostSerialSyscall {
    serial_host: Rc<RefCell<HostSerial>>,
}

impl HostSerialSyscall {
    pub fn new(serial_host: Rc<RefCell<HostSerial>>) -> Self {
        Self { serial_host }
    }
}

impl SerialSyscall for HostSerialSyscall {
    fn serial_write(&self, data: &[u8]) -> i32 {
        self.serial_host.borrow_mut().guest_write(data)
    }

    fn serial_read(&self, buf: &mut [u8]) -> i32 {
        self.serial_host.borrow_mut().guest_read(buf)
    }

    fn serial_has_data(&self) -> bool {
        self.serial_host.borrow().has_data()
    }
}

pub struct TestHostSerial(Rc<RefCell<HostSerial>>);

impl TestHostSerial {
    pub fn read(&self, buffer: &mut [u8]) -> Result<usize, SerialError> {
        self.0.borrow_mut().host_read(buffer)
    }

    pub fn write(&self, buffer: &[u8]) -> Result<usize, SerialError> {
        self.0.borrow_mut().host_write(buffer)
    }

    pub fn read_line(&self) -> alloc::string::String {
        self.0.borrow_mut().host_read_line()
    }

    pub fn write_line(&self, line: &str) -> Result<usize, SerialError> {
        self.0.borrow_mut().host_write_line(line)
    }
}

pub struct TestGuestSerial(RefCell<GuestSerial<HostSerialSyscall>>);

impl TestGuestSerial {
    pub fn read_line(&self) -> alloc::string::String {
        self.0.borrow_mut().read_line()
    }

    pub fn write(&self, data: &[u8]) -> i32 {
        self.0.borrow_mut().write(data)
    }

    pub fn write_line(&self, line: &str) -> i32 {
        self.0.borrow_mut().write_line(line)
    }
}

/// Create a test pair of HostSerial and GuestSerial that share the same underlying buffers
pub fn serial_pair() -> (TestHostSerial, TestGuestSerial) {
    serial_pair_sized(
        HostSerial::DEFAULT_BUF_SIZE,
        GuestSerial::<HostSerialSyscall>::DEFAULT_BUF_SIZE,
    )
}
pub fn serial_pair_sized(
    host_buf_size: usize,
    guest_buf_size: usize,
) -> (TestHostSerial, TestGuestSerial) {
    let host_serial = Rc::new(RefCell::new(HostSerial::new(host_buf_size)));
    let guest_serial = RefCell::new(GuestSerial::new_with_capacity(
        HostSerialSyscall::new(host_serial.clone()),
        guest_buf_size,
    ));

    (TestHostSerial(host_serial), TestGuestSerial(guest_serial))
}
