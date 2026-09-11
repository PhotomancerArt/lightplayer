//! `RMT` at `0x3FF5_6000` — the classic's remote-control transceiver as the
//! WS281x path drives it: the PAC register file, the **512-word RAM at
//! `+0x800`** (`0x3FF5_6800`, eight 64-word blocks), and up to eight TX
//! engines on the scheduler that consume words at the channel's clock and
//! raise `ch<N>_tx_end` / `ch<N>_tx_thr_event` / `ch<N>_err` on interrupt
//! source **47**.
//!
//! Every offset comes out of [`crate::regs::RMT`] (62 registers, esp32 PAC
//! 0.40.2) and every bit position out of that PAC's field docs, cited per
//! register below. The two drivers that write the block are esp-hal 1.1.1's
//! `chip_specific` module for `any(esp32, esp32s2)`
//! (`third_party/esp-hal/src/rmt.rs:2863-3190`) and this project's
//! `lp-fw/fw-esp32v3/src/output/rmt/v3_rmt.rs` backend for `lp-ws281x`.
//!
//! # ⚠️ This is not the C6's block, and three of its semantics are inverted
//!
//! The C6's `lp-emu-esp32c6/src/periph/rmt.rs` is the **shape** of this file
//! — a register file, per-channel TX engines on the scheduler, a pulse log,
//! a word log, [`RefillStats`] — and its *tick arithmetic* ports unchanged
//! because arithmetic is not layout. Its **semantics do not port**, and a
//! model that borrowed them would pass every register test and still be
//! wrong on the wire (plan decision D2, M4 ruling R8: this view is written
//! fresh and nothing is extracted into `lp-emu-esp-common`; the candidates
//! for M8 are named at the bottom of this doc).
//!
//! 1. **`tx_lim` is a repeating count, not a position in the window.** PAC
//!    `rmt/ch_tx_lim.rs:5`: *"When channel0 sends more than
//!    reg_rmt_tx_lim_ch0 datas then channel0 produce the relative
//!    interrupt."* It counts words **sent** and re-arms itself, so one
//!    programmed value fires every `tx_lim` words for the whole frame. The
//!    C6's names a word offset in the window
//!    (`lp-emu-esp32c6/src/periph/rmt.rs:27-32`). The firmware knows, and
//!    clamps the driver core's alternating half/window request down to a
//!    fixed period because of it (`v3_rmt.rs:398-420`, with the measurement
//!    that proved it: `guard_trips` exactly equal to `frames` on every
//!    channel whose frame outgrew one window). Copying the C6's semantics
//!    here truncates every frame. See [`TxEngine::since_thr`] for where the
//!    counter is reset and what is *not* modelled about it.
//! 2. **`apb_conf.mem_tx_wrap_en` is global** — one bit (bit 1) for all
//!    eight channels (PAC `rmt/apb_conf.rs:19`), not a per-channel
//!    `mem_tx_wrap_en` in `chNconf0` as on the C6. Without it the
//!    transmitter runs off the end of the window instead of wrapping onto
//!    the half the driver has just refilled, and ping-pong refill does not
//!    work at all (`v3_rmt.rs:66-72`).
//! 3. **There is no `conf_update`.** Writes take effect immediately and
//!    esp-hal's `update()` is a literal no-op on this chip
//!    (`third_party/esp-hal/src/rmt.rs:2944-2946`; `v3_rmt.rs:52-56`). The
//!    C6 model's "pulses take effect at `conf_update`" rule is **not**
//!    carried over: a `tx_start` write starts the channel then and there.
//!
//! Two more the classic has that the C6 does not:
//!
//! 4. **There is no `tx_stop` bit** (`rmt.has_tx_immediate_stop = false`;
//!    esp-hal's `stop_tx` for this chip fills the window with end markers,
//!    `rmt.rs:2959-2966`). A stop is a window full of end markers and the
//!    transmitter halts at the next word boundary with `tx_end`
//!    (`v3_rmt.rs:73-77`).
//! 5. **The read pointer is absolute.** Ten bits over all 512 words; the
//!    channel's window begins at `64 × first_block` and the driver subtracts
//!    it (`v3_rmt.rs:78-81`, `read_pos`; esp-hal's `hw_offset`,
//!    `rmt.rs:3183-3189`).
//!
//! ⚠️ **and where the read pointer lives is not where the phase brief said
//! it was.** The PAC puts `MEM_WADDR_EX` at **bits 0:9** and `MEM_RADDR_EX`
//! at **bits 12:21** (`esp32-0.40.2/src/rmt/chstatus.rs:27-35`) — the
//! opposite of the layout M4's notes carried, and the SVD's own field
//! *descriptions* are swapped on top of that (`MEM_WADDR_EX` is documented
//! as *"The current memory read address"*). What settles it is that the
//! firmware and esp-hal both read the pointer through the **accessor named
//! `mem_raddr_ex`**, which is bits 12:21 — so that is where this view
//! publishes it. A model that had followed the brief would have handed
//! `read_pos` a constant zero and every refill would have filled the half
//! the transmitter was standing in.
//!
//! # Two more rules the firmware's cross-core design depends on
//!
//! - `int_clr` is **write-only and W1C**: each write clears exactly the bits
//!   it names, which is what makes it race-free with the ISR on core 1 and
//!   thread context on core 0 both writing it (`v3_rmt.rs:92-95`).
//! - `mem_owner` (`chNconf1` bit 5) is cleared by `start_tx` for the channel
//!   **and for every extra block its window extends into** (`v3_rmt.rs`'s
//!   `start_tx`; esp-hal's `rmt.rs:3077-3080`). It is recorded and never
//!   enforced, and `chNstatus.mem_owner_err` never rises: this model has one
//!   RAM and no arbiter.
//!
//! # The engine, and the timing arithmetic
//!
//! One word is two pulses of `(dur1, level1)` / `(dur2, level2)` with `dur`
//! in **channel ticks** (15 bits each, level in bits 15 and 31 — the same
//! encoding `lp_ws281x::pulse` writes, `lp-fw/lp-ws281x/src/pulse.rs:105`).
//! The next word is due at `start_cycle + cycles_for(ticks_since_start)`,
//! integer arithmetic over the **absolute** tick count since `tx_start` —
//! never accumulated per word, never taken from the dispatch cycle (the
//! `uart.rs` `tx_due` rule). The RAM is **live**: the engine reads
//! `ram[raddr]` when it fetches, so a refill overwrites what the consumer
//! has not fetched yet. That is what a single-ported RAM does and it is what
//! the driver's guard word exists for.
//!
//! An end marker — a word whose **first** duration field is zero — ends the
//! transmission before it; a zero **second** half emits half 1 and then ends
//! it. Both drivers only ever write the all-zero word.
//!
//! The clock is APB at [`super::APB_HZ`] (80 MHz) with a per-channel
//! `div_cnt`; the firmware asks for exactly 80 MHz because esp-hal's classic
//! `validate_clock` accepts only the source frequency
//! (`rmt.rs:2886-2892`, `shared_driver.rs:33-39`) and a divider of 1, so one
//! tick is 12.5 ns. The CPU runs at [`memmap::CPU_HZ`] (240 MHz), so one
//! tick at `div_cnt = 1` is three CPU cycles — computed from the two
//! constants, never written as a 3.
//!
//! # Register grades
//!
//! | grade | registers |
//! |---|---|
//! | `measured` | none. A waveform is not a register's bit map; M5 earns the first measured claim on this chip. |
//! | `documented` | `int_raw`, `int_st`, `int_ena`, `int_clr`, `ch*conf0`, `ch*conf1`, `ch*status`, `ch*_tx_lim`, `apb_conf` — the PAC's bit map read out loud. |
//! | `modeled` | `ch*data` and `ch*addr` (the APB FIFO, not modelled), `ch*carrier_duty` (no carrier), `date`. Accept-and-remember at the PAC's reset. |
//!
//! # What is not here
//!
//! - **RX.** The classic firmware never sets `rx_en` (`rmt_rx` is the C6's
//!   payload). The bits are accepted and remembered; `rx_en` set on a
//!   channel is a `log::warn!` naming the channel, not a model.
//! - **Carrier** modulation: `chNconf0.carrier_en` / `chNcarrier_duty` are
//!   accepted at the PAC's reset with one note. The driver turns it off
//!   (`esp32v3_rmt_ws281x_driver.rs:213`).
//! - **The APB FIFO** (`chNdata`, `chNaddr`): `apb_conf.apb_fifo_mask` is
//!   what esp-hal's `Rmt::new` sets (`rmt.rs:2903`) and direct RAM access is
//!   the only path this firmware uses.
//! - **The decoder and any frame claim** — M4 P3 and P4.
//!
//! # For M8, not for now
//!
//! Three things in this file are the C6's file's too, and would survive an
//! extraction into `lp-emu-esp-common` once a third chip asks: the tick
//! arithmetic ([`Clock::cycles_for`] over absolute ticks), the pulse/word
//! observation logs, and [`RefillStats`] with [`lag_bucket`]. **Nothing is
//! extracted here** (D2, R8) — this is the note the ruling asked for.

