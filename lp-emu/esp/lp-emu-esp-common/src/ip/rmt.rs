//! `RMT` — the remote-control transceiver as the WS281x path drives it: the
//! PAC register file, the pulse RAM, and one TX engine per channel on the
//! scheduler that consumes words at the configured clock and raises `tx_end`
//! / `tx_thr_event` / `tx_err` on the chip's own interrupt source — and, on a
//! chip that asks for them, RX engines that sample a routed input signal back
//! into the same RAM and raise `rx_end` / `rx_thr_event` on the same source.
//!
//! The C6's view (M5 P1, M2 P3, M5 P3) is the whole of the behaviour here.
//! Xtensa M6 P07 **moved** the file and parameterised it: every number that
//! differs between the two parts is in [`Config`], and the C6's behaviour is
//! unchanged, note for note (ruling D4/DD64). The register facts are the two
//! PACs' `rmt` blocks (offsets in each chip's `regs` table; bit positions
//! cited per field below) and the drivers that write them: esp-hal 1.1.1's
//! `Rmt::new` / `configure_tx` / `with_pin` and this project's
//! `lp-fw/fw-esp32c6/src/output/rmt/c6_rmt.rs` and
//! `lp-fw/fw-esp32s3/src/output/rmt/s3_rmt.rs` backends for `lp-ws281x`.
//!
//! # Why one file serves two chips (the layout identity, quoted)
//!
//! [`crate::ip`]'s rule is that a layout may live here only when two chips'
//! PACs agree on it, verified and quoted. The S3's RMT **is** the C6's IP
//! (`m6/notes.md` §3.2): `ch_tx_conf0`, `ch_rx_conf0`, `ch_rx_conf1`,
//! `ch_tx_status`, `ch_rx_status`, `ch_tx_lim`, `ch_rx_lim`,
//! `ch_rx_carrier_rm`, `sys_conf`, `tx_sim` and `ref_cnt_rst` exist on both
//! by name, with the same fields in the same order, and **none of them exists
//! on the classic** (whose eight bidirectional channels of 64 words are a
//! genuinely different block, and keep their own file). What differs is a
//! table of offsets and a handful of field positions, and that table is
//! [`Config`].
//!
//! ⚠️ **The differences that pass every register test and fail the first
//! frame.** Each is a [`Config`] field with a test of its own in the two
//! chips' crates:
//!
//! 1. **The interrupt bits are grouped by event on the S3 and share a nibble
//!    on the C6.** S3: `chN_tx_end` 0–3, `chN_tx_err` 4–7, `chN_tx_thr_event`
//!    8–11, `chN_tx_loop` 12–15, `chN_rx_end` 16–19, `rx_err` 20–23,
//!    `rx_thr_event` 24–27. C6: bits 0–3 are `ch0_tx_end, ch1_tx_end,
//!    ch2_rx_end, ch3_rx_end`, because *its* RX channels are 2 and 3. The
//!    `ch_tx_*(n)` PAC accessors have the same names on both chips, which is
//!    exactly why the difference is easy to miss (`s3_rmt.rs:29-34`). So
//!    [`Config::int_bit`] is a **function per chip**, not a shift constant.
//! 2. **`sys_conf` carries the clock divider on the S3.** `sclk_div_num`
//!    4:11, `sclk_div_a` 12:17, `sclk_div_b` 18:23, `sclk_sel` 24:25,
//!    `sclk_active` 26. On the C6 the divider is in `PCR.rmt_sclk_conf` and
//!    `sys_conf` has five bits. So the clock is a [`ClockLine`] the chip
//!    supplies, and it is handed the block's own `sys_conf` word.
//! 3. **`ch_tx_conf0.mem_size` is bits 16:19 on the S3 and 16:18 on the C6**
//!    — eight blocks addressable against four.
//! 4. **`ch_rx_conf0.mem_size` is bits 24:27 on the S3 and 23:25 on the C6**
//!    (`esp32s3-0.35.2` / `esp32c6-0.23.2`, `rmt/ch_rx_conf0.rs`), which is
//!    why the two chips' reset values differ (`0x317f_ff02` against
//!    `0x30ff_ff02`) at the same `mem_size = 1`. Not in the phase brief's
//!    list of three; found by reading both PACs field for field.
//! 5. **The status words are laid out differently.** S3: `mem_raddr_ex` /
//!    `mem_waddr_ex` bits **0:9** (ten bits, absolute over the whole RAM),
//!    `state` 22:24, `mem_empty` 25. C6: `mem_raddr_ex` 0:8, `state` 9:11,
//!    `mem_empty` 22. A driver reading `read_pos` through the C6's mask on an
//!    S3 would fold the state bits into the pointer.
//!
//! # What the WS281x driver needs from this block
//!
//! `start_frame` fills both halves of the channel's window, arms `tx_lim` at
//! the half, and starts; every `tx_thr_event` the ISR reads `mem_raddr_ex`,
//! flips `tx_lim` between `half` and `window_words`, plants a STOP guard in
//! the half just left, and refills it — racing the transmitter across a
//! half-window deadline. The all-zero word ends the frame with `tx_end`. So
//! the model has to get four things exactly right, and each is pinned by a
//! `Sandbox` test in the chip crates:
//!
//! 1. **`tx_lim` is a position in the window**, not a repeating count: the
//!    event fires when the read pointer reaches word `tx_lim`, and
//!    `tx_lim == window_words` is the wrap back to word 0 (M5 discovery §4 —
//!    silicon's `refills == wanted` over 5,520 frames; a counter that reset
//!    on every event would fire the alternating 24/48 thresholds at 24, 24,
//!    0, 0 and truncate every frame).
//! 2. **Time is anchored on the previous due cycle.** One word's two pulses
//!    take `(dur1 + dur2)` ticks; the next word is due at
//!    `start_cycle + cycles_for(ticks_since_start)` with integer arithmetic
//!    over the absolute tick count since `tx_start` — never accumulated per
//!    word, never taken from the dispatch cycle.
//! 3. **The RAM is live.** The engine reads `ram[raddr]` when it fetches; a
//!    refill overwrites what the consumer has not fetched yet, which is what
//!    a single-ported RAM does.
//! 4. **The ISR's `tx_lim` write takes effect immediately**, with no
//!    `conf_update` (the driver rewrites it mid-frame).
//!
//! # Modeled choices (M5 discovery §10.1–5), each named where it is made
//!
//! - §10.2 `mem_raddr_ex` = the **next word to fetch** (the driver tolerates
//!   ±1 through `guard_skips`), **absolute** over the whole RAM rather than
//!   window-relative — the same quirk the classic has, and what
//!   `s3_rmt.rs:35-37,111` says the S3 has too.
//! - §10.3 `tx_start` / `mem_rd_rst` / `apb_mem_rst` are pulses that take
//!   effect at `conf_update` (both drivers write it right after); a
//!   `tx_start` with no `conf_update` in the same slice is acted on at the
//!   slice boundary with a note, so a frame never silently fails to start.
//!   `tx_stop` is sticky and reads back.
//! - §10.4 the end marker: a zero duration in half 1 ends the transmission
//!   before it; in half 2, half 1 is emitted and the word ends it. Both
//!   drivers only ever write the all-zero word.
//! - §10.5 `ref_cnt_rst` is accepted (a divider phase reset is unobservable
//!   at `div_cnt = 1`); no function clock **stalls** the engine, which
//!   resumes when the clock returns; a source the chip cannot name is refused
//!   as no clock rather than guessed at.
//! - `tx_stop` raises no `tx_end` (esp-hal's `stop` expects none).
//!
//! # The receivers, and the chip that does not model them
//!
//! On a chip whose [`Config::model_rx`] is set (the C6, whose `rmt_rx`
//! payload exercises them), each RX channel is an engine: [`RxEngine`]
//! samples the level of the pad routed to the channel's **input** signal,
//! measures each run in channel ticks and writes them into the channel's RAM
//! window two to a word. `idle_thres` ends the reception, `rx_filter`
//! swallows a pulse narrower than its threshold, `rx_lim` raises
//! `rx_thr_event` and `mem_rx_wrap_en` wraps the write pointer; `rx_end` and
//! `rx_thr_event` go out with the same status and clear semantics the TX side
//! has.
//!
//! Each is fed the fabric's **edges** rather than sampled tick by tick — see
//! [`RxEngine`] for why that is the same model and what it costs (a slice of
//! latency on the two interrupts, never on the words).
//!
//! ⚠️ **A chip with `model_rx` clear accepts the registers and models
//! nothing.** The S3 has four RX channels and the shipped firmware uses none
//! (`s3_rmt.rs:50-52` — `CH0..=CH3` transmit, `CH4..=CH7` receive), so
//! `rx_en` set on a channel there is a `log::warn!` naming the channel rather
//! than an engine that would have to be believed. Every RX register still
//! answers, is remembered and is graded.
//!
//! # What is not here
//!
//! The APB FIFO (`ch*data`, `sys_conf.apb_fifo_mask = 0`) is not modelled —
//! every driver this project runs uses direct RAM access. Neither is the
//! carrier: modulation on the way out, demodulation on the way in, and
//! `ch*_rx_carrier_rm` is accepted with one note. The RAM has one port and no
//! arbiter, so `ch_rx_conf1.mem_owner` is recorded rather than enforced and
//! `ch_rx_status.mem_owner_err` never rises. Where the waveform goes is the
//! signal fabric's; the per-channel **pulse log** and **fetched-word log** are
//! the TX side's other observation.
//!
//! # The refill measurement (M5 P3)
//!
//! The block also watches the race it is half of, and reports it: for every
//! `tx_thr_event` it counts the words the transmitter consumes before the
//! guest's next `ch_tx_lim` write (the **entry** delay) and then before the
//! last RAM write of that refill (the **fill**), in the same units
//! `lp-ws281x` measures them in from `read_pos` — words. Every measurement is
//! a `RMT REFILL ch=0 at=<cyc> entry=<w> fill=<w>` trace note, and the totals
//! are a nine-bucket histogram per channel ([`RefillStats`]).
//!
//! It is **reported and never gated** (D13/PD9), and the reason is in the
//! numbers rather than in the policy: the emulator's ISR path is
//! RAM-resident by construction and the machine has no flash-miss cost, so
//! the entry half is a floor rather than a prediction of silicon's 20–29
//! words. What it is good for is the shape — a fill that grows, or a bucket
//! that starts landing at "≥ half", is the model or the driver getting
//! slower at the deadline.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lp_emu_core::sched::{Cycles, EventId};

