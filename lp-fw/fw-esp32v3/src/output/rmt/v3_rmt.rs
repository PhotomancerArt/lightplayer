//! Classic ESP32 (LX6) RMT backend for [`lp_ws281x::RmtHw`].
//!
//! All the chip knowledge in this firmware lives here: the RMT RAM address,
//! the `CHnCONF1` start/stop dance, the interrupt-register bit layout, and the
//! fact that `MEM_RADDR_EX` is an offset into the *whole* RMT RAM rather than
//! into the channel's own window. Everything else — what to write and when —
//! is [`lp_ws281x::Ws281xDriver`], which never sees a register name.
//!
//! # Provenance
//!
//! Ported from this project's own classic-ESP32 experiment firmware
//! (`2026-esp32s3-experiment`, `fw/led-lab-esp32/src/esp32_rmt.rs` @ `b1dff8f`),
//! which ran the loopback, soak and channel-count-sweep suites on this exact
//! chip. The adaptation is packaging — crate paths, a manifest-derived block
//! plan ([`plan_for_declared`]) instead of the harness's `option_env!`, and
//! the removal of the harness-only RAM probe — not redesign.
//!
//! Register derivation sources (license-safe, in the order they settled each
//! question): esp-hal 1.1.1 `src/rmt.rs` `chip_specific` module for
//! `any(esp32, esp32s2)` (MIT/Apache-2.0), the `esp32` PAC 0.40.2 field docs,
//! and `esp-metadata-generated` (`rmt.*` properties for `esp32`). No GPL
//! source was consulted; see `AGENTS.md`.
//!
//! # Where the RAM is
//!
//! The classic ESP32 RMT RAM sits at **`RMT_BASE + 0x800` = `0x3FF5_6800`**
//! (`esp-metadata-generated` `rmt.ram_start` = `1073047552`; PAC RMT base =
//! `0x3FF5_6000`). That is the *same* `+0x800` offset as the ESP32-S3 —
//! coincidence, not shared layout: the classic has eight 64-word blocks
//! (512 words) against the S3's eight 48-word ones, and a smaller register
//! file in front of them. The C6's `+0x400` remains the odd one out. The
//! experiment firmware proved the constant on silicon with a FIFO-port probe
//! at every demo boot; that probe is not carried over here because this crate
//! has no harness entrypoint to run it from, and the app path proves the
//! offset end to end anyway (a wrong offset cannot transmit a single frame).
//!
//! # Structural differences from the S3 backend (each verified in the sources
//! above, none recalled from memory)
//!
//! * **8 channels, each TX-or-RX.** There is no fixed TX/RX split; a channel
//!   transmits because `CHnCONF1.tx_start` was set and receives because
//!   `rx_en` was. This backend drives channels `0..TX_CHANNELS` as
//!   transmitters and never touches the rest.
//! * **64-word blocks** (`rmt.channel_ram_size` = 64) → 32-word halves at one
//!   block per channel, 64-word halves at the two a four-channel manifest
//!   gets. The bit-cursor core handles any split untouched.
//! * **Per-channel `CHnCONF0`/`CHnCONF1`** instead of the S3's single
//!   `CH_TX_CONF0`: divider/memsize/idle-threshold live in CONF0; start/reset
//!   /owner/idle-output bits in CONF1. There is **no `conf_update`** — writes
//!   take effect immediately (esp-hal's `update()` is a no-op here).
//! * **`INT_*` bits interleave by channel**: `chN_tx_end` = bit `3N`,
//!   `chN_rx_end` = `3N+1`, `chN_err` = `3N+2`, and `chN_tx_thr_event` = bit
//!   `24+N` (PAC `int_raw` field docs). The S3 groups by event
//!   (`tx_end` 0..=3, `tx_err` 4..=7, `tx_thr_event` 8..=11). Note `ch_err`
//!   is a *combined* TX/RX error bit — there is no separate `tx_err`. RX
//!   errors cannot reach [`RmtHw::take_interrupts`] because this firmware
//!   never sets `int_ena` for channels it does not transmit on, and the
//!   snapshot reads `INT_ST` (= raw & ena).
//! * **Wrap enable is global**: `APB_CONF.mem_tx_wrap_en` (bit 1), not a
//!   per-channel `mem_tx_wrap_en` in the channel's own conf register. Set
//!   once by [`init_tx`]; every transmitting channel here wants wrap.
//!   ⚠️ Without this bit ping-pong refill does not work **at all** on this
//!   chip (experiment findings.md §11.5) — the transmitter runs off the end
//!   of the window instead of wrapping back onto the refilled half.
//! * **No immediate TX stop.** `rmt.has_tx_immediate_stop` = false: there is
//!   no `tx_stop` bit anywhere on this chip. esp-hal stops a channel by
//!   filling its whole RAM window with end markers, and [`RmtHw::stop_tx`]
//!   here does the same — the transmitter halts at the next word boundary
//!   (≤ 1.25 µs for WS2812 timing) and raises `tx_end`.
//! * **`mem_owner` exists** (`CHnCONF1` bit 5, 1 = receiver owns the RAM):
//!   [`RmtHw::start_tx`] clears it for the channel and for every extra block
//!   the window extends into, exactly as esp-hal's classic `start_tx` does.
//! * `CHnSTATUS.mem_raddr_ex` is 10 bits (max 1023 — wide enough for all 512
//!   words, the tell that it is an absolute offset) and `CH_TX_LIM.tx_lim` is
//!   9 bits (max 511; a full 8-block window of 512 words would not fit, which
//!   is why [`MAX_BLOCKS_PER_CHANNEL`] caps every plan at four blocks).
//!
//! # Cross-core register rules
//!
//! The interrupt handler runs on the APP core (see `shared_driver`), so
//! thread context and the ISR touch this peripheral from **different cores**
//! concurrently. The register-level invariants that keep that sound:
//!
//! * **`INT_ENA` is read-modify-written from thread context only**
//!   ([`enable_tx_interrupts`], at channel open). The ISR never writes it. A
//!   second RMW writer on the other core would lose updates — do not add one.
//! * **`INT_CLR` is write-1-to-clear** and written from both sides
//!   ([`RmtHw::take_interrupts`] on the ISR core, [`RmtHw::start_tx`]'s
//!   stale-cause drop on the thread core). W1C is race-free by construction:
//!   each write clears exactly the bits it names.
//! * **Per-channel registers** (`CHnCONF1`, `CH_TX_LIM`, RAM words) are only
//!   ever RMW'd cross-core for the *same* channel during an abort race —
//!   after `stop_tx` has halted the transmitter — where a lost update is
//!   harmless. In steady state the ISR owns a transmitting channel's
//!   `CH_TX_LIM`/RAM and thread context owns idle channels'.
//! * `APB_CONF` is written once at [`init_tx`], before the APP core binds.

