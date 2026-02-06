# Phase 5: Integrate Serial Transport into client_connect

## Scope of phase

Update `client_connect()` to handle `HostSpecifier::Serial` by using the serial port detection module and creating the hardware serial transport.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update client_connect

**File**: `lp-cli/src/client/client_connect.rs`

Add import:

```rust
#[cfg(feature = "serial")]
use crate::client::serial_port::detect_serial_port;
#[cfg(feature = "serial")]
use lp_client::transport_serial::create_hardware_serial_transport_pair;
```

Update the `HostSpecifier::Serial` match arm:

```rust
#[cfg(feature = "serial")]
HostSpecifier::Serial { port, baud_rate } => {
    // Detect/select serial port
    let port_config = detect_serial_port(port.as_deref(), *baud_rate)
        .context("Failed to detect serial port")?;

    // Create hardware serial transport
    let transport = create_hardware_serial_transport_pair(
        &port_config.port,
        port_config.baud_rate,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create serial transport: {e}"))?;

    Ok(Box::new(transport))
}
#[cfg(not(feature = "serial"))]
HostSpecifier::Serial { .. } => {
    bail!("Serial transport requires 'serial' feature to be enabled");
}
```

### 2. Update Error Messages

Update the error message in `HostSpecifier::parse()` to include baud rate syntax:

```rust
bail!(
    "Invalid host specifier: '{s}'. Supported formats: ws://host:port/, wss://host:port/, serial:auto, serial:/dev/ttyUSB1, serial:/dev/cu.usbmodem2101?baud=115200, local, emu"
)
```

### 3. Update Tests

**File**: `lp-cli/src/client/client_connect.rs`

Update or add tests:

```rust
#[test]
#[cfg(feature = "serial")]
fn test_client_connect_serial_auto() {
    // This test would require a serial port, so may need to be skipped
    // or use a mock. For now, just verify it parses correctly.
    let spec = HostSpecifier::parse("serial:auto").unwrap();
    // Note: actual connection will fail without a real port, but parsing should work
}

#[test]
fn test_client_connect_serial_with_baud() {
    let spec = HostSpecifier::parse("serial:/dev/cu.usbmodem2101?baud=115200").unwrap();
    // Verify parsing works
    assert!(spec.is_serial());
}
```

## Validate

Run the following commands to validate the phase:

```bash
cd lp-cli
cargo check --features serial
cargo test --features serial
```

Fix any warnings or errors before proceeding.

Note: Integration tests that require actual hardware may be skipped. The main validation is that the code compiles and the connection logic is correct.