use crate::pins::Edge;
use crate::regfile::{lane_of, merge_lane};
use crate::regnames::RegNames;
use crate::{
    BusCx, Peripheral, RegFile, RegGrade, RegGrades, SignalId, Width, event_id, event_local,
};

// ---------------------------------------------------------------------------
// The bit positions the two chips agree on. Everything that differs is in
// `Config`; everything here is quoted from both PACs and identical in both.
// ---------------------------------------------------------------------------

// `ch_tx_conf0` bits.
pub const CONF_TX_START: u32 = 1 << 0;
pub const CONF_MEM_RD_RST: u32 = 1 << 1;
pub const CONF_APB_MEM_RST: u32 = 1 << 2;
pub const CONF_MEM_TX_WRAP_EN: u32 = 1 << 4;
pub const CONF_IDLE_OUT_LV: u32 = 1 << 5;
pub const CONF_TX_STOP: u32 = 1 << 7;
pub const CONF_DIV_CNT_SHIFT: u32 = 8;
pub const CONF_CONF_UPDATE: u32 = 1 << 24;
/// The strobes: read back 0 whatever was written (§10.3).
pub const CONF_PULSES: u32 = CONF_TX_START | CONF_MEM_RD_RST | CONF_APB_MEM_RST | CONF_CONF_UPDATE;
pub const SYS_CONF_APB_FIFO_MASK: u32 = 1 << 0;

// `ch_rx_conf0` bits (the two that are in the same place on both).
pub const RX_CONF0_DIV_CNT_SHIFT: u32 = 0;
pub const RX_CONF0_IDLE_THRES_SHIFT: u32 = 8;
pub const RX_CONF0_IDLE_THRES_MASK: u32 = 0x7fff;
pub const RX_CONF0_CARRIER_EN: u32 = 1 << 28;

