//! RMT interrupt handler and transmission control
//!
//! This module contains the interrupt handler that manages double-buffered transmission
//! and the function to start a new transmission.

use crate::output::rmt::buffer::{write_buffer_guard, write_half_buffer};
use crate::output::rmt::config::{BUFFER_LEDS, BUFFER_SIZE, HALF_BUFFER_SIZE, RMT_CH_IDX};
use crate::output::rmt::state::CHANNEL_STATE;
use core::sync::atomic::Ordering;
use smart_leds::RGB8;

/// Start a transmission with the given buffer
///
/// This function configures the RMT hardware and starts the transmission of LED data.
/// The buffer information is stored in ChannelState for the interrupt handler to access.
///
/// # Safety
/// This function is unsafe because it performs raw pointer operations and direct register access.
#[allow(
    unsafe_op_in_unsafe_fn,
    reason = "unsafe operations required for direct RMT register access"
)]
// Used internally by LedChannel::start_transmission
#[allow(dead_code, reason = "used internally by LedChannel")]
pub(crate) unsafe fn start_transmission_with_state(
    channel_idx: u8,
    led_buffer_ptr: *mut RGB8,
    num_leds: usize,
) {
    let rmt = esp_hal::peripherals::RMT::regs();
    let ch_idx = channel_idx as usize;

    // Store buffer info in ChannelState for interrupt handler
    CHANNEL_STATE[ch_idx]
        .led_buffer_ptr
        .store(led_buffer_ptr, Ordering::Release);
    CHANNEL_STATE[ch_idx]
        .num_leds
        .store(num_leds, Ordering::Release);

    rmt.ch_tx_conf0(ch_idx).modify(|_, w| w.tx_stop().set_bit());
    rmt.ch_tx_conf0(ch_idx)
        .modify(|_, w| w.conf_update().set_bit());

    CHANNEL_STATE[ch_idx]
        .frame_complete
        .store(false, Ordering::Release);
    CHANNEL_STATE[ch_idx]
        .led_counter
        .store(0, Ordering::Relaxed);

    // Clear the buffer
    let rmt_base = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
    for j in 0..BUFFER_LEDS {
        rmt_base.add(j).write_volatile(0);
    }

    //log::trace!("start_transmission: buffer cleared");
    // Init the buffer
    write_half_buffer(true, channel_idx);
    write_half_buffer(false, channel_idx);

    // Clear interrupts
    rmt.int_clr().write(|w| {
        w.ch_tx_end(channel_idx).set_bit();
        w.ch_tx_err(channel_idx).set_bit();
        w.ch_tx_loop(channel_idx).set_bit();
        w.ch_tx_thr_event(channel_idx).set_bit()
    });

    // Enable interrupts
    rmt.int_ena().modify(|_, w| {
        w.ch_tx_thr_event(channel_idx).set_bit();
        w.ch_tx_end(channel_idx).set_bit();
        w.ch_tx_err(channel_idx).set_bit();
        w.ch_tx_loop(channel_idx).clear_bit()
    });

    // Set the threshold for halfway (like esp-hal start_send does)
    rmt.ch_tx_lim(ch_idx).modify(|_, w| {
        w.loop_count_reset().set_bit();
        w.tx_loop_cnt_en().set_bit();
        unsafe {
            w.tx_loop_num().bits(0);
            w.tx_lim().bits(HALF_BUFFER_SIZE as u16)
        }
    });

    // Configure (like esp-hal start_send - set continuous mode, wrap mode)
    rmt.ch_tx_conf0(ch_idx).modify(|_, w| {
        w.tx_conti_mode().clear_bit(); // single-shot (not continuous)
        w.mem_tx_wrap_en().set_bit() // wrap enabled for single-shot
    });

    // Update configuration (like esp-hal does BEFORE start_tx)
    rmt.ch_tx_conf0(ch_idx)
        .modify(|_, w| w.conf_update().set_bit());

    //log::trace!("start_transmission: configuration updated");

    // Start transmission (like esp-hal start_tx)
    rmt.ch_tx_conf0(ch_idx).modify(|_, w| {
        w.mem_rd_rst().set_bit();
        w.apb_mem_rst().set_bit();
        w.tx_start().set_bit()
    });

    //log::trace!("start_transmission: transmission started");

    // Update again after starting (like esp-hal does)
    rmt.ch_tx_conf0(ch_idx)
        .modify(|_, w| w.conf_update().set_bit());

    // Write the guard. With any luck we are past the first byte at this point.
    write_buffer_guard(false);

    //log::trace!("transmission started");
}

/// RMT interrupt handler - this is where the magic happens
///
/// CRITICAL: Keep this as fast as possible to prevent timing disruption
///
/// This handler manages double-buffered transmission by:
/// 1. Handling threshold interrupts to write the next half-buffer
/// 2. Handling end interrupts to signal frame completion
/// 3. Handling error interrupts (currently just logged)
#[unsafe(no_mangle)]
pub(crate) extern "C" fn rmt_interrupt_handler() {
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
            // Error interrupt - could log here if needed
        }
        // End interrupt
        else if is_end_int {
            // Signal that we've reached the end of the frame
            CHANNEL_STATE[RMT_CH_IDX]
                .frame_complete
                .store(true, Ordering::Release);
            CHANNEL_STATE[RMT_CH_IDX]
                .frame_counter
                .fetch_add(1, Ordering::Relaxed);
        } else if is_thresh_int {
            // Threshold interrupt - time to write the next half-buffer

            // Current position of the buffer
            let hw_pos_start = rmt.ch_tx_status(0).read().mem_raddr_ex().bits();

            let is_halfway = hw_pos_start >= HALF_BUFFER_SIZE as u16;

            if is_halfway {
                // Set the threshold for end
                rmt.ch_tx_lim(0)
                    .modify(|_, w| w.tx_lim().bits(BUFFER_SIZE as u16));
            } else {
                // Set the threshold for halfway
                rmt.ch_tx_lim(0)
                    .modify(|_, w| w.tx_lim().bits(HALF_BUFFER_SIZE as u16));
            }

            write_buffer_guard(is_halfway);
            if write_half_buffer(is_halfway, RMT_CH_IDX as u8) {
                // End reached
            }

            let hw_pos_end = rmt.ch_tx_status(0).read().mem_raddr_ex().bits();

            let bytes_elapsed = (hw_pos_end as i32) - (hw_pos_start as i32);
            CHANNEL_STATE[RMT_CH_IDX]
                .stats_sum
                .fetch_add(bytes_elapsed, Ordering::Relaxed);
            CHANNEL_STATE[RMT_CH_IDX]
                .stats_count
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}