use esp_hal::peripherals::RMT;
use lp_ws281x::{BlockPlan, InterruptFlags, RamWindow, RmtHw, SharedBlockPlan};

/// RMT channels the classic ESP32 has, all of which can transmit.
///
/// This is the **hardware** count, not the output count: how many of these
/// slots are usable at once is the block plan's decision, and how many are
/// *offered* is the board manifest's (ADR
/// `2026-07-31-lp-ws281x-multi-channel-driver-adoption`).
pub const TX_CHANNELS: usize = 8;

/// Words in one classic-ESP32 RMT memory block (`rmt.channel_ram_size`).
pub const BLOCK_WORDS: usize = 64;

/// Widest window any one channel may take, in blocks.
///
/// **Four, because of `tx_lim`'s width, not preference**: the threshold
/// register is 9 bits (max 511), and the driver core arms it with the full
/// window size at every other crossing — a 512-word all-blocks window would
/// mask to 0 and never fire. Four blocks (256 words) is the widest
/// expressible power-of-two split, and its 128-word halves already relax the
/// refill deadline to ~160 µs.
pub const MAX_BLOCKS_PER_CHANNEL: u8 = 4;

/// The block plan, computed once at driver init from the number of WS281x
/// channels the board manifest declares ([`plan_for_declared`]) and read by
/// the backend, the drivers and the interrupt handler from then on.
///
/// Fail-closed: until a driver publishes a plan, every channel reads as
/// unavailable and nothing can transmit.
pub static TX_PLAN: SharedBlockPlan<TX_CHANNELS> = SharedBlockPlan::new();

