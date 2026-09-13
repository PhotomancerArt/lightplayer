//! `RMT` at `0x6000_6000` — the C6's numbers for the shared view, and the
//! C6's clock.
//!
//! The behaviour is [`lp_emu_esp_common::ip::rmt`]: the PAC register file,
//! the 192-word RAM at `+0x400`, two TX engines on the scheduler that consume
//! words at the configured clock and raise `tx_end` / `tx_thr_event` /
//! `tx_err` on source 49, and two RX engines that sample a routed input
//! signal back into the same RAM. **Xtensa M6 P07 moved that file** into the
//! shared crate and parameterised it (ruling D4/DD64), because the S3's RMT
//! *is* this IP — every register name, every bitfield — while the classic's
//! is a genuinely different block. Nothing about what this chip does changed:
//! the move is guarded by the oracle sweep and every committed C6 transcript.
//!
//! What is left here is the chip: [`CONFIG`], the table of offsets, field
//! positions, channel counts and signal numbers, plus [`Ch6Clock`] — the C6
//! reads its function clock from **PCR** (`rmt_sclk_conf`, `rmt_conf.clk_en`;
//! this chip's `sys_conf` has no `sclk_*` fields), fed in on a
//! [`RmtClockLine`] the way the UART clocks are, which is exactly the
//! behaviour the S3 does not share.
//!
//! Register facts are the esp32c6 PAC 0.23.2 `rmt` block (offsets in
//! [`regs::RMT`]; bit positions cited per constant below) and the two drivers
//! that write it: esp-hal 1.1.1's `Rmt::new` / `configure_tx` / `with_pin`
//! and this project's `lp-fw/fw-esp32c6/src/output/rmt/c6_rmt.rs` backend for
//! `lp-ws281x` (M5 discovery §1–§2).
//!
//! # Register grades
//!
//! | grade | registers |
//! |---|---|
//! | `measured` | none. The `rmt-chase` transcripts agree with silicon frame for frame, but a waveform is not a register's bit map — `validate.toml`'s `pin` entry has the argument. |
//! | `documented` | `ch0_tx_conf0`, `ch1_tx_conf0`, `ch0_tx_status`, `ch1_tx_status`, `ch0_tx_lim`, `ch1_tx_lim`, `ch2_rx_conf0`, `ch3_rx_conf0`, `ch2_rx_conf1`, `ch3_rx_conf1`, `ch0_rx_status`, `ch1_rx_status` (channels 2 and 3 — the PAC's own numbering), `ch0_rx_lim`, `ch1_rx_lim`, `int_raw`, `int_st`, `int_ena`, `int_clr`, `sys_conf`. The PAC's bit map is the source and every field is that bit map read out loud. |
//! | `modeled` | `ch0data`…`ch3data` (the APB FIFO, not modelled), `ch0carrier_duty`, `ch1carrier_duty`, `ch0_rx_carrier_rm`, `ch1_rx_carrier_rm` (carrier modulation and demodulation, not modelled), `tx_sim`, `ref_cnt_rst`, `date`. Accept-and-remember, at the PAC's reset value. |

use lp_emu_esp_common::SignalId;
use lp_emu_esp_common::ip::rmt::{ClockLine, Config, Dir, IntKind};
// The bit positions the two chips share live in the shared file; they are
// re-exported here so a reader of this block — and this file's own tests —
// finds the whole register map in one place.
pub use lp_emu_esp_common::ip::rmt::{
    CONF_APB_MEM_RST, CONF_CONF_UPDATE, CONF_DIV_CNT_SHIFT, CONF_IDLE_OUT_LV, CONF_MEM_RD_RST,
    CONF_MEM_TX_WRAP_EN, CONF_PULSES, CONF_TX_START, CONF_TX_STOP, Clock, LAG_BUCKETS,
    PULSE_LOG_CAP, Pulse, RX_CONF0_CARRIER_EN, RX_CONF0_DIV_CNT_SHIFT, RX_CONF0_IDLE_THRES_MASK,
    RX_CONF0_IDLE_THRES_SHIFT, RX_CONF1_APB_MEM_RST, RX_CONF1_CONF_UPDATE, RX_CONF1_FILTER_EN,
    RX_CONF1_FILTER_THRES_MASK, RX_CONF1_FILTER_THRES_SHIFT, RX_CONF1_MEM_OWNER,
    RX_CONF1_MEM_RX_WRAP_EN, RX_CONF1_MEM_WR_RST, RX_CONF1_PULSES, RX_CONF1_RX_EN, RefillStats,
    Rmt, SYS_CONF_APB_FIFO_MASK, TxState, WORD_LOG_CAP, lag_bucket,
};