// `ch_rx_conf1` bits — identical on both chips, field for field.
pub const RX_CONF1_RX_EN: u32 = 1 << 0;
pub const RX_CONF1_MEM_WR_RST: u32 = 1 << 1;
pub const RX_CONF1_APB_MEM_RST: u32 = 1 << 2;
pub const RX_CONF1_MEM_OWNER: u32 = 1 << 3;
pub const RX_CONF1_FILTER_EN: u32 = 1 << 4;
pub const RX_CONF1_FILTER_THRES_SHIFT: u32 = 5;
pub const RX_CONF1_FILTER_THRES_MASK: u32 = 0xff;
pub const RX_CONF1_MEM_RX_WRAP_EN: u32 = 1 << 13;
/// Bit 15, the PAC's *"synchronization bit"* — `conf_update`, which esp-hal's
/// `DynChannelAccess::update` writes for an RX channel exactly where it
/// writes `ch_tx_conf0.conf_update` for a TX one.
pub const RX_CONF1_CONF_UPDATE: u32 = 1 << 15;
/// The strobes: read back 0 whatever was written, as the TX side's do.
pub const RX_CONF1_PULSES: u32 = RX_CONF1_MEM_WR_RST | RX_CONF1_APB_MEM_RST | RX_CONF1_CONF_UPDATE;

/// The largest duration one half of a word can hold: 15 bits.
pub const DURATION_MAX: u32 = 0x7fff;

/// Log caps: generous — a 256-LED frame is 6,146 words / 12,292 pulses, so
/// these hold hundreds of frames — with a note when hit.
pub const PULSE_LOG_CAP: usize = 4_000_000;
pub const WORD_LOG_CAP: usize = 2_000_000;

/// Which interrupt a [`Config::int_bit`] call is asking about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntKind {
    /// `chN_tx_end` / `chN_rx_end`.
    End,
    /// `chN_tx_err` / `chN_rx_err`.
    Err,
    /// `chN_tx_thr_event` / `chN_rx_thr_event`.
    Thr,
}

/// Which half of the block a channel index belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// A TX channel, indexed by its own number (0-based).
    Tx,
    /// A receiver, indexed by its **receiver index** — 0 is the first RX
    /// channel, whatever absolute channel number that is.
    Rx,
}

/// The function clock, as a source and a fractional divider: the channel tick
/// is `src / (div_num + 1 + a/b) / div_cnt`.
///
/// Which register the four numbers come out of is the chip's
/// ([`ClockLine`]); the arithmetic below is not, and it is exact — every
/// conversion is over the **absolute** tick count from an anchor, so no
/// per-word rounding ever accumulates across a 6,144-word frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    pub src_hz: u64,
    pub div_num: u32,
    pub div_a: u32,
    pub div_b: u32,
    /// The CPU the cycles are counted in. Constant for a machine; carried
    /// here so the arithmetic is self-contained.
    pub cpu_hz: u64,
}

impl Clock {
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
    pub fn hz(&self) -> u64 {
        let (num, den) = self.divider();
        (u128::from(self.src_hz) * den / num) as u64
    }

    /// CPU cycles for `ticks` channel ticks at `div_cnt`, exactly: floor of
    /// `ticks × CPU_HZ × div_cnt × divider / src`. Over absolute ticks, so
    /// no per-word rounding ever accumulates.
    pub fn cycles_for(&self, ticks: u64, div_cnt: u32) -> Cycles {
        let (num, den) = self.divider();
        let n = u128::from(ticks) * u128::from(self.cpu_hz) * u128::from(div_cnt) * num;
        let d = u128::from(self.src_hz) * den;
        (n / d) as Cycles
    }

    /// The inverse, for the receiver: how many channel ticks `cycles` CPU
    /// cycles are, exactly, floored.
    fn ticks_for(&self, cycles: Cycles, div_cnt: u32) -> u64 {
        let (num, den) = self.divider();
        let n = u128::from(cycles) * u128::from(self.src_hz) * den;
        let d = u128::from(self.cpu_hz) * u128::from(div_cnt) * num;
        (n / d) as u64
    }
}

/// Where the block's function clock comes from — the one behaviour the two
/// chips genuinely do not share.
///
/// The C6 reads `PCR.rmt_conf` and `PCR.rmt_sclk_conf`, two registers in a
/// different block, handed in on a shared cell the way the UART clocks are.
/// The S3 reads its **own** `sys_conf`, which is why the block's stored word
/// is passed in: an implementation takes whichever it needs and ignores the
/// other.
pub trait ClockLine: core::fmt::Debug + Send {
    /// Decode the current clock, or say why there is none. The string is a
    /// trace note the guest may read in a stall message, so it names the
    /// register and the value rather than the conclusion.
    fn decode(&self, sys_conf: u32, cpu_hz: u64) -> Result<Clock, &'static str>;
}

/// Every number that differs between two parts.
///
/// A `&'static` table the chip crate writes out. Nothing here is derived and
/// nothing is defaulted: a field that a chip got wrong is a frame that does
/// not come out, so each one is read from that chip's own PAC.
pub struct Config {
    /// The block's register-name table, for the trace and `reg_name`.
    pub reg_names: RegNames,
    /// The chip's interrupt source number for this block.
    pub source: u16,
    /// The CPU the machine counts cycles in.
    pub cpu_hz: u64,
    /// The APB clock the RX filter's threshold is counted in.
    pub apb_hz: u64,
    /// How often a stalled engine re-checks the clock.
    pub clock_poll_cycles: u64,
    /// The parenthesis on the stall note: which block a reader should look at
    /// to find out why the engine has no clock. The C6's is PCR's; the S3's
    /// is its own `sys_conf`. It is a string rather than a derived phrase
    /// because the C6's committed transcripts carry the exact bytes.
    pub stall_clock_hint: &'static str,

    /// The PAC register block's length: `ch0data` through `date`, rounded up.
    pub regs_len: u32,
    /// Where the pulse RAM starts inside the window.
    pub ram_offset: u32,
    /// Words of pulse RAM: `block_words × (tx + rx channels)`.
    pub ram_words: usize,
    /// Words in one channel's block.
    pub block_words: u32,
    /// The whole window the bus maps: registers, the gap, the RAM.
    pub len: u32,

    pub tx_channels: usize,
    pub rx_channels: usize,
    /// The absolute channel number of receiver index 0.
    pub rx_ch_base: usize,
    /// Whether the receivers are **modelled**. Clear on a chip whose firmware
    /// never receives: the registers still answer and `rx_en` is warned about.
    pub model_rx: bool,

    /// One past the last `chNdata` register — the APB FIFO window.
    pub ch_data_end: u32,
    pub ch_tx_conf0: &'static [u32],
    pub ch_rx_conf0: &'static [u32],
    pub ch_rx_conf1: &'static [u32],
    pub ch_tx_status: &'static [u32],
    pub ch_rx_status: &'static [u32],
    pub ch_rx_carrier_rm: &'static [u32],
    pub ch_tx_lim: &'static [u32],
    pub ch_rx_lim: &'static [u32],
    pub int_raw: u32,
    pub int_st: u32,
    pub int_ena: u32,
    pub int_clr: u32,
    pub sys_conf: u32,
    pub ref_cnt_rst: u32,

