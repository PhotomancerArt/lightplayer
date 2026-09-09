//! `RMT` at `0x6000_6000` — the remote-control transceiver as the WS281x
//! path drives it: the PAC register file, the 192-word RAM at `+0x400`, and
//! two TX engines on the scheduler that consume words at the configured
//! clock and raise `tx_end` / `tx_thr_event` / `tx_err` on source 49 — and,
//! since M2 P3, two RX engines that sample a routed input signal back into
//! the same RAM and raise `rx_end` / `rx_thr_event` on the same source.
//!
//! Register facts are the esp32c6 PAC 0.23.2 `rmt` block (offsets in
//! `regs::RMT`; bit positions cited per register below) and the two drivers
//! that write it: esp-hal 1.1.1's `Rmt::new` / `configure_tx` / `with_pin`
//! and this project's `lp-fw/fw-esp32c6/src/output/rmt/c6_rmt.rs` backend
//! for `lp-ws281x` (M5 discovery §1–§2). The clock is **PCR's**
//! (`rmt_sclk_conf`, `rmt_conf.clk_en` — the C6's `sys_conf` has no `sclk_*`
//! fields), fed in on a [`RmtClockLine`] the way the UART clocks are.
//!
//! # What the WS281x driver needs from this block
//!
//! `start_frame` fills both halves of the channel's window, arms `tx_lim` at
//! the half, and starts; every `tx_thr_event` the ISR reads `mem_raddr_ex`,
//! flips `tx_lim` between `half` and `window_words`, plants a STOP guard in
//! the half just left, and refills it — racing the transmitter across a
//! half-window deadline (96 words = 19,200 cycles on the harness's one-channel
//! plan, 24 words = 4,800 on the shipped two-channel one). The all-zero word
//! ends the frame with `tx_end`. So the model has to get four things exactly
//! right, and each is pinned by a `Sandbox` test at the bottom:
//!
//! 1. **`tx_lim` is a position in the window**, not a repeating count: the
//!    event fires when the read pointer reaches word `tx_lim`, and
//!    `tx_lim == window_words` is the wrap back to word 0 (discovery §4 —
//!    silicon's `refills == wanted` over 5,520 frames; a counter that reset
//!    on every event would fire the alternating 24/48 thresholds at 24, 24,
//!    0, 0 and truncate every frame).
//! 2. **Time is anchored on the previous due cycle.** One word's two pulses
//!    take `(dur1 + dur2)` ticks; the next word is due at
//!    `start_cycle + cycles_for(ticks_since_start)` with integer arithmetic
//!    over the absolute tick count since `tx_start` — never accumulated per
//!    word, never taken from the dispatch cycle (`uart.rs`'s `tx_due` rule).
//!    At the shipped settings one tick is exactly 2 cycles and a data word
//!    200; the latch word 48,000.
//! 3. **The RAM is live.** The engine reads `ram[raddr]` when it fetches; a
//!    refill overwrites what the consumer has not fetched yet, which is what
//!    a single-ported RAM does.
//! 4. **The ISR's `tx_lim` write takes effect immediately**, with no
//!    `conf_update` (the driver rewrites it mid-frame).
//!
//! # Modeled choices (discovery §10.1–5), each named where it is made
//!
//! - §10.2 `mem_raddr_ex` = the **next word to fetch** (the driver tolerates
//!   ±1 through `guard_skips`).
//! - §10.3 `tx_start` / `mem_rd_rst` / `apb_mem_rst` are pulses that take
//!   effect at `conf_update` (both drivers write it right after); a
//!   `tx_start` with no `conf_update` in the same slice is acted on at the
//!   slice boundary with a note, so a frame never silently fails to start.
//!   `tx_stop` is sticky and reads back.
//! - §10.4 the end marker: a zero duration in half 1 ends the transmission
//!   before it; in half 2, half 1 is emitted and the word ends it. Both
//!   drivers only ever write the all-zero word.
//! - §10.5 `ref_cnt_rst` is accepted (a divider phase reset is unobservable
//!   at `div_cnt = 1`); no function clock (`rmt_conf.clk_en = 0`, `sclk_en
//!   = 0`, `sclk_sel = 0`) **stalls** the engine, which resumes when the
//!   clock returns; `sclk_sel = 2` (FOSC, §10.6) is refused as no clock
//!   rather than guessed at.
//! - `tx_stop` raises no `tx_end` (esp-hal's `stop` expects none).
//!
//! # The receivers (M2 P3)
//!
//! Channels 2 and 3 are engines too: `RxEngine` (private) samples the level of the
//! pad routed to the channel's **input** signal (`GPIO.func_in_sel_cfg[71]`
//! for channel 2, through the fabric's `route_in`), measures each run in
//! channel ticks and writes them into the channel's RAM window two to a word.
//! `idle_thres` ends the reception, `rx_filter` swallows a pulse narrower
//! than its threshold, `rx_lim` raises `rx_thr_event` and `mem_rx_wrap_en`
//! wraps the write pointer; `rx_end` and `rx_thr_event` go out on source 49
//! with the same status and clear semantics the TX side has.
//!
//! Each is fed the fabric's **edges** rather than sampled tick by tick — see
//! `RxEngine` (private) for why that is the same model and what it costs (a
//! slice of latency on the two interrupts, never on the words). Where the
//! modelled behaviour is a choice rather than a bit map, the choice is named
//! at the place it is made: `RxEngine::armed` (the reception starts at the
//! first edge), `Rmt::rx_write_word` (private; `rx_lim` counts, it is not a
//! position) and `Rmt::rx_end` (private; the trailing idle run is stored and
//! the last word is an end marker).
//!
//! # Register grades
//!
//! | grade | registers |
//! |---|---|
//! | `measured` | none. The `rmt-chase` transcripts agree with silicon frame for frame, but a waveform is not a register's bit map — `validate.toml`'s `pin` entry has the argument. |
//! | `documented` | `ch0_tx_conf0`, `ch1_tx_conf0`, `ch0_tx_status`, `ch1_tx_status`, `ch0_tx_lim`, `ch1_tx_lim`, `ch2_rx_conf0`, `ch3_rx_conf0`, `ch2_rx_conf1`, `ch3_rx_conf1`, `ch0_rx_status`, `ch1_rx_status` (channels 2 and 3 — the PAC's own numbering), `ch0_rx_lim`, `ch1_rx_lim`, `int_raw`, `int_st`, `int_ena`, `int_clr`, `sys_conf`. The PAC's bit map is the source and every field above is that bit map read out loud. |
//! | `modeled` | `ch0data`…`ch3data` (the APB FIFO, not modelled), `ch0carrier_duty`, `ch1carrier_duty`, `ch0_rx_carrier_rm`, `ch1_rx_carrier_rm` (carrier modulation and demodulation, not modelled), `tx_sim`, `ref_cnt_rst`, `date`. Accept-and-remember, at the PAC's reset value. |
//!
//! # What is not here
//!
//! The APB FIFO (`ch*data`, `sys_conf.apb_fifo_mask = 0`) is not modelled —
//! both drivers use direct RAM access. Neither is the carrier: modulation on
//! the way out, demodulation on the way in, and `ch*_rx_carrier_rm` is
//! accepted with one note. The RAM has one port and no arbiter, so
//! `ch_rx_conf1.mem_owner` is recorded rather than enforced and
//! `ch_rx_status.mem_owner_err` never rises. Where the waveform goes (GPIO18
//! through `func_out_sel_cfg`) is the signal fabric's; the per-channel
//! **pulse log** and **fetched-word log** are the TX side's other
//! observation, read by the machine's `rmt_pulses` / `rmt_words` /
//! `rmt_frames_ended`.
//!
//! # The refill measurement (M5 P3)
//!
//! The block also watches the race it is half of, and reports it: for every
//! `tx_thr_event` it counts the words the transmitter consumes before the
//! guest's next `ch_tx_lim` write (the **entry** delay) and then before the
//! last RAM write of that refill (the **fill**), in the same units
//! `lp-ws281x` measures them in from `read_pos` — words. Every measurement is
//! a `RMT REFILL ch=0 at=<cyc> entry=<w> fill=<w>` trace note, and the totals
//! are a nine-bucket histogram per channel ([`RefillStats`]), printed in the
//! CLI's exit summary beside the guest's own `hist=`/`entry_hist=`.
//!
//! It is **reported and never gated** (D13/PD9), and the reason is in the
//! numbers rather than in the policy: the emulator's ISR path is
//! RAM-resident by construction and the machine has no flash-miss cost, so
//! the entry half is a floor rather than a prediction of silicon's 20–29
//! words. What it is good for is the shape — a fill that grows, or a bucket
//! that starts landing at "≥ half", is the model or the driver getting
//! slower at the deadline, and neither of those was visible from anywhere
//! before.

use lp_emu_core::sched::{Cycles, EventId};
use lp_emu_esp_common::pins::Edge;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{
    BusCx, Peripheral, RegFile, RegGrade, RegGrades, SignalId, Width, event_id, event_local,
};

use super::pcr::RmtClockLine;
use super::systimer::Reader;
use crate::memmap;
use crate::regs::output_signals::{RMT_RX_SIG_0, RMT_SIG_0};
use crate::regs::{self, source};

// Register offsets (`regs::RMT`).
//
// The PAC numbers the two halves of the block differently and the names below
// are its names, not a typo: the *configuration* registers carry the absolute
// channel number (`ch2_rx_conf0`, `ch3_rx_conf1`) while the status, limit and
// carrier registers carry the receiver's index among the receivers
// (`ch0_rx_status` is channel 2's). esp-hal indexes every one of them by the
// receiver index (`DynChannelAccess<Rx>::ch_idx`, 0 for channel 2), which is
// what these arrays are indexed by too.
const CH_DATA_END: u32 = 0x010;
const CH_TX_CONF0: [u32; TX_CHANNELS] = [0x010, 0x014];
const CH_RX_CONF0: [u32; RX_CHANNELS] = [0x018, 0x020];
const CH_RX_CONF1: [u32; RX_CHANNELS] = [0x01c, 0x024];
const CH_TX_STATUS: [u32; TX_CHANNELS] = [0x028, 0x02c];
const CH_RX_STATUS: [u32; RX_CHANNELS] = [0x030, 0x034];
const INT_RAW: u32 = 0x038;
const INT_ST: u32 = 0x03c;
const INT_ENA: u32 = 0x040;
const INT_CLR: u32 = 0x044;
const CH_RX_CARRIER_RM: [u32; RX_CHANNELS] = [0x050, 0x054];
const CH_TX_LIM: [u32; TX_CHANNELS] = [0x058, 0x05c];
const CH_RX_LIM: [u32; RX_CHANNELS] = [0x060, 0x064];
const SYS_CONF: u32 = 0x068;
const REF_CNT_RST: u32 = 0x070;

/// The PAC register block: `ch0data` through `date` (`+0x0cc`).
pub const REGS_LEN: u32 = 0x0d0;
/// The RMT RAM: `rmt.ram_start = 0x6000_6400`, four 48-word blocks
/// (`esp-metadata-generated-0.4.0`, `c6_rmt.rs::RAM_OFFSET`).
pub const RAM_OFFSET: u32 = 0x400;
pub const RAM_WORDS: usize = 192;
pub const BLOCK_WORDS: u32 = 48;
/// The whole window the bus maps: registers, the gap, the RAM.
pub const LEN: u32 = 0x700;
/// TX channels: 0 and 1.
pub const TX_CHANNELS: usize = 2;
/// RX channels: 2 and 3. Indexed here by their **receiver index** — 0 is
/// channel 2 — because that is how the PAC's status registers and esp-hal's
/// `DynChannelAccess<Rx>` index them.
pub const RX_CHANNELS: usize = 2;
/// The absolute channel number of receiver index 0.
pub const RX_CH_BASE: usize = 2;

// `ch_tx_conf0` bits (`rmt/ch_tx_conf0.rs`).
const CONF_TX_START: u32 = 1 << 0;
const CONF_MEM_RD_RST: u32 = 1 << 1;
const CONF_APB_MEM_RST: u32 = 1 << 2;
const CONF_MEM_TX_WRAP_EN: u32 = 1 << 4;
const CONF_IDLE_OUT_LV: u32 = 1 << 5;
const CONF_TX_STOP: u32 = 1 << 7;
const CONF_DIV_CNT_SHIFT: u32 = 8;
const CONF_MEM_SIZE_SHIFT: u32 = 16;
const CONF_CONF_UPDATE: u32 = 1 << 24;
/// The strobes: read back 0 whatever was written (§10.3).
const CONF_PULSES: u32 = CONF_TX_START | CONF_MEM_RD_RST | CONF_APB_MEM_RST | CONF_CONF_UPDATE;
/// PAC reset: `div_cnt 2`, `mem_size 1`, the carrier bits set.
const CH_TX_CONF0_RESET: u32 = 0x0071_0200;
const TX_LIM_MASK: u32 = 0x1ff;
const SYS_CONF_APB_FIFO_MASK: u32 = 1 << 0;

// `ch_rx_conf0` bits (`rmt/ch_rx_conf0.rs`).
const RX_CONF0_DIV_CNT_SHIFT: u32 = 0;
const RX_CONF0_IDLE_THRES_SHIFT: u32 = 8;
const RX_CONF0_IDLE_THRES_MASK: u32 = 0x7fff;
const RX_CONF0_MEM_SIZE_SHIFT: u32 = 23;
const RX_CONF0_CARRIER_EN: u32 = 1 << 28;
/// PAC reset: `div_cnt 2`, `idle_thres 0x7fff`, `mem_size 1`, carrier bits set.
const CH_RX_CONF0_RESET: u32 = 0x30ff_ff02;

// `ch_rx_conf1` bits (`rmt/ch_rx_conf1.rs`).
const RX_CONF1_RX_EN: u32 = 1 << 0;
const RX_CONF1_MEM_WR_RST: u32 = 1 << 1;
const RX_CONF1_APB_MEM_RST: u32 = 1 << 2;
const RX_CONF1_MEM_OWNER: u32 = 1 << 3;
const RX_CONF1_FILTER_EN: u32 = 1 << 4;
const RX_CONF1_FILTER_THRES_SHIFT: u32 = 5;
const RX_CONF1_FILTER_THRES_MASK: u32 = 0xff;
const RX_CONF1_MEM_RX_WRAP_EN: u32 = 1 << 13;
/// Bit 15, the PAC's *"synchronization bit"* — `conf_update`, which esp-hal's
/// `DynChannelAccess::update` writes for an RX channel exactly where it
/// writes `ch_tx_conf0.conf_update` for a TX one.
const RX_CONF1_CONF_UPDATE: u32 = 1 << 15;
/// The strobes: read back 0 whatever was written, as the TX side's do.
const RX_CONF1_PULSES: u32 = RX_CONF1_MEM_WR_RST | RX_CONF1_APB_MEM_RST | RX_CONF1_CONF_UPDATE;
/// PAC reset: `mem_owner 1`, `rx_filter_thres 15`, everything else clear.
const CH_RX_CONF1_RESET: u32 = 0x0000_01e8;

/// `ch_rx_lim.rx_lim`, bits 0:8.
const RX_LIM_MASK: u32 = 0x1ff;

// `ch_rx_status` fields (`rmt/ch_rx_status.rs`).
const RX_STATUS_APB_RADDR_SHIFT: u32 = 12;
const RX_STATUS_STATE_SHIFT: u32 = 22;
const RX_STATUS_MEM_FULL: u32 = 1 << 26;

/// The largest duration one half of a word can hold: 15 bits.
const DURATION_MAX: u32 = 0x7fff;

// `int_*` bits: TX and RX interleave in pairs (`rmt/int_raw.rs`) — bit 0/1
// are `ch0/ch1_tx_end`, bit 2/3 `ch2/ch3_rx_end`, bits 4..7 the four `err`
// bits, bits 8/9 `ch0/ch1_tx_thr_event` and 10/11 `ch2/ch3_rx_thr_event`. So
// the three shifts below are the same for both directions and the *channel
// number* selects the bit: `raise(2, INT_END_SHIFT)` is `ch2_rx_end`.
const INT_TX_END_SHIFT: u32 = 0;
const INT_TX_ERR_SHIFT: u32 = 4;
const INT_TX_THR_SHIFT: u32 = 8;
const INT_MASK: u32 = 0x3fff;

// `ch_tx_status` fields.
const STATUS_STATE_SHIFT: u32 = 9;
const STATUS_MEM_EMPTY: u32 = 1 << 22;

