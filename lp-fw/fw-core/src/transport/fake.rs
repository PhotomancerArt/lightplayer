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
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // Log the message (if logging is available)
        #[cfg(any(feature = "emu", feature = "esp32"))]
        log::debug!("FakeTransport: Would send message id={}", msg.id);

        // Suppress unused variable warning when logging features are disabled
        #[cfg(not(any(feature = "emu", feature = "esp32")))]
        let _ = msg;

        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Return queued messages first, then None
        if !self.message_queue.is_empty() {
            Ok(Some(self.message_queue.remove(0)))
        } else {
            Ok(None)
        }
    }

    fn close(&mut self) -> Result<(), TransportError> {
        // Nothing to close
        Ok(())
    }
}