/// The plan for a board manifest declaring `declared` WS281x channels, or
/// `None` for a board with none: `floor(8 / channels)` consecutive blocks
/// each, capped at [`MAX_BLOCKS_PER_CHANNEL`].
///
/// The DOM-Z-102's four declared channels get two blocks each —
/// `[2, 0, 2, 0, 2, 0, 2, 0]`, the exact plan the old compile-time constant
/// produced, so the shipped board's windows, interrupt demand and stress
/// baselines are unchanged. Fewer declared channels widen the windows (one
/// strip gets a 256-word window); more than four get one block each, which
/// the experiment's `findings.md` §12 measured as truncating from channel 3
/// up — the chip's ISR-throughput ceiling (~46–55 k interrupts/s regardless
/// of demand) sits right at that demand curve:
///
/// ```text
///   blocks  half_words  demand/channel  channels under a ~48 k/s ceiling
///   1       32          25 000/s        2   (measured: truncation from ch 3 up)
///   2       64          12 500/s        ~4  (arithmetic; MARGINAL — 50 k vs 48 k)
///   4       128          6 250/s        ~2  (only two windows exist at 4 blocks)
/// ```
///
/// A manifest that declares more channels than its strips need is therefore
/// paying real margin for phantom outputs — the manifest is the tuning knob
/// now, which is the point: it is the sole authority on channel count.
///
/// **Above four declared wires the plan stops growing**: silicon transmitters
/// cap at [`POOLED_SLOT_CAP`] two-block slots and the extra wires share them
/// by per-transmission pin muxing (the driver's slot pool — see
/// `esp32v3_rmt_ws281x_driver`). The old behaviour — five-plus declared
/// channels degrading everyone to one-block windows — put the whole board on
/// 40 µs deadlines the ISR-throughput ceiling cannot honour (§12: truncation
/// from channel 3 up); the pool keeps every transmitter on the measured-clean
/// 80 µs geometry instead and pays with waves.
pub fn plan_for_declared(declared: usize) -> Option<BlockPlan<TX_CHANNELS>> {
    if declared == 0 {
        return None;
    }
    let channels = declared.min(POOLED_SLOT_CAP);
    let blocks_each = ((TX_CHANNELS / channels) as u8).min(MAX_BLOCKS_PER_CHANNEL);
    // Validated by the `for_channels` tests in `lp-ws281x`; cannot fail for
    // 1..=8 channels at a capped width.
    BlockPlan::for_channels(channels, blocks_each).ok()
}

/// The most silicon transmitters any plan configures, however many wires the
/// manifest declares.
///
/// Four two-block slots is the measured-clean maximum concurrent shape on
/// this chip (zero trips at cap 4 after the fill-path hoists; 2026-08-05).
/// Wires beyond this count time-share the slots via pin muxing rather than
/// shrinking everyone's refill deadline.
pub const POOLED_SLOT_CAP: usize = 4;

/// Total RMT RAM, in words — the bound every pointer here respects. All eight
/// blocks belong to transmitters on this chip (there are no receive-only
/// channels), so this is the whole 512-word RMT RAM.
const TX_RAM_WORDS: usize = BLOCK_WORDS * TX_CHANNELS;

/// Byte offset from the RMT peripheral base to the start of RMT RAM.
///
/// **`0x800` on the classic ESP32** — numerically the same as the S3's, per
/// the module docs.
pub const RAM_OFFSET: usize = 0x800;

/// Absolute address of the classic ESP32 RMT RAM (`0x3FF5_6800`).
pub const RAM_BASE: usize = 0x3FF5_6000 + RAM_OFFSET;

/// Widest value `CH_TX_LIM.tx_lim` (bits 0..=8) can hold.
const TX_LIM_MAX: u16 = 0x1FF;

/// Bit position of `chN_tx_end` in the `INT_*` registers: three bits per
/// channel, interleaved by channel (see module docs).
const fn int_tx_end_bit(ch: u8) -> u32 {
    1u32 << (3 * ch)
}

/// Bit position of `chN_err` (combined TX/RX error) in the `INT_*` registers.
const fn int_err_bit(ch: u8) -> u32 {
    1u32 << (3 * ch + 2)
}

/// Bit position of `chN_tx_thr_event` in the `INT_*` registers.
const fn int_thr_bit(ch: u8) -> u32 {
    1u32 << (24 + ch)
}