// `PCR.rmt_sclk_conf` fields (`pcr/rmt_sclk_conf.rs`) and `rmt_conf`.
const SCLK_DIV_B_MASK: u32 = 0x3f;
const SCLK_DIV_A_SHIFT: u32 = 6;
const SCLK_DIV_A_MASK: u32 = 0x3f;
const SCLK_DIV_NUM_SHIFT: u32 = 12;
const SCLK_DIV_NUM_MASK: u32 = 0xff;
const SCLK_SEL_SHIFT: u32 = 20;
const SCLK_EN: u32 = 1 << 22;
const PCR_RMT_CLK_EN: u32 = 1 << 0;

/// Events: `EV_WORD + ch` (the current word's pulses end), `EV_LATE_START +
/// ch` (a `tx_start` with no `conf_update`, §10.3), `EV_CLOCK_POLL` (a
/// stalled engine asking whether the clock is back).
const EV_WORD: u16 = 0;
const EV_LATE_START: u16 = 2;
const EV_CLOCK_POLL: u16 = 4;
/// `EV_RX_IDLE + rxi`: the receiver's idle threshold expired with no edge, so
/// the reception ends here.
const EV_RX_IDLE: u16 = 5;

/// How often a stalled engine re-checks the clock: 1 ms of guest time.
/// Nothing in the firmware ever gates the clock, so this is a diagnostic
/// path; deterministic either way.
pub const CLOCK_POLL_CYCLES: u64 = 1_000 * memmap::CYCLES_PER_US;

/// Log caps: generous — a 256-LED frame is 6,146 words / 12,292 pulses, so
/// these hold hundreds of frames — with a note when hit. P2's fabric
/// replaces the logs.
pub const PULSE_LOG_CAP: usize = 4_000_000;
pub const WORD_LOG_CAP: usize = 2_000_000;

/// The GPIO-matrix output signal TX channel `ch` drives: `RMT_SIG_0 + ch`
/// (M5 discovery §8). Whether any pad listens is the fabric's business, and
/// the RMT never learns the answer — it cannot see the GPIO block.
pub const fn signal_of(ch: usize) -> SignalId {
    SignalId(RMT_SIG_0 + ch as u16)
}

/// The GPIO-matrix **input** signal receiver `rxi` reads — `RMT_RX_SIG_0 +
/// rxi`, so RX channel 2 reads `InputSignal::RMT_SIG_0`.
///
/// The C6 has four channels and two signals in each direction: the metadata's
/// `for_each_rmt_channel!` maps `rx(2, 0), (3, 1)`
/// (`esp-metadata-generated-0.4.0/src/_generated_esp32c6.rs:657`), and
/// esp-hal's `Channel<Rx>::with_pin` connects that signal to the pad. Which
/// pad it is is the fabric's business (`Fabric::route_in`, written by the
/// GPIO block's `func_in_sel_cfg` view), and the RMT never learns the pad's
/// number — only its level.
pub const fn rx_signal_of(rxi: usize) -> SignalId {
    SignalId(RMT_RX_SIG_0 + rxi as u16)
}

/// Buckets in a refill histogram: eighths of a half-window, plus one for
/// "≥ half".
///
/// The same nine buckets, and the same edges, as `lp_ws281x::LAG_BUCKETS`
/// (`state.rs::lag_bucket(advanced, half) = advanced * 8 / half`, saturating
/// at 8). Deliberately identical so that the emulator's histogram and the
/// guest's `hist=`/`entry_hist=` in the `[WS281X]` line can be printed side
/// by side and read as the same shape — two measurements of one race, one
/// from inside the ISR and one from the transmitter it is racing.
pub const LAG_BUCKETS: usize = 9;

/// Which eighth of `half` `words` falls in; `LAG_BUCKETS - 1` for `≥ half`.
pub fn lag_bucket(words: u64, half: u32) -> usize {
    if half == 0 {
        return LAG_BUCKETS - 1;
    }
    ((words * 8) / u64::from(half)).min(LAG_BUCKETS as u64 - 1) as usize
}

/// What one channel's refills cost, in words — the emulator's own reading of
/// the race the `ws281x_telemetry` line reports from the other side.
///
/// **Reported, never gated** (D13/PD9). The two figures are the two halves of
/// discovery §4's `entry delay` and `refill lag`, measured here in the units
/// the driver measures them in — words the transmitter consumed:
///
/// * `entry` — from the `tx_thr_event` to the guest's next `ch_tx_lim` write
///   for that channel. On silicon that is the interrupt latency plus esp-hal's
///   dispatch, which is where the flash misses live (silicon's `entry_max` is
///   20–29 words); here the ISR path is RAM-resident by construction and the
///   machine has no flash-miss cost at all, so `t1`'s figure is a floor, not
///   a prediction.
/// * `fill` — from that write to the last RAM write inside the channel's
///   window before the next threshold or the end of the frame. This is the
///   `fill_half` loop itself, and it is the half the two configurations can
///   sensibly be compared on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefillStats {
    /// Threshold events whose refill was measured end to end.
    pub refills: u64,
    pub entry_max: u64,
    pub entry_hist: [u64; LAG_BUCKETS],
    pub fill_max: u64,
    pub fill_hist: [u64; LAG_BUCKETS],
    /// Threshold events where the guest never wrote `tx_lim` before the next
    /// threshold or the end of the frame. Not an error — the last threshold
    /// of a frame is answered by `finish`, not by `refill` — but a number
    /// that should stay small, and a large one means the ISR is missing
    /// events.
    pub unanswered: u64,
    /// The half-window the buckets were computed against, as latched at the
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
    /// A threshold fired at `at` with the engine `words` into the frame;
    /// waiting for the ISR's `ch_tx_lim` write.
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

/// The function clock as PCR describes it: `src / (div_num + 1 + a/b)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Clock {
    src_hz: u64,
    div_num: u32,
    div_a: u32,
    div_b: u32,
}

impl Clock {
    /// Decode the two PCR words, or say why there is no clock.
    fn decode(conf: u32, sclk: u32) -> Result<Self, &'static str> {
        if conf & PCR_RMT_CLK_EN == 0 {
            return Err("PCR rmt_conf.clk_en = 0");
        }
        if sclk & SCLK_EN == 0 {
            return Err("PCR rmt_sclk_conf.sclk_en = 0");
        }
        let src_hz = match (sclk >> SCLK_SEL_SHIFT) & 3 {
            1 => 80_000_000,
            3 => super::XTAL_HZ,
            // FOSC's calibrated frequency is not known here (discovery
            // §10.6); refuse rather than guess.
            2 => return Err("PCR rmt_sclk_conf.sclk_sel = 2 (FOSC): not modelled"),
            _ => return Err("PCR rmt_sclk_conf.sclk_sel = 0: no clock"),
        };
        Ok(Self {
            src_hz,
            div_num: (sclk >> SCLK_DIV_NUM_SHIFT) & SCLK_DIV_NUM_MASK,
            div_a: (sclk >> SCLK_DIV_A_SHIFT) & SCLK_DIV_A_MASK,
            div_b: sclk & SCLK_DIV_B_MASK,
        })
    }

    /// The divider as a fraction `num / den` of the source period
    /// (`div_num + 1 + a/b`; `b = 0` means no fractional part).
    fn divider(&self) -> (u128, u128) {
        let whole = u128::from(self.div_num) + 1;
        if self.div_b == 0 {
            (whole, 1)
        } else {
            (
                whole * u128::from(self.div_b) + u128::from(self.div_a),
                u128::from(self.div_b),
            )
        }
    }

    /// The function clock in Hz, for the log.
    fn hz(&self) -> u64 {
        let (num, den) = self.divider();
        (u128::from(self.src_hz) * den / num) as u64
    }

    /// CPU cycles for `ticks` channel ticks at `div_cnt`, exactly: floor of
    /// `ticks × CPU_HZ × div_cnt × divider / src`. Over absolute ticks, so
    /// no per-word rounding ever accumulates.
    fn cycles_for(&self, ticks: u64, div_cnt: u32) -> Cycles {
        let (num, den) = self.divider();
        let n = u128::from(ticks) * u128::from(memmap::CPU_HZ) * u128::from(div_cnt) * num;
        let d = u128::from(self.src_hz) * den;
        (n / d) as Cycles
    }

    /// The inverse, for the receiver: how many channel ticks `cycles` CPU
    /// cycles are, exactly, floored.
    ///
    /// Applied to the **absolute** cycle offset from the receive's anchor and
    /// never per pulse, for [`TxEngine`]'s reason — a per-pulse conversion
    /// would accumulate a rounding error across a 6,144-word frame. At the
    /// shipped settings (80 MHz PLL, `div_cnt = 1`, a 160 MHz CPU) one tick is
    /// exactly two cycles and nothing rounds at all.
    fn ticks_for(&self, cycles: Cycles, div_cnt: u32) -> u64 {
        let (num, den) = self.divider();
        let n = u128::from(cycles) * u128::from(self.src_hz) * den;
        let d = u128::from(memmap::CPU_HZ) * u128::from(div_cnt) * num;
        (n / d) as u64
    }
}

/// Why the current word is the frame's last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    No,
    /// A zero second half (§10.4): half 1 goes out, then `tx_end`.
    HalfTwoZero,
    /// The window ran out with `mem_tx_wrap_en` clear: `tx_err` was raised
    /// at the fetch; the engine stops when the word ends.
    Overrun,
}

/// One TX channel's engine.
#[derive(Debug)]
struct TxEngine {
    /// `ch_tx_conf0` as latched at the last `conf_update`.
    latched: u32,
    /// Strobe bits written since the last `conf_update`.
    pending: u32,
    running: bool,
    /// Running, but with no function clock; resumes on the poll.
    stalled: bool,
    /// Absolute word index into the 192-word RAM: the next word to fetch
    /// (modeled, §10.2).
    raddr: u32,
    /// Ticks consumed since `tx_start`.
    ticks_since_start: u64,
    /// The anchor: tick `anchor_ticks` happened at cycle `anchor_cycle`.
    /// `tx_start` sets (0, start cycle); a stall/resume or a divider change
    /// re-anchors at the current word so nothing before it moves.
    anchor_cycle: Cycles,
    anchor_ticks: u64,
    anchor_clock: Option<Clock>,
    /// The pending `EV_WORD`'s due cycle.
    word_due: Cycles,
    ending: Ending,
    mem_empty: bool,
    /// Words fetched in the current frame, for the `end` note.
    frame_words: u32,
    /// Words fetched since the machine started, on this channel. The unit
    /// both halves of the refill measurement count in, and monotonic across
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
            latched: CH_TX_CONF0_RESET,
            pending: 0,
            running: false,
            stalled: false,
            raddr: BLOCK_WORDS * ch as u32,
            ticks_since_start: 0,
            anchor_cycle: 0,
            anchor_ticks: 0,
            anchor_clock: None,
            word_due: 0,
            ending: Ending::No,
            mem_empty: false,
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

    fn div_cnt(&self) -> u32 {
        match (self.latched >> CONF_DIV_CNT_SHIFT) & 0xff {
            0 => 256,
            n => n,
        }
    }

    fn window_words(&self) -> u32 {
        BLOCK_WORDS * ((self.latched >> CONF_MEM_SIZE_SHIFT) & 0x7)
    }

    fn wrap(&self) -> bool {
        self.latched & CONF_MEM_TX_WRAP_EN != 0
    }

    fn idle_level(&self) -> bool {
        self.latched & CONF_IDLE_OUT_LV != 0
    }

    /// The cycle at which absolute tick `ticks` falls, from the anchor.
    fn cycle_at(&self, clock: &Clock, ticks: u64) -> Cycles {
        self.anchor_cycle
            .saturating_add(clock.cycles_for(ticks - self.anchor_ticks, self.div_cnt()))
    }
}

/// One RX channel's engine — a **sampler**, not a decoder (M2 P3).
///
/// What it models is the receiver the TRM describes: the level of the pad
/// routed to the channel's input signal, measured in channel ticks, written
/// into the channel's RAM window as (level, duration) pairs two to a word,
/// with `idle_thres` ending the reception, `rx_filter` swallowing narrow
/// pulses, `rx_lim` raising `rx_thr_event` and `mem_rx_wrap_en` wrapping the
/// write pointer. Both events go out on source 49 with exactly the status and
/// clear semantics the TX side already has.
///
/// # It is driven by edges, not by a clock
///
/// A tick-by-tick sampler would be 100 events per WS2812 bit and 614,400 per
/// frame, so the engine is fed the fabric's **edges** instead — the same
/// stream the pin log and the strip decoders read, handed over by the machine
/// at every slice boundary — and converts each edge's cycle into a tick with
/// [`Clock::ticks_for`]. The result is identical to sampling because a
/// sampler's only output is where the level changed; what is *not* modelled
/// is a change narrower than a tick, which the fabric has no way to express
/// either (its edges are stamped in cycles and levels alternate).
///
/// The consequence to state plainly: a receiver sees an edge at the slice
/// boundary after it happened, so `rx_end` and `rx_thr_event` are raised up
/// to one slice late — the words themselves carry the edges' own cycles, so
/// nothing in the RAM moves. It is deterministic (slice boundaries are guest
/// cycles) and it is the same latency the GPIO block's input side has.
#[derive(Debug)]
struct RxEngine {
    /// `ch_rx_conf0` / `ch_rx_conf1` as latched at the last `conf_update`.
    conf0: u32,
    conf1: u32,
    /// Strobe bits written since the last `conf_update`.
    pending: u32,
    /// Receiving: `rx_en` was latched and the engine has not ended.
    running: bool,
    /// Running, but still waiting for the first edge.
    ///
    /// **Modeled.** The reception begins at the first level change and the
    /// leading idle is neither stored nor counted towards `idle_thres`; a
    /// receiver that started its idle timer on an already-idle line would end
    /// before the frame it was armed for ever arrived, and no driver could
    /// use it. esp-hal's blocking `receive`/`wait` is written against exactly
    /// that behaviour — it arms and then spins until `rx_end`.
    armed: bool,
    /// Absolute word index into the 192-word RAM: the **next word to write**,
    /// which is what `ch_rx_status.mem_waddr_ex` reads back (esp-hal's
    /// `hw_offset`, `rmt/reader.rs`: "the next code the hardware would
    /// write").
    waddr: u32,
    /// Tick 0 of this reception happened at cycle `anchor_cycle`.
    anchor_cycle: Cycles,
    anchor_clock: Option<Clock>,
    /// The run being measured: its level, and the tick it started at.
    run_level: bool,
    run_start: u64,
    /// An edge the filter has accepted no verdict on yet: `(tick, the level
    /// after it)`. Only ever occupied while `rx_filter_en` is set with a
    /// non-zero threshold.
    pending_edge: Option<(u64, bool)>,
    /// The first half of the word being assembled, when one is open.
    half: Option<(bool, u32)>,
    /// Words written since the last `rx_thr_event` — the `rx_lim` counter.
    since_thr: u32,
    /// Words written in this reception, for the note and for a test.
    frame_words: u64,
    /// Words written since the machine started, on this channel.
    words_written: u64,
    /// `ch_rx_status.mem_full` (bit 26), sticky until the next start.
    mem_full: bool,
    /// The cycle the pending `EV_RX_IDLE` is due at.
    idle_due: Cycles,
    /// One note per reception that had no pad routed to it.
    warned_unrouted: bool,
}

impl RxEngine {
    fn new(rxi: usize) -> Self {
        Self {
            conf0: CH_RX_CONF0_RESET,
            conf1: CH_RX_CONF1_RESET,
            pending: 0,
            running: false,
            armed: false,
            waddr: BLOCK_WORDS * Self::channel(rxi) as u32,
            anchor_cycle: 0,
            anchor_clock: None,
            run_level: false,
            run_start: 0,
            pending_edge: None,
            half: None,
            since_thr: 0,
            frame_words: 0,
            words_written: 0,
            mem_full: false,
            idle_due: 0,
            warned_unrouted: false,
        }
    }

    /// The absolute channel number: receiver index 0 is channel 2.
    const fn channel(rxi: usize) -> usize {
        RX_CH_BASE + rxi
    }

    fn div_cnt(&self) -> u32 {
        match (self.conf0 >> RX_CONF0_DIV_CNT_SHIFT) & 0xff {
            0 => 256,
            n => n,
        }
    }

    /// `idle_thres`, in channel ticks: *"when no edge is detected on the input
    /// signal and continuous clock cycles is longer than this register value,
    /// received process is finished"* (PAC, `ch_rx_conf0.IDLE_THRES`).
    fn idle_thres(&self) -> u32 {
        (self.conf0 >> RX_CONF0_IDLE_THRES_SHIFT) & RX_CONF0_IDLE_THRES_MASK
    }

    fn window_words(&self) -> u32 {
        BLOCK_WORDS * ((self.conf0 >> RX_CONF0_MEM_SIZE_SHIFT) & 0x7)
    }

    fn wrap(&self) -> bool {
        self.conf1 & RX_CONF1_MEM_RX_WRAP_EN != 0
    }

