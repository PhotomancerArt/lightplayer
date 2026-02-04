use core::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

extern crate alloc;
use alloc::boxed::Box;

use esp_hal::Blocking;
use esp_hal::gpio::Level;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::rmt::{
    Channel, Error as RmtError, LoopMode, Rmt, Tx, TxChannelConfig, TxChannelCreator,
};
use esp_println::println;
use smart_leds::RGB8;

// Configuration constants
static mut ACTUAL_NUM_LEDS: usize = 0; // Actual number of LEDs (set at init)
static mut LED_DATA_BUFFER_PTR: *mut RGB8 = core::ptr::null_mut(); // Dynamically allocatedar

// Buffer size for 8 LEDs worth of data (double buffered)
// Using memsize(4) = 192 words. 8 LEDs = 192 words exactly, 4 LEDs per half
const BUFFER_LEDS: usize = 8;
const HALF_BUFFER_LEDS: usize = BUFFER_LEDS / 2;
const BITS_PER_LED: usize = 3 * 8;
const HALF_BUFFER_SIZE: usize = (BUFFER_LEDS * BITS_PER_LED) / 2;
const BUFFER_SIZE: usize = BUFFER_LEDS * BITS_PER_LED;

// Channel index constant (easy to find and change later for multi-channel support)
const RMT_CH_IDX: usize = 0;

/// Per-channel state for interrupt handler coordination
#[derive(Debug)]
struct ChannelState {
    frame_complete: AtomicBool,
    led_counter: AtomicUsize,
    frame_counter: AtomicUsize,
    stats_count: AtomicI32,
    stats_sum: AtomicI32,
}

impl ChannelState {
    const fn new() -> Self {
        Self {
            frame_complete: AtomicBool::new(true),
            led_counter: AtomicUsize::new(0),
            frame_counter: AtomicUsize::new(0),
            stats_count: AtomicI32::new(0),
            stats_sum: AtomicI32::new(0),
        }
    }
}

// Helper function to create array of ChannelState (can't use [ChannelState::new(); 2] because atomics aren't Copy)
const fn make_channel_state_array() -> [ChannelState; 2] {
    [ChannelState::new(), ChannelState::new()]
}

// Global state for interrupt handling (one per channel, currently only [0] used)
static CHANNEL_STATE: [ChannelState; 2] = make_channel_state_array();

/// LED channel for WS2811/WS2812 LEDs using RMT
pub struct LedChannel<'ch> {
    channel: Channel<'ch, Blocking, Tx>,
    channel_idx: u8,
    num_leds: usize,
    led_buffer: Box<[RGB8]>,
}

