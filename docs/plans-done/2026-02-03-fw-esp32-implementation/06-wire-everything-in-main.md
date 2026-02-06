# Phase 6: Wire Everything in main.rs

## Scope of phase

Initialize all components in main.rs and start the server loop. This is the integration phase where everything comes together.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update main.rs

Wire everything together:

```rust
//! ESP32 firmware application.
//!
//! This binary is the main entry point for LightPlayer server firmware running on
//! ESP32 microcontrollers. It initializes the hardware, sets up serial communication,
//! and runs the LightPlayer server loop.

#![no_std]
#![no_main]

mod board;
mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use board::{init_board, start_runtime};
use esp_backtrace as _;
use esp_hal::peripherals::Peripherals;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::Async;
use esp_println::println;
use fw_core::log::init_esp32_logger;
use fw_core::transport::SerialTransport;
use lp_model::AsLpPath;
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;

use output::Esp32OutputProvider;
use serial::Esp32UsbSerialIo;
use server_loop::run_server_loop;
use time::Esp32TimeProvider;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    // Initialize logger with esp_println
    init_esp32_logger(|s| {
        esp_println::println!("{}", s);
    });

    println!("fw-esp32 starting...");

    // Initialize board (clock, heap, runtime)
    let (sw_int, timg0) = init_board();
    start_runtime(timg0, sw_int);

    // Get peripherals
    let peripherals = Peripherals::take();

    // Initialize USB-serial
    let usb_serial = UsbSerialJtag::new(peripherals.USB_SERIAL_JTAG);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let transport = SerialTransport::new(serial_io);

    // Initialize RMT peripheral for output
    let rmt = esp_hal::rmt::Rmt::new(peripherals.RMT, esp_hal::rmt::RmtConfig::default());
    
    // Initialize output provider
    let output_provider: Rc<RefCell<dyn OutputProvider>> =
        Rc::new(RefCell::new(Esp32OutputProvider::new(rmt)));

    // Create filesystem (in-memory for now)
    let base_fs = Box::new(LpFsMemory::new());

    // Create server
    let server = LpServer::new(output_provider, base_fs, "projects/".as_path());

    // Create time provider
    let time_provider = Esp32TimeProvider::new();

    println!("fw-esp32 initialized, starting server loop...");

    // Run server loop (never returns)
    run_server_loop(server, transport, time_provider).await;
}
```

### 2. Fix OutputProvider RMT initialization

The challenge from Phase 2: we need to initialize RMT channels when `open()` is called, but we don't have access to RMT peripheral there.

**Solution**: Store RMT in a static or pass it differently. Options:

**Option A**: Store RMT in a static (unsafe, but works):
```rust
// In output/provider.rs
static mut RMT_PERIPHERAL: Option<Rmt<'static, Blocking>> = None;

pub fn set_rmt(rmt: Rmt<'static, Blocking>) {
    unsafe {
        RMT_PERIPHERAL = Some(rmt);
    }
}
```

**Option B**: Use a different approach - initialize RMT channels upfront for common pins (not flexible)

**Option C**: Refactor RMT driver to not consume RMT (if possible)

For now, we'll use Option A (static) as it's the simplest. We can refactor later if needed.

Update `output/provider.rs`:

```rust
// Add static RMT storage
static mut RMT_PERIPHERAL: Option<esp_hal::rmt::Rmt<'static, esp_hal::Blocking>> = None;

impl Esp32OutputProvider {
    /// Set the RMT peripheral (must be called before opening channels)
    pub fn set_rmt(rmt: esp_hal::rmt::Rmt<'static, esp_hal::Blocking>) {
        unsafe {
            RMT_PERIPHERAL = Some(rmt);
        }
    }

    // ... rest of implementation ...
    
    fn open(...) -> Result<OutputChannelHandle, OutputError> {
        // ... validation ...
        
        // Get RMT peripheral
        let rmt = unsafe {
            RMT_PERIPHERAL.as_mut()
                .ok_or_else(|| OutputError::HardwareError("RMT not initialized".into()))?
        };
        
        // TODO: Get GPIO pin from pin number
        // This is tricky - we need to convert pin number to GPIO
        // For now, we'll need to pass GPIO pins differently
        
        // Initialize RMT channel
        // let transaction = rmt_ws2811_init(rmt, pin_gpio, num_leds)?;
        
        // ... rest of implementation ...
    }
}
```

**Note**: Converting pin number to GPIO is also a challenge. We may need to:
1. Store GPIO pins in a static map
2. Use a different API that takes pin numbers directly
3. Initialize GPIO pins upfront

For now, we'll use a simplified approach and document the limitation.

### 3. Update board/esp32c6.rs

Return peripherals if needed, or access them differently:

```rust
// Update init_board to return peripherals or make them accessible
// This depends on esp-hal API - may need to adjust
```

## Notes

- RMT initialization is complex - we need RMT peripheral and GPIO pins
- We'll use static storage for RMT as a workaround
- GPIO pin conversion may need special handling
- This phase integrates everything, but some parts may need refinement

## Validate

Run:
```bash
cd lp-fw/fw-esp32
cargo check --features esp32c6
```

Expected: Code compiles. Some parts may need adjustment based on actual esp-hal API.
