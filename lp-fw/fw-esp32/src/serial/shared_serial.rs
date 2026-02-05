//! Shared SerialIo wrapper
//!
//! Allows sharing a SerialIo instance between logging and transport
//! using Rc<RefCell<>> for interior mutability.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use fw_core::serial::{SerialError, SerialIo};

/// Shared SerialIo wrapper
///
/// Wraps a SerialIo instance in Rc<RefCell<>> to allow sharing
/// between multiple consumers (e.g., logging and transport).
pub struct SharedSerialIo<Io: SerialIo> {
    inner: Rc<RefCell<Io>>,
}

impl<Io: SerialIo> SharedSerialIo<Io> {
    /// Create a new SharedSerialIo wrapper
    pub fn new(inner: Rc<RefCell<Io>>) -> Self {
        Self { inner }
    }

    /// Get a reference to the inner Rc<RefCell<Io>>
    #[allow(dead_code)]
    pub fn inner(&self) -> &Rc<RefCell<Io>> {
        &self.inner
    }
}

impl<Io: SerialIo> SerialIo for SharedSerialIo<Io> {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        self.inner
            .try_borrow_mut()
            .map_err(|_| SerialError::Other("SerialIo is already borrowed".into()))?
            .write(data)
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        self.inner
            .try_borrow_mut()
            .map_err(|_| SerialError::Other("SerialIo is already borrowed".into()))?
            .read_available(buf)
    }

    fn has_data(&self) -> bool {
        self.inner
            .try_borrow()
            .map(|io| io.has_data())
            .unwrap_or(true)
    }
}