use super::pcr::RmtClockLine;
use crate::memmap;
use crate::regs;
use crate::regs::output_signals::{RMT_RX_SIG_0, RMT_SIG_0};
use crate::regs::source;

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
/// (`esp-metadata-generated-0.4.0`, `c6_rmt.rs::RAM_OFFSET`). ⚠️ **The S3's
/// is `+0x800`** — carrying this constant across would write into the tail of
/// the register file.
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

// `ch_tx_conf0`'s shared bits (`rmt/ch_tx_conf0.rs`) are re-exported above;
// what is here is the field this chip does not share.
/// `mem_size`, bits **16:18** — four blocks addressable. The S3's is 16:19.
const CONF_MEM_SIZE_SHIFT: u32 = 16;
const CONF_MEM_SIZE_MASK: u32 = 0x7;
/// PAC reset: `div_cnt 2`, `mem_size 1`, the carrier bits set.
const CH_TX_CONF0_RESET: u32 = 0x0071_0200;
const TX_LIM_MASK: u32 = 0x1ff;

// `ch_rx_conf0` bits (`rmt/ch_rx_conf0.rs`).
/// `mem_size`, bits **23:25**. The S3's is 24:27, which is why the two
/// chips' reset values differ at the same `mem_size = 1`.
const RX_CONF0_MEM_SIZE_SHIFT: u32 = 23;
const RX_CONF0_MEM_SIZE_MASK: u32 = 0x7;
/// PAC reset: `div_cnt 2`, `idle_thres 0x7fff`, `mem_size 1`, carrier bits set.
const CH_RX_CONF0_RESET: u32 = 0x30ff_ff02;

/// PAC reset for `ch_rx_conf1`: `mem_owner 1`, `rx_filter_thres 15`,
/// everything else clear.
const CH_RX_CONF1_RESET: u32 = 0x0000_01e8;

/// `ch_rx_lim.rx_lim`, bits 0:8.
const RX_LIM_MASK: u32 = 0x1ff;

// `ch_tx_status` fields (`rmt/ch_tx_status.rs`): `mem_raddr_ex` 0:8, `state`
// 9:11, `mem_empty` 22. ⚠️ The S3's are 0:9, 22:24 and 25.
const STATUS_RADDR_MASK: u32 = 0x1ff;
const STATUS_STATE_SHIFT: u32 = 9;
const STATUS_MEM_EMPTY: u32 = 1 << 22;

// `ch_rx_status` fields (`rmt/ch_rx_status.rs`): `mem_waddr_ex` 0:8,
// `apb_mem_raddr` 12:20, `state` 22:24, `mem_full` 26.
const RX_STATUS_WADDR_MASK: u32 = 0x1ff;
const RX_STATUS_APB_RADDR_SHIFT: u32 = 12;
const RX_STATUS_STATE_SHIFT: u32 = 22;
const RX_STATUS_MEM_FULL: u32 = 1 << 26;

// `int_*` bits: TX and RX interleave in pairs (`rmt/int_raw.rs`) — bit 0/1
// are `ch0/ch1_tx_end`, bit 2/3 `ch2/ch3_rx_end`, bits 4..7 the four `err`
// bits, bits 8/9 `ch0/ch1_tx_thr_event` and 10/11 `ch2/ch3_rx_thr_event`. So
// the three shifts below are the same for both directions and the *channel
// number* selects the bit: an `end` on receiver 0 is bit `0 + 2`.
//
// ⚠️ This is the chip's own bit list and **not** a shape the S3 shares: the
// S3 groups by event, so the two chips supply two functions rather than two
// shift constants. See `Config::int_bit`.
const INT_TX_END_SHIFT: u32 = 0;
const INT_TX_ERR_SHIFT: u32 = 4;
const INT_TX_THR_SHIFT: u32 = 8;
const INT_MASK: u32 = 0x3fff;

