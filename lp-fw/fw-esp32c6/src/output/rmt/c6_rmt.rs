//! ESP32-C6 RMT backend for [`lp_ws281x::RmtHw`].
//!
//! All the chip knowledge in this backend lives here: the RMT RAM address, the
//! `CH_TX_CONF0` start/stop dance, the interrupt-register bit layout, and the
//! fact that `MEM_RADDR_EX` is an offset into the *whole* RMT RAM rather than
//! into the channel's own window. Everything else — what to write and when —
//! is [`lp_ws281x::Ws281xDriver`], which never sees a register name.
//!
//! # Where the RAM is
//!
//! The ESP32-C6 RMT RAM sits at **`RMT_BASE + 0x400` = `0x6000_6400`**, *not*
//! at the `+0x800` the ESP32-S3 and the classic ESP32 use. The C6 has four
//! 48-word blocks against the S3's eight, and the register file in front of
//! them is correspondingly smaller. Carrying the S3 constant over would write
//! past the end of the RMT RAM entirely.
//!
//! Value verified two ways: `esp-metadata-generated` v0.4.0 (`rmt.ram_start`
//! for `esp32c6` = `1610638336` = `0x6000_6400`; MIT/Apache-2.0, the table
//! esp-hal's own RMT driver reads), and this firmware's own legacy driver,
//! which has driven WS2812 strips off `RMT::ptr() + 0x400` on this silicon
//! since the project started.
//!
//! # What the C6 splits differently from the S3
//!
//! * **Two TX channels, two RX channels.** `CH0`/`CH1` transmit and `CH2`/`CH3`
//!   receive; unlike the classic ESP32 a channel does not choose its
//!   direction. Only the two TX blocks are modelled here — a TX window is
//!   never allowed to grow into the RX half, which esp-hal *would* permit
//!   (`reserve_channel_memory` bounds `memsize` by `NUM_CHANNELS - channel`,
//!   i.e. 4 blocks for channel 0). Two blocks is all this driver ever wants:
//!   at that setting there is exactly one transmitter left.
//! * **`INT_*` interleaves TX and RX in pairs**: `ch_tx_end` 0..=1,
//!   `ch_rx_end` 2..=3, `ch_tx_err` 4..=5, `ch_rx_err` 6..=7,
//!   `ch_tx_thr_event` 8..=9, `ch_rx_thr_event` 10..=11, `ch_tx_loop`
//!   12..=13 (PAC `int_raw` field docs). The *shifts* coincide with the S3's
//!   (4, 8, 12) because the S3 packs four TX channels where the C6 packs two
//!   TX plus two RX — but the field **width** is two bits, not four, so an
//!   S3 mask would sweep up RX causes. [`TX_CH_MASK`] is the difference.
//! * `CH_TX_LIM.tx_lim` is 9 bits (max 511) and `CH_TX_STATUS.mem_raddr_ex` is
//!   9 bits (max 511) — both comfortably wider than the 192 words of RMT RAM,
//!   and `mem_raddr_ex` is an absolute offset (esp-hal's `hw_offset()`
//!   subtracts `channel * 48` from it, exactly as [`RmtHw::read_pos`] does
//!   here through the block plan).
//! * `tx_lim` on this chip is a **position in the window**, like the S3's and
//!   unlike the classic ESP32's repeating entry count: the legacy C6 driver
//!   this firmware still ships alternates it between the half size and the
//!   full window on every threshold interrupt and has done so on silicon for
//!   the life of the project. That is precisely the sequence
//!   [`lp_ws281x::Ws281xDriver`] performs, so no clamp is needed here.
//! * `mem_tx_wrap_en` is per channel (`CH_TX_CONF0` bit 4), like the S3's and
//!   unlike the classic's global `APB_CONF` bit, so [`RmtHw::start_tx`] sets
//!   it with the rest of the channel's configuration.
//! * `rmt.has_tx_immediate_stop` is true, so [`RmtHw::stop_tx`] is the single
//!   `tx_stop` bit rather than the classic's fill-the-window-with-STOP dance.
//!
//! # Provenance
//!
//! Structure adapted from this project's own `fw-esp32s3/src/output/rmt/
//! s3_rmt.rs`; every register fact re-derived for this chip from the `esp32c6`
//! PAC 0.23.2 field documentation, `esp-metadata-generated` 0.4.0's `rmt.*`
//! properties, and esp-hal 1.1.1's `rmt::chip_specific` module for
//! `not(any(esp32, esp32s2))` (MIT/Apache-2.0) — which is the *same* code path
//! the S3 uses, so the start/stop sequence transfers unchanged. No GPL source
//! was consulted; see `AGENTS.md`.

use esp_hal::peripherals::RMT;
use lp_ws281x::{BlockPlan, InterruptFlags, RmtHw};

