# Phase 8: Update Tests and Fix Compilation Errors

## Scope of phase

Update all tests to use async transport, fix any remaining compilation errors, and ensure all code compiles successfully.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update tests in `lp-fw/fw-core/src/transport/serial.rs`

Update tests to use async and create mock async I/O types:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use core::pin::Pin;
    use core::task::{Context, Poll};
    use embedded_io_async::{Error, ErrorKind, Read, Write};
    
    // Mock async I/O types for testing
    struct MockAsyncTx {
        written: alloc::vec::Vec<u8>,
    }
    
    struct MockAsyncRx {
        read_data: alloc::vec::Vec<u8>,
        read_pos: usize,
    }
    
    impl Write for MockAsyncTx {
        type Error = MockError;
        
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, Self::Error>> {
            self.get_mut().written.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }
        
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }
    
    impl Read for MockAsyncRx {
        type Error = MockError;
        
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<Result<usize, Self::Error>> {
            let mock = self.get_mut();
            let available = mock.read_data.len() - mock.read_pos;
            if available == 0 {
                return Poll::Ready(Ok(0));
            }
            let to_read = available.min(buf.len());
            buf[..to_read].copy_from_slice(&mock.read_data[mock.read_pos..mock.read_pos + to_read]);
            mock.read_pos += to_read;
            Poll::Ready(Ok(to_read))
        }
    }
    
    #[derive(Debug)]
    struct MockError;
    
    impl Error for MockError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }
    
    // Update tests to use async
    // Note: For no_std tests, we may need embassy-futures or similar
    #[test]
    fn test_send_message() {
        // Use block_on for sync test context
        let mut tx = MockAsyncTx { written: Vec::new() };
        let mut rx = MockAsyncRx {
            read_data: Vec::new(),
            read_pos: 0,
        };
        let mut transport = SerialTransport::new(tx, rx);
        
        // Test would need async runtime - may need to use embassy-futures::block_on
        // or mark as async test
    }
}
```

**Note:** For `no_std` tests, we may need to use `embassy-futures::block_on` or mark tests as async with appropriate runtime.

### 2. Fix any compilation errors

Go through each crate and fix compilation errors:

- **lp-shared**: Ensure async trait compiles
- **fw-core**: Ensure SerialTransport compiles with async I/O
- **fw-esp32**: Ensure async USB serial works
- **fw-emu**: Ensure async adapter works
- **lp-cli**: Ensure server loops compile
- **lp-client**: Ensure AsyncLocalServerTransport compiles
- **lp-server**: Should not need changes (doesn't call transport)

### 3. Update error handling

Ensure error types are compatible with `embedded_io_async::Error`:

- Check if `embedded_io_async` requires specific error types
- May need to implement `Error` trait for custom error types
- May need error conversion utilities

### 4. Handle async runtime requirements

For different platforms:

- **ESP32**: Uses Embassy runtime (already async)
- **fw-emu**: Uses `embassy-futures::block_on` in sync context
- **CLI**: Uses tokio runtime (already async)
- **Tests**: May need tokio or embassy-futures runtime

## Tests

Update all tests:

1. **Transport tests**: Update to use async
2. **Server loop tests**: Update to use async transport
3. **Integration tests**: Update to use async transport

## Validate

Run compilation checks for all affected crates:

```bash
# Check lp-shared
cd lp-core/lp-shared
cargo check

# Check fw-core
cd lp-fw/fw-core
cargo check

# Check fw-esp32
cd lp-fw/fw-esp32
cargo check --target riscv32imac-unknown-none-elf --features esp32c6

# Check fw-emu
cd lp-fw/fw-emu
cargo check

# Check lp-client
cd lp-core/lp-client
cargo check

# Check lp-cli
cd lp-cli
cargo check

# Check lp-server (should not need changes)
cd lp-core/lp-server
cargo check
```

**Expected:** All code compiles. Some tests may need updates, but compilation should succeed.

**Note:** This phase focuses on compilation. Test failures are expected and will be fixed as we update tests to use async.