    /// The filter width in channel ticks, or `None` when the filter is off.
    ///
    /// The register is *"in APB clock periods"* (PAC,
    /// `ch_rx_conf1.RX_FILTER_THRES`) — the filter sits on the pad's side of
    /// the divider, so it does **not** scale with `div_cnt`. The conversion
    /// to ticks is therefore `thres × f_channel / f_apb`, and on this chip
    /// `f_apb` is [`APB_HZ`].
    fn filter_ticks(&self, clock: &Clock) -> Option<u64> {
        if self.conf1 & RX_CONF1_FILTER_EN == 0 {
            return None;
        }
        let thres =
            u64::from((self.conf1 >> RX_CONF1_FILTER_THRES_SHIFT) & RX_CONF1_FILTER_THRES_MASK);
        if thres == 0 {
            return None;
        }
        // Channel ticks the filter's APB periods are worth, rounded up so a
        // pulse that is exactly the threshold wide still passes.
        let (num, den) = clock.divider();
        let channel_hz = (u128::from(clock.src_hz) * den / num) as u64;
        let ticks = (u128::from(thres) * u128::from(channel_hz))
            .div_ceil(u128::from(APB_HZ) * u128::from(self.div_cnt()));
        Some((ticks as u64).max(1))
    }

    /// The cycle at which absolute tick `ticks` falls, from the anchor.
    fn cycle_at(&self, clock: &Clock, ticks: u64) -> Cycles {
        self.anchor_cycle
            .saturating_add(clock.cycles_for(ticks, self.div_cnt()))
    }

    /// The absolute tick that cycle `at` falls on, from the anchor.
    fn tick_at(&self, clock: &Clock, at: Cycles) -> u64 {
        clock.ticks_for(at.saturating_sub(self.anchor_cycle), self.div_cnt())
    }
}

/// The APB clock the RX filter's threshold is counted in.
///
/// **Documented.** The ESP32-C6 has no configurable APB divider: the TRM's
/// clock tree fixes `APB_CLK` at 80 MHz from the PLL, and esp-hal carries the
/// same number (`esp-metadata-generated-0.4.0`, the C6's `apb_clock`). It
/// matters only when `rx_filter_en` is set, which the `rmt-rx` payload leaves
/// off — the filter is exercised by this file's own tests instead.
pub const APB_HZ: u64 = 80_000_000;

/// The RMT block.
#[derive(Debug)]
pub struct Rmt {
    index: usize,
    regs: RegFile,
    grades: RegGrades,
    ram: Box<[u32; RAM_WORDS]>,
    clock: RmtClockLine,
    ch: [TxEngine; TX_CHANNELS],
    rx: [RxEngine; RX_CHANNELS],
    /// Sticky `int_raw` bits: TX at 0..1 / 4..5 / 8..9, RX at 2..3 / 6..7 /
    /// 10..11.
    sticky: u32,
    poll_armed: bool,
    warned_fifo: bool,
    warned_rx_carrier: [bool; RX_CHANNELS],
    warned_gap: bool,
    warned_ref_cnt: [bool; 4],
    warned_late_start: [bool; TX_CHANNELS],
    /// Whether the pulse and word logs are kept. **Off by default**: since
    /// P2 the waveform reaches the fabric, and the logs are the word-level
    /// oracle a test compares the decoder against — a 24-frame run holds
    /// 305,490 pulses, which a long CLI run has no use for. `just
    /// test-emu-c6`'s gates turn them on through
    /// [`crate::machine::Esp32C6Builder::rmt_logs`].
    keep_logs: bool,
}

/// A trace note, formatted only when the trace is on.
fn note(cx: &mut BusCx<'_>, f: impl FnOnce() -> String) {
    if cx.trace.is_enabled() {
        let line = f();
        cx.trace.note(&line);
    }
}

impl Rmt {
    pub fn new(clock: RmtClockLine) -> Self {
        let regs = RegFile::new("RMT", REGS_LEN).with_names(regs::RMT);
        Self {
            index: 0,
            regs,
            grades: Self::grades(),
            ram: Box::new([0; RAM_WORDS]),
            clock,
            ch: [TxEngine::new(0), TxEngine::new(1)],
            rx: [RxEngine::new(0), RxEngine::new(1)],
            sticky: 0,
            poll_armed: false,
            warned_fifo: false,
            warned_rx_carrier: [false; RX_CHANNELS],
            warned_gap: false,
            warned_ref_cnt: [false; 4],
            warned_late_start: [false; TX_CHANNELS],
            keep_logs: false,
        }
    }

    /// The block's register grades (M2 P3 — the debt sweep graded the accept
    /// blocks and left the modelled ones to their owners).
    ///
    /// Everything unlisted is `Modeled`, which for this block means
    /// accept-and-remember at the PAC's reset value. Nothing here is
    /// `Measured`: the `rmt-chase` transcripts agree with silicon frame for
    /// frame, but what they measure is the *waveform*, not a register's bit
    /// map, and `validate.toml`'s `pin` entry gives the argument for why that
    /// is not the same claim.
    pub fn grades() -> RegGrades {
        let mut g = RegGrades::new()
            .with_grade(INT_RAW, RegGrade::Documented)
            .with_grade(INT_ST, RegGrade::Documented)
            .with_grade(INT_ENA, RegGrade::Documented)
            .with_grade(INT_CLR, RegGrade::Documented)
            .with_grade(SYS_CONF, RegGrade::Documented);
        for ch in 0..TX_CHANNELS {
            g = g
                .with_grade(CH_TX_CONF0[ch], RegGrade::Documented)
                .with_grade(CH_TX_STATUS[ch], RegGrade::Documented)
                .with_grade(CH_TX_LIM[ch], RegGrade::Documented);
        }
        for rxi in 0..RX_CHANNELS {
            g = g
                .with_grade(CH_RX_CONF0[rxi], RegGrade::Documented)
                .with_grade(CH_RX_CONF1[rxi], RegGrade::Documented)
                .with_grade(CH_RX_STATUS[rxi], RegGrade::Documented)
                .with_grade(CH_RX_LIM[rxi], RegGrade::Documented);
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

    // ---- observation --------------------------------------------------------

    /// Every pulse channel `ch` has put on its signal, in order.
    pub fn pulses(&self, ch: usize) -> &[Pulse] {
        &self.ch[ch].pulses
    }

    /// Every word channel `ch` fetched, with the cycle it was fetched at —
    /// STOP words included, so a frame is `data … latch STOP`.
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

    /// The level the signal rests at between frames (`idle_out_lv`, as
    /// latched) — what P2's fabric drives when no pulse is on the wire.
    pub fn idle_level(&self, ch: usize) -> bool {
        self.ch[ch].idle_level()
    }

    /// The RAM as the engine sees it.
    pub fn ram(&self) -> &[u32; RAM_WORDS] {
        &self.ram
    }

    // ---- clock --------------------------------------------------------------

    fn clock(&self) -> Result<Clock, &'static str> {
        Clock::decode(self.clock.conf.get(), self.clock.sclk.get())
    }

    // ---- interrupts ---------------------------------------------------------

    fn raise(&mut self, ch: usize, shift: u32) {
        self.sticky |= 1 << (shift + ch as u32);
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.sticky & self.regs.stored(INT_ENA);
        cx.irq.set_level(source::RMT, st != 0);
    }

    // ---- the engine ---------------------------------------------------------

    fn tx_lim(&self, ch: usize) -> u32 {
        self.regs.stored(CH_TX_LIM[ch]) & TX_LIM_MASK
    }

    fn cancel_word(&mut self, ch: usize, cx: &mut BusCx<'_>) {
        cx.sched.cancel(event_id(self.index, EV_WORD + ch as u16));
    }

    // ---- the refill measurement (reported, never gated) ---------------------

    /// Close whatever refill measurement channel `ch` has in flight, then
    /// optionally open a new one at `now`.
    ///
    /// Called at every threshold (close the previous, open this one) and at
    /// the end of a frame (close, open nothing). A measurement that never saw
    /// a `ch_tx_lim` write is counted as unanswered rather than recorded with
    /// an invented entry delay.
    fn close_refill(&mut self, ch: usize, cx: &mut BusCx<'_>) {
        let half = self.ch[ch].window_words() / 2;
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

    /// The ISR wrote `ch_tx_lim`: the entry delay is settled, the fill begins.
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
            let start = BLOCK_WORDS * ch as u32;
            let end = start + self.ch[ch].window_words();
            if word_index < start || word_index >= end {
                continue;
            }
            let words = self.ch[ch].words_consumed;
            if let RefillProbe::Filling { last_write, .. } = &mut self.ch[ch].refill {
                *last_write = words;
            }
        }
    }

    /// What channel `ch`'s refills have cost, in words.
    pub fn refill_stats(&self, ch: usize) -> RefillStats {
        self.ch[ch].refill_stats
    }

    /// `conf_update`: latch the configuration and act on the strobes
    /// written since the last one (§10.3).
    fn conf_update(&mut self, ch: usize, cx: &mut BusCx<'_>) {
        let latched = self.regs.stored(CH_TX_CONF0[ch]);
        let pending = core::mem::take(&mut self.ch[ch].pending);
        self.ch[ch].latched = latched;
        // A `tx_start` written before this `conf_update` armed the
        // slice-boundary fallback; this is the update it was waiting for.
        cx.sched
            .cancel(event_id(self.index, EV_LATE_START + ch as u16));
        if pending & CONF_MEM_RD_RST != 0 {
            self.ch[ch].raddr = BLOCK_WORDS * ch as u32;
        }
        // `apb_mem_rst` resets the APB write pointer, which direct RAM
        // access never uses: accepted.
        if latched & CONF_TX_STOP != 0 {
            if self.ch[ch].running {
                self.stop(ch, cx);
            }
            if pending & CONF_TX_START != 0 {
                let now = cx.now;
                note(cx, || {
                    format!("cyc={now} RMT ch{ch} tx_start ignored: tx_stop is set")
                });
            }
            return;
        }
        if pending & CONF_TX_START != 0 {
            let now = cx.now;
            self.start(ch, now, cx);
        }
    }