/// The `INT_*` bits belonging to TX channel `ch` (end, err, thr_event).
///
/// `chN_rx_end` (bit `3N+1`) is deliberately excluded: it is an RX cause and
/// this backend never enables or clears causes for a role it does not drive.
const fn tx_event_mask(ch: u8) -> u32 {
    int_tx_end_bit(ch) | int_err_bit(ch) | int_thr_bit(ch)
}

/// Pointer to word `word_idx` of channel `ch`'s RAM window under [`TX_PLAN`],
/// or `None` if either index is out of range.
///
/// The bounds check costs one compare per word on the refill path and buys a
/// handler that can never scribble outside the RMT RAM, whatever the caller
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
    // SAFETY: `RAM_BASE` is the RMT peripheral's memory window, at least
    // `TX_RAM_WORDS` u32 words long (the peripheral has exactly 512); `index`
    // was just bounded to that range, so the result stays inside one allocated
    // MMIO object and the byte offset (< 2 KiB) cannot overflow an `isize`.
    Some(unsafe { (RAM_BASE as *mut u32).add(index) })
}

/// The seven register operations `lp-ws281x` needs, on the classic ESP32.
///
/// Carries no state at all — the memory-block plan lives in [`TX_PLAN`] and
/// everything else it addresses is memory-mapped — so it is
/// `const`-constructible and can live in a `static` shared with the
/// interrupt handler.
#[derive(Debug, Clone, Copy, Default)]
pub struct V3Rmt;

impl V3Rmt {
    /// A backend handle. Touches no hardware; reads [`TX_PLAN`].
    pub const fn new() -> Self {
        Self
    }
}

impl RmtHw for V3Rmt {
    #[inline]
    fn ram_words(&self, ch: u8) -> usize {
        TX_PLAN.window_words(ch, BLOCK_WORDS)
    }

    // `always`: must land inside the IRAM-sectioned refill path (see the
    // `isr-in-ram` feature in lp-ws281x) — an outlined copy would sit in
    // flash and reintroduce the cross-core cache-stall this exists to avoid.
    #[inline(always)]
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

    // `always`: must land inside the IRAM-sectioned refill path (see the
    // `isr-in-ram` feature in lp-ws281x) — an outlined copy would sit in
    // flash and reintroduce the cross-core cache-stall this exists to avoid.
    //
    // This is the hoisted form of [`ram_word`]'s per-word plan lookup: the
    // same window derivation, performed once per fill (the boot probe
    // measured the per-word form at ~4 CPU cycles/word — see
    // `refill_floor_probe`). The bounds argument is identical: the base is
    // `window_start` into the 512-word RMT RAM and the driver's stores stay
    // below `words == window_words`, which the `index < TX_RAM_WORDS` check
    // here bounds as a whole instead of per word.
    #[inline(always)]
    fn ram_window(&self, ch: u8) -> Option<RamWindow> {
        let words = TX_PLAN.window_words(ch, BLOCK_WORDS);
        if words == 0 {
            return None;
        }
        let start = TX_PLAN.window_start(ch, BLOCK_WORDS);
        if start + words > TX_RAM_WORDS {
            return None;
        }
        // SAFETY: `RAM_BASE` is the RMT RAM window, `TX_RAM_WORDS` u32 words
        // long; `start + words` was just bounded to it, so the base and every
        // word the driver may store through it stay inside one MMIO object.
        Some(RamWindow {
            base: unsafe { (RAM_BASE as *mut u32).add(start) },
            words,
        })
    }

