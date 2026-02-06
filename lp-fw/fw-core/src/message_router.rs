//! Message router for decoupling main loop from I/O
//!
//! Provides a central abstraction for routing messages between tasks using
//! embassy-sync channels. Designed to be reusable for multi-transport scenarios.

extern crate alloc;

use alloc::{string::String, vec::Vec};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, TryReceiveError, TrySendError};

/// Message router for task communication
///
/// Uses embassy-sync channels to decouple message producers (I/O tasks) from
/// consumers (main loop). Supports multiple producers and consumers (MPMC).
///
/// # Example
///
/// ```no_run
/// use embassy_sync::channel::Channel;
/// use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
/// use fw_core::MessageRouter;
///
/// static INCOMING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
/// static OUTGOING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
///
/// let router = MessageRouter::new(&INCOMING, &OUTGOING);
///
/// // Main loop
/// let messages = router.receive_all();
/// let _ = router.send("response".to_string());
///
/// // I/O task
/// let _ = INCOMING.sender().try_send("message".to_string());
/// let _ = OUTGOING.receiver().try_receive();
/// ```
pub struct MessageRouter {
    /// Channel for incoming messages (I/O → main loop)
    incoming: &'static Channel<CriticalSectionRawMutex, String, 32>,
    /// Channel for outgoing messages (main loop → I/O)
    outgoing: &'static Channel<CriticalSectionRawMutex, String, 32>,
}

impl MessageRouter {
    /// Create a new message router with the given channels
    ///
    /// # Arguments
    ///
    /// * `incoming` - Channel for incoming messages (I/O task pushes here)
    /// * `outgoing` - Channel for outgoing messages (main loop pushes here)
    pub fn new(
        incoming: &'static Channel<CriticalSectionRawMutex, String, 32>,
        outgoing: &'static Channel<CriticalSectionRawMutex, String, 32>,
    ) -> Self {
        Self { incoming, outgoing }
    }

    /// Receive all available messages (non-blocking)
    ///
    /// Drains the incoming channel and returns all available messages.
    /// Returns empty vector if no messages available.
    ///
    /// # Returns
    ///
    /// Vector of all available messages (may be empty)
    pub fn receive_all(&self) -> Vec<String> {
        let mut messages = Vec::new();
        let receiver = self.incoming.receiver();

        loop {
            match receiver.try_receive() {
                Ok(msg) => messages.push(msg),
                Err(TryReceiveError::Empty) => break,
            }
        }

        messages
    }

    /// Send a message (non-blocking)
    ///
    /// Attempts to send a message to the outgoing channel. Returns an error
    /// if the channel is full (backpressure).
    ///
    /// # Arguments
    ///
    /// * `msg` - Message to send
    ///
    /// # Returns
    ///
    /// * `Ok(())` if message was sent
    /// * `Err(TrySendError<String>)` if channel is full (contains the message)
    pub fn send(&self, msg: String) -> Result<(), TrySendError<String>> {
        let sender = self.outgoing.sender();
        sender.try_send(msg)
    }

    /// Get reference to incoming channel (for I/O tasks)
    ///
    /// Allows I/O tasks to push messages directly to the incoming channel.
    pub fn incoming(&self) -> &'static Channel<CriticalSectionRawMutex, String, 32> {
        self.incoming
    }

    /// Get reference to outgoing channel (for I/O tasks)
    ///
    /// Allows I/O tasks to drain messages from the outgoing channel.
    pub fn outgoing(&self) -> &'static Channel<CriticalSectionRawMutex, String, 32> {
        self.outgoing
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::{format, string::ToString};
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;

    static TEST_INCOMING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
    static TEST_OUTGOING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

    #[test]
    fn test_receive_all_empty() {
        // Clear incoming channel first (in case previous test left data)
        while TEST_INCOMING.receiver().try_receive().is_ok() {}

        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);
        let messages = router.receive_all();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_receive_all_multiple() {
        // Clear incoming channel first (in case previous test left data)
        while TEST_INCOMING.receiver().try_receive().is_ok() {}

        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);

        // Push messages
        TEST_INCOMING.sender().try_send("msg1".to_string()).unwrap();
        TEST_INCOMING.sender().try_send("msg2".to_string()).unwrap();
        TEST_INCOMING.sender().try_send("msg3".to_string()).unwrap();

        // Receive all
        let messages = router.receive_all();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], "msg1");
        assert_eq!(messages[1], "msg2");
        assert_eq!(messages[2], "msg3");

        // Should be empty now
        let empty = router.receive_all();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_send_receive() {
        // Clear outgoing channel first (in case previous test left data)
        while TEST_OUTGOING.receiver().try_receive().is_ok() {}

        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);

        // Send message
        router.send("test".to_string()).unwrap();

        // Receive from outgoing channel
        let msg = TEST_OUTGOING.receiver().try_receive().unwrap();
        assert_eq!(msg, "test");

        // Verify channel is empty now
        assert!(TEST_OUTGOING.receiver().try_receive().is_err());
    }

    #[test]
    fn test_send_full_channel() {
        // Clear outgoing channel first
        while TEST_OUTGOING.receiver().try_receive().is_ok() {}

        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);

        // Fill channel to capacity (32 messages)
        for i in 0..32 {
            let result = router.send(format!("msg{}", i));
            assert!(result.is_ok(), "Should be able to send message {}", i);
        }

        // Verify channel is full
        assert!(
            TEST_OUTGOING.is_full(),
            "Channel should be full after 32 messages"
        );

        // Next send should fail
        let result = router.send("overflow".to_string());
        assert!(result.is_err(), "Should fail when channel is full");

        // Clean up: clear channel for next test
        while TEST_OUTGOING.receiver().try_receive().is_ok() {}
    }
}