/// TX channels the ESP32-C6 RMT exposes. `CH0`/`CH1` transmit; `CH2`/`CH3` are
/// receive-only and are not addressed by this driver.
pub const TX_CHANNELS: usize = 2;

/// Words in one ESP32-C6 RMT memory block (`rmt.channel_ram_size`).
pub const BLOCK_WORDS: usize = 48;

/// Memory blocks given to each transmitting channel — **one** by default.
///
/// One block each is the shipped default (the plan's Q3 override: the C6
/// declares two WS281x channels): a 48-word window halves into 24 words =
/// exactly one LED, which is the tightest refill deadline the hardware can
/// pose (~30 µs at 800 kHz) *and* the only way to get both outputs.
///
/// The cargo feature **`ws281x_2blocks`** flips this to two, trading the
/// second output for twice the refill headroom: a 96-word window halving into
/// 48-word (~60 µs) halves, with channel 1's block absorbed into channel 0's
/// and therefore unusable. Which of the two ships is a measurement question
/// (the stress matrix), not an authoring one — hence a const behind a cargo
/// feature and never an env var: `option_env!` does not make cargo rebuild,
/// and this project has already lost a run to that trap.
#[cfg(not(feature = "ws281x_2blocks"))]
pub const BLOCKS_PER_CHANNEL: u8 = 1;

/// Two blocks per channel — see the default-configuration docs above.
#[cfg(feature = "ws281x_2blocks")]
pub const BLOCKS_PER_CHANNEL: u8 = 2;

/// The TX-side allocation of the RMT's two transmit memory blocks.
///
/// Validated at compile time: an overlapping or oversized plan is a build
/// error, not a runtime surprise. The *same* value is handed to the driver
/// (which refuses to configure an absorbed channel) and to [`C6Rmt`] (which
/// sizes and bounds its RAM window from it), so the two can never disagree.
pub const TX_BLOCKS: BlockPlan<TX_CHANNELS> = match BlockPlan::uniform(BLOCKS_PER_CHANNEL) {
    Ok(plan) => plan,
    Err(_) => panic!("BLOCKS_PER_CHANNEL does not divide the ESP32-C6's two TX memory blocks"),
};

/// Spacing between usable channel slots.
///
/// A channel given `n` blocks absorbs the next `n-1` channels' blocks, so at
/// two blocks each only slot **0** can be configured — slot 1 has no memory of
/// its own and asking esp-hal to configure it fails with
/// `ConfigError::MemoryBlockNotAvailable`. Manifest channel `k` therefore
/// drives RMT slot `k * SLOT_STRIDE`; see [`slot_for_index`].
pub const SLOT_STRIDE: usize = BLOCKS_PER_CHANNEL as usize;

/// How many RMT slots the block plan leaves transmittable — the ceiling on the
/// number of WS281x endpoints this chip can back, whatever the manifest says.
pub const USABLE_CHANNELS: usize = TX_CHANNELS / SLOT_STRIDE;

/// The RMT slot backing manifest channel `index` (`/rmt/ws281x<index>`), or
/// `None` when the block plan has no slot for it.
///
/// This is the whole of the absorbed-slot guard: callers enumerate *manifest*
/// channels `0..USABLE_CHANNELS` and get back only slots the plan marks
/// available, so an absorbed slot is never handed to `configure_tx` in the
/// first place. The `is_available` check is belt-and-braces — with a uniform
/// plan the stride already lands on owners — and keeps this honest under a
/// hand-written non-uniform plan.
pub const fn slot_for_index(index: usize) -> Option<u8> {
    let slot = index * SLOT_STRIDE;
    if slot >= TX_CHANNELS {
        return None;
    }
    let slot = slot as u8;
    if TX_BLOCKS.is_available(slot) {
        Some(slot)
    } else {
        None
    }
}

/// RAM words a channel owns under [`TX_BLOCKS`], for the channels that have
/// any. Reported in the driver's boot line so a serial log says what refill
/// deadline the run was measured against (`CHANNEL_WORDS / 2` words per half,
/// 1.25 µs per word at 800 kHz).
pub const CHANNEL_WORDS: usize = BLOCK_WORDS * BLOCKS_PER_CHANNEL as usize;

/// Total TX-side RMT RAM, in words — the bound every pointer here respects.
///
/// The two RX blocks that follow are deliberately outside it: this driver does
/// not receive, and a TX window that ran into them would be writing over a
/// peripheral resource nothing in this firmware owns.
const TX_RAM_WORDS: usize = BLOCK_WORDS * TX_CHANNELS;

/// Byte offset from the RMT peripheral base to the start of RMT RAM.
///
/// **`0x400` on the ESP32-C6.** See the module docs — the S3 and classic value
/// is `0x800`.
pub const RAM_OFFSET: usize = 0x400;