use lp_emu_core::sched::{Cycles, EventId};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{
    BusCx, Peripheral, RegFile, RegGrade, RegGrades, SignalId, Width, event_id, event_local,
};

use crate::memmap;
use crate::regs;

// ---- the layout ---------------------------------------------------------
//
// Every offset below is the one `regs::RMT` carries; `rmt_registers.rs`
// asserts the name at each against that table, and `just lint-emu-regnames`
// checks the table against the PAC.

/// TX channels the classic has. All eight can transmit — there is no fixed
/// TX/RX split on this chip (`v3_rmt.rs:44-49`).
pub const TX_CHANNELS: usize = 8;
/// Words in one RMT memory block (`esp-metadata-generated`
/// `rmt.channel_ram_size` = 64; `v3_rmt.rs:132`).
pub const BLOCK_WORDS: u32 = 64;
/// The whole RMT RAM: eight blocks of 64 words.
pub const RAM_WORDS: usize = BLOCK_WORDS as usize * TX_CHANNELS;
/// The RAM's offset from the block's base: `rmt.ram_start` is `0x3FF5_6800`
/// and the base is `0x3FF5_6000` ([`memmap::periph::RMT_RAM`],
/// `v3_rmt.rs:207`).
pub const RAM_OFFSET: u32 = 0x800;
/// The PAC register block: `ch0data` (`+0x000`) through `date` (`+0x0fc`).
pub const REGS_LEN: u32 = 0x100;
/// The whole window the bus maps: registers, the gap, the RAM. Unchanged
/// from the accept block M3 P8 registered here (`accept::RMT_LEN`), so the
/// aperture a strict run was proved against does not move.
pub const LEN: u32 = 0x1000;

/// `chNdata` — the APB FIFO port, `+0x000 + 4n`.
const CH_DATA_END: u32 = 0x020;
/// `chNconf0`, `+0x020 + 8n` — a **stride-8 pair** with `chNconf1`.
const fn ch_conf0(ch: usize) -> u32 {
    0x020 + 8 * ch as u32
}
/// `chNconf1`, `+0x024 + 8n`.
const fn ch_conf1(ch: usize) -> u32 {
    0x024 + 8 * ch as u32
}
/// `chNstatus`, `+0x060 + 4n` — read-only in the PAC, derived here.
const fn ch_status(ch: usize) -> u32 {
    0x060 + 4 * ch as u32
}
/// `chNaddr`, `+0x080 + 4n` — read-only, the APB FIFO's relative address.
const CH_ADDR: u32 = 0x080;
const CH_ADDR_END: u32 = 0x0a0;
const INT_RAW: u32 = 0x0a0;
const INT_ST: u32 = 0x0a4;
const INT_ENA: u32 = 0x0a8;
const INT_CLR: u32 = 0x0ac;
/// `chNcarrier_duty`, `+0x0b0 + 4n`.
const CH_CARRIER_DUTY: u32 = 0x0b0;
const CH_CARRIER_DUTY_END: u32 = 0x0d0;
/// `chN_tx_lim`, `+0x0d0 + 4n`.
const fn ch_tx_lim(ch: usize) -> u32 {
    0x0d0 + 4 * ch as u32
}
const APB_CONF: u32 = 0x0f0;

// `chNconf0` bits (`esp32-0.40.2/src/rmt/chconf0.rs:34-64`).
const CONF0_DIV_CNT_MASK: u32 = 0xff;
const CONF0_MEM_SIZE_SHIFT: u32 = 24;
const CONF0_MEM_SIZE_MASK: u32 = 0xf;
const CONF0_CARRIER_EN: u32 = 1 << 28;
/// PAC reset: `div_cnt 2`, `idle_thres 0x1000`, `mem_size 1`, `carrier_en`
/// and `carrier_out_lv` set. Seeded from `regs::RMT`'s `resets` table, not
/// from this constant — it is here so `tests/rmt_registers.rs` can assert
/// the two agree.
pub const CH_CONF0_RESET: u32 = 0x3110_0002;

// `chNconf1` bits (`esp32-0.40.2/src/rmt/chconf1.rs:58-118`).
const CONF1_TX_START: u32 = 1 << 0;
const CONF1_RX_EN: u32 = 1 << 1;
const CONF1_MEM_WR_RST: u32 = 1 << 2;
const CONF1_MEM_RD_RST: u32 = 1 << 3;
const CONF1_APB_MEM_RST: u32 = 1 << 4;
const CONF1_MEM_OWNER: u32 = 1 << 5;
const CONF1_TX_CONTI_MODE: u32 = 1 << 6;
const CONF1_REF_CNT_RST: u32 = 1 << 16;
/// Bit 17, *"This bit is used to select base clock. 1'b1:clk_apb
/// 1'b0:clk_ref"*.
const CONF1_REF_ALWAYS_ON: u32 = 1 << 17;
const CONF1_IDLE_OUT_LV: u32 = 1 << 18;
const CONF1_IDLE_OUT_EN: u32 = 1 << 19;
/// The **write-to-trigger** bits: *"Set this bit to …"* in the PAC, and they
/// read back 0.
///
/// **Modeled, and the driver is the evidence.** Nothing in the PAC says
/// these self-clear, but both writers read-modify-write `chNconf1` — the
/// firmware's `start_tx` does it three times per frame and esp-hal's does it
/// twice (`v3_rmt.rs`'s `start_tx`, `rmt.rs:3071-3086`). If `tx_start` read
/// back 1, the first `ref_cnt_rst` modify of the *next* frame would rewrite
/// it and restart a channel nobody started, and `mem_rd_rst` would rewind a
/// running transmitter. Silicon runs this firmware for 240-second soaks, so
/// the bits self-clear. `ref_cnt_rst` is deliberately **not** in this set:
/// the firmware sets it and then clears it in two separate writes, which
/// only makes sense for a bit that holds.
const CONF1_STROBES: u32 = CONF1_TX_START | CONF1_MEM_WR_RST | CONF1_MEM_RD_RST | CONF1_APB_MEM_RST;
/// PAC reset: `mem_owner 1`, `rx_filter_thres 15` — and, note,
/// `ref_always_on` **clear**. Seeded from `regs::RMT`; see
/// [`CH_CONF0_RESET`].
pub const CH_CONF1_RESET: u32 = 0x0000_0f20;

// `chNstatus` fields (`esp32-0.40.2/src/rmt/chstatus.rs:27-64`). See the
// module docs on where the read pointer actually lives.
/// `mem_waddr_ex`, bits 0:9.
const STATUS_WADDR_MASK: u32 = 0x3ff;
/// `mem_raddr_ex`, bits **12:21** — the transmitter's read pointer, absolute
/// over all 512 words, and what `v3_rmt::read_pos` reads.
const STATUS_RADDR_SHIFT: u32 = 12;
const STATUS_RADDR_MASK: u32 = 0x3ff;
/// `state`, bits 24:26: *"3'h0 : idle, 3'h1 : send, 3'h2 : read memory,
/// 3'h3 : receive, 3'h4 : wait"*.
const STATUS_STATE_SHIFT: u32 = 24;
const STATUS_STATE_SEND: u32 = 1;
/// `mem_empty`, bit 29.
const STATUS_MEM_EMPTY: u32 = 1 << 29;

/// `chN_tx_lim.tx_lim`, bits 0:8 — nine bits, max 511, which is why the
/// firmware caps a window at four blocks (`v3_rmt.rs:136-146`).
const TX_LIM_MASK: u32 = 0x1ff;

// `apb_conf` bits (`esp32-0.40.2/src/rmt/apb_conf.rs:14-19`).
const APB_CONF_FIFO_MASK: u32 = 1 << 0;
/// Bit 1 — **global**, one bit for all eight channels.
const APB_CONF_MEM_TX_WRAP_EN: u32 = 1 << 1;

