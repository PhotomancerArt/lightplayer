# Phase 4: Update main.rs to Use MessageRouterTransport

## Scope of Phase

Update `main.rs` to:
1. Initialize serial early for logging (handle failures gracefully)
2. Create `MessageRouter` with static channels
3. Spawn I/O task with USB device
4. Create `MessageRouterTransport` and pass to server loop
5. Ensure main loop starts even if serial initialization fails

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update main.rs imports

Add necessary imports:

```rust
use fw_core::message_router::MessageRouter;
use fw_core::transport::MessageRouterTransport;
use crate::serial::{get_message_channels, io_task};
```

### 2. Update serial initialization

Make serial initialization graceful (don't block if it fails):

```rust
// Try to initialize USB-serial for logging (but don't block if it fails)
let usb_serial_result = {
    // Try to create USB serial (may fail if no USB host connected)
    let usb_serial = UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    Ok(Rc::new(RefCell::new(serial_io)))
};

match usb_serial_result {
    Ok(serial_io_shared) => {
        // Serial initialized successfully - set up logging
        crate::logger::set_log_serial(serial_io_shared.clone());
        crate::logger::init(crate::logger::log_write_bytes);
        crate::logger::set_esp_println_serial(serial_io_shared.clone());
        unsafe {
            esp_println::set_custom_writer(crate::logger::esp_println_write_bytes);
        }
        info!("USB serial initialized for logging");
    }
    Err(e) => {
        // Serial initialization failed - log via esp_println (may be lost)
        esp_println::println!("[WARN] Failed to initialize USB serial for logging: {:?}", e);
        esp_println::println!("[WARN] Continuing without serial logging");
    }
}
```

**Note**: Actually, `UsbSerialJtag::new()` doesn't fail - it always succeeds. The issue is that it may not work properly without a USB host. We should still try to set up logging, but the I/O task will handle the actual serial communication.

Better approach: Initialize serial for logging as before, but don't let failures block. The I/O task will handle transport separately.

### 3. Create MessageRouter and spawn I/O task

After board initialization:

```rust
// Create message router with static channels
let (incoming_channel, outgoing_channel) = get_message_channels();
let router = MessageRouter::new(incoming_channel, outgoing_channel);

// Spawn I/O task (handles serial communication)
spawner.spawn(io_task(usb_device)).ok();

// Create transport wrapper
let transport = MessageRouterTransport::new(router);
```

### 4. Update server loop call

Replace `FakeTransport` with `MessageRouterTransport`:

```rust
// Run server loop (never returns)
run_server_loop(server, transport, time_provider).await;
```

### 5. Remove FakeTransport usage

Remove the code that creates `FakeTransport` and queues the `LoadProject` message. The server will start without any initial messages (or we can queue them via the router if needed).

## Validate

Run the following commands to validate:

```bash
# Check compilation
cargo check --package fw-esp32 --features esp32c6,server

# Verify no FakeTransport usage
grep -r "FakeTransport" lp-fw/fw-esp32/src/main.rs
# Should return nothing (or only in comments)
```

Ensure:
- `main.rs` compiles
- I/O task is spawned
- `MessageRouterTransport` is created and passed to server loop
- Serial initialization failures don't block startup
- `FakeTransport` is removed