    // `always`: must land inside the IRAM-sectioned refill path (see the
    // `isr-in-ram` feature in lp-ws281x) — an outlined copy would sit in
    // flash and reintroduce the cross-core cache-stall this exists to avoid.
    #[inline(always)]
    fn set_tx_threshold(&self, ch: u8, words: u16) {
        if ch as usize >= TX_CHANNELS {
            return;
        }
        // **The classic's `tx_lim` is a repeating count, not a position.**
        //
        // The PAC puts it plainly: "when channel N sends more than
        // `tx_lim` datas then channel N produces the relative interrupt" — a
        // count of *entries sent*, which re-arms itself, so a fixed value
        // fires once per that many words for the whole frame. esp-hal's own
        // classic driver relies on exactly that: it programs
        // `memsize.codes() / 2` once at the start of a transmission and never
        // touches the register again.
        //
        // The driver core, written against the S3 where the threshold names a
        // *word offset in the window*, alternates its request between the half
        // size and the full window so the event lands at each half boundary in
        // turn. Passing that alternation through unchanged asks this chip for
        // an event every 128 words instead of every 64 — the second refill then
        // arrives a whole half late and the transmitter walks into the guard
        // word planted by the first. Measured before this clamp: `guard_trips`
        // exactly equal to `frames` on every channel whose frame outgrew one
        // RAM window (8/16/100/256 LEDs), with `refill_lag_avg_words` a
        // comfortable 7.0 — the refills were not late, they were *asked for*
        // late.
        //
        // So the request is clamped to the channel's half size, which is the
        // only period that produces the boundary events the core expects. A
        // request smaller than a half (the core never makes one today) is
        // passed through, so the test hook that suppresses a threshold still
        // behaves as written.
        let half = (TX_PLAN.window_words(ch, BLOCK_WORDS) / 2) as u16;
        let period = if half == 0 { words } else { words.min(half) };
        // A note on what was tried here, so it is not tried again blind.
        // Writing `CH_TX_LIM` restarts the channel's entry counter, so the
        // core's once-per-refill call re-arms mid-frame — a plausible cause of
        // the multi-channel truncation `sweep_channels` measures (see the
        // README). Caching the value and skipping the redundant writes was
        // tried on silicon, both with and without a per-frame re-arm in
        // `start_tx`. **Neither fixed it**: channel 2 went clean and channel 1
        // got *worse* (10.0 -> 4.0 refills per frame, a guard skip on every
        // frame), so the rewrite shifts the failure without causing it. The
        // unconditional write is kept because it is the form the loopback
        // suite and the golden vectors were validated against.
        // SAFETY (register): `tx_lim` is a plain 9-bit field; the value is
        // masked to that width, so no reserved bits are disturbed. The PAC
        // marks the setter unsafe only because it cannot check the width.
        RMT::regs()
            .ch_tx_lim(ch as usize)
            .modify(|_, w| unsafe { w.tx_lim().bits(period & TX_LIM_MAX) });
    }

    // `always`: must land inside the IRAM-sectioned refill path (see the
    // `isr-in-ram` feature in lp-ws281x) — an outlined copy would sit in
    // flash and reintroduce the cross-core cache-stall this exists to avoid.
    #[inline(always)]
    fn read_pos(&self, ch: u8) -> u16 {
        let window = TX_PLAN.window_words(ch, BLOCK_WORDS);
        if window == 0 {
            return 0;
        }
        let absolute = RMT::regs()
            .chstatus(ch as usize)
            .read()
            .mem_raddr_ex()
            .bits();
        // `mem_raddr_ex` counts from the start of the *whole* RMT RAM — same
        // absolute-offset semantics as the S3, different register name
        // (`CHnSTATUS` here, `CH_TX_STATUS` there); esp-hal's classic
        // `hw_offset()` subtracts the same term. The modulo keeps a reading
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
        // width; the mask only names this channel's TX event bits.
        rmt.int_clr()
            .write(|w| unsafe { w.bits(tx_event_mask(ch)) });

        // Reset the channel clock divider so the first pulse is a full tick
        // rather than the remainder of one already in progress. Unlike the
        // S3's shared `REF_CNT_RST` bitmask register, this is a per-channel
        // bit in CHnCONF1 (bit 16, PAC `ref_cnt_rst`).
        rmt.chconf1(idx).modify(|_, w| w.ref_cnt_rst().set_bit());
        rmt.chconf1(idx).modify(|_, w| w.ref_cnt_rst().clear_bit());

        // A window wider than one block extends into the *following* channels'
        // blocks; each of those blocks is owned via its own channel's
        // `mem_owner` bit, so hand every one of them to the transmitter —
        // esp-hal's classic `start_tx` does exactly this dance.
        for extra in 1..TX_PLAN.blocks(ch) {
            rmt.chconf1(idx + extra as usize)
                .modify(|_, w| w.mem_owner().clear_bit());
        }

        // Single-shot, wrapping around the window (wrap enable itself is the
        // global `APB_CONF` bit set once by `init_tx`): the driver keeps
        // refilling behind the read pointer and ends the frame with a STOP
        // word. `mem_rd_rst` rewinds the transmitter to word 0 of the window,
        // `apb_mem_rst` the APB side; both are write-to-trigger, there is no
        // `conf_update` on this chip, and `mem_owner` clear = transmitter owns
        // the RAM.
        rmt.chconf1(idx).modify(|_, w| {
            w.tx_conti_mode().clear_bit();
            w.mem_owner().clear_bit();
            w.mem_rd_rst().set_bit();
            w.apb_mem_rst().set_bit();
            w.tx_start().set_bit()
        });
    }