    /// `ch_tx_conf0.mem_size` — 16:18 on the C6, 16:19 on the S3.
    pub conf_mem_size_shift: u32,
    pub conf_mem_size_mask: u32,
    /// `ch_rx_conf0.mem_size` — 23:25 on the C6, 24:27 on the S3.
    pub rx_conf0_mem_size_shift: u32,
    pub rx_conf0_mem_size_mask: u32,
    pub ch_tx_conf0_reset: u32,
    pub ch_rx_conf0_reset: u32,
    pub ch_rx_conf1_reset: u32,

    /// `ch_tx_lim.tx_lim` and `ch_rx_lim.rx_lim`, both nine bits on both
    /// chips (max 511).
    pub tx_lim_mask: u32,
    pub rx_lim_mask: u32,

    /// `ch_tx_status.mem_raddr_ex` — 0:8 on the C6, 0:9 on the S3.
    pub tx_status_raddr_mask: u32,
    pub tx_status_state_shift: u32,
    pub tx_status_mem_empty: u32,
    /// `ch_rx_status.mem_waddr_ex` — 0:8 on the C6, 0:9 on the S3.
    pub rx_status_waddr_mask: u32,
    pub rx_status_apb_raddr_shift: u32,
    pub rx_status_state_shift: u32,
    pub rx_status_mem_full: u32,

    /// The `int_*` bits this chip has at all.
    pub int_mask: u32,
    /// **A function, not a shift.** Which `int_raw` bit one interrupt of one
    /// channel is. The C6 interleaves TX and RX in the same nibbles because
    /// its RX channels *are* channels 2 and 3; the S3 groups by event. See
    /// the module header's delta 1.
    pub int_bit: fn(Dir, IntKind, usize) -> u32,

    /// `OutputSignal::RMT_SIG_0` on this chip: TX channel `ch` drives
    /// `rmt_sig_0 + ch`.
    pub rmt_sig_0: u16,
    /// `InputSignal::RMT_SIG_0`, read from the **input** enumeration. A
    /// separate number because on the classic the two are four apart, and a
    /// coincidence recorded as an identity is a bug waiting for the next chip.
    pub rmt_rx_sig_0: u16,
}

impl core::fmt::Debug for Config {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("rmt::Config")
            .field("block", &self.reg_names.block)
            .field("tx_channels", &self.tx_channels)
            .field("rx_channels", &self.rx_channels)
            .field("block_words", &self.block_words)
            .field("ram_offset", &self.ram_offset)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// The GPIO-matrix output signal TX channel `ch` drives. Whether any pad
    /// listens is the fabric's business, and the RMT never learns the answer
    /// — it cannot see the GPIO block.
    pub const fn signal_of(&self, ch: usize) -> SignalId {
        SignalId(self.rmt_sig_0 + ch as u16)
    }

    /// The GPIO-matrix **input** signal receiver `rxi` reads. Which pad it is
    /// is the fabric's business (`Fabric::route_in`, written by the GPIO
    /// block's `func_in_sel_cfg`), and the RMT never learns the pad's number
    /// — only its level.
    pub const fn rx_signal_of(&self, rxi: usize) -> SignalId {
        SignalId(self.rmt_rx_sig_0 + rxi as u16)
    }

    /// The absolute channel number of receiver index `rxi`.
    pub const fn rx_channel(&self, rxi: usize) -> usize {
        self.rx_ch_base + rxi
    }

    /// Every event id this block uses, laid out from the channel counts:
    /// `EV_WORD + ch`, then `EV_LATE_START + ch`, then one clock poll, then
    /// `EV_RX_IDLE + rxi`. On a two-TX chip that is 0, 2, 4, 5 — the numbers
    /// the C6's view has always used.
    const fn ev_late_start(&self) -> u16 {
        self.tx_channels as u16
    }

    const fn ev_clock_poll(&self) -> u16 {
        2 * self.tx_channels as u16
    }

