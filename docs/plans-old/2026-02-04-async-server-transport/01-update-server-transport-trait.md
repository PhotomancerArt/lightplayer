# Phase 1: Update ServerTransport Trait to Async

## Scope of phase

Update the `ServerTransport` trait in `lp-shared` to use async methods instead of synchronous ones. This is the foundational change that all other phases depend on.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-core/lp-shared/src/transport/server.rs`

Change all trait methods to async:

```rust
/// Trait for server-side transport implementations
///
/// This trait provides an async interface for sending and receiving
/// messages. Messages are consumed (moved) on send, and receive is non-blocking
/// (returns `None` if no message is available).
///
/// The transport handles serialization/deserialization internally.
pub trait ServerTransport {
    /// Send a server message (consumes the message)
    ///
    /// The transport handles serialization internally.
    ///
    /// # Arguments
    ///
    /// * `msg` - The server message to send (consumed/moved)
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the message was sent successfully
    /// * `Err(TransportError)` if sending failed
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError>;

    /// Receive a client message (non-blocking)
    ///
    /// The transport handles deserialization internally.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(ClientMessage))` if a message is available
    /// * `Ok(None)` if no message is available (non-blocking)
    /// * `Err(TransportError)` if receiving failed
    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError>;

    /// Receive all available client messages (non-blocking)
    ///
    /// Drains all available messages from the transport in a single call.
    /// This is more efficient than calling `receive()` in a loop.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<ClientMessage>)` - Vector of all available messages (may be empty)
    /// * `Err(TransportError)` if receiving failed
    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        let mut messages = Vec::new();
        loop {
            match self.receive().await? {
                Some(msg) => messages.push(msg),
                None => break,
            }
        }
        Ok(messages)
    }

    /// Close the transport connection
    ///
    /// Explicitly closes the transport connection. This method is idempotent -
    /// calling it multiple times is safe and will return `Ok(())` if already closed.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the transport was closed successfully (or already closed)
    /// * `Err(TransportError)` if closing failed
    async fn close(&mut self) -> Result<(), TransportError>;
}
```

**Key changes:**
- All methods are now `async fn`
- `receive_all()` default implementation uses `.await` when calling `receive()`
- Documentation updated to reflect async nature

### 2. Update `lp-core/lp-shared/src/transport/mod.rs`

Ensure the trait is properly exported (should already be exported, but verify):

```rust
pub use server::ServerTransport;
```

### 3. Add async runtime dependency (if needed)

Check if `lp-shared` needs async runtime support. Since we're using `async fn` in traits, we need to ensure the crate can compile with async support. Check `Cargo.toml`:

- If using `async-trait` crate, add it
- If using native async traits (Rust 1.75+), ensure edition is 2021
- May need to add `futures` or similar for async utilities

**Note:** For `no_std` environments (firmware), we'll need to ensure async traits work. Check if we need `async-trait` or if we can use native async traits.

## Tests

Update any existing tests that use `ServerTransport`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    // Update test to use async
    #[tokio::test]  // or #[async_std::test] depending on runtime
    async fn test_send_message() {
        // Test implementation
    }
}
```

## Validate

Run:
```bash
cd lp-core/lp-shared
cargo check
```

**Expected:** Code compiles, but all implementations will fail (that's expected - we'll fix them in later phases).

**Note:** This phase will break compilation for all `ServerTransport` implementations. That's expected and will be fixed in subsequent phases.
