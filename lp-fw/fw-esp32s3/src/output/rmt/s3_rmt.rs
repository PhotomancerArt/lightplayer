//! ESP32-S3 RMT backend for [`lp_ws281x::RmtHw`].
//!
//! All the chip knowledge in this firmware lives here: the RMT RAM address, the
//! `CH_TX_CONF0` start/stop dance, the interrupt-register bit layout, and the
//! fact that `MEM_RADDR_EX` is an offset into the *whole* RMT RAM rather than
//! into the channel's own window. Everything else — what to write and when —
//! is [`lp_ws281x::Ws281xDriver`], which never sees a register name.
//!
//! # Where the RAM is
//!
//! The ESP32-S3 RMT RAM sits at **`RMT_BASE + 0x800` = `0x6001_6800`**, *not*
//! at the `+0x400` the ESP32-C6 uses (the C6's RMT base is `0x6000_6000` and
//! its RAM starts at `0x6000_6400`). The S3 has eight 48-word blocks against
//! the C6's four, and the register block in front of them is correspondingly
//! larger. Carrying the C6 constant over would write into the tail of the
//! register file and transmit whatever the RAM happened to hold.
//!
//! Value taken from `esp-metadata-generated` v0.4.0 (`rmt.ram_start` for
//! `esp32s3` = `1610704896` = `0x6001_6800`; MIT/Apache-2.0, the same table
//! esp-hal's own RMT driver reads), and confirmed on the bench by
//! [`probe_ram_address`], which makes the peripheral itself write a word
//! through the APB FIFO port and checks that it lands at this address. The
//! probe is compiled only into the `test_loopback` harness — the app build has
//! no use for it, and the harness proves the offset end to end anyway (a wrong
//! offset cannot decode a single frame).
//!
//! # Register names that differ from the C6
//!
//! * `INT_RAW`/`INT_ST`/`INT_ENA`/`INT_CLR` are laid out **by event, then by
//!   channel**: `tx_end` in bits 0..=3, `tx_err` in 4..=7, `tx_thr_event` in
//!   8..=11, `tx_loop` in 12..=15, then the RX events. The C6 interleaves TX
//!   and RX (`ch_tx_end` 0..=1, `ch_rx_end` 2..=3, …), so a hand-written mask
//!   does not port. The `ch_tx_*(n)` PAC accessors have the same *names* on
//!   both chips, which is exactly why the difference is easy to miss.
//! * `CH_TX_LIM.tx_lim` is 9 bits here (max 511) and `CH_TX_STATUS.mem_raddr_ex`
//!   is 10 bits (max 1023) — wide enough to address all 384 words of RAM,
//!   which is the tell that it is an absolute offset.
//!
//! # Provenance
//!
//! Adapted from this project's own ESP32-S3 experiment firmware
//! (`2026-esp32s3-experiment`, `fw/led-lab-esp32s3/src/s3_rmt.rs`), which ran
//! the loopback and stress suites on this exact chip. The adaptation is
//! packaging — crate paths and harness gating — not redesign. No GPL source
//! was consulted; see `AGENTS.md`.

use esp_hal::peripherals::RMT;
use lp_ws281x::{BlockPlan, InterruptFlags, RmtHw, SharedBlockPlan};

/// TX channels the ESP32-S3 RMT exposes. `CH0..=CH3` transmit; `CH4..=CH7` are
/// receive-only and are not addressed by this driver.
pub const TX_CHANNELS: usize = 4;

/// Words in one ESP32-S3 RMT memory block.
pub const BLOCK_WORDS: usize = 48;

/// The block plan, computed once at driver init from the number of WS281x
/// channels the board manifest declares ([`plan_for_declared`]) and read by
/// the backend, the drivers and the interrupt handler from then on.
///
/// Fail-closed: until a driver publishes a plan, every channel reads as
/// unavailable and nothing can transmit.
pub static TX_PLAN: SharedBlockPlan<TX_CHANNELS> = SharedBlockPlan::new();