/// The largest duration one half of a word can hold: 15 bits.
const DURATION_MAX: u32 = 0x7fff;

/// `ch<N>_tx_end` — three bits per channel, interleaved **by channel**.
///
/// PAC `rmt/int_raw.rs:27-62`: bit 0 is channel 0's, bit 3 channel 1's, up
/// to bit 21 for channel 7. `v3_rmt.rs:225-241` computes with exactly these.
pub const fn int_tx_end_bit(ch: usize) -> u32 {
    1 << (3 * ch)
}

/// `ch<N>_rx_end` (`rmt/int_raw.rs:82-117`). Never raised by this view — the
/// receivers are not modelled — but named so the mask arithmetic is legible.
pub const fn int_rx_end_bit(ch: usize) -> u32 {
    1 << (3 * ch + 1)
}

/// `ch<N>_err` — a **combined TX/RX error** bit (`rmt/int_raw.rs:137-172`).
/// There is no separate `tx_err` on this chip.
pub const fn int_err_bit(ch: usize) -> u32 {
    1 << (3 * ch + 2)
}

/// `ch<N>_tx_thr_event` — a flat block at the top, bits 24..=31
/// (`rmt/int_raw.rs:192-227`).
pub const fn int_thr_bit(ch: usize) -> u32 {
    1 << (24 + ch)
}

/// Every bit `int_ena` can hold: the three-per-channel block fills bits
/// 0..=23 and the threshold block bits 24..=31, so **all 32 are defined**
/// and nothing is masked away. Named rather than left implicit so a reader
/// does not go looking for the mask the C6 has.
const INT_MASK: u32 = u32::MAX;

/// The classic's `RMT` interrupt source: **47**
/// (`esp32-0.40.2/src/lib.rs:305-306`, `RMT = 47`).
pub const SOURCE_RMT: u16 = 47;

/// The GPIO-matrix **output** signal TX channel `ch` drives.
///
/// Classic-ESP32 output signal indices **87..=94** —
/// `OutputSignal::RMT_SIG_0..RMT_SIG_7`
/// (`esp-metadata-generated-0.4.0/src/_generated_esp32.rs:4428`, used by
/// `v3_rmt::rmt_output_signal`; M3 P8's GPIO test already pins 87 for gpio18
/// at `periph/gpio.rs:1092-1094`).
///
/// ⚠️ **Not the input number.** `InputSignal::RMT_SIG_0` is **83** on this
/// chip (`_generated_esp32.rs:4255`), so 87 is `RMT_SIG_4` on the input
/// side. The two spaces are separate tables and must stay separate; this is
/// the output table and the classic has no RMT RX in this plan.
pub const RMT_SIG_0: u16 = 87;

/// The signal TX channel `ch` drives. Whether any pad listens is the
/// fabric's business (`GPIO.func_out_sel_cfg`), and the RMT never learns the
/// answer — it cannot see the GPIO block.
pub const fn signal_of(ch: usize) -> SignalId {
    SignalId(RMT_SIG_0 + ch as u16)
}

/// Events: `EV_WORD + ch` — the current word's pulses end.
const EV_WORD: u16 = 0;

/// Log caps: generous — a 300-LED frame is 7,201 words / 14,402 pulses, so
/// these hold dozens of frames — with a note when hit.
pub const PULSE_LOG_CAP: usize = 4_000_000;
pub const WORD_LOG_CAP: usize = 2_000_000;

/// Buckets in a refill histogram: eighths of a half-window, plus one for
/// "≥ half".
///
/// The same nine buckets and the same edges as `lp_ws281x::LAG_BUCKETS`, so
/// this block's histogram and the guest's own `hist=` / `entry_hist=` in the
/// `[WS281X]` telemetry line can be printed side by side and read as one
/// shape — two measurements of one race, one from inside the ISR and one
/// from the transmitter it is racing.
pub const LAG_BUCKETS: usize = 9;

/// Which eighth of `half` `words` falls in; `LAG_BUCKETS - 1` for `≥ half`.
pub fn lag_bucket(words: u64, half: u32) -> usize {
    if half == 0 {
        return LAG_BUCKETS - 1;
    }
    ((words * 8) / u64::from(half)).min(LAG_BUCKETS as u64 - 1) as usize
}

/// What one channel's refills cost, in words — this block's own reading of
/// the race the `[WS281X]` line reports from the other side.
///
/// **Reported, never gated** (D13/PD9). The classic wants it more than the
/// C6 did, because the classic's ISR-throughput ceiling (~46–55 k/s,
/// `v3_rmt.rs:170-178`) is the number its whole block plan is designed
/// around.
///
/// * `entry` — from the `tx_thr_event` to the guest's next `chN_tx_lim`
///   write for that channel. On silicon that is interrupt latency plus
///   esp-hal's dispatch, which is where the flash misses live; here the ISR
///   path is RAM-resident by construction and the machine has no flash-miss
///   cost, so `t1`'s figure is a **floor**, not a prediction.
/// * `fill` — from that write to the last RAM write inside the channel's
///   window before the next threshold or the end of the frame: the
///   `fill_half` loop itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefillStats {
    /// Threshold events whose refill was measured end to end.
    pub refills: u64,
    pub entry_max: u64,
    pub entry_hist: [u64; LAG_BUCKETS],
    pub fill_max: u64,
    pub fill_hist: [u64; LAG_BUCKETS],
    /// Threshold events where the guest never wrote `chN_tx_lim` before the
    /// next threshold or the end of the frame. Not an error — the last
    /// threshold of a frame is answered by `finish`, not by `refill` — but a
    /// number that should stay small.
    pub unanswered: u64,
    /// The half-window the buckets were computed against, latched at the
    /// first measured refill. Zero until then.
    pub half_words: u32,
}

impl RefillStats {
    fn record(&mut self, entry: u64, fill: u64, half: u32) {
        self.refills += 1;
        self.half_words = half;
        self.entry_max = self.entry_max.max(entry);
        self.fill_max = self.fill_max.max(fill);
        self.entry_hist[lag_bucket(entry, half)] += 1;
        self.fill_hist[lag_bucket(fill, half)] += 1;
    }

    /// `a:b:c:…:i`, the shape the `[WS281X]` line prints its histograms in.
    pub fn hist_string(hist: &[u64; LAG_BUCKETS]) -> String {
        hist.iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(":")
    }
}

/// A refill measurement in flight on one channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RefillProbe {
    #[default]
    Idle,
    /// A threshold fired at `at` with the engine `words` into the run;
    /// waiting for the ISR's `chN_tx_lim` write.
    AwaitingLimit { at: Cycles, words: u64 },
    /// The ISR answered; measuring how far the transmitter gets before the
    /// last word of the refill lands.
    Filling {
        at: Cycles,
        entry: u64,
        words: u64,
        /// `words_consumed` as of the last RAM write in this channel's
        /// window; starts equal to `words`, so a refill that writes nothing
        /// measures zero.
        last_write: u64,
    },
}

/// One level held for `ticks` channel ticks from cycle `at`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pulse {
    pub at: Cycles,
    pub level: bool,
    pub ticks: u16,
}

/// The channel's base clock, as `chNconf1.ref_always_on` selects it.
///
/// *"1'b1:clk_apb 1'b0:clk_ref"* (`rmt/chconf1.rs:45-47`). esp-hal writes
/// the bit from its `ClockSource` for every channel in `configure_clock`
/// (`rmt.rs:2895-2900`); the classic's sources are `RefTick = 0` and
/// `Apb = 1` with `Apb` the default (`_generated_esp32.rs:436-444`), and the
/// firmware asks for 80 MHz, which is APB.
///
/// ⚠️ The PAC's reset for `chNconf1` is `0x0000_0f20`, so bit 17 is **clear**
/// out of reset — a channel that has never been configured selects
/// `clk_ref`, not APB. That is the opposite of what M4's notes expected, and
/// it is why [`Clock::decode`] refuses rather than assuming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Clock {
    src_hz: u64,
}

