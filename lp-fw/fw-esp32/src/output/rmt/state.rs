//! Per-channel state management for interrupt handler coordination
//!
//! This module manages the shared state between the main thread and the interrupt handler.
//! All state is stored atomically to ensure thread-safe access.

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicUsize};
use smart_leds::RGB8;

/// Per-channel state for interrupt handler coordination
#[derive(Debug)]
pub(crate) struct ChannelState {
    /// Flag indicating if the current frame transmission is complete
    pub(crate) frame_complete: AtomicBool,
    /// Current LED position in the transmission
    pub(crate) led_counter: AtomicUsize,
    /// Counter for completed frames
    pub(crate) frame_counter: AtomicUsize,
    /// Statistics: number of threshold interrupts
    pub(crate) stats_count: AtomicI32,
    /// Statistics: sum of bytes elapsed per interrupt
    pub(crate) stats_sum: AtomicI32,
    /// Pointer to the current LED buffer (for interrupt handler)
    pub(crate) led_buffer_ptr: AtomicPtr<RGB8>,
    /// Number of LEDs in the current transmission
    pub(crate) num_leds: AtomicUsize,
}

impl ChannelState {
    const fn new() -> Self {
        Self {
            frame_complete: AtomicBool::new(true),
            led_counter: AtomicUsize::new(0),
            frame_counter: AtomicUsize::new(0),
            stats_count: AtomicI32::new(0),
            stats_sum: AtomicI32::new(0),
            led_buffer_ptr: AtomicPtr::new(core::ptr::null_mut()),
            num_leds: AtomicUsize::new(0),
        }
    }
}

// Helper function to create array of ChannelState (can't use [ChannelState::new(); 2] because atomics aren't Copy)
const fn make_channel_state_array() -> [ChannelState; 2] {
    [ChannelState::new(), ChannelState::new()]
}

/// Global state for interrupt handling (one per channel, currently only [0] used)
pub(crate) static CHANNEL_STATE: [ChannelState; 2] = make_channel_state_array();