/// The plan for a board manifest declaring `declared` WS281x channels, or
/// `None` for a board with none: `floor(4 / channels)` of the four TX blocks
/// each.
///
/// Four declared channels (the XIAO ESP32-S3 Plus) get one 48-word block
/// each — 24-word halves, the tightest refill deadline the hardware can pose
/// (~30 µs at 800 kHz), and all four outputs; the exact plan the old
/// compile-time constant produced. One declared channel (the S3 DevKitC)
/// gets the whole 192-word TX group — ~120 µs deadlines. A channel's window
/// extends into the blocks of the channels above it, which then cannot
/// transmit at all (see [`lp_ws281x::BlockPlan`] and the interrupt-rate
/// table in the core's README). The four RX blocks are left alone: this
/// firmware has an RX-side user (the loopback harness), and no S3 manifest
/// needs the extra margin.
pub fn plan_for_declared(declared: usize) -> Option<BlockPlan<TX_CHANNELS>> {
    if declared == 0 {
        return None;
    }
    let channels = declared.min(TX_CHANNELS);
    let blocks_each = (TX_CHANNELS / channels) as u8;
    // Validated by the `for_channels` tests in `lp-ws281x`; cannot fail for
    // 1..=4 channels.
    BlockPlan::for_channels(channels, blocks_each).ok()
}

/// Total TX-side RMT RAM, in words — the bound every pointer here respects.
const TX_RAM_WORDS: usize = BLOCK_WORDS * TX_CHANNELS;

/// Byte offset from the RMT peripheral base to the start of RMT RAM.
///
/// **`0x800` on the ESP32-S3.** See the module docs — the C6 value is `0x400`.
pub const RAM_OFFSET: usize = 0x800;

/// Absolute address of the ESP32-S3 RMT RAM (`0x6001_6800`).
pub const RAM_BASE: usize = 0x6001_6000 + RAM_OFFSET;

/// Mask covering the four TX channels within one event field of `INT_*`.
const TX_CH_MASK: u32 = 0b1111;

/// Bit offset of the `tx_err` field within `INT_*`.
const ERR_SHIFT: u32 = 4;

/// Bit offset of the `tx_thr_event` field within `INT_*`.
const THR_SHIFT: u32 = 8;

/// Widest value `CH_TX_LIM.tx_lim` (bits 0..=8) can hold.
const TX_LIM_MAX: u16 = 0x1FF;

/// The seven register operations `lp-ws281x` needs, on the ESP32-S3.
///
/// Carries no state at all — the memory-block plan lives in [`TX_PLAN`] and
/// everything else it addresses is memory-mapped — so it is
/// `const`-constructible and can live in a `static` shared with the
/// interrupt handler.
#[derive(Debug, Clone, Copy, Default)]
pub struct S3Rmt;

impl S3Rmt {
    /// A backend handle. Touches no hardware; reads [`TX_PLAN`].
    pub const fn new() -> Self {
        Self
    }
}

impl RmtHw for S3Rmt {
    #[inline]
    fn ram_words(&self, ch: u8) -> usize {
        TX_PLAN.window_words(ch, BLOCK_WORDS)
    }

    #[inline]
    fn write_ram(&self, ch: u8, word_idx: usize, value: u32) {
        let Some(ptr) = ram_word(ch, word_idx) else {
            return;
        };
        // SAFETY: `ram_word` returned an in-range, naturally aligned pointer
        // into the RMT RAM window. The write must be volatile: the transmitter
        // reads this memory behind the compiler's back, so the store can be
        // neither elided nor reordered with the surrounding register writes.
        unsafe { ptr.write_volatile(value) };
    }

    #[inline]
    fn set_tx_threshold(&self, ch: u8, words: u16) {
        if ch as usize >= TX_CHANNELS {
            return;
        }
        // SAFETY (register): `tx_lim` is a plain 9-bit field; the value is
        // masked to that width, so no reserved bits are disturbed. The PAC
        // marks the setter unsafe only because it cannot check the width.
        RMT::regs()
            .ch_tx_lim(ch as usize)
            .modify(|_, w| unsafe { w.tx_lim().bits(words & TX_LIM_MAX) });
    }

    #[inline]
    fn read_pos(&self, ch: u8) -> u16 {
        let window = TX_PLAN.window_words(ch, BLOCK_WORDS);
        if window == 0 {
            return 0;
        }
        let absolute = RMT::regs()
            .ch_tx_status(ch as usize)
            .read()
            .mem_raddr_ex()
            .bits();
        // `mem_raddr_ex` counts from the start of the *whole* RMT RAM, so a
        // channel's window begins at its first block. (esp-hal's own
        // `hw_offset()` subtracts the same term.) The modulo keeps a reading
        // taken mid-wrap inside the window instead of panicking or aliasing.
        let base = TX_PLAN.window_start(ch, BLOCK_WORDS) as u16;
        absolute.wrapping_sub(base) % window as u16
    }