impl Clock {
    /// The channel's base clock, or why there is none.
    ///
    /// `REF_TICK` is **refused rather than guessed at**, the way the C6
    /// refuses `sclk_sel = 2`: the classic's REF_TICK is derived from the
    /// crystal through `APB_CTRL.clk_conf`'s dividers, this machine does not
    /// model that tree, and a transmitter clocked at an invented rate would
    /// put a waveform on the wire that nothing could check.
    fn decode(conf1: u32) -> Result<Self, &'static str> {
        if conf1 & CONF1_REF_ALWAYS_ON == 0 {
            return Err("chNconf1.ref_always_on = 0 (REF_TICK): not modelled");
        }
        Ok(Self {
            src_hz: super::APB_HZ,
        })
    }

    /// CPU cycles for `ticks` channel ticks at `div_cnt`, exactly: floor of
    /// `ticks × CPU_HZ × div_cnt / src`. Applied to the **absolute** tick
    /// count since `tx_start`, so no per-word rounding ever accumulates.
    ///
    /// At the shipped settings (APB 80 MHz, `div_cnt = 1`, a 240 MHz CPU)
    /// one tick is exactly three cycles — which is the quotient of two
    /// named constants and never a literal.
    fn cycles_for(&self, ticks: u64, div_cnt: u32) -> Cycles {
        let n = u128::from(ticks) * u128::from(memmap::CPU_HZ) * u128::from(div_cnt);
        (n / u128::from(self.src_hz)) as Cycles
    }
}

/// Why the current word is the frame's last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    No,
    /// A zero second half: half 1 goes out, then `tx_end`.
    HalfTwoZero,
    /// The window ran out with `mem_tx_wrap_en` clear: `ch_err` was raised
    /// at the fetch; the engine stops when the word ends.
    Overrun,
}

/// One TX channel's engine.
#[derive(Debug)]
struct TxEngine {
    running: bool,
    /// Running, but with no base clock (`ref_always_on` clear): the word is
    /// held and nothing is emitted. Nothing in the firmware can reach this
    /// — esp-hal writes the bit for every channel before any transmission —
    /// so it is a diagnostic state, named rather than silently mis-timed.
    stalled: bool,
    /// Absolute word index into the 512-word RAM: the **next word to
    /// fetch**, which is what `chNstatus.mem_raddr_ex` reads back (esp-hal's
    /// `hw_offset`; the driver tolerates ±1 through `guard_skips`).
    raddr: u32,
    /// Ticks consumed since `tx_start`.
    ticks_since_start: u64,
    /// The anchor: tick `anchor_ticks` happened at cycle `anchor_cycle`.
    /// `tx_start` sets (0, start cycle); a stall/resume or a divider change
    /// re-anchors at the current word so nothing already emitted moves.
    anchor_cycle: Cycles,
    anchor_ticks: u64,
    anchor_clock: Option<Clock>,
    anchor_div: u32,
    /// The pending `EV_WORD`'s due cycle.
    word_due: Cycles,
    ending: Ending,
    mem_empty: bool,
    /// Words sent since the threshold counter was last armed.
    ///
    /// **This is the repeating count** (quirk 1). It is reset at `tx_start`
    /// and **at each threshold**, which is what "it re-arms itself" means:
    /// with a constant `tx_lim` the events land at exact multiples of it for
    /// the whole frame, which is the only behaviour under which the shipped
    /// driver's ping-pong refill can survive a 3,600-word frame.
    ///
    /// ⚠️ **Not modelled: a `chN_tx_lim` write does not reset it.**
    /// `v3_rmt.rs:411-413` asserts in passing that writing the register
    /// "restarts the channel's entry counter", with no PAC or TRM citation,
    /// inside a comment whose own conclusion is that the hypothesis built on
    /// it was **disproven on silicon**. If it were true the phase would
    /// advance by the ISR's entry delay at every refill — ~7 words measured
    /// (`refill_lag_avg_words` 7.0) against a 64-word half — and a 56-refill
    /// frame would walk out of its half well before it ended, which is not
    /// what the board does (five wires, 1,500 LEDs, zero guard trips, a
    /// 240-second soak; `../bench.md`). So the write changes the period and
    /// nothing else here. **P4's frame comparison is what settles it**, and
    /// if it disagrees this is the line to change.
    since_thr: u32,
    /// Words fetched in the current frame, for the `end` note.
    frame_words: u32,
    /// Words fetched since the machine started, on this channel — the unit
    /// both halves of the refill measurement count in, monotonic across
    /// frames so a probe that spans a frame boundary still subtracts
    /// correctly.
    words_consumed: u64,
    refill: RefillProbe,
    refill_stats: RefillStats,
    frames_ended: usize,
    pulses: Vec<Pulse>,
    words: Vec<(Cycles, u32)>,
    pulse_cap_noted: bool,
    word_cap_noted: bool,
    stall_noted: bool,
}

impl TxEngine {
    fn new(ch: usize) -> Self {
        Self {
            running: false,
            stalled: false,
            raddr: BLOCK_WORDS * ch as u32,
            ticks_since_start: 0,
            anchor_cycle: 0,
            anchor_ticks: 0,
            anchor_clock: None,
            anchor_div: 0,
            word_due: 0,
            ending: Ending::No,
            mem_empty: false,
            since_thr: 0,
            frame_words: 0,
            words_consumed: 0,
            refill: RefillProbe::Idle,
            refill_stats: RefillStats::default(),
            frames_ended: 0,
            pulses: Vec::new(),
            words: Vec::new(),
            pulse_cap_noted: false,
            word_cap_noted: false,
            stall_noted: false,
        }
    }

    /// The cycle at which absolute tick `ticks` falls, from the anchor.
    fn cycle_at(&self, clock: &Clock, div_cnt: u32, ticks: u64) -> Cycles {
        self.anchor_cycle
            .saturating_add(clock.cycles_for(ticks - self.anchor_ticks, div_cnt))
    }
}

/// A trace note, formatted only when the trace is on.
fn note(cx: &mut BusCx<'_>, f: impl FnOnce() -> String) {
    if cx.trace.is_enabled() {
        let line = f();
        cx.trace.note(&line);
    }
}

/// The classic's RMT block.
#[derive(Debug)]
pub struct Rmt {
    index: usize,
    regs: RegFile,
    grades: RegGrades,
    ram: Box<[u32; RAM_WORDS]>,
    ch: [TxEngine; TX_CHANNELS],
    /// Sticky `int_raw` bits, in the interleaved layout above.
    sticky: u32,
    warned_fifo: bool,
    warned_gap: bool,
    warned_rx: [bool; TX_CHANNELS],
    warned_carrier: [bool; TX_CHANNELS],
    warned_conti: [bool; TX_CHANNELS],
    warned_ref_cnt: [bool; TX_CHANNELS],
    /// Whether the pulse and word logs are kept. **Off by default**: a run
    /// that only wants a boot has no use for hundreds of thousands of
    /// pulses. `Esp32V3Builder::rmt_logs` turns them on.
    keep_logs: bool,
}

impl Rmt {
    pub fn new() -> Self {
        let regs = RegFile::new("RMT", LEN).with_names(regs::RMT);
        Self {
            index: 0,
            regs,
            grades: Self::grades(),
            ram: Box::new([0; RAM_WORDS]),
            ch: core::array::from_fn(TxEngine::new),
            sticky: 0,
            warned_fifo: false,
            warned_gap: false,
            warned_rx: [false; TX_CHANNELS],
            warned_carrier: [false; TX_CHANNELS],
            warned_conti: [false; TX_CHANNELS],
            warned_ref_cnt: [false; TX_CHANNELS],
            keep_logs: false,
        }
    }

    /// The block's register grades — see the module docs' table.
    ///
    /// Everything unlisted is `Modeled`, which for this block means
    /// accept-and-remember at the PAC's reset value. Nothing is `Measured`.
    pub fn grades() -> RegGrades {
        let mut g = RegGrades::new()
            .with_grade(INT_RAW, RegGrade::Documented)
            .with_grade(INT_ST, RegGrade::Documented)
            .with_grade(INT_ENA, RegGrade::Documented)
            .with_grade(INT_CLR, RegGrade::Documented)
            .with_grade(APB_CONF, RegGrade::Documented);
        for ch in 0..TX_CHANNELS {
            g = g
                .with_grade(ch_conf0(ch), RegGrade::Documented)
                .with_grade(ch_conf1(ch), RegGrade::Documented)
                .with_grade(ch_status(ch), RegGrade::Documented)
                .with_grade(ch_tx_lim(ch), RegGrade::Documented);
        }
        g
    }