    const fn ev_rx_idle(&self) -> u16 {
        2 * self.tx_channels as u16 + 1
    }
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
/// M5 discovery §4's `entry delay` and `refill lag`, measured here in the
/// units the driver measures them in — words the transmitter consumed:
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
    /// The two chip numbers this engine needs on its own, copied from
    /// [`Config`] at construction so the hot paths take no second borrow.
    block_words: u32,
    mem_size_shift: u32,
    mem_size_mask: u32,
    /// `ch_tx_conf0` as latched at the last `conf_update`.
    latched: u32,
    /// Strobe bits written since the last `conf_update`.
    pending: u32,
    running: bool,
    /// Running, but with no function clock; resumes on the poll.
    stalled: bool,
    /// Absolute word index into the RAM: the next word to fetch
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
    fn new(ch: usize, cfg: &Config) -> Self {
        Self {
            block_words: cfg.block_words,
            mem_size_shift: cfg.conf_mem_size_shift,
            mem_size_mask: cfg.conf_mem_size_mask,
            latched: cfg.ch_tx_conf0_reset,
            pending: 0,
            running: false,
            stalled: false,
            raddr: cfg.block_words * ch as u32,
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
        self.block_words * ((self.latched >> self.mem_size_shift) & self.mem_size_mask)
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
/// write pointer. Both events go out with exactly the status and clear
/// semantics the TX side already has.
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
    block_words: u32,
    mem_size_shift: u32,
    mem_size_mask: u32,
    apb_hz: u64,
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
    /// Absolute word index into the RAM: the **next word to write**, which is
    /// what `ch_rx_status.mem_waddr_ex` reads back (esp-hal's `hw_offset`,
    /// `rmt/reader.rs`: "the next code the hardware would write").
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
    /// `ch_rx_status.mem_full`, sticky until the next start.
    mem_full: bool,
    /// The cycle the pending `EV_RX_IDLE` is due at.
    idle_due: Cycles,
    /// One note per reception that had no pad routed to it.
    warned_unrouted: bool,
}

impl RxEngine {
    fn new(rxi: usize, cfg: &Config) -> Self {
        Self {
            block_words: cfg.block_words,
            mem_size_shift: cfg.rx_conf0_mem_size_shift,
            mem_size_mask: cfg.rx_conf0_mem_size_mask,
            apb_hz: cfg.apb_hz,
            conf0: cfg.ch_rx_conf0_reset,
            conf1: cfg.ch_rx_conf1_reset,
            pending: 0,
            running: false,
            armed: false,
            waddr: cfg.block_words * cfg.rx_channel(rxi) as u32,
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
        self.block_words * ((self.conf0 >> self.mem_size_shift) & self.mem_size_mask)
    }

    fn wrap(&self) -> bool {
        self.conf1 & RX_CONF1_MEM_RX_WRAP_EN != 0
    }

    /// The filter width in channel ticks, or `None` when the filter is off.
    ///
    /// The register is *"in APB clock periods"* (PAC,
    /// `ch_rx_conf1.RX_FILTER_THRES`) — the filter sits on the pad's side of
    /// the divider, so it does **not** scale with `div_cnt`. The conversion
    /// to ticks is therefore `thres × f_channel / f_apb`.
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
            .div_ceil(u128::from(self.apb_hz) * u128::from(self.div_cnt()));
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

/// One TX engine's observable state — the seam a snapshot-identity test
/// reads, because the engine itself is private.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxState {
    pub raddr: u32,
    pub ticks_since_start: u64,
    pub word_due: Cycles,
    pub anchor_cycle: Cycles,
    pub latched: u32,
    pub running: bool,
    pub stalled: bool,
}

/// The RMT block.
#[derive(Debug)]
pub struct Rmt {
    cfg: &'static Config,
    index: usize,
    regs: RegFile,
    grades: RegGrades,
    ram: Box<[u32]>,
    clock: Box<dyn ClockLine>,
    ch: Vec<TxEngine>,
    rx: Vec<RxEngine>,
    /// Sticky `int_raw` bits, laid out by [`Config::int_bit`].
    sticky: u32,
    poll_armed: bool,
    warned_fifo: bool,
    warned_rx_carrier: Vec<bool>,
    warned_rx_unmodelled: Vec<bool>,
    warned_gap: bool,
    warned_ref_cnt: Vec<bool>,
    warned_late_start: Vec<bool>,
    /// Whether the pulse and word logs are kept. **Off by default**: the
    /// waveform reaches the fabric, and the logs are the word-level oracle a
    /// test compares the decoder against — a 24-frame run holds 305,490
    /// pulses, which a long CLI run has no use for.
    keep_logs: bool,
}

/// A trace note, formatted only when the trace is on.
fn note(cx: &mut BusCx<'_>, f: impl FnOnce() -> String) {
    if cx.trace.is_enabled() {
        let line = f();
        cx.trace.note(&line);
    }
}

/// A little-endian cursor for the state blobs.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_le_bytes(*head))
    }

    fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_le_bytes(*head))
    }
}

impl Rmt {
    /// A block at one chip's numbers, reading one chip's clock.
    pub fn new(cfg: &'static Config, clock: Box<dyn ClockLine>) -> Self {
        let regs = RegFile::new("RMT", cfg.regs_len).with_names(cfg.reg_names);
        Self {
            cfg,
            index: 0,
            regs,
            grades: Self::grades(cfg),
            ram: vec![0u32; cfg.ram_words].into_boxed_slice(),
            clock,
            ch: (0..cfg.tx_channels)
                .map(|c| TxEngine::new(c, cfg))
                .collect(),
            rx: (0..cfg.rx_channels)
                .map(|r| RxEngine::new(r, cfg))
                .collect(),
            sticky: 0,
            poll_armed: false,
            warned_fifo: false,
            warned_rx_carrier: vec![false; cfg.rx_channels],
            warned_rx_unmodelled: vec![false; cfg.rx_channels],
            warned_gap: false,
            warned_ref_cnt: vec![false; cfg.tx_channels + cfg.rx_channels],
            warned_late_start: vec![false; cfg.tx_channels],
            keep_logs: false,
        }
    }