    fn stop_tx(&self, ch: u8) {
        if ch as usize >= TX_CHANNELS {
            return;
        }
        // The classic ESP32 has no immediate-stop bit
        // (`rmt.has_tx_immediate_stop` = false; the S3's `tx_stop` does not
        // exist in CHnCONF1). The only way to stop a transmitter is the way
        // esp-hal does it: fill the channel's whole RAM window with STOP
        // markers so the wrap-mode reader lands on one within a word. An
        // all-zero word is the STOP marker, so this doubles as leaving the
        // channel in the safest possible state — and it is harmless on an
        // already-idle channel, which the driver's cleanup path relies on.
        for word in 0..TX_PLAN.window_words(ch, BLOCK_WORDS) {
            if let Some(ptr) = ram_word(ch, word) {
                // SAFETY: in-range, aligned RMT RAM pointer from `ram_word`;
                // volatile because the transmitter reads this memory
                // concurrently — that race is the mechanism, not a hazard: an
                // aligned u32 store is single-copy-atomic and every value the
                // reader can observe (old word or STOP) is a valid pulse word.
                unsafe { ptr.write_volatile(0) };
            }
        }
    }

    // `always`: must land inside the IRAM-sectioned refill path (see the
    // `isr-in-ram` feature in lp-ws281x) — an outlined copy would sit in
    // flash and reintroduce the cross-core cache-stall this exists to avoid.
    #[inline(always)]
    fn take_interrupts(&self) -> InterruptFlags {
        let rmt = RMT::regs();
        // `int_st` is `int_raw & int_ena`, so causes this firmware never asked
        // for (RX events, other channels) cannot reach the driver.
        let status = rmt.int_st().read().bits();

        let mut end = 0u32;
        let mut error = 0u32;
        let mut threshold = 0u32;
        let mut pending = 0u32;
        // The interleaved bit layout has no contiguous per-event field to mask
        // out in one shift (the S3 does); eight compares are cheap next to the
        // register read itself.
        let mut ch = 0u8;
        while (ch as usize) < TX_CHANNELS {
            let one = 1u32 << ch;
            if status & int_tx_end_bit(ch) != 0 {
                end |= one;
                pending |= int_tx_end_bit(ch);
            }
            if status & int_err_bit(ch) != 0 {
                error |= one;
                pending |= int_err_bit(ch);
            }
            if status & int_thr_bit(ch) != 0 {
                threshold |= one;
                pending |= int_thr_bit(ch);
            }
            ch += 1;
        }

        if pending != 0 {
            // Acknowledge exactly what is being reported, in the same handler
            // pass — the driver's contract is that no cause is lost between
            // the read and the clear.
            // SAFETY (register): write-1-to-clear; `pending` is a subset of
            // the bits just read from `int_st`.
            rmt.int_clr().write(|w| unsafe { w.bits(pending) });
        }

        InterruptFlags {
            threshold,
            end,
            error,
        }
    }
}

/// One-time TX-side peripheral setup: enable wrap mode.
///
/// ⚠️ **Load-bearing.** On the classic ESP32 wrap enable is the **global**
/// `APB_CONF.mem_tx_wrap_en` bit rather than a per-channel conf bit, and
/// without it the ping-pong refill scheme does not work at all (experiment
/// findings.md §11.5): the transmitter runs past the end of the window instead
/// of wrapping onto the half the driver has just refilled. Every transmitting
/// channel here wants wrap, so it is set once at init instead of read-modified
/// on every `start_tx` (which would put an unnecessary shared-register RMW on
/// the frame-start path). `apb_fifo_mask` (bit 0, 1 = direct RAM access rather
/// than FIFO) is already set by esp-hal's `Rmt::new`.
pub fn init_tx() {
    RMT::regs()
        .apb_conf()
        .modify(|_, w| w.mem_tx_wrap_en().set_bit());
}