impl<'ch> LedChannel<'ch> {
    /// Create a new LED channel
    ///
    /// # Arguments
    /// * `rmt` - RMT peripheral (takes ownership, will set interrupt handler if first channel)
    /// * `pin` - GPIO pin for LED data output
    /// * `num_leds` - Number of LEDs in the strip
    ///
    /// # Returns
    /// `LedChannel` instance that owns the RMT channel
    pub fn new<O>(mut rmt: Rmt<'ch, Blocking>, pin: O, num_leds: usize) -> Result<Self, RmtError>
    where
        O: PeripheralOutput<'ch>,
    {
        use alloc::vec;

        // Set up interrupt handler (only needs to be done once, but safe to call multiple times)
        // TODO: Use a static flag to only set up once
        let handler = InterruptHandler::new(rmt_interrupt_handler, Priority::max());
        rmt.set_interrupt_handler(handler);

        // Configure the RMT channel (takes ownership of channel0)
        let config = create_rmt_config();
        let channel = rmt.channel0.configure_tx(pin, config)?;

        // Allocate LED buffer
        let led_buffer = vec![RGB8 { r: 0, g: 0, b: 0 }; num_leds].into_boxed_slice();

        // Initialize RMT memory with zeros
        let rmt_base = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
        unsafe {
            for j in 0..BUFFER_SIZE {
                rmt_base.add(j).write_volatile(0);
            }
        }

        // Enable interrupts
        let rmt_regs = esp_hal::peripherals::RMT::regs();
        rmt_regs.int_ena().modify(|_, w| {
            w.ch_tx_thr_event(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_end(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_err(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_loop(RMT_CH_IDX as u8).clear_bit()
        });

        // Set initial threshold configuration
        rmt_regs.ch_tx_lim(RMT_CH_IDX).modify(|_, w| {
            w.loop_count_reset().set_bit();
            w.tx_loop_cnt_en().set_bit();
            unsafe {
                w.tx_loop_num().bits(0);
                w.tx_lim().bits(HALF_BUFFER_SIZE as u16)
            }
        });

        // Configure initial channel settings (single-shot, wrap enabled)
        rmt_regs.ch_tx_conf0(RMT_CH_IDX).modify(|_, w| {
            w.tx_conti_mode().clear_bit(); // single-shot
            w.mem_tx_wrap_en().set_bit() // wrap enabled
        });

        // Update configuration
        rmt_regs
            .ch_tx_conf0(RMT_CH_IDX)
            .modify(|_, w| w.conf_update().set_bit());

        // Set up globals for backward compatibility with old API
        // TODO: Remove this when old API is removed
        unsafe {
            ACTUAL_NUM_LEDS = num_leds;
            LED_DATA_BUFFER_PTR = led_buffer.as_ptr() as *mut RGB8;
        }

        Ok(Self {
            channel,
            channel_idx: RMT_CH_IDX as u8,
            num_leds,
            led_buffer,
        })
    }
}

const SRC_CLOCK_MHZ: u32 = 80;
const PULSE_ZERO: u32 = // Zero
    pulse_code(
        Level::High,
        ((SK68XX_T0H_NS * SRC_CLOCK_MHZ) / 1000) as u16,
        Level::Low,
        ((SK68XX_T0L_NS * SRC_CLOCK_MHZ) / 1000) as u16,
    );

// One
const PULSE_ONE: u32 = pulse_code(
    Level::High,
    ((SK68XX_T1H_NS * SRC_CLOCK_MHZ) / 1000) as u16,
    Level::Low,
    ((SK68XX_T1L_NS * SRC_CLOCK_MHZ) / 1000) as u16,
);

// Latch - WS2812 requires 50us+ low to latch
// At 80MHz: 50us = 4000 ticks, split as 2000+2000
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

/// Write a byte as RMT pulse-codes for a WS2812/SK6812 LED
#[inline(always)]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn write_ws2811_byte(base_ptr: *mut u32, byte_value: u8, byte_offset: usize) {
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

// Start transmission of current LED buffer
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn start_transmission() {
    let rmt = esp_hal::peripherals::RMT::regs();

    rmt.ch_tx_conf0(0).modify(|_, w| w.tx_stop().set_bit());
    rmt.ch_tx_conf0(0).modify(|_, w| w.conf_update().set_bit());

    CHANNEL_STATE[RMT_CH_IDX]
        .frame_complete
        .store(false, Ordering::Release);
    CHANNEL_STATE[RMT_CH_IDX]
        .led_counter
        .store(0, Ordering::Relaxed);

    // Clear the buffer
    let rmt_base = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
    for j in 0..BUFFER_LEDS {
        rmt_base.add(j).write_volatile(0);
    }

    println!("start_transmission: buffer cleared");
    // Init the buffer
    write_half_buffer(true);
    write_half_buffer(false);

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

    // Set the threshold for halfway (like esp-hal start_send does)
    rmt.ch_tx_lim(0).modify(|_, w| {
        w.loop_count_reset().set_bit();
        w.tx_loop_cnt_en().set_bit();
        w.tx_loop_num().bits(0);

        w.tx_lim().bits(HALF_BUFFER_SIZE as u16)
    });

    // Configure (like esp-hal start_send - set continuous mode, wrap mode)
    rmt.ch_tx_conf0(0).modify(|_, w| {
        w.tx_conti_mode().clear_bit(); // single-shot (not continuous)
        w.mem_tx_wrap_en().set_bit() // wrap enabled for single-shot
    });

    // Update configuration (like esp-hal does BEFORE start_tx)
    rmt.ch_tx_conf0(0).modify(|_, w| w.conf_update().set_bit());

    println!("start_transmission: configuration updated");

    // Start transmission (like esp-hal start_tx)
    rmt.ch_tx_conf0(0).modify(|_, w| {
        w.mem_rd_rst().set_bit();
        w.apb_mem_rst().set_bit();
        w.tx_start().set_bit()
    });

    println!("start_transmission: transmission started");

    // Update again after starting (like esp-hal does)
    rmt.ch_tx_conf0(0).modify(|_, w| w.conf_update().set_bit());

    // Write the guard. With any luck we are past the first byte at this point.
    write_buffer_guard(false);

    println!("transmission started");
}

// Check if current frame transmission is complete
fn is_frame_complete() -> bool {
    CHANNEL_STATE[RMT_CH_IDX]
        .frame_complete
        .load(Ordering::Acquire)
}

// LED timing constants for WS2812/SK6812
const SK68XX_CODE_PERIOD: u32 = 1250; // 800kHz
const SK68XX_T0H_NS: u32 = 400;
const SK68XX_T0L_NS: u32 = SK68XX_CODE_PERIOD - SK68XX_T0H_NS;
const SK68XX_T1H_NS: u32 = 850;
const SK68XX_T1L_NS: u32 = SK68XX_CODE_PERIOD - SK68XX_T1H_NS;

const SK68XX_LATCH_NS: u32 = 50_000;

fn create_rmt_config() -> TxChannelConfig {
    TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output_level(Level::Low)
        .with_carrier_modulation(false)
        .with_idle_output(true)
        .with_memsize(4) // Use all 4 memory blocks - 192 words
}

// RMT interrupt handler - this is where the magic happens
// CRITICAL: Keep this as fast as possible to prevent timing disruption
#[unsafe(no_mangle)]
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

        // println!(
        //     "rmt_interrupt_handler: is_end_int: {}, is_thresh_int: {}, is_err_int: {}",
        //     is_end_int, is_thresh_int, is_err_int
        // );

        if is_err_int {
            // info!("error interrupt");
        }
        // End interrupt
        else if is_end_int {
            // info!("end interrupt");

            // Signal that we've reached the end of the frame
            // On the next interrupt, this will stop the transmission
            CHANNEL_STATE[RMT_CH_IDX]
                .frame_complete
                .store(true, Ordering::Release);
            CHANNEL_STATE[RMT_CH_IDX]
                .frame_counter
                .fetch_add(1, Ordering::Relaxed);
        } else if is_thresh_int {
            // info!("loop: {}, threshold: {}", is_loop_int, is_thresh_int);

            // Current position of the buffer
            let hw_pos_start = rmt.ch_tx_status(0).read().mem_raddr_ex().bits();

            let is_halfway = hw_pos_start >= HALF_BUFFER_SIZE as u16;
            // info!("pos: {}", hw_pos_start);

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
            if write_half_buffer(is_halfway) {
                // info!("end reached");
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

/// Writes a STOP instruction into the RMT buffer at the start or halfway point.
///
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
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn write_buffer_guard(into_second_half: bool) {
    // Write a zero after our half buffer to guard against interrupt loss
    let base_ptr = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
    if into_second_half {
        base_ptr.add(HALF_BUFFER_SIZE).write_volatile(0);
    } else {
        base_ptr.write_volatile(0);
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn write_half_buffer(is_first_half: bool) -> bool {
    let base_ptr = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;

    let half_ptr = base_ptr.add(if is_first_half { 0 } else { HALF_BUFFER_SIZE });

    for i in 0..HALF_BUFFER_LEDS {
        let led_ptr = half_ptr.add(i * BITS_PER_LED);

        // Load current LED counter atomically
        let led_counter = CHANNEL_STATE[RMT_CH_IDX]
            .led_counter
            .load(Ordering::Acquire);

        if led_counter >= ACTUAL_NUM_LEDS {
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
            let color = LED_DATA_BUFFER_PTR.add(led_counter).read_volatile();

            // WS2812 uses GRB order
            write_ws2811_byte(led_ptr, color.g, 0); // Green first
            write_ws2811_byte(led_ptr, color.r, 1); // Red second
            write_ws2811_byte(led_ptr, color.b, 2); // Blue third

            // Increment LED counter atomically
            CHANNEL_STATE[RMT_CH_IDX]
                .led_counter
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    false
}

/// Initialize the WS2811/WS2812 LED driver
///
/// # Arguments
/// * `rmt` - RMT peripheral
/// * `pin` - GPIO pin for LED data output
/// * `num_leds` - Number of LEDs in the strip
///
/// # Returns
/// Transaction handle that must be kept alive
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
        ACTUAL_NUM_LEDS = num_leds;

        // Allocate LED buffer dynamically
        let buffer = vec![RGB8 { r: 0, g: 0, b: 0 }; num_leds].into_boxed_slice();
        LED_DATA_BUFFER_PTR = Box::into_raw(buffer) as *mut RGB8;
    }

    // Set up the interrupt handler with max priority
    let handler = InterruptHandler::new(rmt_interrupt_handler, Priority::max());
    rmt.set_interrupt_handler(handler);

    // Configure the RMT channel
    let config = create_rmt_config();
    let channel = rmt.channel0.configure_tx(pin, config)?;

    // HACK: If we don't call transmit_continuously, things work, but the debug output stops
    //       working.
    //
    let dummy_buffer = [pulse_code(Level::Low, 1, Level::High, 1)];
    let transaction = channel.transmit_continuously(&dummy_buffer, LoopMode::Infinite)?;

    Ok(transaction)
}

pub fn rmt_ws2811_init2<'d, O>(
    mut rmt: esp_hal::rmt::Rmt<'d, Blocking>,
    pin: O,
    num_leds: usize,
) -> Result<(), RmtError>
where
    O: PeripheralOutput<'d>,
{
    // Update globals for old API compatibility
    unsafe {
        ACTUAL_NUM_LEDS = num_leds;
    }

    // Create LedChannel (takes ownership of rmt)
    let channel = LedChannel::new(rmt, pin, num_leds)?;

    // Store channel in static to keep it alive (for backward compatibility with old API)
    // TODO: Remove this when old API is removed
    unsafe {
        // Update LED_DATA_BUFFER_PTR for old API
        LED_DATA_BUFFER_PTR = channel.led_buffer.as_ptr() as *mut RGB8;
        // Leak the channel to keep it alive
        // CRITICAL: Keep the channel alive by leaking it
        // If the channel is dropped, it will deconfigure the RMT hardware
        Box::leak(Box::new(channel));
    }

    Ok(())
}

/// Write LED data and start transmission from raw RGB bytes
///
/// # Arguments
/// * `rgb_bytes` - Raw RGB bytes (R,G,B,R,G,B,...) must be at least num_leds * 3 bytes
pub fn rmt_ws2811_write_bytes(rgb_bytes: &[u8]) {
    rmt_ws2811_wait_complete();

    unsafe {
        let buffer = core::slice::from_raw_parts_mut(LED_DATA_BUFFER_PTR, ACTUAL_NUM_LEDS);

        // Clear first
        for led in buffer.iter_mut() {
            *led = RGB8 { r: 0, g: 0, b: 0 };
        }

        // Convert from bytes to RGB8 as we copy
        let num_leds = (rgb_bytes.len() / 3).min(ACTUAL_NUM_LEDS);
        for i in 0..num_leds {
            let idx = i * 3;
            buffer[i] = RGB8 {
                r: rgb_bytes[idx],
                g: rgb_bytes[idx + 1],
                b: rgb_bytes[idx + 2],
            };
        }

        // Memory fence to ensure buffer writes complete before starting transmission
        // This prevents RTT or other interrupts from causing partial reads
        // core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

        // Start transmission
        start_transmission();
    }
}

/// Wait for the current frame transmission to complete
pub fn rmt_ws2811_wait_complete() {
    //println!("rmt_ws2811_wait_complete: waiting for frame complete");
    while !is_frame_complete() {
        // Small delay to avoid busy waiting
        esp_hal::delay::Delay::new().delay_micros(10);
    }
}