    /// Keep the per-channel pulse and word logs. See [`Rmt::keep_logs`].
    pub fn set_keep_logs(&mut self, keep: bool) {
        self.keep_logs = keep;
    }

    pub fn keep_logs(&self) -> bool {
        self.keep_logs
    }

    // ---- observation -----------------------------------------------------

    /// Every pulse channel `ch` has put on its signal, in order.
    pub fn pulses(&self, ch: usize) -> &[Pulse] {
        &self.ch[ch].pulses
    }

    /// Every word channel `ch` fetched, with the cycle it was fetched at —
    /// end markers included, so a frame reads `data … latch STOP`.
    pub fn words(&self, ch: usize) -> &[(Cycles, u32)] {
        &self.ch[ch].words
    }

    /// `tx_end`s raised on channel `ch`.
    pub fn frames_ended(&self, ch: usize) -> usize {
        self.ch[ch].frames_ended
    }

    pub fn is_running(&self, ch: usize) -> bool {
        self.ch[ch].running
    }

    /// What channel `ch`'s refills have cost, in words.
    pub fn refill_stats(&self, ch: usize) -> RefillStats {
        self.ch[ch].refill_stats
    }

    /// The RAM as the engine sees it.
    pub fn ram(&self) -> &[u32; RAM_WORDS] {
        &self.ram
    }

    // ---- the channel's configuration -------------------------------------

    fn conf0(&self, ch: usize) -> u32 {
        self.regs.stored(ch_conf0(ch))
    }

    fn conf1(&self, ch: usize) -> u32 {
        self.regs.stored(ch_conf1(ch))
    }

    /// `chNconf0.div_cnt`, with 0 meaning 256 (the field is the divider's
    /// factor and a zero divider cannot be one).
    fn div_cnt(&self, ch: usize) -> u32 {
        match self.conf0(ch) & CONF0_DIV_CNT_MASK {
            0 => 256,
            n => n,
        }
    }

    /// The channel's window width in words: `chNconf0.mem_size` blocks.
    fn window_words(&self, ch: usize) -> u32 {
        BLOCK_WORDS * ((self.conf0(ch) >> CONF0_MEM_SIZE_SHIFT) & CONF0_MEM_SIZE_MASK)
    }

    /// The first word of channel `ch`'s window: `64 × ch`.
    const fn window_start(ch: usize) -> u32 {
        BLOCK_WORDS * ch as u32
    }

    /// The level the signal rests at between frames: `idle_out_lv` when
    /// `idle_out_en` is set, else low.
    pub fn idle_level(&self, ch: usize) -> bool {
        let c = self.conf1(ch);
        c & CONF1_IDLE_OUT_EN != 0 && c & CONF1_IDLE_OUT_LV != 0
    }

    /// `chNconf1.mem_owner` — *"1'b1：receiver uses the ram 0：transmitter
    /// uses the ram"* (`rmt/chconf1.rs:25`).
    ///
    /// **Recorded, never enforced.** `start_tx` clears it for the channel
    /// and for every extra block the window extends into, and this view
    /// reads it back; but the model has one RAM and no arbiter, so a
    /// transmission with `mem_owner` still set transmits anyway and
    /// `chNstatus.mem_owner_err` never rises. Said here rather than left for
    /// a reader to discover from an absence.
    pub fn mem_owner(&self, ch: usize) -> bool {
        self.conf1(ch) & CONF1_MEM_OWNER != 0
    }

    /// `apb_conf.mem_tx_wrap_en` — **global**, quirk 2.
    fn wrap(&self) -> bool {
        self.regs.stored(APB_CONF) & APB_CONF_MEM_TX_WRAP_EN != 0
    }

    fn tx_lim(&self, ch: usize) -> u32 {
        self.regs.stored(ch_tx_lim(ch)) & TX_LIM_MASK
    }