/// Which `int_raw` bit one interrupt of one channel is, on **this** chip.
///
/// TX and RX share a nibble here because the C6's RX channels *are* channels
/// 2 and 3: the bit is `<event shift> + <absolute channel number>`.
fn int_bit(dir: Dir, kind: IntKind, index: usize) -> u32 {
    let shift = match kind {
        IntKind::End => INT_TX_END_SHIFT,
        IntKind::Err => INT_TX_ERR_SHIFT,
        IntKind::Thr => INT_TX_THR_SHIFT,
    };
    let channel = match dir {
        Dir::Tx => index,
        Dir::Rx => RX_CH_BASE + index,
    };
    shift + channel as u32
}

// `PCR.rmt_sclk_conf` fields (`pcr/rmt_sclk_conf.rs`) and `rmt_conf`.
const SCLK_DIV_B_MASK: u32 = 0x3f;
const SCLK_DIV_A_SHIFT: u32 = 6;
const SCLK_DIV_A_MASK: u32 = 0x3f;
const SCLK_DIV_NUM_SHIFT: u32 = 12;
const SCLK_DIV_NUM_MASK: u32 = 0xff;
const SCLK_SEL_SHIFT: u32 = 20;
const SCLK_EN: u32 = 1 << 22;
const PCR_RMT_CLK_EN: u32 = 1 << 0;

/// `EV_WORD`: the shared view lays its events out from the channel counts,
/// and channel `ch`'s "the word's pulses ended" event is local id `ch`. Named
/// here for the snapshot test that re-arms a restored engine.
pub const EV_WORD: u16 = 0;

/// How often a stalled engine re-checks the clock: 1 ms of guest time.
/// Nothing in the firmware ever gates the clock, so this is a diagnostic
/// path; deterministic either way.
pub const CLOCK_POLL_CYCLES: u64 = 1_000 * memmap::CYCLES_PER_US;

/// The APB clock the RX filter's threshold is counted in.
///
/// **Documented.** The ESP32-C6 has no configurable APB divider: the TRM's
/// clock tree fixes `APB_CLK` at 80 MHz from the PLL, and esp-hal carries the
/// same number (`esp-metadata-generated-0.4.0`, the C6's `apb_clock`). It
/// matters only when `rx_filter_en` is set, which the `rmt-rx` payload leaves
/// off — the filter is exercised by this file's own tests instead.
pub const APB_HZ: u64 = 80_000_000;

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