/// Enable `tx_end`, `err` and `tx_thr_event` for `ch` in `INT_ENA`.
///
/// `chN_err` is the chip's combined TX/RX error bit; enabling it on a channel
/// this firmware transmits on cannot surface RX causes, because a channel is
/// TX-or-RX and this one is TX.
pub fn enable_tx_interrupts(ch: u8) {
    if ch as usize >= TX_CHANNELS {
        return;
    }
    RMT::regs().int_ena().modify(|_, w| {
        w.ch_tx_end(ch).set_bit();
        w.ch_err(ch).set_bit();
        w.ch_tx_thr_event(ch).set_bit()
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

/// The GPIO-matrix output signal RMT slot `slot` drives, or `None` past the
/// slot range. Classic-ESP32 output signal indices 87..=94
/// (`esp-metadata-generated` `_generated_esp32.rs`, `OutputSignal`).
fn rmt_output_signal(slot: u8) -> Option<esp_hal::gpio::OutputSignal> {
    use esp_hal::gpio::OutputSignal as S;
    Some(match slot {
        0 => S::RMT_SIG_0,
        1 => S::RMT_SIG_1,
        2 => S::RMT_SIG_2,
        3 => S::RMT_SIG_3,
        4 => S::RMT_SIG_4,
        5 => S::RMT_SIG_5,
        6 => S::RMT_SIG_6,
        7 => S::RMT_SIG_7,
        _ => return None,
    })
}

/// Route RMT slot `slot`'s output through the GPIO matrix onto pad `gpio`.
///
/// This is the per-transmission half of the wire/slot decoupling: the pool
/// binds a silicon transmitter to whichever wire's pad is about to carry a
/// frame. The pad's *GPIO-function* level is parked low first, so the moment
/// the matrix switches sources the worst the strip sees is a continuing low —
/// the RMT signal itself idles low (`idle_output` in the channel config), so
/// there is no glitch edge in either direction.
///
/// # Safety (caller contract)
///
/// The caller must hold the registry lease for `gpio` (every path here goes
/// through an output's claimed lease), and `gpio` must be a valid, exposed
/// pad — both are the same preconditions the old `with_pin` bind had. Two
/// pads may briefly both point at one slot's signal mid-handover; that is
/// harmless (the matrix fans out) and resolved by parking the old pad first.
pub fn route_rmt_to_gpio(slot: u8, gpio: u8) {
    let Some(signal) = rmt_output_signal(slot) else {
        return;
    };
    use esp_hal::gpio::interconnect::PeripheralOutput;
    // SAFETY: exclusivity for this pad is the registry's (caller holds the
    // lease); the pad number was validated against the chip's pin table
    // before any lease could be granted, so `steal` cannot panic.
    let mut pin = esp_hal::gpio::Flex::new(unsafe { esp_hal::gpio::AnyPin::steal(gpio) });
    pin.set_low();
    pin.apply_output_config(&esp_hal::gpio::OutputConfig::default());
    pin.set_output_enable(true);
    pin.connect_peripheral_to_output(signal);
}

/// Park pad `gpio`: plain GPIO function, output enabled, driven **solid
/// low**.
///
/// A WS281x strand whose pad is left floating or routed to a signal that
/// later moves sees garbage edges; a parked pad holds the strand's latched
/// frame indefinitely. Called when a wire's slot is handed to another wire,
/// when an output opens (so the strand idles clean before its first frame),
/// and when an output drops.
///
/// # Safety (caller contract)
///
/// Same as [`route_rmt_to_gpio`]: the caller holds the pad's lease.
pub fn park_gpio(gpio: u8) {
    use esp_hal::gpio::interconnect::PeripheralOutput;
    // SAFETY: as in `route_rmt_to_gpio` — lease-held, table-validated pad.
    let mut pin = esp_hal::gpio::Flex::new(unsafe { esp_hal::gpio::AnyPin::steal(gpio) });
    pin.set_low();
    pin.set_output_enable(true);
    pin.disconnect_from_peripheral_output();
}
