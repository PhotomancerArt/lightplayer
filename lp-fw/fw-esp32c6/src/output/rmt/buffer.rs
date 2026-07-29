//! Buffer writing helpers for RMT transmission
//!
//! This module contains low-level functions for writing LED data to the RMT buffer
//! in the format required by WS2811/WS2812 LEDs.

use crate::output::rmt::config::{
    BITS_PER_LED, HALF_BUFFER_LEDS, HALF_BUFFER_SIZE, PULSE_LATCH, PULSE_ONE, PULSE_ZERO,
};
use crate::output::rmt::state::CHANNEL_STATE;
use core::sync::atomic::Ordering;

/// Write a byte as RMT pulse-codes for a WS2812/SK6812 LED
///
/// Converts a single byte into 8 RMT pulse codes (one per bit) and writes them
/// to the RMT buffer memory.
///
/// # Arguments
/// * `base_ptr` - Base pointer to RMT buffer memory
/// * `byte_value` - The byte value to encode
/// * `byte_offset` - Offset in bytes (0, 1, or 2 for RGB)
#[inline(always)]
#[allow(
    unsafe_op_in_unsafe_fn,
    reason = "unsafe operations required for direct RMT memory access"
)]
pub(crate) unsafe fn write_ws2811_byte(base_ptr: *mut u32, byte_value: u8, byte_offset: usize) {
    let ptr = base_ptr.add(byte_offset * 8);

    // Loop unrolled for performance
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

/// Writes a STOP instruction into the RMT buffer at the start or halfway point.
///
/// This protects against the case where the RMT interrupt is lost when it's time to write the
/// next half of the buffer.
///
/// Without this protection, if the interrupt is lost, the RMT will continue transmitting the
/// current buffer over and over, causing the LED strip to flicker, and too much data will be
/// written to the strip.
///
/// While this is not ideal, it is necessary, especially in debug mode, where the RTT driver
/// seems to interfere with the RMT interrupts.
#[allow(
    unsafe_op_in_unsafe_fn,
    reason = "unsafe operations required for direct RMT memory access"
)]
pub(crate) unsafe fn write_buffer_guard(into_second_half: bool) {
    // Write a zero after our half buffer to guard against interrupt loss
    let base_ptr = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
    if into_second_half {
        base_ptr.add(HALF_BUFFER_SIZE).write_volatile(0);
    } else {
        base_ptr.write_volatile(0);
    }
}

/// Write half of the LED buffer to RMT memory
///
/// This function writes either the first or second half of the LED buffer to the RMT
/// memory, converting RGB colors to WS2812 pulse codes.
///
/// # Arguments
/// * `is_first_half` - If true, write first half; if false, write second half
/// * `channel_idx` - Channel index to read buffer info from
///
/// # Returns
/// `true` if the end of the LED strip was reached, `false` otherwise
#[allow(
    unsafe_op_in_unsafe_fn,
    reason = "unsafe operations required for direct RMT memory access"
)]
pub(crate) unsafe fn write_half_buffer(is_first_half: bool, channel_idx: u8) -> bool {
    let base_ptr = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
    let ch_idx = channel_idx as usize;

    let half_ptr = base_ptr.add(if is_first_half { 0 } else { HALF_BUFFER_SIZE });

    // Load buffer info from ChannelState
    let num_leds = CHANNEL_STATE[ch_idx].num_leds.load(Ordering::Acquire);
    let buffer_ptr = CHANNEL_STATE[ch_idx].led_buffer_ptr.load(Ordering::Acquire);

    for i in 0..HALF_BUFFER_LEDS {
        let led_ptr = half_ptr.add(i * BITS_PER_LED);

        // Load current LED counter atomically
        let led_counter = CHANNEL_STATE[ch_idx].led_counter.load(Ordering::Acquire);

        if led_counter >= num_leds {
            // Fill the rest of the buffer segment with zero
            for j in 0..BITS_PER_LED * (HALF_BUFFER_LEDS - i) {
                led_ptr
                    .add(j)
                    .write_volatile(if j == 0 { PULSE_LATCH } else { 0 });
            }

            return true;
        } else {
            // Get RGB color from LED data buffer and write directly to RMT buffer
            // Use volatile read to ensure we get the latest data
            let color = buffer_ptr.add(led_counter).read_volatile();

            // WS2812 uses GRB order
            write_ws2811_byte(led_ptr, color.g, 0); // Green first
            write_ws2811_byte(led_ptr, color.r, 1); // Red second
            write_ws2811_byte(led_ptr, color.b, 2); // Blue third

            // Increment LED counter atomically
            CHANNEL_STATE[ch_idx]
                .led_counter
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    false
}