/// This chip's numbers for the shared view.
pub static CONFIG: Config = Config {
    reg_names: regs::RMT,
    source: source::RMT,
    cpu_hz: memmap::CPU_HZ,
    apb_hz: APB_HZ,
    clock_poll_cycles: CLOCK_POLL_CYCLES,
    stall_clock_hint: "(the engine resumes when PCR gives it a clock)",

    regs_len: REGS_LEN,
    ram_offset: RAM_OFFSET,
    ram_words: RAM_WORDS,
    block_words: BLOCK_WORDS,
    len: LEN,

    tx_channels: TX_CHANNELS,
    rx_channels: RX_CHANNELS,
    rx_ch_base: RX_CH_BASE,
    // The `rmt-rx` payload receives, so this chip's receivers are engines.
    model_rx: true,

    ch_data_end: CH_DATA_END,
    ch_tx_conf0: &CH_TX_CONF0,
    ch_rx_conf0: &CH_RX_CONF0,
    ch_rx_conf1: &CH_RX_CONF1,
    ch_tx_status: &CH_TX_STATUS,
    ch_rx_status: &CH_RX_STATUS,
    ch_rx_carrier_rm: &CH_RX_CARRIER_RM,
    ch_tx_lim: &CH_TX_LIM,
    ch_rx_lim: &CH_RX_LIM,
    int_raw: INT_RAW,
    int_st: INT_ST,
    int_ena: INT_ENA,
    int_clr: INT_CLR,
    sys_conf: SYS_CONF,
    ref_cnt_rst: REF_CNT_RST,

    conf_mem_size_shift: CONF_MEM_SIZE_SHIFT,
    conf_mem_size_mask: CONF_MEM_SIZE_MASK,
    rx_conf0_mem_size_shift: RX_CONF0_MEM_SIZE_SHIFT,
    rx_conf0_mem_size_mask: RX_CONF0_MEM_SIZE_MASK,
    ch_tx_conf0_reset: CH_TX_CONF0_RESET,
    ch_rx_conf0_reset: CH_RX_CONF0_RESET,
    ch_rx_conf1_reset: CH_RX_CONF1_RESET,

    tx_lim_mask: TX_LIM_MASK,
    rx_lim_mask: RX_LIM_MASK,

    tx_status_raddr_mask: STATUS_RADDR_MASK,
    tx_status_state_shift: STATUS_STATE_SHIFT,
    tx_status_mem_empty: STATUS_MEM_EMPTY,
    rx_status_waddr_mask: RX_STATUS_WADDR_MASK,
    rx_status_apb_raddr_shift: RX_STATUS_APB_RADDR_SHIFT,
    rx_status_state_shift: RX_STATUS_STATE_SHIFT,
    rx_status_mem_full: RX_STATUS_MEM_FULL,

    int_mask: INT_MASK,
    int_bit,

    rmt_sig_0: RMT_SIG_0,
    rmt_rx_sig_0: RMT_RX_SIG_0,
};

/// The C6's clock: **PCR's**, not the block's own.
///
/// `rmt_conf.clk_en` and `rmt_sclk_conf` live in a different peripheral, so
/// they arrive on the shared cell [`RmtClockLine`] the way the UART clocks
/// do, and the block's `sys_conf` — which on this chip has five bits and no
/// `sclk_*` field at all — is ignored.
#[derive(Debug)]
pub struct Ch6Clock(RmtClockLine);

impl ClockLine for Ch6Clock {
    fn decode(&self, _sys_conf: u32, cpu_hz: u64) -> Result<Clock, &'static str> {
        decode_pcr(self.0.conf.get(), self.0.sclk.get(), cpu_hz)
    }
}

/// Decode the two PCR words, or say why there is no clock.
pub fn decode_pcr(conf: u32, sclk: u32, cpu_hz: u64) -> Result<Clock, &'static str> {
    {
        if conf & PCR_RMT_CLK_EN == 0 {
            return Err("PCR rmt_conf.clk_en = 0");
        }
        if sclk & SCLK_EN == 0 {
            return Err("PCR rmt_sclk_conf.sclk_en = 0");
        }
        let src_hz = match (sclk >> SCLK_SEL_SHIFT) & 3 {
            1 => 80_000_000,
            3 => super::XTAL_HZ,
            // FOSC's calibrated frequency is not known here (M5 discovery
            // §10.6); refuse rather than guess.
            2 => return Err("PCR rmt_sclk_conf.sclk_sel = 2 (FOSC): not modelled"),
            _ => return Err("PCR rmt_sclk_conf.sclk_sel = 0: no clock"),
        };
        Ok(Clock {
            src_hz,
            div_num: (sclk >> SCLK_DIV_NUM_SHIFT) & SCLK_DIV_NUM_MASK,
            div_a: (sclk >> SCLK_DIV_A_SHIFT) & SCLK_DIV_A_MASK,
            div_b: sclk & SCLK_DIV_B_MASK,
            cpu_hz,
        })
    }
}