    fn start_tx(&self, ch: u8) {
        if ch as usize >= TX_CHANNELS {
            return;
        }
        let rmt = RMT::regs();
        let idx = ch as usize;

        // Drop causes left over from the previous frame so the first event of
        // this one is genuinely this one's.
        // SAFETY (register): `int_clr` is write-1-to-clear across its whole
        // width; the mask only names this channel's four TX event bits.
        rmt.int_clr()
            .write(|w| unsafe { w.bits(tx_event_mask(ch)) });

        // Reset the channel clock divider so the first pulse is a full one
        // rather than the remainder of a tick already in progress.
        // SAFETY (register): `ref_cnt_rst` is a bitmask of channels to reset,
        // self-clearing on write; the pulse below touches only `ch`.
        rmt.ref_cnt_rst().write(|w| unsafe { w.bits(1 << ch) });
        // SAFETY (register): releasing the reset again, same field.
        rmt.ref_cnt_rst().write(|w| unsafe { w.bits(0) });

        // Single-shot, wrapping around the window: the driver keeps refilling
        // behind the read pointer and ends the frame with a STOP word.
        rmt.ch_tx_conf0(idx).modify(|_, w| {
            w.tx_stop().clear_bit();
            w.tx_conti_mode().clear_bit();
            w.mem_tx_wrap_en().set_bit()
        });
        rmt.ch_tx_conf0(idx)
            .modify(|_, w| w.conf_update().set_bit());

        // `mem_rd_rst` rewinds the transmitter to word 0 of the window and
        // `apb_mem_rst` the APB side; both are write-to-trigger.
        rmt.ch_tx_conf0(idx).modify(|_, w| {
            w.mem_rd_rst().set_bit();
            w.apb_mem_rst().set_bit();
            w.tx_start().set_bit()
        });
        rmt.ch_tx_conf0(idx)
            .modify(|_, w| w.conf_update().set_bit());
    }

    fn stop_tx(&self, ch: u8) {
        if ch as usize >= TX_CHANNELS {
            return;
        }
        let rmt = RMT::regs();
        let idx = ch as usize;
        rmt.ch_tx_conf0(idx).modify(|_, w| w.tx_stop().set_bit());
        rmt.ch_tx_conf0(idx)
            .modify(|_, w| w.conf_update().set_bit());
    }

    #[inline]
    fn take_interrupts(&self) -> InterruptFlags {
        let rmt = RMT::regs();
        // `int_st` is `int_raw & int_ena`, so causes this firmware never asked
        // for (notably `tx_loop`) cannot reach the driver.
        let status = rmt.int_st().read().bits();

        let end = status & TX_CH_MASK;
        let error = (status >> ERR_SHIFT) & TX_CH_MASK;
        let threshold = (status >> THR_SHIFT) & TX_CH_MASK;

        let pending = end | (error << ERR_SHIFT) | (threshold << THR_SHIFT);
        if pending != 0 {
            // Acknowledge exactly what is being reported, in the same handler
            // pass — the driver's contract is that no cause is lost between the
            // read and the clear.
            // SAFETY (register): write-1-to-clear; `pending` is a subset of the
            // bits just read from `int_st`.
            rmt.int_clr().write(|w| unsafe { w.bits(pending) });
        }

        InterruptFlags {
            threshold,
            end,
            error,
        }
    }
}

/// Enable `tx_end`, `tx_err` and `tx_thr_event` for `ch`; leave `tx_loop` off.
pub fn enable_tx_interrupts(ch: u8) {
    if ch as usize >= TX_CHANNELS {
        return;
    }
    RMT::regs().int_ena().modify(|_, w| {
        w.ch_tx_end(ch).set_bit();
        w.ch_tx_err(ch).set_bit();
        w.ch_tx_thr_event(ch).set_bit();
        w.ch_tx_loop(ch).clear_bit()
    });
}

/// Zero every word of `ch`'s RAM window.
///
/// An all-zero word is the STOP marker, so this leaves the channel in the
/// safest possible state: whatever happens, the transmitter stops at word 0.
pub fn clear_ram(ch: u8) {
    for word in 0..TX_PLAN.window_words(ch, BLOCK_WORDS) {
        if let Some(ptr) = ram_word(ch, word) {
            // SAFETY: in-range, aligned RMT RAM pointer from `ram_word`;
            // volatile because the transmitter also reads this memory.
            unsafe { ptr.write_volatile(0) };
        }
    }
}