    fn clock(&self, ch: usize) -> Result<Clock, &'static str> {
        Clock::decode(self.conf1(ch))
    }

    // ---- interrupts ------------------------------------------------------

    fn raise(&mut self, bit: u32) {
        self.sticky |= bit;
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.sticky & self.regs.stored(INT_ENA);
        cx.irq.set_level(SOURCE_RMT, st != 0);
    }

    // ---- the refill measurement (reported, never gated) -------------------

    /// Close whatever refill measurement channel `ch` has in flight, then
    /// optionally open a new one.
    ///
    /// A measurement that never saw a `chN_tx_lim` write is counted as
    /// unanswered rather than recorded with an invented entry delay.
    fn close_refill(&mut self, ch: usize, cx: &mut BusCx<'_>) {
        let half = self.window_words(ch) / 2;
        match core::mem::take(&mut self.ch[ch].refill) {
            RefillProbe::Idle => {}
            RefillProbe::AwaitingLimit { .. } => {
                self.ch[ch].refill_stats.unanswered += 1;
            }
            RefillProbe::Filling {
                at,
                entry,
                words,
                last_write,
            } => {
                let fill = last_write.saturating_sub(words);
                self.ch[ch].refill_stats.record(entry, fill, half);
                note(cx, || {
                    format!("cyc={at} RMT REFILL ch={ch} at={at} entry={entry} fill={fill}")
                });
            }
        }
    }

    /// A threshold fired: the previous measurement (if any) ends here and a
    /// new one begins.
    fn open_refill(&mut self, ch: usize, at: Cycles, cx: &mut BusCx<'_>) {
        self.close_refill(ch, cx);
        self.ch[ch].refill = RefillProbe::AwaitingLimit {
            at,
            words: self.ch[ch].words_consumed,
        };
    }

    /// The ISR wrote `chN_tx_lim`: the entry delay is settled, the fill
    /// begins. `refill` flips the threshold before it plants the guard or
    /// writes a single word, so this is the earliest moment the handler is
    /// observably present.
    fn refill_answered(&mut self, ch: usize) {
        let words = self.ch[ch].words_consumed;
        if let RefillProbe::AwaitingLimit { at, words: from } = self.ch[ch].refill {
            self.ch[ch].refill = RefillProbe::Filling {
                at,
                entry: words.saturating_sub(from),
                words,
                last_write: words,
            };
        }
    }

    /// A word landed in a channel's window while its refill was being
    /// measured.
    fn refill_wrote(&mut self, word_index: u32) {
        for ch in 0..TX_CHANNELS {
            let start = Self::window_start(ch);
            let end = start + self.window_words(ch);
            if word_index < start || word_index >= end {
                continue;
            }
            let words = self.ch[ch].words_consumed;
            if let RefillProbe::Filling { last_write, .. } = &mut self.ch[ch].refill {
                *last_write = words;
            }
        }
    }

    // ---- the engine ------------------------------------------------------

    fn cancel_word(&mut self, ch: usize, cx: &mut BusCx<'_>) {
        cx.sched.cancel(event_id(self.index, EV_WORD + ch as u16));
    }

    /// `tx_start`, acted on **immediately** — there is no `conf_update` on
    /// this chip (quirk 3).
    fn start(&mut self, ch: usize, now: Cycles, cx: &mut BusCx<'_>) {
        if self.window_words(ch) == 0 {
            note(cx, || {
                format!("cyc={now} RMT ch{ch} start refused: mem_size 0 (no window)")
            });
            return;
        }
        if self.ch[ch].running {
            self.cancel_word(ch, cx);
        }
        let clock = self.clock(ch).ok();
        let div_cnt = self.div_cnt(ch);
        {
            let e = &mut self.ch[ch];
            e.running = true;
            e.stalled = false;
            e.stall_noted = false;
            e.ending = Ending::No;
            e.mem_empty = false;
            e.ticks_since_start = 0;
            e.anchor_cycle = now;
            e.anchor_ticks = 0;
            e.anchor_clock = clock;
            e.anchor_div = div_cnt;
            e.frame_words = 0;
            // The repeating count arms here, at word 0 of the frame.
            e.since_thr = 0;
        }
        let hz = clock.map(|c| c.src_hz / u64::from(div_cnt)).unwrap_or(0);
        let (ws, ww) = (Self::window_start(ch), self.window_words(ch));
        let (wrap, tx_lim, raddr) = (self.wrap(), self.tx_lim(ch), self.ch[ch].raddr);
        note(cx, || {
            format!(
                "cyc={now} RMT ch{ch} start f_ch={hz} div_cnt={div_cnt} window={ws}..{} \
                 raddr={raddr} wrap={} tx_lim={tx_lim}",
                ws + ww,
                u8::from(wrap)
            )
        });
        self.fetch(ch, now, cx);
    }

    /// The frame ends here: `tx_end`, output to the idle level.
    fn end_frame(&mut self, ch: usize, at: Cycles, cx: &mut BusCx<'_>) {
        self.cancel_word(ch, cx);
        // A refill still in flight ends with the frame: the driver's
        // `finish` answers the last threshold, not `refill`.
        self.close_refill(ch, cx);
        let idle_level = self.idle_level(ch);
        let words = {
            let e = &mut self.ch[ch];
            e.running = false;
            e.stalled = false;
            e.ending = Ending::No;
            e.frames_ended += 1;
            e.frame_words
        };
        cx.pins.drive(signal_of(ch), idle_level, at);
        self.raise(int_tx_end_bit(ch));
        note(cx, || {
            format!(
                "cyc={at} RMT ch{ch} end words={words} idle={}",
                u8::from(idle_level)
            )
        });
        self.update_lines(cx);
    }

    /// The engine has no base clock: hold the current word.
    fn stall(&mut self, ch: usize, at: Cycles, reason: &'static str, cx: &mut BusCx<'_>) {
        let e = &mut self.ch[ch];
        e.stalled = true;
        if e.stall_noted {
            return;
        }
        e.stall_noted = true;
        log::warn!(
            "RMT ch{ch}: tx_start at cycle {at} with {reason}; the channel produces no waveform"
        );
        note(cx, || format!("cyc={at} RMT ch{ch} stalled: {reason}"));
    }

    /// The channel's output goes to `pulse.level` at `pulse.at`, and the log
    /// (if it is being kept) records it.
    ///
    /// The two halves of a word are emitted together, at the fetch, each
    /// stamped with the cycle it actually starts at — so the fabric can see
    /// an edge up to one word ahead of the slice boundary. They are never
    /// out of order, which is all a decoder or a pin log needs.
    fn push_pulse(&mut self, ch: usize, pulse: Pulse, cx: &mut BusCx<'_>) {
        cx.pins.drive(signal_of(ch), pulse.level, pulse.at);
        if !self.keep_logs {
            return;
        }
        let e = &mut self.ch[ch];
        if e.pulses.len() < PULSE_LOG_CAP {
            e.pulses.push(pulse);
        } else if !e.pulse_cap_noted {
            e.pulse_cap_noted = true;
            let at = pulse.at;
            note(cx, || {
                format!(
                    "cyc={at} RMT ch{ch} pulse log cap ({PULSE_LOG_CAP}) reached; later pulses are not recorded"
                )
            });
        }
    }

    fn log_word(&mut self, ch: usize, now: Cycles, word: u32, cx: &mut BusCx<'_>) {
        {
            let e = &mut self.ch[ch];
            e.frame_words += 1;
            // The refill measurement's unit: a word the transmitter has
            // taken out of the RAM and will not read again. Counted here
            // rather than in `fetch` because a stalled engine has not
            // consumed the word it is holding.
            e.words_consumed += 1;
        }
        if !self.keep_logs {
            return;
        }
        let e = &mut self.ch[ch];
        if e.words.len() < WORD_LOG_CAP {
            e.words.push((now, word));
        } else if !e.word_cap_noted {
            e.word_cap_noted = true;
            note(cx, || {
                format!(
                    "cyc={now} RMT ch{ch} word log cap ({WORD_LOG_CAP}) reached; later words are not recorded"
                )
            });
        }
    }

    /// Fetch the word at `raddr` at cycle `now` (the word's start), emit its
    /// pulses, schedule the word's end, advance the pointer, and apply the
    /// threshold and wrap rules.
    fn fetch(&mut self, ch: usize, now: Cycles, cx: &mut BusCx<'_>) {
        let raddr = self.ch[ch].raddr as usize;
        let word = self.ram[raddr.min(RAM_WORDS - 1)];
        let dur1 = word & DURATION_MAX;
        let level1 = word & (1 << 15) != 0;
        let dur2 = (word >> 16) & DURATION_MAX;
        let level2 = word & (1 << 31) != 0;
        if dur1 == 0 {
            // The all-zero end marker, or a zero first half: the
            // transmission ends before it — no clock needed. This is also
            // what makes quirk 4 work, a window full of end markers stopping
            // the transmitter at the next word boundary.
            self.log_word(ch, now, word, cx);
            self.end_frame(ch, now, cx);
            return;
        }

        let clock = match self.clock(ch) {
            Ok(c) => c,
            Err(reason) => {
                // Not fetched yet: the word is logged when it actually goes
                // out, on a resume.
                self.stall(ch, now, reason, cx);
                return;
            }
        };
        let div_cnt = self.div_cnt(ch);
        self.log_word(ch, now, word, cx);
        {
            let e = &mut self.ch[ch];
            let t0 = e.ticks_since_start;
            if e.stalled {
                e.stalled = false;
                e.stall_noted = false;
                e.anchor_cycle = now;
                e.anchor_ticks = t0;
                e.anchor_clock = Some(clock);
                e.anchor_div = div_cnt;
                note(cx, || format!("cyc={now} RMT ch{ch} resumed"));
            } else if e.anchor_clock != Some(clock) || e.anchor_div != div_cnt {
                // The divider changed under a running frame (nothing in the
                // firmware does this): re-anchor at this word so the words
                // already emitted keep their cycles.
                e.anchor_cycle = now;
                e.anchor_ticks = t0;
                e.anchor_clock = Some(clock);
                e.anchor_div = div_cnt;
            }
        }
        let t0 = self.ch[ch].ticks_since_start;
        let word_start = self.ch[ch].cycle_at(&clock, div_cnt, t0);
        let second_at = self.ch[ch].cycle_at(&clock, div_cnt, t0 + u64::from(dur1));
        let word_end =
            self.ch[ch].cycle_at(&clock, div_cnt, t0 + u64::from(dur1) + u64::from(dur2));
        self.push_pulse(
            ch,
            Pulse {
                at: word_start,
                level: level1,
                ticks: dur1 as u16,
            },
            cx,
        );
        if dur2 != 0 {
            self.push_pulse(
                ch,
                Pulse {
                    at: second_at,
                    level: level2,
                    ticks: dur2 as u16,
                },
                cx,
            );
        } else {
            // Half 1 goes out; the word ends the frame.
            self.ch[ch].ending = Ending::HalfTwoZero;
        }
        {
            let e = &mut self.ch[ch];
            e.ticks_since_start = t0 + u64::from(dur1) + u64::from(dur2);
            e.word_due = word_end;
        }
        cx.sched
            .schedule_at(word_end, event_id(self.index, EV_WORD + ch as u16));

        // Advance: the pointer now names the next word to fetch.
        let window_start = Self::window_start(ch);
        let window_words = self.window_words(ch);
        self.ch[ch].raddr += 1;
        self.ch[ch].since_thr += 1;

        // **Quirk 1.** A repeating count of words *sent*, which re-arms
        // itself — not a position in the window. A `tx_lim` of 0 never
        // fires, which is what the firmware's `MAX_BLOCKS_PER_CHANNEL` cap
        // exists to avoid (a 512-word window would mask to 0).
        let lim = self.tx_lim(ch);
        if lim != 0 && self.ch[ch].since_thr >= lim {
            self.ch[ch].since_thr = 0;
            self.raise(int_thr_bit(ch));
            let sent = self.ch[ch].frame_words;
            note(cx, || {
                format!("cyc={now} RMT ch{ch} thr sent={sent} lim={lim}")
            });
            // `now` is the word's own start cycle — when the counter reached
            // the threshold — not the dispatch cycle.
            self.open_refill(ch, now, cx);
        }

        let pos = self.ch[ch].raddr - window_start;
        if pos >= window_words {
            if self.wrap() {
                // **Quirk 2**: the global `apb_conf.mem_tx_wrap_en`.
                self.ch[ch].raddr = window_start;
            } else if self.ch[ch].ending == Ending::No {
                self.raise(int_err_bit(ch));
                self.ch[ch].mem_empty = true;
                self.ch[ch].ending = Ending::Overrun;
                note(cx, || {
                    format!(
                        "cyc={now} RMT ch{ch} err mem_empty (window end, apb_conf.mem_tx_wrap_en off)"
                    )
                });
            }
        }
        self.update_lines(cx);
    }

    /// The current word's pulses ended.
    fn word_ended(&mut self, ch: usize, cx: &mut BusCx<'_>) {
        if !self.ch[ch].running || self.ch[ch].stalled {
            return;
        }
        let due = self.ch[ch].word_due;
        match self.ch[ch].ending {
            Ending::No => self.fetch(ch, due, cx),
            Ending::HalfTwoZero => self.end_frame(ch, due, cx),
            Ending::Overrun => {
                self.close_refill(ch, cx);
                let idle_level = self.idle_level(ch);
                let e = &mut self.ch[ch];
                e.running = false;
                e.ending = Ending::No;
                cx.pins.drive(signal_of(ch), idle_level, due);
                note(cx, || {
                    format!("cyc={due} RMT ch{ch} stopped after mem_empty")
                });
            }
        }
    }

    // ---- registers -------------------------------------------------------

    fn read_word(&self, off: u32) -> u32 {
        if off >= RAM_OFFSET {
            let i = ((off - RAM_OFFSET) >> 2) as usize;
            return self.ram.get(i).copied().unwrap_or(0);
        }
        if off >= REGS_LEN {
            return 0;
        }
        for ch in 0..TX_CHANNELS {
            if off == ch_status(ch) {
                let e = &self.ch[ch];
                // `mem_waddr_ex` (bits 0:9) is the APB side's write pointer,
                // which direct RAM access never uses: the window start.
                let mut v = Self::window_start(ch) & STATUS_WADDR_MASK;
                // `mem_raddr_ex` (bits 12:21) — quirk 5, absolute over all
                // 512 words, and what `v3_rmt::read_pos` reads.
                v |= (e.raddr & STATUS_RADDR_MASK) << STATUS_RADDR_SHIFT;
                if e.running {
                    v |= STATUS_STATE_SEND << STATUS_STATE_SHIFT;
                }
                if e.mem_empty {
                    v |= STATUS_MEM_EMPTY;
                }
                // `mem_owner_err` (bit 27) never rises: one RAM, no arbiter.
                return v;
            }
        }
        match off {
            INT_RAW => self.sticky,
            INT_ST => self.sticky & self.regs.stored(INT_ENA),
            INT_CLR => 0,
            other => self.regs.effective(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        if off >= RAM_OFFSET {
            let i = ((off - RAM_OFFSET) >> 2) as usize;
            if let Some(slot) = self.ram.get_mut(i) {
                // Live: the engine reads this on its next fetch, refilled
                // or not.
                *slot = value;
                // …and it is the last word of a refill until another one
                // lands, which is how the fill half of the measurement is
                // read off the guest's own writes.
                self.refill_wrote(i as u32);
            }
            return;
        }
        if off >= REGS_LEN {
            if !self.warned_gap {
                self.warned_gap = true;
                let (now, pc) = (cx.now, cx.pc);
                note(cx, || {
                    format!(
                        "cyc={now} pc=0x{pc:08x} RMT write to +0x{off:03x}: between the register block and the RAM, dropped"
                    )
                });
            }
            return;
        }
        for ch in 0..TX_CHANNELS {
            if off == ch_conf0(ch) {
                self.regs.poke(off, value);
                if value & CONF0_CARRIER_EN != 0 && !self.warned_carrier[ch] {
                    self.warned_carrier[ch] = true;
                    let now = cx.now;
                    note(cx, || {
                        format!(
                            "cyc={now} RMT ch{ch} carrier_en: carrier modulation is not modelled; \
                             the pulses go out unmodulated"
                        )
                    });
                }
                return;
            }
            if off == ch_conf1(ch) {
                // Quirk 3: no `conf_update`. The strobes are acted on here,
                // in the write that carried them, and read back 0.
                let strobes = value & CONF1_STROBES;
                self.regs.poke(off, value & !CONF1_STROBES);
                if strobes & CONF1_MEM_RD_RST != 0 {
                    self.ch[ch].raddr = Self::window_start(ch);
                }
                if value & CONF1_REF_CNT_RST != 0 && !self.warned_ref_cnt[ch] {
                    self.warned_ref_cnt[ch] = true;
                    let now = cx.now;
                    note(cx, || {
                        format!(
                            "cyc={now} RMT ch{ch} ref_cnt_rst: the divider's phase reset is \
                             accepted and unobservable at div_cnt 1 (modeled). Unlike the four \
                             strobes it is NOT self-clearing — the firmware sets it and clears \
                             it in two separate writes (v3_rmt.rs's start_tx), which only makes \
                             sense for a bit that holds."
                        )
                    });
                }
                // `apb_mem_rst` and `mem_wr_rst` reset the APB-side and the
                // receiver's pointers, neither of which this model has.
                if value & CONF1_RX_EN != 0 && !self.warned_rx[ch] {
                    self.warned_rx[ch] = true;
                    log::warn!(
                        "RMT ch{ch}: rx_en set — the classic's RMT receiver is not modelled \
                         (M4 scope: TX only). The bit is remembered and nothing samples a pad."
                    );
                }
                if value & CONF1_TX_CONTI_MODE != 0 && !self.warned_conti[ch] {
                    self.warned_conti[ch] = true;
                    let now = cx.now;
                    note(cx, || {
                        format!(
                            "cyc={now} RMT ch{ch} tx_conti_mode: continuous transmission is not \
                             modelled; the frame ends at its end marker"
                        )
                    });
                }
                if strobes & CONF1_TX_START != 0 {
                    let now = cx.now;
                    self.start(ch, now, cx);
                }
                return;
            }
            if off == ch_tx_lim(ch) {
                // Immediate: the ISR rewrites it mid-frame and there is no
                // `conf_update` to gate it on.
                self.regs.poke(off, value);
                // And this write is the ISR arriving: the entry delay ends
                // here.
                self.refill_answered(ch);
                return;
            }
        }
        match off {
            INT_ENA => {
                self.regs.poke(INT_ENA, value & INT_MASK);
                self.update_lines(cx);
            }
            INT_CLR => {
                // Write-only and W1C: each write clears exactly the bits it
                // names, which is what makes it race-free across two cores.
                self.sticky &= !value;
                self.update_lines(cx);
            }
            INT_RAW | INT_ST => {}
            APB_CONF => {
                self.regs.poke(off, value);
                if value & APB_CONF_FIFO_MASK == 0 && !self.warned_fifo {
                    self.warned_fifo = true;
                    let now = cx.now;
                    note(cx, || {
                        format!(
                            "cyc={now} RMT apb_conf.apb_fifo_mask = 0: the APB FIFO is not modelled (direct RAM access only)"
                        )
                    });
                }
            }
            o if o < CH_DATA_END => {
                if !self.warned_fifo {
                    self.warned_fifo = true;
                    let now = cx.now;
                    note(cx, || {
                        format!(
                            "cyc={now} RMT ch{}data write: the APB FIFO is not modelled (direct RAM access only)",
                            o >> 2
                        )
                    });
                }
            }
            o if (CH_ADDR..CH_ADDR_END).contains(&o) => {}
            o if (CH_CARRIER_DUTY..CH_CARRIER_DUTY_END).contains(&o) => {
                self.regs.poke(o, value);
            }
            other => {
                if (0x060..0x080).contains(&other) {
                    // `chNstatus` is read-only in the PAC.
                    return;
                }
                self.regs.poke(other, value);
            }
        }
    }
}

impl Default for Rmt {
    fn default() -> Self {
        Self::new()
    }
}

impl Peripheral for Rmt {
    fn name(&self) -> &'static str {
        "RMT"
    }

    fn attached(&mut self, index: usize) {
        self.index = index;
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let merged = merge_lane(self.read_word(word), off, width, value);
        self.write_word(word, merged, cx);
    }

    fn on_event(&mut self, id: EventId, cx: &mut BusCx<'_>) {
        let local = event_local(id);
        if local < EV_WORD + TX_CHANNELS as u16 {
            self.word_ended(usize::from(local - EV_WORD), cx);
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::RMT.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        Some(self.grades.grade(off))
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(REGS_LEN as usize + RAM_WORDS * 4 + 512);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.sticky.to_le_bytes());
        let mut flags = u32::from(self.warned_fifo)
            | (u32::from(self.warned_gap) << 1)
            | (u32::from(self.keep_logs) << 2);
        for ch in 0..TX_CHANNELS {
            flags |= u32::from(self.warned_rx[ch]) << (8 + ch);
            flags |= u32::from(self.warned_carrier[ch]) << (16 + ch);
        }
        out.extend_from_slice(&flags.to_le_bytes());
        let mut conti = 0u32;
        for ch in 0..TX_CHANNELS {
            conti |= u32::from(self.warned_conti[ch]) << ch;
            conti |= u32::from(self.warned_ref_cnt[ch]) << (8 + ch);
        }
        out.extend_from_slice(&conti.to_le_bytes());
        for w in self.ram.iter() {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for e in &self.ch {
            let eflags = u32::from(e.running)
                | (u32::from(e.stalled) << 1)
                | (u32::from(e.mem_empty) << 2)
                | (u32::from(e.pulse_cap_noted) << 3)
                | (u32::from(e.word_cap_noted) << 4)
                | (u32::from(e.stall_noted) << 5)
                | ((e.ending as u32) << 8);
            out.extend_from_slice(&eflags.to_le_bytes());
            out.extend_from_slice(&e.raddr.to_le_bytes());
            out.extend_from_slice(&e.since_thr.to_le_bytes());
            out.extend_from_slice(&e.frame_words.to_le_bytes());
            out.extend_from_slice(&e.anchor_div.to_le_bytes());
            out.extend_from_slice(&e.ticks_since_start.to_le_bytes());
            out.extend_from_slice(&e.anchor_cycle.to_le_bytes());
            out.extend_from_slice(&e.anchor_ticks.to_le_bytes());
            out.extend_from_slice(&e.anchor_clock.map_or(0, |c| c.src_hz).to_le_bytes());
            out.extend_from_slice(&e.word_due.to_le_bytes());
            out.extend_from_slice(&(e.frames_ended as u64).to_le_bytes());
            out.extend_from_slice(&(e.pulses.len() as u64).to_le_bytes());
            for p in &e.pulses {
                out.extend_from_slice(&p.at.to_le_bytes());
                out.extend_from_slice(
                    &(u32::from(p.ticks) | (u32::from(p.level) << 16)).to_le_bytes(),
                );
            }
            out.extend_from_slice(&(e.words.len() as u64).to_le_bytes());
            for (at, w) in &e.words {
                out.extend_from_slice(&at.to_le_bytes());
                out.extend_from_slice(&w.to_le_bytes());
            }
            // The refill measurement rides the snapshot like everything else
            // this block observes: a restored run has to produce the same
            // `RMT REFILL` notes as the run it was taken from, or the
            // snapshot-identity gate would be comparing two observers.
            out.extend_from_slice(&e.words_consumed.to_le_bytes());
            let (tag, at, entry, words, last_write) = match e.refill {
                RefillProbe::Idle => (0u64, 0, 0, 0, 0),
                RefillProbe::AwaitingLimit { at, words } => (1, at, 0, words, 0),
                RefillProbe::Filling {
                    at,
                    entry,
                    words,
                    last_write,
                } => (2, at, entry, words, last_write),
            };
            for v in [tag, at, entry, words, last_write] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            let s = &e.refill_stats;
            out.extend_from_slice(&s.refills.to_le_bytes());
            out.extend_from_slice(&s.entry_max.to_le_bytes());
            out.extend_from_slice(&s.fill_max.to_le_bytes());
            out.extend_from_slice(&s.unanswered.to_le_bytes());
            out.extend_from_slice(&s.half_words.to_le_bytes());
            for v in s.entry_hist.iter().chain(s.fill_hist.iter()) {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(index), Some(sticky), Some(flags), Some(conti)) =
            (r.u64(), r.u32(), r.u32(), r.u32())
        else {
            log::warn!("RMT: load_state blob too short, ignored");
            return;
        };
        let mut ram = [0u32; RAM_WORDS];
        for w in ram.iter_mut() {
            let Some(v) = r.u32() else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            *w = v;
        }
        let mut engines = Vec::with_capacity(TX_CHANNELS);
        for ch in 0..TX_CHANNELS {
            let mut e = TxEngine::new(ch);
            let (Some(eflags), Some(raddr), Some(since_thr), Some(frame_words), Some(anchor_div)) =
                (r.u32(), r.u32(), r.u32(), r.u32(), r.u32())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let (Some(ticks), Some(anchor_cycle), Some(anchor_ticks), Some(src), Some(word_due)) =
                (r.u64(), r.u64(), r.u64(), r.u64(), r.u64())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let (Some(frames_ended), Some(npulses)) = (r.u64(), r.u64()) else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let mut pulses = Vec::with_capacity(npulses as usize);
            for _ in 0..npulses {
                let (Some(at), Some(lt)) = (r.u64(), r.u32()) else {
                    log::warn!("RMT: load_state blob too short, ignored");
                    return;
                };
                pulses.push(Pulse {
                    at,
                    level: lt & (1 << 16) != 0,
                    ticks: (lt & 0xffff) as u16,
                });
            }
            let Some(nwords) = r.u64() else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let mut words = Vec::with_capacity(nwords as usize);
            for _ in 0..nwords {
                let (Some(at), Some(w)) = (r.u64(), r.u32()) else {
                    log::warn!("RMT: load_state blob too short, ignored");
                    return;
                };
                words.push((at, w));
            }
            let (
                Some(words_consumed),
                Some(tag),
                Some(probe_at),
                Some(probe_entry),
                Some(probe_words),
                Some(probe_last_write),
            ) = (r.u64(), r.u64(), r.u64(), r.u64(), r.u64(), r.u64())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let (
                Some(refills),
                Some(entry_max),
                Some(fill_max),
                Some(unanswered),
                Some(half_words),
            ) = (r.u64(), r.u64(), r.u64(), r.u64(), r.u32())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let mut stats = RefillStats {
                refills,
                entry_max,
                fill_max,
                unanswered,
                half_words,
                ..RefillStats::default()
            };
            for slot in stats
                .entry_hist
                .iter_mut()
                .chain(stats.fill_hist.iter_mut())
            {
                let Some(v) = r.u64() else {
                    log::warn!("RMT: load_state blob too short, ignored");
                    return;
                };
                *slot = v;
            }
            e.words_consumed = words_consumed;
            e.refill = match tag {
                1 => RefillProbe::AwaitingLimit {
                    at: probe_at,
                    words: probe_words,
                },
                2 => RefillProbe::Filling {
                    at: probe_at,
                    entry: probe_entry,
                    words: probe_words,
                    last_write: probe_last_write,
                },
                _ => RefillProbe::Idle,
            };
            e.refill_stats = stats;
            e.running = eflags & 1 != 0;
            e.stalled = eflags & 2 != 0;
            e.mem_empty = eflags & 4 != 0;
            e.pulse_cap_noted = eflags & 8 != 0;
            e.word_cap_noted = eflags & 16 != 0;
            e.stall_noted = eflags & 32 != 0;
            e.ending = match (eflags >> 8) & 0xff {
                1 => Ending::HalfTwoZero,
                2 => Ending::Overrun,
                _ => Ending::No,
            };
            e.raddr = raddr;
            e.since_thr = since_thr;
            e.frame_words = frame_words;
            e.anchor_div = anchor_div;
            e.ticks_since_start = ticks;
            e.anchor_cycle = anchor_cycle;
            e.anchor_ticks = anchor_ticks;
            e.anchor_clock = (src != 0).then_some(Clock { src_hz: src });
            e.word_due = word_due;
            e.frames_ended = frames_ended as usize;
            e.pulses = pulses;
            e.words = words;
            engines.push(e);
        }
        self.index = index as usize;
        self.sticky = sticky;
        self.warned_fifo = flags & 1 != 0;
        self.warned_gap = flags & 2 != 0;
        self.keep_logs = flags & 4 != 0;
        for ch in 0..TX_CHANNELS {
            self.warned_rx[ch] = flags & (1 << (8 + ch)) != 0;
            self.warned_carrier[ch] = flags & (1 << (16 + ch)) != 0;
            self.warned_conti[ch] = conti & (1 << ch) != 0;
            self.warned_ref_cnt[ch] = conti & (1 << (8 + ch)) != 0;
        }
        *self.ram = ram;
        let mut it = engines.into_iter();
        self.ch = core::array::from_fn(|_| it.next().expect("eight engines"));
        self.regs.load_state(r.0);
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }

    /// The machine-side seam, for one thing only: turning the pulse and word
    /// logs on at build time (`Esp32V3Builder::rmt_logs`). Nothing on the
    /// guest side can reach it.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_le_bytes(*head))
    }

    fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_le_bytes(*head))
    }
}