/// Absolute address of the ESP32-C6 RMT RAM (`0x6000_6400`).
pub const RAM_BASE: usize = 0x6000_6000 + RAM_OFFSET;

/// Mask covering the two TX channels within one event field of `INT_*`.
///
/// Two bits, not the S3's four: bits 2..=3 of the `end` field are `ch_rx_end`.
const TX_CH_MASK: u32 = 0b11;

/// Bit offset of the `ch_tx_err` field within `INT_*`.
const ERR_SHIFT: u32 = 4;

/// Bit offset of the `ch_tx_thr_event` field within `INT_*`.
const THR_SHIFT: u32 = 8;

/// Bit offset of the `ch_tx_loop` field within `INT_*`.
const LOOP_SHIFT: u32 = 12;

/// Widest value `CH_TX_LIM.tx_lim` (bits 0..=8) can hold.
const TX_LIM_MAX: u16 = 0x1FF;

/// The seven register operations `lp-ws281x` needs, on the ESP32-C6.
///
/// Carries only the memory-block plan: everything else it addresses is
/// memory-mapped, so it is `const`-constructible and can live in a `static`
/// shared with the interrupt handler.
#[derive(Debug, Clone, Copy)]
pub struct C6Rmt {
    blocks: BlockPlan<TX_CHANNELS>,
}

impl C6Rmt {
    /// A backend handle for `blocks`. Touches no hardware.
    ///
    /// Pass the same plan to [`lp_ws281x::Ws281xDriver::with_blocks`].
    pub const fn new(blocks: BlockPlan<TX_CHANNELS>) -> Self {
        Self { blocks }
    }
}

impl Default for C6Rmt {
    fn default() -> Self {
        Self::new(TX_BLOCKS)
    }
}

impl RmtHw for C6Rmt {
    #[inline]
    fn ram_words(&self, ch: u8) -> usize {
        self.blocks.window_words(ch, BLOCK_WORDS)
    }

    #[inline]
    fn write_ram(&self, ch: u8, word_idx: usize, value: u32) {
        let Some(ptr) = ram_word(&self.blocks, ch, word_idx) else {
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
        let window = self.blocks.window_words(ch, BLOCK_WORDS);
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
        // taken mid-wrap inside the window instead of aliasing.
        let base = self.blocks.window_start(ch, BLOCK_WORDS) as u16;
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
        // SAFETY (register): `ref_cnt_rst` is a bitmask of channels to reset;
        // TX channel `ch` is bit `ch` (`tx_ref_cnt_rst`, `tx_ref_cnt_rst_ch1`).
        // The pulse below touches only `ch`.
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
        // for (notably `tx_loop`, and every RX cause) cannot reach the driver.
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
pub fn clear_ram(blocks: &BlockPlan<TX_CHANNELS>, ch: u8) {
    for word in 0..blocks.window_words(ch, BLOCK_WORDS) {
        if let Some(ptr) = ram_word(blocks, ch, word) {
            // SAFETY: in-range, aligned RMT RAM pointer from `ram_word`;
            // volatile because the transmitter also reads this memory.
            unsafe { ptr.write_volatile(0) };
        }
    }
}

/// Pointer to word `word_idx` of channel `ch`'s RAM window under `blocks`, or
/// `None` if either index is out of range.
///
/// The bounds check costs one compare per word on the refill path and buys a
/// handler that can never scribble outside the TX half of the RMT RAM,
/// whatever the caller does. With `blocks_per_channel > 1` the window spans
/// several blocks, which is why the size comes from the plan rather than from
/// a constant.
#[inline(always)]
fn ram_word(blocks: &BlockPlan<TX_CHANNELS>, ch: u8, word_idx: usize) -> Option<*mut u32> {
    if word_idx >= blocks.window_words(ch, BLOCK_WORDS) {
        // Covers an out-of-range or absorbed channel too: both have no window.
        return None;
    }
    let index = blocks.window_start(ch, BLOCK_WORDS) + word_idx;
    if index >= TX_RAM_WORDS {
        return None;
    }
    // SAFETY: `RAM_BASE` is the RMT peripheral's memory window, 192 u32 words
    // long, of which the first `TX_RAM_WORDS` belong to the transmitters;
    // `index` was just bounded to that range, so the result stays inside one
    // allocated MMIO object and the byte offset (< 512 B) cannot overflow an
    // `isize`.
    Some(unsafe { (RAM_BASE as *mut u32).add(index) })
}

/// The `INT_*` bits belonging to TX channel `ch` (end, err, thr_event, loop).
///
/// The RX causes interleaved between those fields are deliberately excluded:
/// this backend never enables or clears causes for a role it does not drive.
const fn tx_event_mask(ch: u8) -> u32 {
    let bit = 1u32 << ch;
    bit | (bit << ERR_SHIFT) | (bit << THR_SHIFT) | (bit << LOOP_SHIFT)
}