/// Pointer to word `word_idx` of channel `ch`'s RAM window under [`TX_PLAN`],
/// or `None` if either index is out of range.
///
/// The bounds check costs one compare per word on the refill path and buys a
/// handler that can never scribble outside the peripheral, whatever the caller
/// does. A multi-block window spans its neighbours' blocks, which is why the
/// size comes from the plan rather than from a constant.
#[inline(always)]
fn ram_word(ch: u8, word_idx: usize) -> Option<*mut u32> {
    if word_idx >= TX_PLAN.window_words(ch, BLOCK_WORDS) {
        // Covers an out-of-range or absorbed channel too: both have no window.
        return None;
    }
    let index = TX_PLAN.window_start(ch, BLOCK_WORDS) + word_idx;
    if index >= TX_RAM_WORDS {
        return None;
    }
    // SAFETY: `RAM_BASE` is the RMT peripheral's memory window, `TX_CHANNELS *
    // BLOCK_WORDS` u32 words long; `index` was just bounded to that range, so
    // the result stays inside one allocated MMIO object and the byte offset
    // (< 1 KiB) cannot overflow an `isize`.
    Some(unsafe { (RAM_BASE as *mut u32).add(index) })
}

/// The `INT_*` bits belonging to TX channel `ch`.
const fn tx_event_mask(ch: u8) -> u32 {
    let bit = 1u32 << ch;
    // tx_end | tx_err | tx_thr_event | tx_loop
    bit | (bit << ERR_SHIFT) | (bit << THR_SHIFT) | (bit << 12)
}

/// What [`probe_ram_address`] found.
#[cfg(feature = "test_loopback")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamProbe {
    /// The word read straight back after a direct store — proves the address
    /// behaves like memory at all.
    pub direct_readback: u32,
    /// The word the *peripheral* deposited via its APB FIFO port, read through
    /// [`RAM_BASE`]. Equal to the sentinel only if `RAM_BASE` really is where
    /// the RMT keeps channel `ch`'s data.
    pub fifo_readback: u32,
}

#[cfg(feature = "test_loopback")]
impl RamProbe {
    /// Both halves of the probe agreed with the sentinels.
    pub fn ok(&self, direct: u32, fifo: u32) -> bool {
        self.direct_readback == direct && self.fifo_readback == fifo
    }
}

/// Confirm on-chip that [`RAM_BASE`] is the RMT's channel RAM.
///
/// Two independent checks:
///
/// 1. store a sentinel through the computed pointer and read it back — the
///    address is writable memory rather than a read-only or absent window;
/// 2. clear `SYS_CONF.apb_fifo_mask` (`0` = "access memory by FIFO"), write a
///    second sentinel to `CH<n>DATA`, and restore direct access. That write
///    goes through the peripheral's *own* address generator, so finding it at
///    `RAM_BASE + ch * 48` is the hardware agreeing with the constant. A wrong
///    offset fails this even though it would pass check 1 against any RAM.
///
/// Leaves the window zeroed and the peripheral back in direct-access mode. Must
/// be called while `ch` is idle.
#[cfg(feature = "test_loopback")]
pub fn probe_ram_address(ch: u8, direct_sentinel: u32, fifo_sentinel: u32) -> RamProbe {
    let rmt = RMT::regs();
    let idx = ch as usize;

    let direct_readback = match ram_word(ch, 0) {
        Some(ptr) => {
            // SAFETY: in-range, aligned RMT RAM pointer; volatile so the store
            // and the load either side of it are actually performed.
            unsafe {
                ptr.write_volatile(direct_sentinel);
                ptr.read_volatile()
            }
        }
        None => 0,
    };

    // Rewind the APB write pointer to the start of the channel's window, then
    // hand the peripheral the FIFO sentinel.
    rmt.ch_tx_conf0(idx)
        .modify(|_, w| w.apb_mem_rst().set_bit());
    rmt.ch_tx_conf0(idx)
        .modify(|_, w| w.conf_update().set_bit());
    rmt.sys_conf().modify(|_, w| w.apb_fifo_mask().clear_bit());
    // SAFETY (register): `CH<n>DATA` is a full-width data port; every bit
    // pattern is valid. The PAC marks it unsafe because it has no field
    // constraints to check.
    rmt.chdata(idx).write(|w| unsafe { w.bits(fifo_sentinel) });
    rmt.sys_conf().modify(|_, w| w.apb_fifo_mask().set_bit());

    let fifo_readback = match ram_word(ch, 0) {
        // SAFETY: as above.
        Some(ptr) => unsafe { ptr.read_volatile() },
        None => 0,
    };

    rmt.ch_tx_conf0(idx)
        .modify(|_, w| w.apb_mem_rst().set_bit());
    rmt.ch_tx_conf0(idx)
        .modify(|_, w| w.conf_update().set_bit());
    clear_ram(ch);

    RamProbe {
        direct_readback,
        fifo_readback,
    }
}
