# Phase 1: Create MessageRouter in fw-core

## Scope of phase

Create the `MessageRouter` abstraction in `fw-core` that uses `embassy-sync::channel::Channel` for message queues. This provides a reusable, testable abstraction for routing messages between tasks.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Add embassy-sync dependency

Update `lp-fw/fw-core/Cargo.toml`:
```toml
[dependencies]
# ... existing dependencies ...
embassy-sync = "0.7.2"
```

### 2. Create message_router module

Create `lp-fw/fw-core/src/message_router.rs`:

```rust
//! Message router for decoupling main loop from I/O
//!
//! Provides a central abstraction for routing messages between tasks using
//! embassy-sync channels. Designed to be reusable for multi-transport scenarios.

extern crate alloc;

use alloc::vec::Vec;
use embassy_sync::channel::{Channel, TryReceiveError, TrySendError};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

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
///
/// static INCOMING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
/// static OUTGOING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
///
/// let router = MessageRouter::new(&INCOMING, &OUTGOING);
///
/// // Main loop
/// let messages = router.receive_all();
/// router.send("response".to_string())?;
///
/// // I/O task
/// incoming.try_send("message".to_string())?;
/// let msg = outgoing.try_receive()?;
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
```

### 3. Export from lib.rs

Update `lp-fw/fw-core/src/lib.rs`:
```rust
// ... existing code ...

pub mod message_router;

pub use message_router::MessageRouter;
```

### 4. Add tests

Add tests to `lp-fw/fw-core/src/message_router.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use embassy_sync::channel::Channel;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

    static TEST_INCOMING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
    static TEST_OUTGOING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

    #[test]
    fn test_receive_all_empty() {
        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);
        let messages = router.receive_all();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_receive_all_multiple() {
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
        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);
        
        // Send message
        router.send("test".to_string()).unwrap();
        
        // Receive from outgoing channel
        let msg = TEST_OUTGOING.receiver().try_receive().unwrap();
        assert_eq!(msg, "test");
    }

    #[test]
    fn test_send_full_channel() {
        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);
        
        // Fill channel to capacity
        for i in 0..32 {
            router.send(format!("msg{}", i)).unwrap();
        }
        
        // Next send should fail
        let result = router.send("overflow".to_string());
        assert!(result.is_err());
    }
}
```

## Tests to Write

- Test `receive_all()` with empty channel
- Test `receive_all()` with multiple messages
- Test `send()` and receive from channel
- Test `send()` when channel is full (backpressure)
- Test that messages are received in order (FIFO)

## Validate

Run from `lp-fw/fw-core/` directory:

```bash
cd lp-fw/fw-core
cargo test --package fw-core
cargo check --package fw-core
```

Ensure:
- All tests pass
- No warnings
- Code compiles for both `std` and `no_std` targets
- MessageRouter is exported from `fw-core`
