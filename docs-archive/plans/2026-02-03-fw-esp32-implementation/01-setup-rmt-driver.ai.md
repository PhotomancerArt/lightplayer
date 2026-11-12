# Phase 1: Set up RMT Driver Structure

## Scope of phase

Create the RMT driver module structure and adapt the reference implementation from lpmini2024. This phase sets up the low-level WS2811/WS2812 LED driver using ESP32's RMT peripheral.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Cargo.toml

Add `smart_leds` dependency for RGB8 type:

```toml
[dependencies]
# ... existing dependencies ...
smart_leds = "0.3"
```

### 2. Create output/rmt_driver.rs

Create the RMT driver module. Adapt the reference implementation from `/Users/yona/dev/photomancer/lpmini2024/apps/fw-esp32c3/src/rmt_ws2811_driver.rs`.

Key adaptations:
- Make it work with dynamic pin configuration (pin passed as parameter)
- Return a transaction handle that must be kept alive
- Support multiple channels (one per pin)
- Keep the interrupt-driven double buffering approach
- Use the same timing constants and pulse code generation

Structure:

```rust
//! RMT driver for WS2811/WS2812 LEDs
//!
//! Low-level driver using ESP32 RMT peripheral for WS2811/WS2812 protocol.
//! Uses interrupt-driven double buffering for efficient transmission.

use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::gpio::Level;
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::rmt::{Error as RmtError, TxChannel, TxChannelConfig, TxChannelCreator};
use esp_hal::Blocking;
use smart_leds::RGB8;

// Timing constants for WS2812/SK6812
const SK68XX_CODE_PERIOD: u32 = 1250; // 800kHz
const SK68XX_T0H_NS: u32 = 400;
const SK68XX_T0L_NS: u32 = SK68XX_CODE_PERIOD - SK68XX_T0H_NS;
const SK68XX_T1H_NS: u32 = 850;
const SK68XX_T1L_NS: u32 = SK68XX_CODE_PERIOD - SK68XX_T1H_NS;
const SK68XX_LATCH_NS: u32 = 50_000;

// Clock configuration
const SRC_CLOCK_MHZ: u32 = 80;

// Buffer configuration (double buffered)
const BUFFER_LEDS: usize = 8;
const HALF_BUFFER_LEDS: usize = BUFFER_LEDS / 2;
const BITS_PER_LED: usize = 3 * 8;
const HALF_BUFFER_SIZE: usize = (BUFFER_LEDS * BITS_PER_LED) / 2;
const BUFFER_SIZE: usize = BUFFER_LEDS * BITS_PER_LED;

// Pulse codes
const PULSE_ZERO: u32 = pulse_code(
    Level::High,
    ((SK68XX_T0H_NS * SRC_CLOCK_MHZ) / 1000) as u16,
    Level::Low,
    ((SK68XX_T0L_NS * SRC_CLOCK_MHZ) / 1000) as u16,
);

const PULSE_ONE: u32 = pulse_code(
    Level::High,
    ((SK68XX_T1H_NS * SRC_CLOCK_MHZ) / 1000) as u16,
    Level::Low,
    ((SK68XX_T1L_NS * SRC_CLOCK_MHZ) / 1000) as u16,
);

const PULSE_LATCH: u32 = pulse_code(
    Level::Low,
    ((SK68XX_LATCH_NS / 2 * SRC_CLOCK_MHZ) / 1000) as u16,
    Level::Low,
    ((SK68XX_LATCH_NS / 2 * SRC_CLOCK_MHZ) / 1000) as u16,
);

const fn pulse_code(level1: Level, length1: u16, level2: Level, length2: u16) -> u32 {
    let level1 = (level_bit(level1)) | (length1 as u32 & 0b111_1111_1111_1111);
    let level2 = (level_bit(level2)) | (length2 as u32 & 0b111_1111_1111_1111);
    level1 | (level2 << 16)
}

const fn level_bit(level: Level) -> u32 {
    match level {
        Level::Low => 0u32,
        Level::High => 1u32 << 15,
    }
}

/// Channel-specific state for RMT driver
struct RmtChannelState {
    num_leds: usize,
    led_data_buffer: *mut RGB8,
    frame_complete: bool,
    led_counter: usize,
}

// Global state: one channel state per RMT channel (channel 0 for now)
static mut CHANNEL_STATE: Option<RmtChannelState> = None;

/// Initialize RMT driver for WS2811/WS2812 LEDs
///
/// # Arguments
/// * `rmt` - RMT peripheral
/// * `pin` - GPIO pin for LED data output
/// * `num_leds` - Number of LEDs in the strip
///
/// # Returns
/// Transaction handle that must be kept alive for the driver to work
pub fn rmt_ws2811_init<'d, O>(
    mut rmt: esp_hal::rmt::Rmt<'d, Blocking>,
    pin: O,
    num_leds: usize,
) -> Result<impl core::marker::Sized + 'd, RmtError>
where
    O: PeripheralOutput<'d>,
{
    extern crate alloc;
    use alloc::boxed::Box;
    use alloc::vec;

    unsafe {
        // Allocate LED buffer
        let buffer = vec![RGB8 { r: 0, g: 0, b: 0 }; num_leds].into_boxed_slice();
        let buffer_ptr = Box::into_raw(buffer) as *mut RGB8;

        // Initialize channel state
        CHANNEL_STATE = Some(RmtChannelState {
            num_leds,
            led_data_buffer: buffer_ptr,
            frame_complete: true,
            led_counter: 0,
        });

        // Set up interrupt handler
        let handler = InterruptHandler::new(rmt_interrupt_handler, Priority::max());
        rmt.set_interrupt_handler(handler);

        // Configure RMT channel
        let config = create_rmt_config();
        let channel = rmt.channel0.configure_tx(pin, config)?;

        // HACK: Need to call transmit_continuously to initialize
        let dummy_buffer = [pulse_code(Level::Low, 1, Level::Low, 1)];
        let transaction = channel.transmit_continuously(&dummy_buffer)?;

        Ok(transaction)
    }
}

/// Write LED data and start transmission
///
/// # Arguments
/// * `rgb_bytes` - Raw RGB bytes (R,G,B,R,G,B,...) must be at least num_leds * 3 bytes
pub fn rmt_ws2811_write_bytes(rgb_bytes: &[u8]) {
    rmt_ws2811_wait_complete();

    unsafe {
        if let Some(state) = &mut CHANNEL_STATE {
            let buffer = core::slice::from_raw_parts_mut(state.led_data_buffer, state.num_leds);

            // Clear first
            for led in buffer.iter_mut() {
                *led = RGB8 { r: 0, g: 0, b: 0 };
            }

            // Convert from bytes to RGB8
            let num_leds = (rgb_bytes.len() / 3).min(state.num_leds);
            for i in 0..num_leds {
                let idx = i * 3;
                buffer[i] = RGB8 {
                    r: rgb_bytes[idx],
                    g: rgb_bytes[idx + 1],
                    b: rgb_bytes[idx + 2],
                };
            }

            // Start transmission
            start_transmission();
        }
    }
}

/// Wait for current frame transmission to complete
pub fn rmt_ws2811_wait_complete() {
    unsafe {
        if let Some(state) = &mut CHANNEL_STATE {
            while !state.frame_complete {
                // Small delay to avoid busy waiting
                esp_hal::delay::Delay::new().delay_micros(10);
            }
        }
    }
}

fn create_rmt_config() -> TxChannelConfig {
    TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output_level(Level::Low)
        .with_carrier_modulation(false)
        .with_idle_output(true)
        .with_memsize(4) // Use all 4 memory blocks - 192 words
}

// RMT interrupt handler
#[no_mangle]
extern "C" fn rmt_interrupt_handler() {
    unsafe {
        let rmt = esp_hal::peripherals::RMT::regs();

        let int_reg = rmt.int_raw().read();
        let is_end_int = int_reg.ch_tx_end(0).bit();
        let is_thresh_int = int_reg.ch_tx_thr_event(0).bit();
        let is_err_int = int_reg.ch_tx_err(0).bit();

        // Clear interrupts
        rmt.int_clr().write(|w| {
            w.ch_tx_end(0).set_bit();
            w.ch_tx_err(0).set_bit();
            w.ch_tx_loop(0).set_bit();
            w.ch_tx_thr_event(0).set_bit()
        });

        if is_err_int {
            // Error interrupt - log if needed
        } else if is_end_int {
            // End interrupt - frame complete
            if let Some(state) = &mut CHANNEL_STATE {
                state.frame_complete = true;
            }
        } else if is_thresh_int {
            // Threshold interrupt - refill buffer
            if let Some(state) = &mut CHANNEL_STATE {
                let hw_pos_start = rmt.ch_tx_status(0).read().mem_raddr_ex().bits();
                let is_halfway = hw_pos_start >= HALF_BUFFER_SIZE as u16;

                if is_halfway {
                    rmt.ch_tx_lim(0)
                        .modify(|_, w| w.tx_lim().bits(BUFFER_SIZE as u16));
                } else {
                    rmt.ch_tx_lim(0)
                        .modify(|_, w| w.tx_lim().bits(HALF_BUFFER_SIZE as u16));
                }

                write_buffer_guard(is_halfway);
                write_half_buffer(is_halfway, state);
            }
        }
    }
}

unsafe fn start_transmission() {
    if let Some(state) = &mut CHANNEL_STATE {
        state.frame_complete = false;
        state.led_counter = 0;
        let rmt = esp_hal::peripherals::RMT::regs();

        // Stop current transmission
        rmt.ch_tx_conf0(0)
            .modify(|_, w| w.tx_conti_mode().clear_bit());
        let rmt_base = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
        for j in 0..BUFFER_LEDS {
            rmt_base.add(j).write_volatile(0);
        }

        // Init the buffer
        write_half_buffer(true, state);
        write_half_buffer(false, state);

        // Clear interrupts
        rmt.int_clr().write(|w| {
            w.ch_tx_end(0).set_bit();
            w.ch_tx_err(0).set_bit();
            w.ch_tx_loop(0).set_bit();
            w.ch_tx_thr_event(0).set_bit()
        });

        // Enable interrupts
        rmt.int_ena().modify(|_, w| {
            w.ch_tx_thr_event(0).set_bit();
            w.ch_tx_end(0).set_bit();
            w.ch_tx_err(0).set_bit();
            w.ch_tx_loop(0).clear_bit()
        });

        // Set threshold for halfway
        rmt.ch_tx_lim(0).modify(|_, w| {
            w.loop_count_reset().set_bit();
            w.tx_loop_cnt_en().set_bit();
            w.tx_loop_num().bits(0);
            w.tx_lim().bits(HALF_BUFFER_SIZE as u16)
        });

        // Configure
        rmt.ch_tx_conf0(0).modify(|_, w| {
            w.tx_conti_mode().clear_bit();
            w.mem_tx_wrap_en().set_bit();
            w.conf_update().set_bit()
        });

        // Start
        rmt.ch_tx_conf0(0).modify(|_, w| {
            w.mem_rd_rst().set_bit();
            w.apb_mem_rst().set_bit();
            w.tx_start().set_bit()
        });

        rmt.ch_tx_conf0(0).modify(|_, w| w.conf_update().set_bit());

        // Write guard
        write_buffer_guard(false);
    }
}

unsafe fn write_buffer_guard(into_second_half: bool) {
    let base_ptr = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
    if into_second_half {
        base_ptr.add(HALF_BUFFER_SIZE).write_volatile(0);
    } else {
        base_ptr.write_volatile(0);
    }
}

unsafe fn write_half_buffer(is_first_half: bool, state: &mut RmtChannelState) -> bool {
    let base_ptr = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
    let half_ptr = base_ptr.add(if is_first_half { 0 } else { HALF_BUFFER_SIZE });

    for i in 0..HALF_BUFFER_LEDS {
        let led_ptr = half_ptr.add(i * BITS_PER_LED);

        if state.led_counter >= state.num_leds {
            // Fill rest with zero
            for j in 0..BITS_PER_LED * (HALF_BUFFER_LEDS - i) {
                led_ptr
                    .add(j)
                    .write_volatile(if j == 0 { PULSE_LATCH } else { 0 });
            }
            return true;
        } else {
            // Get RGB color from buffer
            let color = state.led_data_buffer.add(state.led_counter).read_volatile();

            // WS2812 uses GRB order
            write_ws2811_byte(led_ptr, color.g, 0);
            write_ws2811_byte(led_ptr, color.r, 1);
            write_ws2811_byte(led_ptr, color.b, 2);

            state.led_counter += 1;
        }
    }

    false
}

#[inline(always)]
unsafe fn write_ws2811_byte(base_ptr: *mut u32, byte_value: u8, byte_offset: usize) {
    let ptr = base_ptr.add(byte_offset * 8);

    let bit_pulse = |mask: u8| -> u32 {
        if byte_value & mask != 0 {
            PULSE_ONE
        } else {
            PULSE_ZERO
        }
    };

    ptr.add(0).write_volatile(bit_pulse(0x80));
    ptr.add(1).write_volatile(bit_pulse(0x40));
    ptr.add(2).write_volatile(bit_pulse(0x20));
    ptr.add(3).write_volatile(bit_pulse(0x10));
    ptr.add(4).write_volatile(bit_pulse(0x08));
    ptr.add(5).write_volatile(bit_pulse(0x04));
    ptr.add(6).write_volatile(bit_pulse(0x02));
    ptr.add(7).write_volatile(bit_pulse(0x01));
}
```

### 3. Update output/mod.rs

```rust
mod rmt_driver;

pub use rmt_driver::{rmt_ws2811_init, rmt_ws2811_write_bytes, rmt_ws2811_wait_complete};
```

## Notes

- The RMT driver uses unsafe code for direct hardware register access
- The transaction handle returned from `rmt_ws2811_init()` must be kept alive
- For now, we only support one channel (channel 0)
- The interrupt handler is global and handles channel 0
- Channel state is stored in a static mut Option (will be improved in next phase)

## Validate

Run:
```bash
cd lp-fw/fw-esp32
cargo check --features esp32c6
```

Expected: Code compiles without errors. Warnings about unused functions are OK for now.