    fn start(&mut self, ch: usize, now: Cycles, cx: &mut BusCx<'_>) {
        if self.ch[ch].window_words() == 0 {
            note(cx, || {
                format!("cyc={now} RMT ch{ch} start refused: mem_size 0 (no window)")
            });
            return;
        }
        if self.ch[ch].running {
            self.cancel_word(ch, cx);
        }
        let clock = self.clock().ok();
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
            e.frame_words = 0;
        }
        let e = &self.ch[ch];
        let hz = clock.map(|c| c.hz()).unwrap_or(0);
        let (div_cnt, wrap, tx_lim) = (e.div_cnt(), e.wrap(), self.tx_lim(ch));
        let (ws, ww) = (BLOCK_WORDS * ch as u32, e.window_words());
        let raddr = e.raddr;
        note(cx, || {
            format!(
                "cyc={now} RMT ch{ch} start f_rmt={hz} div_cnt={div_cnt} window={ws}..{} \
                 raddr={raddr} wrap={} tx_lim={tx_lim}",
                ws + ww,
                u8::from(wrap)
            )
        });
        self.fetch(ch, now, cx);
    }

    /// `tx_stop` (sticky, at `conf_update`): idle level, no `tx_end`
    /// (modeled — esp-hal's `stop` expects none).
    fn stop(&mut self, ch: usize, cx: &mut BusCx<'_>) {
        self.cancel_word(ch, cx);
        self.close_refill(ch, cx);
        let e = &mut self.ch[ch];
        e.running = false;
        e.stalled = false;
        e.ending = Ending::No;
        let idle_level = e.idle_level();
        let idle = u8::from(idle_level);
        let now = cx.now;
        cx.pins.drive(signal_of(ch), idle_level, now);
        note(cx, || format!("cyc={now} RMT ch{ch} stop idle={idle}"));
    }

    /// The frame ends here: `tx_end`, output to the idle level.
    fn end_frame(&mut self, ch: usize, at: Cycles, cx: &mut BusCx<'_>) {
        self.cancel_word(ch, cx);
        // A refill still in flight ends with the frame: the driver's `finish`
        // answers the last threshold, not `refill`.
        self.close_refill(ch, cx);
        let e = &mut self.ch[ch];
        e.running = false;
        e.stalled = false;
        e.ending = Ending::No;
        e.frames_ended += 1;
        let words = e.frame_words;
        let idle_level = e.idle_level();
        let idle = u8::from(idle_level);
        cx.pins.drive(signal_of(ch), idle_level, at);
        self.raise(ch, INT_TX_END_SHIFT);
        note(cx, || {
            format!("cyc={at} RMT ch{ch} end words={words} idle={idle}")
        });
        self.update_lines(cx);
    }

    /// The engine has no clock: hold the current word until it comes back.
    fn stall(&mut self, ch: usize, at: Cycles, reason: &'static str, cx: &mut BusCx<'_>) {
        let e = &mut self.ch[ch];
        e.stalled = true;
        if !e.stall_noted {
            e.stall_noted = true;
            note(cx, || {
                format!(
                    "cyc={at} RMT ch{ch} stalled: {reason} (the engine resumes when PCR gives it a clock)"
                )
            });
        }
        if !self.poll_armed {
            self.poll_armed = true;
            cx.sched.schedule_at(
                cx.now.saturating_add(CLOCK_POLL_CYCLES),
                event_id(self.index, EV_CLOCK_POLL),
            );
        }
    }

    /// The channel's output goes to `pulse.level` at `pulse.at`, and the log
    /// (if it is being kept) records it.
    ///
    /// The two halves of a word are emitted together, at the fetch, each
    /// stamped with the cycle it actually starts at — so the fabric can see
    /// an edge up to one word ahead of the slice boundary. They are never
    /// out of order (a word's second half starts after its first, and the
    /// next word is fetched at this one's end), which is all a decoder or a
    /// pin log needs.
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

    /// Fetch the word at `raddr` at cycle `now` (the word's start), emit its
    /// pulses, schedule the word's end, advance the pointer, and apply the
    /// threshold and wrap rules.
    fn log_word(&mut self, ch: usize, now: Cycles, word: u32, cx: &mut BusCx<'_>) {
        let e = &mut self.ch[ch];
        e.frame_words += 1;
        // The refill measurement's unit: a word the transmitter has taken out
        // of the RAM and will not read again. Counted here rather than in
        // `fetch` because a stalled engine has not consumed the word it is
        // holding — `fetch` returns before this on the no-clock path.
        e.words_consumed += 1;
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

    fn fetch(&mut self, ch: usize, now: Cycles, cx: &mut BusCx<'_>) {
        let raddr = self.ch[ch].raddr as usize;
        let word = self.ram[raddr.min(RAM_WORDS - 1)];
        let dur1 = word & 0x7fff;
        let level1 = word & (1 << 15) != 0;
        let dur2 = (word >> 16) & 0x7fff;
        let level2 = word & (1 << 31) != 0;
        if dur1 == 0 {
            // The all-zero STOP word, or a zero first half (modeled, §10.4):
            // the transmission ends before it — no clock needed.
            self.log_word(ch, now, word, cx);
            self.end_frame(ch, now, cx);
            return;
        }

        let clock = match self.clock() {
            Ok(c) => c,
            Err(reason) => {
                // Not fetched yet: the word is logged when it actually goes
                // out, on the resume.
                self.stall(ch, now, reason, cx);
                return;
            }
        };
        self.log_word(ch, now, word, cx);
        {
            let e = &mut self.ch[ch];
            let t0 = e.ticks_since_start;
            if e.stalled {
                // Resuming: this word starts now.
                e.stalled = false;
                e.stall_noted = false;
                e.anchor_cycle = now;
                e.anchor_ticks = t0;
                e.anchor_clock = Some(clock);
                note(cx, || format!("cyc={now} RMT ch{ch} resumed"));
            } else if e.anchor_clock != Some(clock) {
                // The divider changed under a running frame (nothing in the
                // firmware does this): re-anchor at this word so the words
                // already emitted keep their cycles.
                e.anchor_cycle = now;
                e.anchor_ticks = t0;
                e.anchor_clock = Some(clock);
            }
        }
        let e = &self.ch[ch];
        let t0 = e.ticks_since_start;
        let word_start = e.cycle_at(&clock, t0);
        let second_at = e.cycle_at(&clock, t0 + u64::from(dur1));
        let word_end = e.cycle_at(&clock, t0 + u64::from(dur1) + u64::from(dur2));
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
            // Half 1 goes out; the word ends the frame (modeled, §10.4).
            self.ch[ch].ending = Ending::HalfTwoZero;
        }
        {
            let e = &mut self.ch[ch];
            e.ticks_since_start = t0 + u64::from(dur1) + u64::from(dur2);
            e.word_due = word_end;
        }
        cx.sched
            .schedule_at(word_end, event_id(self.index, EV_WORD + ch as u16));

        // Advance: the pointer now names the next word to fetch (§10.2).
        let window_start = BLOCK_WORDS * ch as u32;
        let window_words = self.ch[ch].window_words();
        self.ch[ch].raddr += 1;
        let pos = self.ch[ch].raddr - window_start;
        if pos == self.tx_lim(ch) {
            // Position semantics: `tx_lim == window_words` is the wrap.
            self.raise(ch, INT_TX_THR_SHIFT);
            note(cx, || format!("cyc={now} RMT ch{ch} thr pos={pos}"));
            // The previous refill ends here and this one begins. `now` is the
            // word's own start cycle, which is when the pointer reached the
            // threshold — not the dispatch cycle.
            self.open_refill(ch, now, cx);
        }
        if pos >= window_words {
            if self.ch[ch].wrap() {
                self.ch[ch].raddr = window_start;
            } else if self.ch[ch].ending == Ending::No {
                self.raise(ch, INT_TX_ERR_SHIFT);
                self.ch[ch].mem_empty = true;
                self.ch[ch].ending = Ending::Overrun;
                note(cx, || {
                    format!("cyc={now} RMT ch{ch} err mem_empty (window end, wrap off)")
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
                let e = &mut self.ch[ch];
                e.running = false;
                e.ending = Ending::No;
                let idle_level = e.idle_level();
                cx.pins.drive(signal_of(ch), idle_level, due);
                note(cx, || {
                    format!("cyc={due} RMT ch{ch} stopped after mem_empty")
                });
            }
        }
    }

    // ---- the receiver (M2 P3) -----------------------------------------------

    /// `rx_lim` for receiver `rxi`, in words.
    fn rx_lim(&self, rxi: usize) -> u32 {
        self.regs.stored(CH_RX_LIM[rxi]) & RX_LIM_MASK
    }

    /// The first word of receiver `rxi`'s RAM window.
    fn rx_window_start(rxi: usize) -> u32 {
        BLOCK_WORDS * RxEngine::channel(rxi) as u32
    }

    /// `conf_update` on an RX channel: latch both configuration words and act
    /// on the strobes written since the last one, exactly as the TX side's
    /// does (discovery §10.3, and esp-hal's `update()` writes the two bits in
    /// the same place for both directions).
    fn rx_conf_update(&mut self, rxi: usize, cx: &mut BusCx<'_>) {
        let conf0 = self.regs.stored(CH_RX_CONF0[rxi]);
        let conf1 = self.regs.stored(CH_RX_CONF1[rxi]);
        let pending = core::mem::take(&mut self.rx[rxi].pending);
        self.rx[rxi].conf0 = conf0;
        self.rx[rxi].conf1 = conf1;
        if pending & RX_CONF1_MEM_WR_RST != 0 {
            self.rx[rxi].waddr = Self::rx_window_start(rxi);
        }
        // `apb_mem_rst` resets the APB *read* pointer, which the reader in
        // esp-hal tracks in software: accepted.
        let enabled = conf1 & RX_CONF1_RX_EN != 0;
        if enabled && !self.rx[rxi].running {
            let now = cx.now;
            self.rx_start(rxi, now, cx);
        } else if !enabled && self.rx[rxi].running {
            self.rx_stop(rxi, cx);
        }
    }

    /// Arm receiver `rxi`: it now watches its pad and waits for the first
    /// edge.
    fn rx_start(&mut self, rxi: usize, now: Cycles, cx: &mut BusCx<'_>) {
        let ch = RxEngine::channel(rxi);
        if self.rx[rxi].window_words() == 0 {
            note(cx, || {
                format!("cyc={now} RMT ch{ch} rx start refused: mem_size 0 (no window)")
            });
            return;
        }
        let clock = self.clock().ok();
        let signal = rx_signal_of(rxi);
        let route = cx.pins.input_route_of(signal);
        let level = cx.pins.input_level(signal).unwrap_or(false);
        {
            let e = &mut self.rx[rxi];
            e.running = true;
            e.armed = true;
            e.anchor_cycle = now;
            e.anchor_clock = clock;
            e.run_level = level;
            e.run_start = 0;
            e.pending_edge = None;
            e.half = None;
            e.since_thr = 0;
            e.frame_words = 0;
            e.mem_full = false;
        }
        self.cancel_rx_idle(rxi, cx);
        let e = &self.rx[rxi];
        let (idle, div_cnt, lim) = (e.idle_thres(), e.div_cnt(), self.rx_lim(rxi));
        let (ws, ww) = (Self::rx_window_start(rxi), e.window_words());
        let filter = clock.as_ref().and_then(|c| e.filter_ticks(c));
        let wrap = u8::from(e.wrap());
        let waddr = e.waddr;
        let where_from = match route {
            Some((pad, invert)) => format!("{pad}{}", if invert { " (inverted)" } else { "" }),
            None => "nothing".into(),
        };
        note(cx, || {
            format!(
                "cyc={now} RMT ch{ch} rx start from {where_from} div_cnt={div_cnt} \
                 idle_thres={idle} filter={} window={ws}..{} waddr={waddr} wrap={wrap} \
                 rx_lim={lim} level={}",
                filter.map_or("off".to_string(), |t| format!("{t} ticks")),
                ws + ww,
                u8::from(level),
            )
        });
        if self.rx[rxi].conf1 & RX_CONF1_MEM_OWNER == 0 {
            // `mem_owner` 0 is "the APB bus is using the RAM" (PAC). esp-hal
            // sets it in the same write as `rx_en`; a reception started
            // without it is accepted and named rather than refused, and
            // `mem_owner_err` is not raised — the model has one RAM and no
            // arbiter (see the block header's "what is not here").
            note(cx, || {
                format!(
                    "cyc={now} RMT ch{ch} rx_en with mem_owner = 0 (APB owns the RAM): accepted, not modelled"
                )
            });
        }
        if route.is_none() && !self.rx[rxi].warned_unrouted {
            self.rx[rxi].warned_unrouted = true;
            note(cx, || {
                format!(
                    "cyc={now} RMT ch{ch} rx_en with no pad routed to its input signal \
                     (GPIO func_in_sel_cfg): the receiver will never see an edge"
                )
            });
        }
    }

    /// `rx_en` cleared while running: the receiver stops where it is, with no
    /// `rx_end` and nothing flushed (esp-hal's `stop_rx`, which it calls
    /// *after* the reception has already ended, expects neither).
    fn rx_stop(&mut self, rxi: usize, cx: &mut BusCx<'_>) {
        self.cancel_rx_idle(rxi, cx);
        self.rx[rxi].running = false;
        self.rx[rxi].armed = false;
        self.rx[rxi].pending_edge = None;
    }

    fn cancel_rx_idle(&mut self, rxi: usize, cx: &mut BusCx<'_>) {
        cx.sched
            .cancel(event_id(self.index, EV_RX_IDLE + rxi as u16));
    }

    /// The idle timer runs from the start of the current level's run: the
    /// reception ends one tick past `idle_thres` of no edge.
    ///
    /// The **cancel is load-bearing**: `Scheduler::schedule_at` appends
    /// rather than replaces, so an engine that re-armed without cancelling
    /// would leave one stale deadline per edge behind and end its reception
    /// `idle_thres` after the *first* edge of the frame. That is not a
    /// hypothetical — it is what the first loopback run did, and the words it
    /// produced (329 of 1,536, which is 409 us at 1.25 us a bit) named the
    /// bug exactly.
    fn arm_rx_idle(&mut self, rxi: usize, cx: &mut BusCx<'_>) {
        let Ok(clock) = self.clock() else { return };
        self.cancel_rx_idle(rxi, cx);
        let e = &self.rx[rxi];
        if !e.running || e.armed {
            return;
        }
        let due = e.cycle_at(&clock, e.run_start + u64::from(e.idle_thres()) + 1);
        // An edge is handed over at the slice boundary after it happened, so
        // a deadline can already be in the past by the time it is armed. It
        // fires at the next dispatch, which is a guest cycle like any other.
        let due = due.max(cx.now);
        self.rx[rxi].idle_due = due;
        cx.sched
            .schedule_at(due, event_id(self.index, EV_RX_IDLE + rxi as u16));
    }

    /// The machine's slice drain hands every pad edge to the block; a running
    /// receiver takes the ones on the pad its input signal reads.
    ///
    /// The mirror of [`super::gpio::Gpio::observe_edges`], and it reads the
    /// same stream — so a receiver, the pin log and a strip decoder can never
    /// disagree about what was on the wire.
    pub fn observe_edges(&mut self, edges: &[Edge], cx: &mut BusCx<'_>) {
        for rxi in 0..RX_CHANNELS {
            if !self.rx[rxi].running {
                continue;
            }
            let Some((pad, invert)) = cx.pins.input_route_of(rx_signal_of(rxi)) else {
                continue;
            };
            for edge in edges {
                if edge.pad != pad || !self.rx[rxi].running {
                    continue;
                }
                self.rx_edge(rxi, edge.at, edge.level != invert, cx);
            }
        }
        self.update_lines(cx);
    }

    /// One edge on receiver `rxi`'s pad, at cycle `at`, leaving the pad at
    /// `level`.
    fn rx_edge(&mut self, rxi: usize, at: Cycles, level: bool, cx: &mut BusCx<'_>) {
        let Ok(clock) = self.clock() else {
            // No function clock: the receiver counts nothing. Stated rather
            // than silently mis-measured; nothing in the firmware gates the
            // RMT clock while a channel is live.
            return;
        };
        let tick = self.rx[rxi].tick_at(&clock, at);
        if self.rx[rxi].armed {
            // The reception begins here (see `RxEngine::armed`): this is the
            // first run, and the idle that preceded it is not a pulse.
            let e = &mut self.rx[rxi];
            e.armed = false;
            e.run_level = level;
            e.run_start = tick;
            self.arm_rx_idle(rxi, cx);
            return;
        }
        match self.rx[rxi].filter_ticks(&clock) {
            None => self.rx_accept_edge(rxi, tick, level, &clock, cx),
            Some(width) => {
                if let Some((pending_tick, pending_level)) = self.rx[rxi].pending_edge {
                    if tick.saturating_sub(pending_tick) < width {
                        // The level did not hold for the filter's width, so
                        // the pulse never reached the edge detector at all:
                        // this edge and the one that opened it both vanish
                        // and the run underneath carries on.
                        self.rx[rxi].pending_edge = None;
                        let ch = RxEngine::channel(rxi);
                        note(cx, || {
                            format!(
                                "cyc={at} RMT ch{ch} rx filtered a {}-tick pulse (< {width})",
                                tick.saturating_sub(pending_tick)
                            )
                        });
                        return;
                    }
                    self.rx_accept_edge(rxi, pending_tick, pending_level, &clock, cx);
                }
                self.rx[rxi].pending_edge = Some((tick, level));
            }
        }
    }

    /// An edge the filter has passed: the run it ends becomes a pulse.
    fn rx_accept_edge(
        &mut self,
        rxi: usize,
        tick: u64,
        level: bool,
        clock: &Clock,
        cx: &mut BusCx<'_>,
    ) {
        let duration = tick.saturating_sub(self.rx[rxi].run_start);
        let run_level = self.rx[rxi].run_level;
        self.rx_push_pulse(rxi, run_level, duration, cx);
        let e = &mut self.rx[rxi];
        e.run_level = level;
        e.run_start = tick;
        let _ = clock;
        self.arm_rx_idle(rxi, cx);
    }

    /// A measured run becomes half a word; a full word goes into the RAM.
    ///
    /// # The encoding, from the PAC's bit map
    ///
    /// One 32-bit word is two (level, duration) pairs, and it is the same
    /// word the transmitter reads — `lp_ws281x::pulse_code` writes it and
    /// [`Rmt::fetch`] takes it apart:
    ///
    /// | bits | field |
    /// |---|---|
    /// | 0:14 | duration of the **first** pulse, in channel ticks |
    /// | 15 | level of the first pulse |
    /// | 16:30 | duration of the **second** pulse |
    /// | 31 | level of the second pulse |
    ///
    /// A duration of zero is the end marker both directions agree on
    /// (esp-hal's `PulseCode::is_end_marker`: *"length1() == 0 || length2()
    /// == 0"*), so a measured run is written as at least 1 and at most
    /// [`DURATION_MAX`] — a run cannot exceed `idle_thres` anyway, because
    /// the reception ends when it does.
    fn rx_push_pulse(&mut self, rxi: usize, level: bool, duration: u64, cx: &mut BusCx<'_>) {
        let duration = (duration.max(1) as u32).min(DURATION_MAX);
        match self.rx[rxi].half.take() {
            None => self.rx[rxi].half = Some((level, duration)),
            Some((l0, d0)) => {
                let word = d0 | (u32::from(l0) << 15) | (duration << 16) | (u32::from(level) << 31);
                self.rx_write_word(rxi, word, cx);
            }
        }
    }

    /// Write one word into the channel's window and apply the threshold and
    /// wrap rules.
    fn rx_write_word(&mut self, rxi: usize, word: u32, cx: &mut BusCx<'_>) {
        let start = Self::rx_window_start(rxi);
        let window = self.rx[rxi].window_words();
        let ch = RxEngine::channel(rxi);
        let waddr = self.rx[rxi].waddr;
        if let Some(slot) = self.ram.get_mut(waddr as usize) {
            *slot = word;
        }
        {
            let e = &mut self.rx[rxi];
            e.waddr += 1;
            e.frame_words += 1;
            e.words_written += 1;
            e.since_thr += 1;
        }
        // `rx_lim` is a **count**, not a position: the PAC's own words for
        // this interrupt are *"triggered when receiver receive more data than
        // configured value"*, and esp-hal never rewrites `ch_rx_lim` between
        // events the way it rewrites `ch_tx_lim` — it reads half a window on
        // each one and alternates its own offset. Modeled, and the driver's
        // use of it (`rx_lim` = half the window) cannot tell a repeating
        // counter from a position; a `rx_lim` that did not divide the window
        // could, and nothing writes one.
        let lim = self.rx_lim(rxi);
        if lim != 0 && self.rx[rxi].since_thr >= lim {
            self.rx[rxi].since_thr = 0;
            self.raise(ch, INT_TX_THR_SHIFT);
            let at = self.rx[rxi].waddr;
            note(cx, || format!("RMT ch{ch} rx thr waddr={at}"));
        }
        if self.rx[rxi].waddr >= start + window {
            if self.rx[rxi].wrap() {
                self.rx[rxi].waddr = start;
            } else {
                // The window is full with no wrap: `mem_full`, `rx_err`, and
                // the reception stops where it is.
                self.rx[rxi].mem_full = true;
                self.raise(ch, INT_TX_ERR_SHIFT);
                note(cx, || {
                    format!("RMT ch{ch} rx err mem_full (window end, wrap off)")
                });
                self.rx_stop(rxi, cx);
            }
        }
        self.update_lines(cx);
    }

    /// The idle threshold expired: flush what is in hand and raise `rx_end`.
    ///
    /// **Modeled, and the shape is esp-hal's.** The trailing idle run is
    /// stored as a pulse — it is the run the receiver was timing when it gave
    /// up — and then the word is closed so that the **last word written is an
    /// end marker**, which is what `RmtReader::read` asserts about a finished
    /// reception (`rmt/reader.rs`, the `debug_assert!` on
    /// `ram[hw_offset - 1].is_end_marker()`). Where the idle pulse leaves a
    /// half open, that half is the zero; where it closes a word, one all-zero
    /// word follows it.
    fn rx_end(&mut self, rxi: usize, at: Cycles, cx: &mut BusCx<'_>) {
        let ch = RxEngine::channel(rxi);
        let idle = u64::from(self.rx[rxi].idle_thres()) + 1;
        let level = self.rx[rxi].run_level;
        // A pending edge the filter never got a verdict on is one the line
        // then held: it passed by definition, so it is accepted here.
        if let Some((tick, edge_level)) = self.rx[rxi].pending_edge.take()
            && let Ok(clock) = self.clock()
        {
            self.rx_accept_edge(rxi, tick, edge_level, &clock, cx);
            self.cancel_rx_idle(rxi, cx);
            self.rx_push_pulse(rxi, edge_level, idle, cx);
        } else {
            self.rx_push_pulse(rxi, level, idle, cx);
        }
        if self.rx[rxi].half.is_some() {
            // Close the open word: the second half is the zero marker.
            let (l0, d0) = self.rx[rxi].half.take().expect("checked");
            self.rx_write_word(rxi, d0 | (u32::from(l0) << 15), cx);
        } else {
            self.rx_write_word(rxi, 0, cx);
        }
        let words = self.rx[rxi].frame_words;
        self.rx[rxi].running = false;
        self.rx[rxi].armed = false;
        self.cancel_rx_idle(rxi, cx);
        self.raise(ch, INT_TX_END_SHIFT);
        note(cx, || {
            format!(
                "cyc={at} RMT ch{ch} rx end words={words} idle_thres={}",
                idle - 1
            )
        });
        self.update_lines(cx);
    }

    // ---- observation, the receiving half ------------------------------------

    /// Whether receiver `rxi` (0 is channel 2) is armed or receiving.
    pub fn rx_running(&self, rxi: usize) -> bool {
        self.rx[rxi].running
    }

    /// Words receiver `rxi` has written since the machine started.
    pub fn rx_words_written(&self, rxi: usize) -> u64 {
        self.rx[rxi].words_written
    }

    /// The next word receiver `rxi` will write — `ch_rx_status.mem_waddr_ex`.
    pub fn rx_waddr(&self, rxi: usize) -> u32 {
        self.rx[rxi].waddr
    }

    // ---- registers ------------------------------------------------------------

    fn read_word(&self, off: u32) -> u32 {
        if off >= RAM_OFFSET {
            let i = ((off - RAM_OFFSET) >> 2) as usize;
            return self.ram.get(i).copied().unwrap_or(0);
        }
        if off >= REGS_LEN {
            return 0;
        }
        for ch in 0..TX_CHANNELS {
            if off == CH_TX_STATUS[ch] {
                let e = &self.ch[ch];
                let mut v = e.raddr & TX_LIM_MASK;
                if e.running {
                    v |= 1 << STATUS_STATE_SHIFT;
                }
                if e.mem_empty {
                    v |= STATUS_MEM_EMPTY;
                }
                return v;
            }
        }
        for rxi in 0..RX_CHANNELS {
            if off == CH_RX_STATUS[rxi] {
                let e = &self.rx[rxi];
                // `mem_waddr_ex` (bits 0:8) is absolute, the way the TX
                // side's `mem_raddr_ex` is: esp-hal's `hw_offset` subtracts
                // the channel's own window start from it.
                let mut v = e.waddr & RX_LIM_MASK;
                // `apb_mem_raddr_ex` (bits 12:20) is the *APB* side's read
                // pointer, which the driver tracks in software and never
                // reads back (esp-hal's `RmtReader` keeps its own `offset`).
                // Modeled as the window start.
                v |= (Self::rx_window_start(rxi) & RX_LIM_MASK) << RX_STATUS_APB_RADDR_SHIFT;
                if e.running {
                    v |= 1 << RX_STATUS_STATE_SHIFT;
                }
                if e.mem_full {
                    v |= RX_STATUS_MEM_FULL;
                }
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
                let now = cx.now;
                let pc = cx.pc;
                note(cx, || {
                    format!(
                        "cyc={now} pc=0x{pc:08x} RMT write to +0x{off:03x}: between the register block and the RAM, dropped"
                    )
                });
            }
            return;
        }
        for ch in 0..TX_CHANNELS {
            if off == CH_TX_CONF0[ch] {
                let strobes = value & (CONF_TX_START | CONF_MEM_RD_RST | CONF_APB_MEM_RST);
                self.ch[ch].pending |= strobes;
                self.regs.poke(off, value & !CONF_PULSES);
                if value & CONF_CONF_UPDATE != 0 {
                    self.conf_update(ch, cx);
                } else if strobes & CONF_TX_START != 0 {
                    // §10.3: act on it at the slice boundary if no
                    // `conf_update` follows.
                    cx.sched
                        .schedule_at(cx.now, event_id(self.index, EV_LATE_START + ch as u16));
                }
                return;
            }
            if off == CH_TX_LIM[ch] {
                // Immediate: the ISR rewrites it mid-frame.
                self.regs.poke(off, value);
                // And this write is the ISR arriving: the entry delay ends
                // here. `refill` flips `tx_lim` first, before it plants the
                // guard or writes a single word, so this is the earliest
                // moment the handler is observably present.
                self.refill_answered(ch);
                return;
            }
        }
        for rxi in 0..RX_CHANNELS {
            if off == CH_RX_CONF0[rxi] {
                // No strobes here: `div_cnt`, `idle_thres`, `mem_size` and
                // the carrier bits all take effect at the next
                // `conf_update`, which is where esp-hal writes them from.
                self.regs.poke(off, value);
                return;
            }
            if off == CH_RX_CONF1[rxi] {
                let strobes = value & (RX_CONF1_MEM_WR_RST | RX_CONF1_APB_MEM_RST);
                self.rx[rxi].pending |= strobes;
                self.regs.poke(off, value & !RX_CONF1_PULSES);
                if value & RX_CONF1_CONF_UPDATE != 0 {
                    self.rx_conf_update(rxi, cx);
                }
                return;
            }
            if off == CH_RX_LIM[rxi] {
                // Immediate, like `ch_tx_lim`: nothing in the PAC gates it on
                // `conf_update` and esp-hal writes it before the update that
                // starts the reception.
                self.regs.poke(off, value);
                return;
            }
            if off == CH_RX_CARRIER_RM[rxi] {
                self.regs.poke(off, value);
                if self.rx[rxi].conf0 & RX_CONF0_CARRIER_EN != 0 && !self.warned_rx_carrier[rxi] {
                    self.warned_rx_carrier[rxi] = true;
                    let ch = RxEngine::channel(rxi);
                    let now = cx.now;
                    note(cx, || {
                        format!(
                            "cyc={now} RMT ch{ch} rx carrier demodulation is not modelled: the \
                             receiver samples the pad's level as the fabric resolves it"
                        )
                    });
                }
                return;
            }
        }
        match off {
            INT_ENA => {
                self.regs.poke(INT_ENA, value & INT_MASK);
                self.update_lines(cx);
            }
            INT_CLR => {
                self.sticky &= !value;
                self.update_lines(cx);
            }
            INT_RAW | INT_ST => {}
            o if o == CH_TX_STATUS[0] || o == CH_TX_STATUS[1] => {}
            REF_CNT_RST => {
                self.regs.poke(off, value);
                for bit in 0..4usize {
                    if value & (1 << bit) != 0 && !self.warned_ref_cnt[bit] {
                        self.warned_ref_cnt[bit] = true;
                        let now = cx.now;
                        note(cx, || {
                            format!(
                                "cyc={now} RMT ch{bit} ref_cnt_rst: divider phase reset accepted, unobservable at div_cnt 1 (modeled)"
                            )
                        });
                    }
                }
            }
            SYS_CONF => {
                self.regs.poke(off, value);
                if value & SYS_CONF_APB_FIFO_MASK == 0 && !self.warned_fifo {
                    self.warned_fifo = true;
                    let now = cx.now;
                    note(cx, || {
                        format!(
                            "cyc={now} RMT sys_conf.apb_fifo_mask = 0: the APB FIFO is not modelled (direct RAM access only)"
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
            other => self.regs.poke(other, value),
        }
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
        match local {
            l if l < EV_WORD + TX_CHANNELS as u16 => {
                self.word_ended(usize::from(l - EV_WORD), cx);
            }
            l if l < EV_LATE_START + TX_CHANNELS as u16 => {
                let ch = usize::from(l - EV_LATE_START);
                if self.ch[ch].pending & CONF_TX_START != 0 {
                    if !self.warned_late_start[ch] {
                        self.warned_late_start[ch] = true;
                        let now = cx.now;
                        note(cx, || {
                            format!(
                                "cyc={now} RMT ch{ch} tx_start with no conf_update in the slice: acted on at the slice boundary (modeled, discovery §10.3)"
                            )
                        });
                    }
                    self.conf_update(ch, cx);
                }
            }
            l if (EV_RX_IDLE..EV_RX_IDLE + RX_CHANNELS as u16).contains(&l) => {
                let rxi = usize::from(l - EV_RX_IDLE);
                if self.rx[rxi].running && !self.rx[rxi].armed {
                    let due = self.rx[rxi].idle_due;
                    self.rx_end(rxi, due, cx);
                }
            }
            EV_CLOCK_POLL => {
                self.poll_armed = false;
                let stalled: Vec<usize> = (0..TX_CHANNELS)
                    .filter(|&ch| self.ch[ch].running && self.ch[ch].stalled)
                    .collect();
                if stalled.is_empty() {
                    return;
                }
                if self.clock().is_ok() {
                    for ch in stalled {
                        let now = cx.now;
                        self.fetch(ch, now, cx);
                    }
                } else {
                    self.poll_armed = true;
                    cx.sched.schedule_at(
                        cx.now.saturating_add(CLOCK_POLL_CYCLES),
                        event_id(self.index, EV_CLOCK_POLL),
                    );
                }
            }
            _ => {}
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::RMT.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        Some(self.grades.grade(off))
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(REGS_LEN as usize + RAM_WORDS * 4 + 256);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.sticky.to_le_bytes());
        let flags = u32::from(self.poll_armed)
            | (u32::from(self.warned_rx_carrier[0]) << 1)
            | (u32::from(self.warned_fifo) << 2)
            | (u32::from(self.warned_gap) << 3)
            | (u32::from(self.warned_ref_cnt[0]) << 4)
            | (u32::from(self.warned_ref_cnt[1]) << 5)
            | (u32::from(self.warned_ref_cnt[2]) << 6)
            | (u32::from(self.warned_ref_cnt[3]) << 7)
            | (u32::from(self.warned_late_start[0]) << 8)
            | (u32::from(self.warned_late_start[1]) << 9)
            | (u32::from(self.keep_logs) << 10)
            | (u32::from(self.warned_rx_carrier[1]) << 11);
        out.extend_from_slice(&flags.to_le_bytes());
        for w in self.ram.iter() {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for e in &self.ch {
            out.extend_from_slice(&e.latched.to_le_bytes());
            out.extend_from_slice(&e.pending.to_le_bytes());
            let eflags = u32::from(e.running)
                | (u32::from(e.stalled) << 1)
                | (u32::from(e.mem_empty) << 2)
                | (u32::from(e.pulse_cap_noted) << 3)
                | (u32::from(e.word_cap_noted) << 4)
                | (u32::from(e.stall_noted) << 5)
                | ((e.ending as u32) << 8);
            out.extend_from_slice(&eflags.to_le_bytes());
            out.extend_from_slice(&e.raddr.to_le_bytes());
            out.extend_from_slice(&e.ticks_since_start.to_le_bytes());
            out.extend_from_slice(&e.anchor_cycle.to_le_bytes());
            out.extend_from_slice(&e.anchor_ticks.to_le_bytes());
            let (src, num, a, b) = e
                .anchor_clock
                .map(|c| (c.src_hz, c.div_num, c.div_a, c.div_b))
                .unwrap_or((0, 0, 0, 0));
            out.extend_from_slice(&src.to_le_bytes());
            out.extend_from_slice(&num.to_le_bytes());
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
            out.extend_from_slice(&e.word_due.to_le_bytes());
            out.extend_from_slice(&e.frame_words.to_le_bytes());
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
            // the block observes: a restored run has to produce the same
            // `RMT REFILL` notes as the run it was taken from, or the
            // snapshot-identity gates would be comparing two different
            // observers.
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
        // The receivers ride the snapshot for the same reason the refill
        // measurement does: a restored run has to write the same words at the
        // same cycles as the run it was taken from.
        for e in &self.rx {
            out.extend_from_slice(&e.conf0.to_le_bytes());
            out.extend_from_slice(&e.conf1.to_le_bytes());
            out.extend_from_slice(&e.pending.to_le_bytes());
            let eflags = u32::from(e.running)
                | (u32::from(e.armed) << 1)
                | (u32::from(e.run_level) << 2)
                | (u32::from(e.mem_full) << 3)
                | (u32::from(e.warned_unrouted) << 4)
                | (u32::from(e.half.is_some()) << 5)
                | (u32::from(e.half.is_some_and(|(l, _)| l)) << 6)
                | (u32::from(e.pending_edge.is_some()) << 7)
                | (u32::from(e.pending_edge.is_some_and(|(_, l)| l)) << 8);
            out.extend_from_slice(&eflags.to_le_bytes());
            out.extend_from_slice(&e.waddr.to_le_bytes());
            out.extend_from_slice(&e.anchor_cycle.to_le_bytes());
            let (src, num, a, b) = e
                .anchor_clock
                .map(|c| (c.src_hz, c.div_num, c.div_a, c.div_b))
                .unwrap_or((0, 0, 0, 0));
            out.extend_from_slice(&src.to_le_bytes());
            out.extend_from_slice(&num.to_le_bytes());
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
            out.extend_from_slice(&e.run_start.to_le_bytes());
            out.extend_from_slice(&e.pending_edge.map_or(0, |(t, _)| t).to_le_bytes());
            out.extend_from_slice(&e.half.map_or(0, |(_, d)| d).to_le_bytes());
            out.extend_from_slice(&e.since_thr.to_le_bytes());
            out.extend_from_slice(&e.frame_words.to_le_bytes());
            out.extend_from_slice(&e.words_written.to_le_bytes());
            out.extend_from_slice(&e.idle_due.to_le_bytes());
        }
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(index), Some(sticky), Some(flags)) = (r.u64(), r.u32(), r.u32()) else {
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
            let (Some(latched), Some(pending), Some(eflags), Some(raddr)) =
                (r.u32(), r.u32(), r.u32(), r.u32())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let (Some(ticks), Some(anchor_cycle), Some(anchor_ticks)) = (r.u64(), r.u64(), r.u64())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let (Some(src), Some(num), Some(a), Some(b)) = (r.u64(), r.u32(), r.u32(), r.u32())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let (Some(word_due), Some(frame_words), Some(frames_ended), Some(npulses)) =
                (r.u64(), r.u32(), r.u64(), r.u64())
            else {
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
            let mut stats = RefillStats::default();
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
            stats.refills = refills;
            stats.entry_max = entry_max;
            stats.fill_max = fill_max;
            stats.unanswered = unanswered;
            stats.half_words = half_words;
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
            e.latched = latched;
            e.pending = pending;
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
            e.ticks_since_start = ticks;
            e.anchor_cycle = anchor_cycle;
            e.anchor_ticks = anchor_ticks;
            e.anchor_clock = (src != 0).then_some(Clock {
                src_hz: src,
                div_num: num,
                div_a: a,
                div_b: b,
            });
            e.word_due = word_due;
            e.frame_words = frame_words;
            e.frames_ended = frames_ended as usize;
            e.pulses = pulses;
            e.words = words;
            engines.push(e);
        }
        self.index = index as usize;
        self.sticky = sticky;
        self.poll_armed = flags & 1 != 0;
        self.warned_rx_carrier = [flags & 2 != 0, flags & (1 << 11) != 0];
        self.warned_fifo = flags & 4 != 0;
        self.warned_gap = flags & 8 != 0;
        for bit in 0..4 {
            self.warned_ref_cnt[bit] = flags & (16 << bit) != 0;
        }
        self.warned_late_start = [flags & (1 << 8) != 0, flags & (1 << 9) != 0];
        self.keep_logs = flags & (1 << 10) != 0;
        *self.ram = ram;
        let mut it = engines.into_iter();
        self.ch = [
            it.next().expect("two engines"),
            it.next().expect("two engines"),
        ];
        let mut receivers = Vec::with_capacity(RX_CHANNELS);
        for rxi in 0..RX_CHANNELS {
            let mut e = RxEngine::new(rxi);
            let (Some(conf0), Some(conf1), Some(pending), Some(eflags), Some(waddr)) =
                (r.u32(), r.u32(), r.u32(), r.u32(), r.u32())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let (Some(anchor_cycle), Some(src), Some(num), Some(a), Some(b)) =
                (r.u64(), r.u64(), r.u32(), r.u32(), r.u32())
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            let (
                Some(run_start),
                Some(pending_tick),
                Some(half_dur),
                Some(since_thr),
                Some(frame_words),
                Some(words_written),
                Some(idle_due),
            ) = (
                r.u64(),
                r.u64(),
                r.u32(),
                r.u32(),
                r.u64(),
                r.u64(),
                r.u64(),
            )
            else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            e.conf0 = conf0;
            e.conf1 = conf1;
            e.pending = pending;
            e.running = eflags & 1 != 0;
            e.armed = eflags & 2 != 0;
            e.run_level = eflags & 4 != 0;
            e.mem_full = eflags & 8 != 0;
            e.warned_unrouted = eflags & 16 != 0;
            e.half = (eflags & 32 != 0).then_some((eflags & 64 != 0, half_dur));
            e.pending_edge = (eflags & 128 != 0).then_some((pending_tick, eflags & 256 != 0));
            e.waddr = waddr;
            e.anchor_cycle = anchor_cycle;
            e.anchor_clock = (src != 0).then_some(Clock {
                src_hz: src,
                div_num: num,
                div_a: a,
                div_b: b,
            });
            e.run_start = run_start;
            e.since_thr = since_thr;
            e.frame_words = frame_words;
            e.words_written = words_written;
            e.idle_due = idle_due;
            receivers.push(e);
        }
        let mut it = receivers.into_iter();
        self.rx = [
            it.next().expect("two receivers"),
            it.next().expect("two receivers"),
        ];
        self.regs.load_state(r.0);
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }

    /// The machine-side seam, for one thing only: turning the pulse and word
    /// logs on at build time (`Esp32C6Builder::rmt_logs`). Nothing on the
    /// guest side can reach it.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// `lp_ws281x::pulse_code`, locally: two level/duration pairs in one word.
    const fn word(l1: bool, d1: u16, l2: bool, d2: u16) -> u32 {
        ((l1 as u32) << 15)
            | (d1 as u32 & 0x7fff)
            | ((((l2 as u32) << 15) | (d2 as u32 & 0x7fff)) << 16)
    }

    /// The WS2812 codes at 80 MHz (`lp_ws281x::timing`): 100 ticks each.
    const ZERO: u32 = word(true, 32, false, 68);
    const ONE: u32 = word(true, 64, false, 36);
    /// 100 ticks = 200 cycles at div_cnt 1 from the 80 MHz PLL.
    const WORD_CYCLES: u64 = 200;

    const INT_ENA_CH0: u32 = 0x111;
    const THR0: u32 = 1 << INT_TX_THR_SHIFT;
    const END0: u32 = 1 << INT_TX_END_SHIFT;
    const ERR0: u32 = 1 << INT_TX_ERR_SHIFT;

    /// A sandbox with the RMT on esp-hal's 80 MHz clock (`Rmt::new`).
    fn rig() -> (Sandbox, Rmt, RmtClockLine) {
        let clock = RmtClockLine::default();
        clock.sclk.set(0x0050_0000);
        let mut r = Rmt::new(clock.clone());
        r.attached(7);
        // The word and pulse logs are the oracle these tests read; a machine
        // leaves them off (see `Rmt::keep_logs`).
        r.set_keep_logs(true);
        (Sandbox::new(), r, clock)
    }

    fn ram_off(word_index: u32) -> u32 {
        RAM_OFFSET + 4 * word_index
    }

    /// esp-hal's `configure_tx` on channel 0: `div_cnt 1`, idle low,
    /// `mem_size` blocks (no `conf_update`), then `enable_tx_interrupts`.
    fn configure(sb: &mut Sandbox, r: &mut Rmt, mem_size: u32) {
        let conf =
            (1 << CONF_DIV_CNT_SHIFT) | (1 << 6) | (mem_size << CONF_MEM_SIZE_SHIFT) | (1 << 20);
        sb.write(r, CH_TX_CONF0[0], conf);
        sb.write(r, INT_ENA, INT_ENA_CH0);
    }

    /// `c6_rmt::start_tx(0)`, register for register.
    fn start_tx(sb: &mut Sandbox, r: &mut Rmt) {
        sb.write(r, INT_CLR, 0x1111);
        sb.write(r, REF_CNT_RST, 1);
        sb.write(r, REF_CNT_RST, 0);
        let c = sb.read(r, CH_TX_CONF0[0]);
        sb.write(
            r,
            CH_TX_CONF0[0],
            (c & !CONF_TX_STOP & !(1 << 3)) | CONF_MEM_TX_WRAP_EN,
        );
        let c = sb.read(r, CH_TX_CONF0[0]);
        sb.write(r, CH_TX_CONF0[0], c | CONF_CONF_UPDATE);
        let c = sb.read(r, CH_TX_CONF0[0]);
        sb.write(
            r,
            CH_TX_CONF0[0],
            c | CONF_MEM_RD_RST | CONF_APB_MEM_RST | CONF_TX_START,
        );
        let c = sb.read(r, CH_TX_CONF0[0]);
        sb.write(r, CH_TX_CONF0[0], c | CONF_CONF_UPDATE);
    }

    fn fill(sb: &mut Sandbox, r: &mut Rmt, from: u32, to: u32, w: u32) {
        for i in from..to {
            sb.write(r, ram_off(i), w);
        }
    }

    fn read_pos(sb: &mut Sandbox, r: &mut Rmt) -> u32 {
        sb.read(r, CH_TX_STATUS[0]) & 0x1ff
    }

    #[test]
    fn the_reset_state_is_the_pac_and_the_strobes_read_zero() {
        let (mut sb, mut r, _) = rig();
        assert_eq!(sb.read(&mut r, CH_TX_CONF0[0]), 0x0071_0200);
        assert_eq!(sb.read(&mut r, CH_TX_CONF0[1]), 0x0071_0200);
        assert_eq!(sb.read(&mut r, CH_TX_LIM[0]), 0x80);
        assert_eq!(sb.read(&mut r, SYS_CONF), 0x0500_0010);
        assert_eq!(
            sb.read(&mut r, CH_TX_STATUS[1]) & 0x1ff,
            48,
            "ch1's window starts at block 1"
        );
        assert_eq!(r.reg_name(0x028), Some("ch0_tx_status"));
        assert_eq!(r.reg_name(0x400), None, "the RAM has no register name");
        // (e) int_st = raw & ena; int_clr w1c; the strobes read 0; tx_stop reads back.
        r.sticky = THR0 | END0;
        assert_eq!(sb.read(&mut r, INT_RAW), THR0 | END0);
        assert_eq!(sb.read(&mut r, INT_ST), 0, "nothing enabled");
        sb.write(&mut r, INT_ENA, THR0);
        assert_eq!(sb.read(&mut r, INT_ST), THR0);
        assert!(sb.irq.level(source::RMT));
        sb.write(&mut r, INT_CLR, THR0);
        assert_eq!(sb.read(&mut r, INT_RAW), END0, "w1c cleared only thr");
        assert!(!sb.irq.level(source::RMT));
        assert_eq!(sb.read(&mut r, INT_CLR), 0);
        sb.write(
            &mut r,
            CH_TX_CONF0[0],
            CONF_TX_STOP | CONF_CONF_UPDATE | CONF_TX_START | CONF_MEM_RD_RST,
        );
        let c = sb.read(&mut r, CH_TX_CONF0[0]);
        assert_eq!(
            c & CONF_PULSES,
            0,
            "tx_start, mem_rd_rst, conf_update read 0"
        );
        assert_eq!(c & CONF_TX_STOP, CONF_TX_STOP, "tx_stop is sticky");
        assert!(!r.is_running(0), "tx_start under tx_stop does nothing");
        // (h) a byte write into the RAM merges its lane.
        sb.write(&mut r, ram_off(5), 0x1122_3344);
        r.write(ram_off(5) + 1, Width::Byte, 0xaa, &mut sb.cx());
        assert_eq!(sb.read(&mut r, ram_off(5)), 0x1122_aa44);
        assert_eq!(r.read(ram_off(5) + 2, Width::Half, &mut sb.cx()), 0x1122);
        assert_eq!(r.ram()[5], 0x1122_aa44);
        // (g) the RX registers read the PAC's reset values, and the strobes
        // in `ch_rx_conf1` read back 0 the way the TX side's do.
        assert_eq!(sb.read(&mut r, CH_RX_CONF0[0]), CH_RX_CONF0_RESET);
        assert_eq!(sb.read(&mut r, CH_RX_CONF1[1]), CH_RX_CONF1_RESET);
        assert_eq!(sb.read(&mut r, CH_RX_LIM[0]), 0x80);
        assert_eq!(
            sb.read(&mut r, CH_RX_STATUS[0]) & RX_LIM_MASK,
            96,
            "channel 2's window starts at block 2"
        );
        sb.write(
            &mut r,
            CH_RX_CONF1[0],
            CH_RX_CONF1_RESET | RX_CONF1_MEM_WR_RST | RX_CONF1_APB_MEM_RST,
        );
        assert_eq!(
            sb.read(&mut r, CH_RX_CONF1[0]) & RX_CONF1_PULSES,
            0,
            "mem_wr_rst, apb_mem_rst and conf_update read 0"
        );
        assert_eq!(r.reg_name(0x030), Some("ch0_rx_status"));
    }

    /// (a) The driver's exact sequence on a 48-word window, forever.
    #[test]
    fn the_threshold_is_a_position_and_the_wrap_is_tx_lim_equal_to_the_window() {
        let (mut sb, mut r, _) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 24);
        sb.now = 1_000;
        start_tx(&mut sb, &mut r);
        assert!(r.is_running(0));
        assert_eq!(
            sb.read(&mut r, CH_TX_STATUS[0]) >> 9 & 7,
            1,
            "state = sending"
        );
        assert_eq!(
            read_pos(&mut sb, &mut r),
            1,
            "word 0 fetched at start; next to fetch is 1"
        );
        assert_eq!(sb.sched.next_deadline(), Some(1_000 + WORD_CYCLES));

        let start = 1_000u64;
        let mut expected_words = 0u64;
        for round in 0..4 {
            // thr at pos 24: fires when word 23 is fetched.
            let at = start + (expected_words + 23) * WORD_CYCLES;
            sb.run_to(&mut r, at - 1);
            assert!(
                !sb.irq.level(source::RMT),
                "round {round}: not before word 23"
            );
            sb.run_to(&mut r, at);
            assert!(sb.irq.level(source::RMT), "round {round}: thr at 24");
            assert_eq!(sb.read(&mut r, INT_ST), THR0);
            assert_eq!(read_pos(&mut sb, &mut r), 24, "the driver's in_second_half");
            // The ISR: flip tx_lim to the wrap, ack.
            sb.write(&mut r, CH_TX_LIM[0], 48);
            sb.write(&mut r, INT_CLR, THR0);
            assert!(!sb.irq.level(source::RMT));
            // thr at the wrap: fires when word 47 is fetched, pointer → 0.
            let at = start + (expected_words + 47) * WORD_CYCLES;
            sb.run_to(&mut r, at - 1);
            assert!(!sb.irq.level(source::RMT));
            sb.run_to(&mut r, at);
            assert!(sb.irq.level(source::RMT), "round {round}: thr at the wrap");
            assert_eq!(read_pos(&mut sb, &mut r), 0, "wrapped to word 0");
            sb.write(&mut r, CH_TX_LIM[0], 24);
            sb.write(&mut r, INT_CLR, THR0);
            expected_words += 48;
        }
        assert_eq!(r.frames_ended(0), 0, "never ended");
        assert_eq!(
            r.words(0).len(),
            4 * 48,
            "four windows, ending on the fourth wrap"
        );
        // Pulses are exactly two per word, on the 200-cycle grid.
        let p = r.pulses(0);
        assert_eq!(
            p[0],
            Pulse {
                at: start,
                level: true,
                ticks: 32
            }
        );
        assert_eq!(
            p[1],
            Pulse {
                at: start + 64,
                level: false,
                ticks: 68
            }
        );
        assert_eq!(p[2].at, start + 200);
        assert_eq!(
            p[95 * 2].at,
            start + 95 * WORD_CYCLES,
            "word 95 is at 95 periods"
        );
    }

    /// The refill measurement, in the units the driver measures the same
    /// race in: words the transmitter consumed between the threshold and the
    /// ISR's `ch_tx_lim` write, and between that write and the last word of
    /// the refill.
    ///
    /// Reported, never gated (D13/PD9) — so what this pins is that the two
    /// quantities are what they claim to be, not that they are any
    /// particular size.
    #[test]
    fn a_refill_is_measured_in_words_from_the_threshold_to_the_last_word_written() {
        let (mut sb, mut r, _) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 24);
        let start = 1_000u64;
        sb.now = start;
        start_tx(&mut sb, &mut r);
        assert_eq!(r.refill_stats(0).refills, 0, "nothing measured yet");

        // The threshold: word 23 fetched, 24 words consumed.
        sb.run_to(&mut r, start + 23 * WORD_CYCLES);
        assert!(sb.irq.level(source::RMT));

        // The ISR arrives two words late and flips `tx_lim` first, exactly as
        // `lp_ws281x::refill` does.
        sb.run_to(&mut r, start + 25 * WORD_CYCLES);
        sb.write(&mut r, CH_TX_LIM[0], 48);
        sb.write(&mut r, INT_CLR, THR0);

        // …and its last word lands three words later.
        sb.run_to(&mut r, start + 28 * WORD_CYCLES);
        sb.write(&mut r, ram_off(3), ZERO);
        // A write outside this channel's window is not part of its refill.
        sb.run_to(&mut r, start + 40 * WORD_CYCLES);
        sb.write(&mut r, ram_off(100), ZERO);

        // Still open: the measurement closes at the next threshold.
        assert_eq!(r.refill_stats(0).refills, 0);
        sb.run_to(&mut r, start + 47 * WORD_CYCLES);
        let s = r.refill_stats(0);
        assert_eq!(s.refills, 1);
        assert_eq!(s.entry_max, 2, "two words between the threshold and tx_lim");
        assert_eq!(s.fill_max, 3, "three more before the last word landed");
        assert_eq!(s.half_words, 24, "a 48-word window's half");
        assert_eq!(s.unanswered, 0);
        // Buckets are eighths of the half, so three words wide here:
        // `lag_bucket(2, 24) = 0` and `lag_bucket(3, 24) = 1`.
        assert_eq!(s.entry_hist[0], 1);
        assert_eq!(s.fill_hist[1], 1);

        // A threshold the guest never answers is counted, not invented.
        sb.write(&mut r, INT_CLR, THR0);
        fill(&mut sb, &mut r, 0, 48, 0);
        sb.run_to(&mut r, start + 60 * WORD_CYCLES);
        assert_eq!(r.frames_ended(0), 1, "the STOP ended the frame");
        let s = r.refill_stats(0);
        assert_eq!(s.refills, 1, "no second refill was measured");
        assert_eq!(
            s.unanswered, 1,
            "the last threshold of a frame is `finish`'s, not `refill`'s"
        );
    }

    #[test]
    fn the_lag_buckets_are_eighths_of_a_half_with_the_last_one_open() {
        assert_eq!(lag_bucket(0, 96), 0);
        assert_eq!(lag_bucket(11, 96), 0);
        assert_eq!(lag_bucket(12, 96), 1);
        assert_eq!(lag_bucket(95, 96), 7);
        assert_eq!(lag_bucket(96, 96), 8, "≥ half");
        assert_eq!(lag_bucket(4_000, 96), 8, "and it saturates");
        assert_eq!(lag_bucket(3, 0), 8, "an unconfigured window has no eighths");
    }

    /// (b) A STOP at word 0 after the wrap ends the frame with `tx_end`
    /// after 48 + 0 words, and the next start works.
    #[test]
    fn a_stop_guard_at_word_zero_ends_the_frame_after_the_wrap_and_a_restart_works() {
        let (mut sb, mut r, _) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ONE);
        sb.write(&mut r, CH_TX_LIM[0], 24);
        start_tx(&mut sb, &mut r);
        // First thr: the driver plants the guard at half (24) — already
        // fetched? No: pos 24 is next-to-fetch, so the driver skips it
        // (guard_skips) and refills the first half. Then at the wrap thr it
        // plants the guard at 0 and refills the second half. Here: no refill,
        // just the guard at 0.
        sb.run_to(&mut r, 47 * WORD_CYCLES);
        assert_eq!(read_pos(&mut sb, &mut r), 0);
        sb.write(&mut r, ram_off(0), 0);
        sb.write(&mut r, INT_CLR, THR0);
        sb.run_to(&mut r, 48 * WORD_CYCLES - 1);
        assert_eq!(r.frames_ended(0), 0);
        sb.run_to(&mut r, 48 * WORD_CYCLES);
        assert_eq!(r.frames_ended(0), 1);
        assert!(!r.is_running(0));
        assert_eq!(sb.read(&mut r, INT_ST), END0);
        assert_eq!(r.words(0).len(), 49, "48 data words + the STOP");
        assert_eq!(r.words(0)[48], (48 * WORD_CYCLES, 0));
        assert_eq!(r.pulses(0).len(), 96);
        sb.write(&mut r, INT_CLR, END0);
        assert_eq!(sb.sched.live(), 0, "nothing pending after the end");

        // The next frame: refill word 0, start again — a fresh anchor.
        sb.write(&mut r, ram_off(0), ZERO);
        sb.now = 100_000;
        start_tx(&mut sb, &mut r);
        assert!(r.is_running(0));
        assert_eq!(
            read_pos(&mut sb, &mut r),
            1,
            "mem_rd_rst rewound to 0, fetched it"
        );
        sb.run_to(&mut r, 100_000 + 10 * WORD_CYCLES);
        assert_eq!(r.words(0).len(), 49 + 11);
        assert_eq!(r.pulses(0)[96].at, 100_000);
    }

    /// (c) A late refill: RAM never refilled, guard at slot 0 → the frame
    /// truncates where the driver's `guard_trips` would count it.
    #[test]
    fn a_refill_that_never_comes_lets_the_guard_truncate_the_frame() {
        let (mut sb, mut r, _) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 24);
        start_tx(&mut sb, &mut r);
        // The wrap thr; the ISR plants the guard at 0 and then (say) is
        // preempted before the refill.
        sb.run_to(&mut r, 47 * WORD_CYCLES);
        assert_eq!(sb.read(&mut r, INT_ST), THR0);
        sb.write(&mut r, CH_TX_LIM[0], 24);
        sb.write(&mut r, ram_off(0), 0);
        sb.write(&mut r, INT_CLR, THR0);
        sb.run_to(&mut r, 200 * WORD_CYCLES);
        assert_eq!(r.frames_ended(0), 1, "ended on the guard");
        assert_eq!(r.words(0).len(), 49);
        assert_eq!(read_pos(&mut sb, &mut r), 0, "stopped on the guard slot");
    }

    /// (d) Tick arithmetic, and a word straddling a slice boundary.
    #[test]
    fn ticks_become_cycles_exactly_and_a_slice_boundary_never_shifts_a_word() {
        // div_cnt 1 at 80 MHz: 200 cycles per 100-tick word.
        let (mut sb, mut r, _) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 0);
        sb.now = 3;
        start_tx(&mut sb, &mut r);
        // Run in one go to well past the slice cap: every word is exactly
        // one period after the previous one, anchored on due cycles.
        sb.run_to(&mut r, 3 + 8_192);
        let words = r.words(0);
        assert!(words.len() > 40, "{}", words.len());
        for (i, (at, _)) in words.iter().enumerate() {
            assert_eq!(*at, 3 + i as u64 * WORD_CYCLES, "word {i}");
        }
        let p = r.pulses(0);
        assert_eq!(p[0].at, 3);
        assert_eq!(p[1].at, 3 + 64, "32 ticks = 64 cycles");
        assert_eq!(p[2].at, 3 + 200);

        // div_cnt 2 → 400 per word.
        let (mut sb, mut r, _) = rig();
        sb.write(
            &mut r,
            CH_TX_CONF0[0],
            (2 << CONF_DIV_CNT_SHIFT) | (1 << CONF_MEM_SIZE_SHIFT),
        );
        sb.write(&mut r, INT_ENA, INT_ENA_CH0);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 0);
        start_tx(&mut sb, &mut r);
        sb.run_to(&mut r, 10 * 400);
        assert_eq!(r.words(0)[10].0, 4_000);
        assert_eq!(r.pulses(0)[1].at, 128, "32 ticks at div_cnt 2");

        // div_num 1 (the PCR reset: 40 MHz) → 400 per word at div_cnt 1.
        let (mut sb, mut r, clock) = rig();
        clock.sclk.set(super::super::pcr::RMT_SCLK_CONF_RESET);
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 0);
        start_tx(&mut sb, &mut r);
        sb.run_to(&mut r, 10 * 400);
        assert_eq!(r.words(0)[10].0, 4_000);
        // The fractional divider: div_num 0, a = 1, b = 2 → 80 / 1.5 MHz →
        // 3 cycles per tick, exactly, and floor over absolute ticks.
        let c = Clock::decode(1, 0x0050_0000 | (1 << SCLK_DIV_A_SHIFT) | 2).unwrap();
        assert_eq!(c.hz(), 53_333_333);
        assert_eq!(c.cycles_for(100, 1), 300);
        assert_eq!(c.cycles_for(1, 1), 3);
        let c = Clock::decode(1, 0x0050_0000).unwrap();
        assert_eq!(c.hz(), 80_000_000);
        assert_eq!(c.cycles_for(12_000 * 2, 1), 48_000, "the latch word");
        let c = Clock::decode(1, 0x0070_0000).unwrap();
        assert_eq!(c.hz(), 40_000_000);
        assert_eq!(
            Clock::decode(0, 0x0050_0000),
            Err("PCR rmt_conf.clk_en = 0")
        );
        assert!(Clock::decode(1, 0x0060_0000).is_err(), "FOSC refused");
        assert!(Clock::decode(1, 0x0040_0000).is_err(), "sel 0");
        assert!(Clock::decode(1, 0x0010_0000).is_err(), "sclk_en 0");
    }

    /// (f) `save_state`/`load_state` round trip mid-word.
    #[test]
    fn the_state_round_trips_mid_word_with_the_ram_and_the_pending_event() {
        let (mut sb, mut r, clock) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ONE);
        sb.write(&mut r, CH_TX_LIM[0], 24);
        sb.now = 500;
        start_tx(&mut sb, &mut r);
        sb.run_to(&mut r, 500 + 5 * WORD_CYCLES + 37);
        let blob = r.save_state();
        let mut other = Rmt::new(clock.clone());
        other.load_state(&blob);
        assert_eq!(other.index, 7);
        assert!(other.is_running(0));
        assert_eq!(other.ch[0].raddr, 6);
        assert_eq!(other.ch[0].ticks_since_start, 600);
        assert_eq!(other.ch[0].word_due, 500 + 6 * WORD_CYCLES);
        assert_eq!(other.ch[0].anchor_cycle, 500);
        assert_eq!(other.ch[0].latched, r.ch[0].latched);
        assert_eq!(other.ram()[47], ONE);
        assert_eq!(other.words(0), r.words(0));
        assert_eq!(other.pulses(0), r.pulses(0));
        assert_eq!(other.regs.stored(CH_TX_LIM[0]), 24);
        // The restored machine re-schedules from the saved due cycle (the
        // scheduler's own queue is restored by the machine); run it on.
        let mut sb2 = Sandbox::new();
        sb2.sched
            .schedule_at(other.ch[0].word_due, event_id(7, EV_WORD));
        sb2.run_to(&mut other, 500 + 23 * WORD_CYCLES);
        assert!(sb2.irq.level(source::RMT) || sb2.read(&mut other, INT_RAW) & THR0 != 0);
        assert_eq!(other.words(0).len(), 24);
        assert_eq!(
            other.words(0)[23].0,
            500 + 23 * WORD_CYCLES,
            "still on the anchor"
        );
    }

    /// (i) No clock: the engine stalls with a note and resumes when PCR
    /// gives it one.
    #[test]
    fn no_function_clock_stalls_the_engine_and_the_clock_returning_resumes_it() {
        let (mut sb, mut r, clock) = rig();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 0);
        start_tx(&mut sb, &mut r);
        sb.run_to(&mut r, 3 * WORD_CYCLES);
        assert_eq!(r.words(0).len(), 4);
        // PCR gates the clock off; the fourth word's end has been scheduled
        // already, the fifth fetch finds no clock.
        clock.conf.set(0);
        sb.run_to(&mut r, 4 * WORD_CYCLES);
        assert_eq!(r.words(0).len(), 4, "word 4 is not fetched …");
        assert_eq!(r.pulses(0).len(), 8, "… and not emitted");
        assert!(r.ch[0].stalled);
        assert!(
            buf.lines()
                .iter()
                .any(|l| l.contains("RMT ch0 stalled: PCR rmt_conf.clk_en = 0"))
        );
        sb.run_to(&mut r, 4 * WORD_CYCLES + 3 * CLOCK_POLL_CYCLES);
        assert_eq!(r.pulses(0).len(), 8, "still stalled through three polls");
        // The clock returns; the next poll resumes word 4 at the poll cycle.
        clock.conf.set(1);
        let resume = 4 * WORD_CYCLES + 4 * CLOCK_POLL_CYCLES;
        sb.run_to(&mut r, resume);
        assert!(!r.ch[0].stalled);
        assert_eq!(
            r.words(0)[4],
            (resume, ZERO),
            "word 4 fetched at the resume"
        );
        assert_eq!(r.pulses(0)[8].at, resume, "and starts there");
        assert!(buf.lines().iter().any(|l| l.contains("RMT ch0 resumed")));
        sb.run_to(&mut r, resume + 2 * WORD_CYCLES);
        assert_eq!(
            r.words(0)[6].0,
            resume + 2 * WORD_CYCLES,
            "re-anchored, exact from there"
        );
        // sclk_sel 0 stalls too.
        clock.sclk.set(0x0040_0000);
        sb.run_to(&mut r, resume + 3 * WORD_CYCLES);
        assert!(r.ch[0].stalled);
        assert!(buf.lines().iter().any(|l| l.contains("sclk_sel = 0")));
    }

    #[test]
    fn a_tx_start_without_conf_update_is_acted_on_at_the_slice_boundary() {
        let (mut sb, mut r, _) = rig();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        let c = sb.read(&mut r, CH_TX_CONF0[0]);
        sb.write(
            &mut r,
            CH_TX_CONF0[0],
            c | CONF_MEM_TX_WRAP_EN | CONF_CONF_UPDATE,
        );
        sb.now = 40;
        let c = sb.read(&mut r, CH_TX_CONF0[0]);
        sb.write(&mut r, CH_TX_CONF0[0], c | CONF_TX_START | CONF_MEM_RD_RST);
        assert!(!r.is_running(0), "not yet: no conf_update");
        sb.run_to(&mut r, 40);
        assert!(r.is_running(0), "the slice boundary acted on it");
        assert!(
            buf.lines()
                .iter()
                .any(|l| l.contains("tx_start with no conf_update"))
        );
        assert_eq!(r.words(0)[0].0, 40);
    }

    #[test]
    fn wrap_off_runs_off_the_window_with_tx_err_and_mem_empty_and_tx_stop_ends_without_tx_end() {
        let (mut sb, mut r, _) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 0);
        // Start with wrap off.
        let c = sb.read(&mut r, CH_TX_CONF0[0]);
        sb.write(
            &mut r,
            CH_TX_CONF0[0],
            c | CONF_MEM_RD_RST | CONF_TX_START | CONF_CONF_UPDATE,
        );
        sb.run_to(&mut r, 47 * WORD_CYCLES);
        assert_eq!(sb.read(&mut r, INT_ST), ERR0, "tx_err at the last fetch");
        assert!(sb.read(&mut r, CH_TX_STATUS[0]) & STATUS_MEM_EMPTY != 0);
        assert!(r.is_running(0), "word 47 is still going out");
        sb.run_to(&mut r, 48 * WORD_CYCLES);
        assert!(!r.is_running(0));
        assert_eq!(r.frames_ended(0), 0, "no tx_end on an overrun");
        assert_eq!(r.pulses(0).len(), 96);

        // tx_stop mid-frame: idle, no tx_end.
        let (mut sb, mut r, _) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, CH_TX_LIM[0], 0);
        start_tx(&mut sb, &mut r);
        sb.run_to(&mut r, 10 * WORD_CYCLES);
        let c = sb.read(&mut r, CH_TX_CONF0[0]);
        sb.write(&mut r, CH_TX_CONF0[0], c | CONF_TX_STOP);
        sb.write(&mut r, CH_TX_CONF0[0], c | CONF_TX_STOP | CONF_CONF_UPDATE);
        assert!(!r.is_running(0));
        assert_eq!(sb.read(&mut r, INT_RAW) & END0, 0);
        sb.run_to(&mut r, 20 * WORD_CYCLES);
        assert_eq!(r.words(0).len(), 11, "nothing fetched after the stop");
        assert_eq!(sb.sched.live(), 0);
        // Half-level rule (§10.4): a zero second half emits half 1 then ends.
        let (mut sb, mut r, _) = rig();
        configure(&mut sb, &mut r, 1);
        fill(&mut sb, &mut r, 0, 48, ZERO);
        sb.write(&mut r, ram_off(2), word(true, 40, false, 0));
        sb.write(&mut r, CH_TX_LIM[0], 0);
        start_tx(&mut sb, &mut r);
        sb.run_to(&mut r, 2 * WORD_CYCLES + 80);
        assert_eq!(r.frames_ended(0), 1);
        assert_eq!(r.pulses(0).len(), 5, "two words and one half");
        assert_eq!(r.words(0).len(), 3);
    }

    #[test]
    fn channel_one_has_its_own_window_engine_and_interrupt_bits() {
        let (mut sb, mut r, _) = rig();
        // Two channels, one block each (the shipped plan).
        sb.write(
            &mut r,
            CH_TX_CONF0[1],
            (1 << CONF_DIV_CNT_SHIFT) | (1 << 6) | (1 << CONF_MEM_SIZE_SHIFT),
        );
        sb.write(&mut r, INT_ENA, 0x333);
        fill(&mut sb, &mut r, 48, 96, ONE);
        sb.write(&mut r, ram_off(96), 0);
        sb.write(&mut r, CH_TX_LIM[1], 24);
        let c = sb.read(&mut r, CH_TX_CONF0[1]);
        sb.write(
            &mut r,
            CH_TX_CONF0[1],
            c | CONF_MEM_TX_WRAP_EN | CONF_MEM_RD_RST | CONF_TX_START | CONF_CONF_UPDATE,
        );
        assert!(r.is_running(1) && !r.is_running(0));
        assert_eq!(
            sb.read(&mut r, CH_TX_STATUS[1]) & 0x1ff,
            49,
            "absolute: block 1 + 1"
        );
        sb.run_to(&mut r, 23 * WORD_CYCLES);
        assert_eq!(sb.read(&mut r, INT_ST), 1 << (INT_TX_THR_SHIFT + 1));
        sb.write(&mut r, INT_CLR, 0xffff);
        sb.write(&mut r, CH_TX_LIM[1], 48);
        sb.run_to(&mut r, 47 * WORD_CYCLES);
        assert_eq!(
            sb.read(&mut r, CH_TX_STATUS[1]) & 0x1ff,
            48,
            "wrapped to its own block"
        );
        sb.write(&mut r, ram_off(48), 0);
        sb.run_to(&mut r, 48 * WORD_CYCLES);
        assert_eq!(r.frames_ended(1), 1);
        assert_eq!(r.frames_ended(0), 0);
        assert_eq!(sb.read(&mut r, INT_RAW) & 0b11, 0b10);
        assert!(r.pulses(0).is_empty());
        assert_eq!(r.words(1).len(), 49);
    }

    // ---- the receiver (M2 P3) -------------------------------------------

    /// The pad the loopback's receiver reads, and the RX channel that reads
    /// it: gpio19 into `InputSignal::RMT_SIG_0`, which is channel 2.
    const RX_PAD: lp_emu_esp_common::PadId = lp_emu_esp_common::PadId(19);
    const RXI: usize = 0;
    /// One tick is two CPU cycles at the shipped settings (80 MHz PLL,
    /// `div_cnt = 1`, a 160 MHz CPU), which is what makes every duration in
    /// these tests exact.
    const TICK_CYCLES: u64 = 2;

    /// Route gpio19 into the receiver's input signal and give the channel
    /// esp-hal's `configure_rx` settings: `div_cnt 1`, `mem_size 1`, the
    /// idle threshold and the filter as asked for.
    fn configure_rx(sb: &mut Sandbox, r: &mut Rmt, idle_thres: u32, filter: Option<u32>) {
        sb.pins.route_in(rx_signal_of(RXI), RX_PAD, false);
        sb.pins.set_pad_input_enable(RX_PAD, true);
        let conf0 = (1 << RX_CONF0_DIV_CNT_SHIFT)
            | (idle_thres << RX_CONF0_IDLE_THRES_SHIFT)
            | (1 << RX_CONF0_MEM_SIZE_SHIFT);
        sb.write(r, CH_RX_CONF0[RXI], conf0);
        let mut conf1 = RX_CONF1_MEM_OWNER | RX_CONF1_MEM_RX_WRAP_EN;
        if let Some(thres) = filter {
            conf1 |= RX_CONF1_FILTER_EN | (thres << RX_CONF1_FILTER_THRES_SHIFT);
        }
        sb.write(r, CH_RX_CONF1[RXI], conf1);
        sb.write(r, INT_ENA, 0xffff);
    }

    /// `Channel<Rx>::receive` → `start_receive`: the threshold, the wrap, an
    /// update, then `rx_en` with the two resets, then another update.
    fn start_rx(sb: &mut Sandbox, r: &mut Rmt, rx_lim: u32) {
        sb.write(r, INT_CLR, 0xffff);
        sb.write(r, CH_RX_LIM[RXI], rx_lim);
        let c = sb.read(r, CH_RX_CONF1[RXI]);
        sb.write(r, CH_RX_CONF1[RXI], c | RX_CONF1_CONF_UPDATE);
        let c = sb.read(r, CH_RX_CONF1[RXI]);
        sb.write(
            r,
            CH_RX_CONF1[RXI],
            c | RX_CONF1_MEM_OWNER | RX_CONF1_MEM_WR_RST | RX_CONF1_APB_MEM_RST | RX_CONF1_RX_EN,
        );
        let c = sb.read(r, CH_RX_CONF1[RXI]);
        sb.write(r, CH_RX_CONF1[RXI], c | RX_CONF1_CONF_UPDATE);
    }

    /// Put an edge on the pad at cycle `at` and hand the block the edge the
    /// fabric recorded, the way the machine's slice drain does.
    fn pad(sb: &mut Sandbox, r: &mut Rmt, at: Cycles, level: bool) {
        sb.now = at;
        sb.pins.drive_pad(RX_PAD, level, at);
        let edges = sb.pins.take_edges();
        r.observe_edges(&edges, &mut sb.cx());
    }

    /// One WS2812 bit as the transmitter puts it on the wire: high for `high`
    /// ticks, then low for the rest of the 100-tick slot.
    fn bit(sb: &mut Sandbox, r: &mut Rmt, slot: u64, high: u64, start: Cycles) {
        let t0 = start + slot * 100 * TICK_CYCLES;
        pad(sb, r, t0, true);
        pad(sb, r, t0 + high * TICK_CYCLES, false);
    }

    fn rx_word(sb: &mut Sandbox, r: &mut Rmt, word_index: u32) -> u32 {
        sb.read(r, ram_off(word_index))
    }

    /// (a) The whole of the loopback in miniature: the receiver's words are
    /// the transmitter's words, half for half.
    #[test]
    fn the_received_words_are_the_transmitted_words() {
        let (mut sb, mut r, _) = rig();
        configure_rx(&mut sb, &mut r, 4_000, None);
        sb.now = 1_000;
        start_rx(&mut sb, &mut r, 24);
        assert!(r.rx_running(RXI));
        assert_eq!(
            sb.read(&mut r, CH_RX_STATUS[RXI]) >> RX_STATUS_STATE_SHIFT & 7,
            1,
            "state = receiving"
        );

        // Four WS2812 bits — 0, 1, 1, 0 — on the 100-tick grid the chase
        // transmits on, starting well after `rx_en` so the leading idle is
        // demonstrably not a pulse.
        let start = 2_000;
        for (slot, high) in [(0u64, 32u64), (1, 64), (2, 64), (3, 32)] {
            bit(&mut sb, &mut r, slot, high, start);
        }
        // The words land as their second halves close, so three are in the
        // RAM and the fourth is still open.
        assert_eq!(rx_word(&mut sb, &mut r, 96), ZERO, "bit 0 is a WS2812 zero");
        assert_eq!(rx_word(&mut sb, &mut r, 97), ONE);
        assert_eq!(rx_word(&mut sb, &mut r, 98), ONE);
        assert_eq!(r.rx_waddr(RXI), 99, "three words written, one half open");

        // The line then rests: the idle threshold ends the reception, the
        // last measured run is stored, and the last word is an end marker.
        let last_edge = start + 3 * 100 * TICK_CYCLES + 32 * TICK_CYCLES;
        sb.run_to(&mut r, last_edge + 4_001 * TICK_CYCLES);
        assert!(!r.rx_running(RXI), "idle_thres ended it");
        let closing = rx_word(&mut sb, &mut r, 99);
        assert_eq!(closing & 0x7fff, 32, "the fourth bit's high half");
        assert_eq!(closing & (1 << 15), 1 << 15, "…and it was high");
        assert_eq!(
            (closing >> 16) & 0x7fff,
            4_001,
            "the trailing idle run, idle_thres + 1"
        );
        assert_eq!(closing & (1 << 31), 0, "…and it was low");
        assert_eq!(rx_word(&mut sb, &mut r, 100), 0, "the end marker");
        // `rx_end` is ch2's bit — bit 2 of `int_raw`, not bit 0.
        assert_eq!(
            sb.read(&mut r, INT_RAW) & 0b1111,
            1 << (INT_TX_END_SHIFT + 2),
            "ch2_rx_end, and no tx bit"
        );
        assert!(sb.irq.level(source::RMT), "source 49 is up");
        sb.write(&mut r, INT_CLR, 1 << (INT_TX_END_SHIFT + 2));
        assert!(!sb.irq.level(source::RMT), "w1c drops it");
    }

    /// (b) G3-3, the threshold half: a level held past `idle_thres` ends the
    /// reception and one held less does not.
    #[test]
    fn the_idle_threshold_ends_the_reception_and_a_shorter_gap_does_not() {
        let (mut sb, mut r, _) = rig();
        configure_rx(&mut sb, &mut r, 500, None);
        sb.now = 1_000;
        start_rx(&mut sb, &mut r, 24);

        pad(&mut sb, &mut r, 2_000, true);
        pad(&mut sb, &mut r, 2_000 + 100 * TICK_CYCLES, false);
        // A gap of 400 ticks: shorter than the threshold, so nothing ends.
        sb.run_to(&mut r, 2_000 + 500 * TICK_CYCLES);
        assert!(r.rx_running(RXI), "400 ticks of idle is not 500");
        pad(&mut sb, &mut r, 2_000 + 500 * TICK_CYCLES, true);
        pad(&mut sb, &mut r, 2_000 + 600 * TICK_CYCLES, false);
        assert!(r.rx_running(RXI));
        assert_eq!(sb.read(&mut r, INT_RAW) & 0b1111, 0, "no rx_end yet");

        // And now a gap that is longer.
        sb.run_to(&mut r, 2_000 + 600 * TICK_CYCLES + 501 * TICK_CYCLES);
        assert!(!r.rx_running(RXI));
        assert_eq!(
            sb.read(&mut r, INT_RAW) & 0b1111,
            1 << (INT_TX_END_SHIFT + 2)
        );
        // Two bits' worth of runs went in: high, low, high, then the idle.
        assert_eq!(rx_word(&mut sb, &mut r, 96), word(true, 100, false, 400));
        let closing = rx_word(&mut sb, &mut r, 97);
        assert_eq!(closing & 0x7fff, 100);
        assert_eq!((closing >> 16) & 0x7fff, 501);
    }

    /// (c) G3-3, the filter half: a pulse narrower than `rx_filter_thres`
    /// never reaches the edge detector, and one wider than it does.
    ///
    /// This is the test a sampler that ignores `rx_filter` fails, and the
    /// numbers are chosen so that it cannot pass by accident: the glitch and
    /// the kept pulse differ only in width.
    #[test]
    fn a_pulse_narrower_than_the_filter_is_swallowed_and_a_wider_one_is_kept() {
        // The filter's threshold is in APB periods and the APB and the
        // channel run at the same 80 MHz here, so 40 periods is 40 ticks.
        let (mut sb, mut r, _) = rig();
        configure_rx(&mut sb, &mut r, 4_000, Some(40));
        sb.now = 1_000;
        start_rx(&mut sb, &mut r, 24);

        // A clean 100-tick high, then a 20-tick glitch, then a 100-tick high.
        pad(&mut sb, &mut r, 2_000, true);
        pad(&mut sb, &mut r, 2_000 + 100 * TICK_CYCLES, false);
        let glitch = 2_000 + 300 * TICK_CYCLES;
        pad(&mut sb, &mut r, glitch, true);
        pad(&mut sb, &mut r, glitch + 20 * TICK_CYCLES, false);
        pad(&mut sb, &mut r, 2_000 + 600 * TICK_CYCLES, true);
        pad(&mut sb, &mut r, 2_000 + 700 * TICK_CYCLES, false);
        sb.run_to(&mut r, 2_000 + 700 * TICK_CYCLES + 4_001 * TICK_CYCLES);
        assert!(!r.rx_running(RXI));

        // Two pulses in, two runs out plus the idle: the glitch left no trace
        // and the low run it interrupted is one 500-tick run, not three.
        assert_eq!(
            rx_word(&mut sb, &mut r, 96),
            word(true, 100, false, 500),
            "the glitch and the low around it are one run"
        );
        let closing = rx_word(&mut sb, &mut r, 97);
        assert_eq!(closing & 0x7fff, 100, "the second clean high");
        assert_eq!((closing >> 16) & 0x7fff, 4_001, "then the idle");
        assert_eq!(r.rx_words_written(RXI), 3, "two words and the end marker");

        // The same run with the filter off keeps the glitch, which is what
        // makes the assertion above about the filter rather than about the
        // arithmetic.
        let (mut sb, mut r, _) = rig();
        configure_rx(&mut sb, &mut r, 4_000, None);
        sb.now = 1_000;
        start_rx(&mut sb, &mut r, 24);
        pad(&mut sb, &mut r, 2_000, true);
        pad(&mut sb, &mut r, 2_000 + 100 * TICK_CYCLES, false);
        pad(&mut sb, &mut r, glitch, true);
        pad(&mut sb, &mut r, glitch + 20 * TICK_CYCLES, false);
        assert_eq!(
            rx_word(&mut sb, &mut r, 96),
            word(true, 100, false, 200),
            "unfiltered, the low run ends at the glitch"
        );
    }

    /// (d) G3-4: `rx_lim` words raise `rx_thr_event`, the writer wraps, and
    /// both events clear the way the TX side's do.
    #[test]
    fn the_threshold_fires_every_rx_lim_words_and_the_writer_wraps() {
        let (mut sb, mut r, _) = rig();
        configure_rx(&mut sb, &mut r, 20_000, None);
        sb.now = 1_000;
        // esp-hal's `start_receive` arms the threshold at half the window.
        start_rx(&mut sb, &mut r, 24);

        let start = 2_000;
        // A word closes when its *second* half does, and a bit's low run only
        // ends at the next bit's rising edge — so 24 bits on the wire is 23
        // words in the RAM, and the 24th lands when bit 24 starts.
        for slot in 0..24u64 {
            bit(&mut sb, &mut r, slot, 32, start);
        }
        assert_eq!(r.rx_words_written(RXI), 23);
        assert_eq!(sb.read(&mut r, INT_ST) & 0b1111_0000_0000, 0, "not yet");
        bit(&mut sb, &mut r, 24, 32, start);
        assert_eq!(
            sb.read(&mut r, INT_RAW) & 0xfff,
            1 << (INT_TX_THR_SHIFT + 2),
            "ch2_rx_thr_event, bit 10"
        );
        assert!(sb.irq.level(source::RMT));
        assert_eq!(r.rx_waddr(RXI), 96 + 24, "half the window");
        sb.write(&mut r, INT_CLR, 0xffff);
        assert!(!sb.irq.level(source::RMT));

        // The second half, then the wrap: 48 words in, the pointer is back at
        // the window's first word and the threshold has fired twice.
        for slot in 25..48u64 {
            bit(&mut sb, &mut r, slot, 32, start);
        }
        // Bit 48 is a WS2812 one, so the word that overwrites word 0 after
        // the wrap can be told apart from the zeros around it.
        bit(&mut sb, &mut r, 48, 64, start);
        assert_eq!(
            sb.read(&mut r, INT_RAW) & 0xfff,
            1 << (INT_TX_THR_SHIFT + 2),
            "and again at 48"
        );
        assert_eq!(r.rx_waddr(RXI), 96, "wrapped to the window's start");
        assert_eq!(r.rx_words_written(RXI), 48);
        // The wrap overwrites: word 96 now holds bit 48's code, not bit 0's.
        bit(&mut sb, &mut r, 49, 32, start);
        assert_eq!(rx_word(&mut sb, &mut r, 96), ONE, "bit 48 landed on bit 0");
    }

    /// (d2) A reception longer than `idle_thres` does not end in the middle
    /// of itself.
    ///
    /// The regression the first loopback run found: the idle deadline is
    /// re-armed on every edge, and `Scheduler::schedule_at` appends rather
    /// than replaces, so an engine that did not cancel the previous one ended
    /// `idle_thres` after the FIRST edge of a frame. Here the frame is 200
    /// bits — 250 us — against a 100-tick threshold, so a stale deadline
    /// would fire 51 words in and this test would see 51 words instead of
    /// 200.
    #[test]
    fn a_reception_longer_than_the_idle_threshold_does_not_end_in_the_middle_of_itself() {
        let (mut sb, mut r, _) = rig();
        configure_rx(&mut sb, &mut r, 100, None);
        sb.now = 1_000;
        start_rx(&mut sb, &mut r, 0);

        for slot in 0..200u64 {
            bit(&mut sb, &mut r, slot, 32, 2_000);
        }
        assert!(
            r.rx_running(RXI),
            "200 bits at 100 ticks each, none of them 100 ticks of silence"
        );
        assert_eq!(
            r.rx_words_written(RXI),
            199,
            "one word per bit, less the open one"
        );

        // …and it ends when the line finally does go quiet.
        let last = 2_000 + 199 * 100 * TICK_CYCLES + 32 * TICK_CYCLES;
        sb.run_to(&mut r, last + 101 * TICK_CYCLES);
        assert!(!r.rx_running(RXI));
        assert_eq!(
            r.rx_words_written(RXI),
            201,
            "199, the closing word, the marker"
        );
    }

    /// (e) With `mem_rx_wrap_en` clear the window fills instead: `mem_full`,
    /// `rx_err` on ch2's bit, and the receiver stops where it is.
    #[test]
    fn a_full_window_without_wrap_is_mem_full_and_an_error() {
        let (mut sb, mut r, _) = rig();
        configure_rx(&mut sb, &mut r, 20_000, None);
        // Clear the wrap bit the helper set.
        let c = sb.read(&mut r, CH_RX_CONF1[RXI]);
        sb.write(&mut r, CH_RX_CONF1[RXI], c & !RX_CONF1_MEM_RX_WRAP_EN);
        sb.now = 1_000;
        start_rx(&mut sb, &mut r, 0);

        for slot in 0..49u64 {
            bit(&mut sb, &mut r, slot, 32, 2_000);
        }
        assert!(!r.rx_running(RXI), "the window filled");
        assert_eq!(
            sb.read(&mut r, INT_RAW) & 0xff,
            1 << (INT_TX_ERR_SHIFT + 2),
            "ch2_rx_err, bit 6"
        );
        assert!(
            sb.read(&mut r, CH_RX_STATUS[RXI]) & RX_STATUS_MEM_FULL != 0,
            "mem_full"
        );
    }

    /// (f) A receiver with no pad routed to its input signal says so and
    /// never invents a level — the failure mode a loopback run would hit if
    /// `func_in_sel_cfg` were still accept-and-remember.
    #[test]
    fn a_receiver_with_no_input_route_records_nothing_and_says_so() {
        let (mut sb, mut r, _) = rig();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        // Everything except the route.
        let conf0 = (1 << RX_CONF0_DIV_CNT_SHIFT)
            | (4_000 << RX_CONF0_IDLE_THRES_SHIFT)
            | (1 << RX_CONF0_MEM_SIZE_SHIFT);
        sb.write(&mut r, CH_RX_CONF0[RXI], conf0);
        sb.write(&mut r, CH_RX_CONF1[RXI], RX_CONF1_MEM_OWNER);
        sb.now = 1_000;
        start_rx(&mut sb, &mut r, 24);
        pad(&mut sb, &mut r, 2_000, true);
        pad(&mut sb, &mut r, 2_400, false);
        assert_eq!(r.rx_words_written(RXI), 0);
        let lines: Vec<String> = buf
            .lines()
            .into_iter()
            .filter(|l| l.contains("no pad routed"))
            .collect();
        assert_eq!(lines.len(), 1, "{lines:?}");
    }

    /// (g) The receiver's state survives a snapshot: the same words, at the
    /// same cycles, on both sides of a save/load.
    #[test]
    fn a_reception_in_flight_rides_the_snapshot() {
        let (mut sb, mut r, clock) = rig();
        configure_rx(&mut sb, &mut r, 4_000, None);
        sb.now = 1_000;
        start_rx(&mut sb, &mut r, 24);
        for (slot, high) in [(0u64, 32u64), (1, 64)] {
            bit(&mut sb, &mut r, slot, high, 2_000);
        }
        let blob = r.save_state();

        let mut restored = Rmt::new(clock);
        restored.attached(7);
        restored.load_state(&blob);
        assert!(restored.rx_running(RXI));
        assert_eq!(restored.rx_waddr(RXI), r.rx_waddr(RXI));
        assert_eq!(restored.rx_words_written(RXI), r.rx_words_written(RXI));
        // …and it goes on measuring from where it was: bit 1's word closes on
        // bit 2's rising edge, on both sides.
        bit(&mut sb, &mut restored, 2, 64, 2_000);
        bit(&mut sb, &mut r, 2, 64, 2_000);
        assert_eq!(restored.ram()[97], r.ram()[97]);
        assert_eq!(restored.ram()[97], ONE);
        assert_eq!(restored.ram()[96], ZERO);
    }
}
