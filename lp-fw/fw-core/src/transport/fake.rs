//! Fake transport implementation for testing and development
//!
//! A no-op transport that implements ServerTransport but doesn't actually
//! send or receive messages. Useful for testing the server without hardware.
//! Can be configured with a queue of messages to simulate client requests.

extern crate alloc;

use alloc::vec::Vec;
use lp_model::{ClientMessage, ServerMessage, TransportError};
use lp_shared::transport::ServerTransport;

/// Fake transport that can simulate client messages
///
/// Implements ServerTransport but:
/// - `send()` logs the message and returns Ok(())
/// - `receive()` returns queued messages, then Ok(None)
/// - `close()` does nothing
pub struct FakeTransport {
    /// Queue of messages to return from receive()
    message_queue: Vec<ClientMessage>,
}

impl FakeTransport {
    /// Create a new fake transport
    pub fn new() -> Self {
        Self {
            message_queue: Vec::new(),
        }
    }

    /// Queue a message to be returned by receive()
    pub fn queue_message(&mut self, msg: ClientMessage) {
        self.message_queue.push(msg);
    }
}

impl ServerTransport for FakeTransport {
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        #[cfg(any(feature = "emu", feature = "esp32"))]
        log::debug!("FakeTransport: Would send message id={}", msg.id);
        #[cfg(not(any(feature = "emu", feature = "esp32")))]
        let _ = msg;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        Ok(if self.message_queue.is_empty() {
            None
        } else {
            Some(self.message_queue.remove(0))
        })
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        Ok(core::mem::take(&mut self.message_queue))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
