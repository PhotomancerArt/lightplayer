//! `RMT` at `0x6000_6000` — the remote-control transceiver as the WS281x
//! path drives it: the PAC register file, the 192-word RAM at `+0x400`, and
//! two TX engines on the scheduler that consume words at the configured
//! clock and raise `tx_end` / `tx_thr_event` / `tx_err` on source 49.
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
//! # What is not here
//!
//! RX channels 2/3 are accept-and-remember; a write that sets `rx_en` says so
//! once. The APB FIFO (`ch*data`, `sys_conf.apb_fifo_mask = 0`) is not
//! modelled — both drivers use direct RAM access. Where the waveform goes
//! (GPIO18 through `func_out_sel_cfg`) is P2's signal fabric; until then the
//! per-channel **pulse log** and **fetched-word log** are the observation,
//! read by the machine's `rmt_pulses` / `rmt_words` / `rmt_frames_ended`.

use lp_emu_core::sched::{Cycles, EventId};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, SignalId, Width, event_id, event_local};

use super::pcr::RmtClockLine;
use super::systimer::Reader;
use crate::memmap;
use crate::regs::output_signals::RMT_SIG_0;
use crate::regs::{self, source};

// Register offsets (`regs::RMT`).
const CH_DATA_END: u32 = 0x010;
const CH_TX_CONF0: [u32; 2] = [0x010, 0x014];
const CH2_RX_CONF1: u32 = 0x01c;
const CH3_RX_CONF1: u32 = 0x024;
const CH_TX_STATUS: [u32; 2] = [0x028, 0x02c];
const INT_RAW: u32 = 0x038;
const INT_ST: u32 = 0x03c;
const INT_ENA: u32 = 0x040;
const INT_CLR: u32 = 0x044;
const CH_TX_LIM: [u32; 2] = [0x058, 0x05c];
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
/// TX channels. Channels 2 and 3 receive only.
pub const TX_CHANNELS: usize = 2;

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
/// PAC reset: `tx_lim 128`.
const CH_TX_LIM_RESET: u32 = 0x80;
const TX_LIM_MASK: u32 = 0x1ff;
/// PAC reset: `mem_clk_force_on`, `clk_en`.
const SYS_CONF_RESET: u32 = 0x0500_0010;
const SYS_CONF_APB_FIFO_MASK: u32 = 1 << 0;

// `int_*` bits: TX and RX interleave in pairs (`rmt/int_raw.rs`).
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

/// The RMT block.
#[derive(Debug)]
pub struct Rmt {
    index: usize,
    regs: RegFile,
    ram: Box<[u32; RAM_WORDS]>,
    clock: RmtClockLine,
    ch: [TxEngine; TX_CHANNELS],
    /// Sticky `int_raw` bits (TX only: 0..1, 4..5, 8..9).
    sticky: u32,
    poll_armed: bool,
    warned_rx: bool,
    warned_fifo: bool,
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
        let regs = RegFile::new("RMT", REGS_LEN)
            .with_names(regs::RMT)
            .with_reset(CH_TX_CONF0[0], CH_TX_CONF0_RESET)
            .with_reset(CH_TX_CONF0[1], CH_TX_CONF0_RESET)
            .with_reset(CH_TX_LIM[0], CH_TX_LIM_RESET)
            .with_reset(CH_TX_LIM[1], CH_TX_LIM_RESET)
            .with_reset(SYS_CONF, SYS_CONF_RESET);
        Self {
            index: 0,
            regs,
            ram: Box::new([0; RAM_WORDS]),
            clock,
            ch: [TxEngine::new(0), TxEngine::new(1)],
            sticky: 0,
            poll_armed: false,
            warned_rx: false,
            warned_fifo: false,
            warned_gap: false,
            warned_ref_cnt: [false; 4],
            warned_late_start: [false; TX_CHANNELS],
            keep_logs: false,
        }
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
            CH2_RX_CONF1 | CH3_RX_CONF1 => {
                self.regs.poke(off, value);
                if value & 1 != 0 && !self.warned_rx {
                    self.warned_rx = true;
                    let ch = if off == CH2_RX_CONF1 { 2 } else { 3 };
                    let now = cx.now;
                    note(cx, || {
                        format!(
                            "cyc={now} RMT ch{ch} rx_en set: RX channels are not modelled (M5 models TX only)"
                        )
                    });
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

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(REGS_LEN as usize + RAM_WORDS * 4 + 256);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.sticky.to_le_bytes());
        let flags = u32::from(self.poll_armed)
            | (u32::from(self.warned_rx) << 1)
            | (u32::from(self.warned_fifo) << 2)
            | (u32::from(self.warned_gap) << 3)
            | (u32::from(self.warned_ref_cnt[0]) << 4)
            | (u32::from(self.warned_ref_cnt[1]) << 5)
            | (u32::from(self.warned_ref_cnt[2]) << 6)
            | (u32::from(self.warned_ref_cnt[3]) << 7)
            | (u32::from(self.warned_late_start[0]) << 8)
            | (u32::from(self.warned_late_start[1]) << 9)
            | (u32::from(self.keep_logs) << 10);
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
        self.warned_rx = flags & 2 != 0;
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
        // (g) rx_en.
        let mut sb2 = Sandbox::new();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb2.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        sb2.write(&mut r, CH2_RX_CONF1, 1);
        sb2.write(&mut r, CH3_RX_CONF1, 1);
        let rx: Vec<String> = buf
            .lines()
            .into_iter()
            .filter(|l| l.contains("rx_en"))
            .collect();
        assert_eq!(rx.len(), 1, "{rx:?}");
        assert!(rx[0].contains("RMT ch2 rx_en set: RX channels are not modelled"));
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
}