/// The C6's RMT block: the shared view at [`CONFIG`], on PCR's clock.
pub fn new(clock: RmtClockLine) -> Rmt {
    Rmt::new(&CONFIG, Box::new(Ch6Clock(clock)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_core::sched::Cycles;
    use lp_emu_esp_common::{Peripheral, Sandbox, Width, event_id};

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
        let mut r = new(clock.clone());
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
        r.set_sticky(THR0 | END0);
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
        let c = decode_pcr(1, 0x0050_0000 | (1 << SCLK_DIV_A_SHIFT) | 2, memmap::CPU_HZ).unwrap();
        assert_eq!(c.hz(), 53_333_333);
        assert_eq!(c.cycles_for(100, 1), 300);
        assert_eq!(c.cycles_for(1, 1), 3);
        let c = decode_pcr(1, 0x0050_0000, memmap::CPU_HZ).unwrap();
        assert_eq!(c.hz(), 80_000_000);
        assert_eq!(c.cycles_for(12_000 * 2, 1), 48_000, "the latch word");
        let c = decode_pcr(1, 0x0070_0000, memmap::CPU_HZ).unwrap();
        assert_eq!(c.hz(), 40_000_000);
        assert_eq!(
            decode_pcr(0, 0x0050_0000, memmap::CPU_HZ),
            Err("PCR rmt_conf.clk_en = 0")
        );
        assert!(decode_pcr(1, 0x0060_0000, memmap::CPU_HZ).is_err(), "FOSC refused");
        assert!(decode_pcr(1, 0x0040_0000, memmap::CPU_HZ).is_err(), "sel 0");
        assert!(decode_pcr(1, 0x0010_0000, memmap::CPU_HZ).is_err(), "sclk_en 0");
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
        let mut other = new(clock.clone());
        other.load_state(&blob);
        assert_eq!(other.index(), 7);
        assert!(other.is_running(0));
        assert_eq!(other.tx_state(0).raddr, 6);
        assert_eq!(other.tx_state(0).ticks_since_start, 600);
        assert_eq!(other.tx_state(0).word_due, 500 + 6 * WORD_CYCLES);
        assert_eq!(other.tx_state(0).anchor_cycle, 500);
        assert_eq!(other.tx_state(0).latched, r.tx_state(0).latched);
        assert_eq!(other.ram()[47], ONE);
        assert_eq!(other.words(0), r.words(0));
        assert_eq!(other.pulses(0), r.pulses(0));
        assert_eq!(other.stored(CH_TX_LIM[0]), 24);
        // The restored machine re-schedules from the saved due cycle (the
        // scheduler's own queue is restored by the machine); run it on.
        let mut sb2 = Sandbox::new();
        sb2.sched
            .schedule_at(other.tx_state(0).word_due, event_id(7, EV_WORD));
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
        assert!(r.tx_state(0).stalled);
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
        assert!(!r.tx_state(0).stalled);
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
        assert!(r.tx_state(0).stalled);
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

    /// The pad each TX channel's signal is routed to in the two-channel test.
    const TWO_CH_PADS: [lp_emu_esp_common::PadId; 2] =
        [lp_emu_esp_common::PadId(8), lp_emu_esp_common::PadId(9)];

    /// Start TX on `ch` with one memory block, wrap on, in one write — the
    /// shape `channel_one_has_its_own_window_engine_and_interrupt_bits` uses,
    /// applied to either channel so the two are started symmetrically.
    fn start_ch(sb: &mut Sandbox, r: &mut Rmt, ch: usize) {
        let c = sb.read(r, CH_TX_CONF0[ch]);
        sb.write(
            r,
            CH_TX_CONF0[ch],
            c | CONF_MEM_TX_WRAP_EN | CONF_MEM_RD_RST | CONF_TX_START | CONF_CONF_UPDATE,
        );
    }

    /// Both TX channels sending the same word at the same cycles, with
    /// `first` started first. Returns the pin log as the fabric recorded it.
    fn two_channel_edges(first: usize) -> Vec<lp_emu_esp_common::pins::Edge> {
        let (mut sb, mut r, _) = rig();
        for ch in 0..2 {
            sb.pins.route(
                TWO_CH_PADS[ch],
                lp_emu_esp_common::pins::RouteSource::Signal(signal_of(ch), false),
                false,
                0,
            );
            sb.pins.set_gpio_enable(TWO_CH_PADS[ch], true, 0);
            let conf =
                (1 << CONF_DIV_CNT_SHIFT) | (1 << 6) | (1 << CONF_MEM_SIZE_SHIFT) | (1 << 20);
            sb.write(&mut r, CH_TX_CONF0[ch], conf);
            // Four identical words and a stop word, in each channel's block.
            let base = 48 * ch as u32;
            fill(&mut sb, &mut r, base, base + 4, ZERO);
            sb.write(&mut r, ram_off(base + 4), 0);
            sb.write(&mut r, CH_TX_LIM[ch], 24);
        }
        sb.write(&mut r, INT_ENA, 0x333);
        start_ch(&mut sb, &mut r, first);
        start_ch(&mut sb, &mut r, 1 - first);
        let mut edges = sb.pins.take_edges();
        for step in 1..=6u64 {
            sb.run_to(&mut r, step * WORD_CYCLES);
            edges.extend(sb.pins.take_edges());
        }
        edges
    }

    /// (g) **Two TX channels: the pin log is in call order, and call order is
    /// the scheduler's dispatch order — not `(at, seq)` order over the
    /// edges.** The trap `vision.md` §2 registered and P1d could not test,
    /// because no product image drives two TX channels (`render-basic` and
    /// `render-rocaille` start ch0 only).
    ///
    /// It is worse than "two edges at one cycle tie": `push_pulse` emits
    /// **both halves of a word at the fetch**, each stamped with its own true
    /// cycle, so a second channel's word is appended *behind* the first
    /// channel's already-future edge. The `at` column of the combined log is
    /// therefore not monotone at all — it reads 0, 64, 0, 64, 200, 264, …
    ///
    /// Nothing downstream sorts it: `Machine::drain_pins` writes
    /// `Fabric::take_edges` straight into `--pin-log`. What saves the product
    /// path is that every consumer is **per pad** — each pad's own edges are
    /// strictly increasing, and a `Ws281xDecoder` exists per pad.
    #[test]
    fn two_tx_channels_put_the_pin_log_in_dispatch_order_not_at_order() {
        let a = two_channel_edges(0);
        assert_eq!(
            a,
            two_channel_edges(0),
            "the same two-channel run gives the same pin log twice"
        );

        // Each pad's own edges are strictly increasing in `at` — this is the
        // invariant every consumer actually depends on.
        for pad in TWO_CH_PADS {
            let ats: Vec<Cycles> = a.iter().filter(|e| e.pad == pad).map(|e| e.at).collect();
            assert_eq!(ats.len(), 8, "four words, two halves each, on {pad:?}");
            assert!(
                ats.windows(2).all(|w| w[0] < w[1]),
                "pad {pad:?} is out of order: {ats:?}"
            );
        }

        // The combined stream is not. Both of ch0's first-word edges precede
        // both of ch1's, and ch1's first edge is back-dated to cycle 0.
        let head: Vec<(Cycles, u8, bool)> =
            a.iter().take(4).map(|e| (e.at, e.pad.0, e.level)).collect();
        assert_eq!(
            head,
            vec![(0, 8, true), (64, 8, false), (0, 9, true), (64, 9, false)],
            "a whole word of ch0, then a whole word of ch1 at the same cycles"
        );
        assert!(
            a.windows(2).any(|w| w[0].at > w[1].at),
            "the `at` column is not monotone"
        );

        // Start ch1 first and the wire is identical while the log's order
        // flips: the order is the scheduler's dispatch order, nothing else.
        let b = two_channel_edges(1);
        assert_ne!(a, b, "the log's order follows which channel started first");
        let (mut sa, mut sb_) = (a.clone(), b.clone());
        sa.sort_by_key(|e| (e.at, e.pad.0));
        sb_.sort_by_key(|e| (e.at, e.pad.0));
        assert_eq!(sa, sb_, "…and the wire itself is the same either way");
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

        let mut restored = new(clock);
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