    /// The chip's numbers, for a caller that has the block and wants them.
    pub fn config(&self) -> &'static Config {
        self.cfg
    }

    /// The bus index this block was attached at — what every event id it
    /// schedules is packed with.
    pub fn index(&self) -> usize {
        self.index
    }

    /// `hz`, for a test that wants the decoded function clock.
    pub fn clock_hz(&self) -> Option<u64> {
        self.clock().ok().map(|c| c.hz())
    }

    /// The block's register grades.
    ///
    /// Everything unlisted is `Modeled`, which for this block means
    /// accept-and-remember at the PAC's reset value. Nothing here is
    /// `Measured`: the `rmt-chase` transcripts agree with silicon frame for
    /// frame, but what they measure is the *waveform*, not a register's bit
    /// map, and `validate.toml`'s `pin` entry gives the argument for why that
    /// is not the same claim.
    pub fn grades(cfg: &Config) -> RegGrades {
        let mut g = RegGrades::new()
            .with_grade(cfg.int_raw, RegGrade::Documented)
            .with_grade(cfg.int_st, RegGrade::Documented)
            .with_grade(cfg.int_ena, RegGrade::Documented)
            .with_grade(cfg.int_clr, RegGrade::Documented)
            .with_grade(cfg.sys_conf, RegGrade::Documented);
        for ch in 0..cfg.tx_channels {
            g = g
                .with_grade(cfg.ch_tx_conf0[ch], RegGrade::Documented)
                .with_grade(cfg.ch_tx_status[ch], RegGrade::Documented)
                .with_grade(cfg.ch_tx_lim[ch], RegGrade::Documented);
        }
        for rxi in 0..cfg.rx_channels {
            g = g
                .with_grade(cfg.ch_rx_conf0[rxi], RegGrade::Documented)
                .with_grade(cfg.ch_rx_conf1[rxi], RegGrade::Documented)
                .with_grade(cfg.ch_rx_status[rxi], RegGrade::Documented)
                .with_grade(cfg.ch_rx_lim[rxi], RegGrade::Documented);
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
    /// latched) — what the fabric drives when no pulse is on the wire.
    pub fn idle_level(&self, ch: usize) -> bool {
        self.ch[ch].idle_level()
    }

    /// The RAM as the engine sees it.
    pub fn ram(&self) -> &[u32] {
        &self.ram
    }

    /// The sticky `int_raw` word.
    pub fn sticky(&self) -> u32 {
        self.sticky
    }

    /// Force the sticky word — the unit tests' seam for the read paths, and
    /// nothing on the guest side can reach it.
    pub fn set_sticky(&mut self, value: u32) {
        self.sticky = value;
    }

    /// A register as stored, for a test that wants what a restore kept.
    pub fn stored(&self, off: u32) -> u32 {
        self.regs.stored(off)
    }

    /// One TX engine's state, for a snapshot-identity test.
    pub fn tx_state(&self, ch: usize) -> TxState {
        let e = &self.ch[ch];
        TxState {
            raddr: e.raddr,
            ticks_since_start: e.ticks_since_start,
            word_due: e.word_due,
            anchor_cycle: e.anchor_cycle,
            latched: e.latched,
            running: e.running,
            stalled: e.stalled,
        }
    }

    // ---- clock --------------------------------------------------------------

    fn clock(&self) -> Result<Clock, &'static str> {
        self.clock
            .decode(self.regs.stored(self.cfg.sys_conf), self.cfg.cpu_hz)
    }

    // ---- interrupts ---------------------------------------------------------

    fn raise(&mut self, dir: Dir, kind: IntKind, index: usize) {
        self.sticky |= 1 << (self.cfg.int_bit)(dir, kind, index);
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.sticky & self.regs.stored(self.cfg.int_ena);
        cx.irq.set_level(self.cfg.source, st != 0);
    }

    // ---- the engine ---------------------------------------------------------

    fn tx_lim(&self, ch: usize) -> u32 {
        self.regs.stored(self.cfg.ch_tx_lim[ch]) & self.cfg.tx_lim_mask
    }

    fn cancel_word(&mut self, ch: usize, cx: &mut BusCx<'_>) {
        cx.sched.cancel(event_id(self.index, ch as u16));
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
        for ch in 0..self.cfg.tx_channels {
            let start = self.cfg.block_words * ch as u32;
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
        let latched = self.regs.stored(self.cfg.ch_tx_conf0[ch]);
        let pending = core::mem::take(&mut self.ch[ch].pending);
        self.ch[ch].latched = latched;
        // A `tx_start` written before this `conf_update` armed the
        // slice-boundary fallback; this is the update it was waiting for.
        cx.sched
            .cancel(event_id(self.index, self.cfg.ev_late_start() + ch as u16));
        if pending & CONF_MEM_RD_RST != 0 {
            self.ch[ch].raddr = self.cfg.block_words * ch as u32;
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
        let (ws, ww) = (self.cfg.block_words * ch as u32, e.window_words());
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
        cx.pins.drive(self.cfg.signal_of(ch), idle_level, now);
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
        cx.pins.drive(self.cfg.signal_of(ch), idle_level, at);
        self.raise(Dir::Tx, IntKind::End, ch);
        note(cx, || {
            format!("cyc={at} RMT ch{ch} end words={words} idle={idle}")
        });
        self.update_lines(cx);
    }

    /// The engine has no clock: hold the current word until it comes back.
    fn stall(&mut self, ch: usize, at: Cycles, reason: &'static str, cx: &mut BusCx<'_>) {
        let hint = self.cfg.stall_clock_hint;
        let e = &mut self.ch[ch];
        e.stalled = true;
        if !e.stall_noted {
            e.stall_noted = true;
            note(cx, || {
                format!("cyc={at} RMT ch{ch} stalled: {reason} {hint}")
            });
        }
        if !self.poll_armed {
            self.poll_armed = true;
            cx.sched.schedule_at(
                cx.now.saturating_add(self.cfg.clock_poll_cycles),
                event_id(self.index, self.cfg.ev_clock_poll()),
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
        cx.pins.drive(self.cfg.signal_of(ch), pulse.level, pulse.at);
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

    /// Count a fetched word and, if the logs are on, record it.
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

    /// Fetch the word at `raddr` at cycle `now` (the word's start), emit its
    /// pulses, schedule the word's end, advance the pointer, and apply the
    /// threshold and wrap rules.
    fn fetch(&mut self, ch: usize, now: Cycles, cx: &mut BusCx<'_>) {
        let raddr = self.ch[ch].raddr as usize;
        let word = self.ram[raddr.min(self.ram.len() - 1)];
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
            .schedule_at(word_end, event_id(self.index, ch as u16));

        // Advance: the pointer now names the next word to fetch (§10.2).
        let window_start = self.cfg.block_words * ch as u32;
        let window_words = self.ch[ch].window_words();
        self.ch[ch].raddr += 1;
        let pos = self.ch[ch].raddr - window_start;
        if pos == self.tx_lim(ch) {
            // Position semantics: `tx_lim == window_words` is the wrap.
            self.raise(Dir::Tx, IntKind::Thr, ch);
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
                self.raise(Dir::Tx, IntKind::Err, ch);
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
                cx.pins.drive(self.cfg.signal_of(ch), idle_level, due);
                note(cx, || {
                    format!("cyc={due} RMT ch{ch} stopped after mem_empty")
                });
            }
        }
    }

    // ---- the receiver (M2 P3) -----------------------------------------------

    /// `rx_lim` for receiver `rxi`, in words.
    fn rx_lim(&self, rxi: usize) -> u32 {
        self.regs.stored(self.cfg.ch_rx_lim[rxi]) & self.cfg.rx_lim_mask
    }

    /// The first word of receiver `rxi`'s RAM window.
    fn rx_window_start(&self, rxi: usize) -> u32 {
        self.cfg.block_words * self.cfg.rx_channel(rxi) as u32
    }

    /// `conf_update` on an RX channel: latch both configuration words and act
    /// on the strobes written since the last one, exactly as the TX side's
    /// does (§10.3, and esp-hal's `update()` writes the two bits in the same
    /// place for both directions).
    fn rx_conf_update(&mut self, rxi: usize, cx: &mut BusCx<'_>) {
        let conf0 = self.regs.stored(self.cfg.ch_rx_conf0[rxi]);
        let conf1 = self.regs.stored(self.cfg.ch_rx_conf1[rxi]);
        let pending = core::mem::take(&mut self.rx[rxi].pending);
        self.rx[rxi].conf0 = conf0;
        self.rx[rxi].conf1 = conf1;
        if pending & RX_CONF1_MEM_WR_RST != 0 {
            self.rx[rxi].waddr = self.rx_window_start(rxi);
        }
        // `apb_mem_rst` resets the APB *read* pointer, which the reader in
        // esp-hal tracks in software: accepted.
        let enabled = conf1 & RX_CONF1_RX_EN != 0;
        if !self.cfg.model_rx {
            // The chip's firmware never receives, so the registers answer and
            // nothing is invented. Said out loud, once per channel, rather
            // than silently ignored.
            if enabled && !self.warned_rx_unmodelled[rxi] {
                self.warned_rx_unmodelled[rxi] = true;
                let ch = self.cfg.rx_channel(rxi);
                log::warn!(
                    "RMT ch{ch}: rx_en set, and this chip's receivers are not modelled — the \
                     registers answer and no words are written"
                );
            }
            return;
        }
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
        let ch = self.cfg.rx_channel(rxi);
        if self.rx[rxi].window_words() == 0 {
            note(cx, || {
                format!("cyc={now} RMT ch{ch} rx start refused: mem_size 0 (no window)")
            });
            return;
        }
        let clock = self.clock().ok();
        let signal = self.cfg.rx_signal_of(rxi);
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
        let ws = self.rx_window_start(rxi);
        let e = &self.rx[rxi];
        let (idle, div_cnt, lim) = (e.idle_thres(), e.div_cnt(), self.rx_lim(rxi));
        let ww = e.window_words();
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
            .cancel(event_id(self.index, self.cfg.ev_rx_idle() + rxi as u16));
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
        cx.sched.schedule_at(
            due,
            event_id(self.index, self.cfg.ev_rx_idle() + rxi as u16),
        );
    }

    /// The machine's slice drain hands every pad edge to the block; a running
    /// receiver takes the ones on the pad its input signal reads.
    ///
    /// The mirror of the GPIO block's `observe_edges`, and it reads the same
    /// stream — so a receiver, the pin log and a strip decoder can never
    /// disagree about what was on the wire.
    pub fn observe_edges(&mut self, edges: &[Edge], cx: &mut BusCx<'_>) {
        for rxi in 0..self.cfg.rx_channels {
            if !self.rx[rxi].running {
                continue;
            }
            let Some((pad, invert)) = cx.pins.input_route_of(self.cfg.rx_signal_of(rxi)) else {
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
                        let ch = self.cfg.rx_channel(rxi);
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
        let start = self.rx_window_start(rxi);
        let window = self.rx[rxi].window_words();
        let ch = self.cfg.rx_channel(rxi);
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
            self.raise(Dir::Rx, IntKind::Thr, rxi);
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
                self.raise(Dir::Rx, IntKind::Err, rxi);
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
        let ch = self.cfg.rx_channel(rxi);
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
        self.raise(Dir::Rx, IntKind::End, rxi);
        note(cx, || {
            format!(
                "cyc={at} RMT ch{ch} rx end words={words} idle_thres={}",
                idle - 1
            )
        });
        self.update_lines(cx);
    }

    // ---- observation, the receiving half ------------------------------------

    /// Whether receiver `rxi` is armed or receiving.
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
        if off >= self.cfg.ram_offset {
            let i = ((off - self.cfg.ram_offset) >> 2) as usize;
            return self.ram.get(i).copied().unwrap_or(0);
        }
        if off >= self.cfg.regs_len {
            return 0;
        }
        for ch in 0..self.cfg.tx_channels {
            if off == self.cfg.ch_tx_status[ch] {
                let e = &self.ch[ch];
                let mut v = e.raddr & self.cfg.tx_status_raddr_mask;
                if e.running {
                    v |= 1 << self.cfg.tx_status_state_shift;
                }
                if e.mem_empty {
                    v |= self.cfg.tx_status_mem_empty;
                }
                return v;
            }
        }
        for rxi in 0..self.cfg.rx_channels {
            if off == self.cfg.ch_rx_status[rxi] {
                let e = &self.rx[rxi];
                // `mem_waddr_ex` is absolute, the way the TX side's
                // `mem_raddr_ex` is: esp-hal's `hw_offset` subtracts the
                // channel's own window start from it.
                let mut v = e.waddr & self.cfg.rx_status_waddr_mask;
                // `apb_mem_raddr_ex` is the *APB* side's read pointer, which
                // the driver tracks in software and never reads back
                // (esp-hal's `RmtReader` keeps its own `offset`). Modeled as
                // the window start.
                v |= (self.rx_window_start(rxi) & self.cfg.rx_status_waddr_mask)
                    << self.cfg.rx_status_apb_raddr_shift;
                if e.running {
                    v |= 1 << self.cfg.rx_status_state_shift;
                }
                if e.mem_full {
                    v |= self.cfg.rx_status_mem_full;
                }
                return v;
            }
        }
        if off == self.cfg.int_raw {
            return self.sticky;
        }
        if off == self.cfg.int_st {
            return self.sticky & self.regs.stored(self.cfg.int_ena);
        }
        if off == self.cfg.int_clr {
            return 0;
        }
        self.regs.effective(off)
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        if off >= self.cfg.ram_offset {
            let i = ((off - self.cfg.ram_offset) >> 2) as usize;
            if i < self.ram.len() {
                // Live: the engine reads this on its next fetch, refilled
                // or not.
                self.ram[i] = value;
                // …and it is the last word of a refill until another one
                // lands, which is how the fill half of the measurement is
                // read off the guest's own writes.
                self.refill_wrote(i as u32);
            }
            return;
        }
        if off >= self.cfg.regs_len {
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
        for ch in 0..self.cfg.tx_channels {
            if off == self.cfg.ch_tx_conf0[ch] {
                let strobes = value & (CONF_TX_START | CONF_MEM_RD_RST | CONF_APB_MEM_RST);
                self.ch[ch].pending |= strobes;
                self.regs.poke(off, value & !CONF_PULSES);
                if value & CONF_CONF_UPDATE != 0 {
                    self.conf_update(ch, cx);
                } else if strobes & CONF_TX_START != 0 {
                    // §10.3: act on it at the slice boundary if no
                    // `conf_update` follows.
                    cx.sched.schedule_at(
                        cx.now,
                        event_id(self.index, self.cfg.ev_late_start() + ch as u16),
                    );
                }
                return;
            }
            if off == self.cfg.ch_tx_lim[ch] {
                // Immediate: the ISR rewrites it mid-frame.
                self.regs.poke(off, value);
                // And this write is the ISR arriving: the entry delay ends
                // here. `refill` flips `tx_lim` first, before it plants the
                // guard or writes a single word, so this is the earliest
                // moment the handler is observably present.
                self.refill_answered(ch);
                return;
            }
            if off == self.cfg.ch_tx_status[ch] {
                return;
            }
        }
        for rxi in 0..self.cfg.rx_channels {
            if off == self.cfg.ch_rx_conf0[rxi] {
                // No strobes here: `div_cnt`, `idle_thres`, `mem_size` and
                // the carrier bits all take effect at the next
                // `conf_update`, which is where esp-hal writes them from.
                self.regs.poke(off, value);
                return;
            }
            if off == self.cfg.ch_rx_conf1[rxi] {
                let strobes = value & (RX_CONF1_MEM_WR_RST | RX_CONF1_APB_MEM_RST);
                self.rx[rxi].pending |= strobes;
                self.regs.poke(off, value & !RX_CONF1_PULSES);
                if value & RX_CONF1_CONF_UPDATE != 0 {
                    self.rx_conf_update(rxi, cx);
                }
                return;
            }
            if off == self.cfg.ch_rx_lim[rxi] {
                // Immediate, like `ch_tx_lim`: nothing in the PAC gates it on
                // `conf_update` and esp-hal writes it before the update that
                // starts the reception.
                self.regs.poke(off, value);
                return;
            }
            if off == self.cfg.ch_rx_carrier_rm[rxi] {
                self.regs.poke(off, value);
                if self.rx[rxi].conf0 & RX_CONF0_CARRIER_EN != 0 && !self.warned_rx_carrier[rxi] {
                    self.warned_rx_carrier[rxi] = true;
                    let ch = self.cfg.rx_channel(rxi);
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
        if off == self.cfg.int_ena {
            self.regs.poke(self.cfg.int_ena, value & self.cfg.int_mask);
            self.update_lines(cx);
            return;
        }
        if off == self.cfg.int_clr {
            self.sticky &= !value;
            self.update_lines(cx);
            return;
        }
        if off == self.cfg.int_raw || off == self.cfg.int_st {
            return;
        }
        if off == self.cfg.ref_cnt_rst {
            self.regs.poke(off, value);
            for bit in 0..self.warned_ref_cnt.len() {
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
            return;
        }
        if off == self.cfg.sys_conf {
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
            return;
        }
        if off < self.cfg.ch_data_end {
            if !self.warned_fifo {
                self.warned_fifo = true;
                let now = cx.now;
                note(cx, || {
                    format!(
                        "cyc={now} RMT ch{}data write: the APB FIFO is not modelled (direct RAM access only)",
                        off >> 2
                    )
                });
            }
            return;
        }
        self.regs.poke(off, value);
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
        let tx = self.cfg.tx_channels as u16;
        if local < tx {
            self.word_ended(usize::from(local), cx);
            return;
        }
        if local < self.cfg.ev_late_start() + tx {
            let ch = usize::from(local - self.cfg.ev_late_start());
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
            return;
        }
        if local == self.cfg.ev_clock_poll() {
            self.poll_armed = false;
            let stalled: Vec<usize> = (0..self.cfg.tx_channels)
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
                    cx.now.saturating_add(self.cfg.clock_poll_cycles),
                    event_id(self.index, self.cfg.ev_clock_poll()),
                );
            }
            return;
        }
        let rx_idle = self.cfg.ev_rx_idle();
        if (rx_idle..rx_idle + self.cfg.rx_channels as u16).contains(&local) {
            let rxi = usize::from(local - rx_idle);
            if self.rx[rxi].running && !self.rx[rxi].armed {
                let due = self.rx[rxi].idle_due;
                self.rx_end(rxi, due, cx);
            }
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.cfg.reg_names.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        Some(self.grades.grade(off))
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.cfg.regs_len as usize + self.ram.len() * 4 + 256);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.sticky.to_le_bytes());
        let flags = u32::from(self.poll_armed)
            | (u32::from(self.warned_fifo) << 1)
            | (u32::from(self.warned_gap) << 2)
            | (u32::from(self.keep_logs) << 3);
        out.extend_from_slice(&flags.to_le_bytes());
        // The per-channel "said it once" flags, as bitmaps rather than as
        // fixed bit positions: the channel counts are the chip's.
        for bits in [
            bitmap(&self.warned_ref_cnt),
            bitmap(&self.warned_late_start),
            bitmap(&self.warned_rx_carrier),
            bitmap(&self.warned_rx_unmodelled),
        ] {
            out.extend_from_slice(&bits.to_le_bytes());
        }
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
        let (Some(ref_cnt), Some(late_start), Some(rx_carrier), Some(rx_unmodelled)) =
            (r.u64(), r.u64(), r.u64(), r.u64())
        else {
            log::warn!("RMT: load_state blob too short, ignored");
            return;
        };
        let mut ram = vec![0u32; self.ram.len()];
        for w in ram.iter_mut() {
            let Some(v) = r.u32() else {
                log::warn!("RMT: load_state blob too short, ignored");
                return;
            };
            *w = v;
        }
        let mut engines = Vec::with_capacity(self.cfg.tx_channels);
        for ch in 0..self.cfg.tx_channels {
            let mut e = TxEngine::new(ch, self.cfg);
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
                cpu_hz: self.cfg.cpu_hz,
            });
            e.word_due = word_due;
            e.frame_words = frame_words;
            e.frames_ended = frames_ended as usize;
            e.pulses = pulses;
            e.words = words;
            engines.push(e);
        }
        let mut receivers = Vec::with_capacity(self.cfg.rx_channels);
        for rxi in 0..self.cfg.rx_channels {
            let mut e = RxEngine::new(rxi, self.cfg);
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
                cpu_hz: self.cfg.cpu_hz,
            });
            e.run_start = run_start;
            e.since_thr = since_thr;
            e.frame_words = frame_words;
            e.words_written = words_written;
            e.idle_due = idle_due;
            receivers.push(e);
        }
        self.index = index as usize;
        self.sticky = sticky;
        self.poll_armed = flags & 1 != 0;
        self.warned_fifo = flags & 2 != 0;
        self.warned_gap = flags & 4 != 0;
        self.keep_logs = flags & 8 != 0;
        unbitmap(ref_cnt, &mut self.warned_ref_cnt);
        unbitmap(late_start, &mut self.warned_late_start);
        unbitmap(rx_carrier, &mut self.warned_rx_carrier);
        unbitmap(rx_unmodelled, &mut self.warned_rx_unmodelled);
        self.ram = ram.into_boxed_slice();
        self.ch = engines;
        self.rx = receivers;
        self.regs.load_state(r.0);
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }

    /// The machine-side seam, for one thing only: turning the pulse and word
    /// logs on at build time. Nothing on the guest side can reach it.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

fn bitmap(flags: &[bool]) -> u64 {
    flags
        .iter()
        .enumerate()
        .filter(|(_, set)| **set)
        .fold(0u64, |acc, (i, _)| acc | (1u64 << i))
}

fn unbitmap(bits: u64, flags: &mut [bool]) {
    for (i, slot) in flags.iter_mut().enumerate() {
        *slot = bits & (1u64 << i) != 0;
    }
}
