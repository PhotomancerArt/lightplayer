//! `RMT` at `0x6001_6000` — the S3's numbers for the shared view, and the
//! S3's clock.
//!
//! The behaviour is [`lp_emu_esp_common::ip::rmt`], which is the C6's view
//! moved and parameterised (ruling D4/DD64): the S3's RMT **is** the C6's IP
//! — `ch_tx_conf0`, `ch_rx_conf0/1`, `ch_tx_status`, `ch_rx_status`,
//! `ch_tx_lim`, `ch_rx_lim`, `ch_rx_carrier_rm`, `sys_conf`, `tx_sim` and
//! `ref_cnt_rst` exist on both by name with the same fields in the same
//! order, and none of them exists on the classic (`m6/notes.md` §3.2). What
//! is in this file is the chip: [`CONFIG`] and [`S3Clock`].
//!
//! # The five numbers that pass every register test and fail the first frame
//!
//! Each has a test at the bottom of this file that fails without it.
//!
//! 1. **The interrupt bits are grouped by event here and share a nibble on
//!    the C6.** `chN_tx_end` 0–3, `chN_tx_err` 4–7, `chN_tx_thr_event` 8–11,
//!    `chN_tx_loop` 12–15, `chN_rx_end` 16–19, `chN_rx_err` 20–23,
//!    `chN_rx_thr_event` 24–27 (`esp32s3-0.35.2/src/rmt/int_raw.rs`). The C6's
//!    bits 0–3 are `ch0_tx_end, ch1_tx_end, ch2_rx_end, ch3_rx_end`, because
//!    *its* RX channels are channels 2 and 3. The firmware names the trap
//!    itself: the `ch_tx_*(n)` PAC accessors have the same names on both
//!    chips, *"which is exactly why the difference is easy to miss"*
//!    (`lp-fw/fw-esp32s3/src/output/rmt/s3_rmt.rs:29-34`). So [`int_bit`] is
//!    a **function**, not a shift constant.
//! 2. **`sys_conf` carries the clock divider.** On the C6 it lives in
//!    `PCR.rmt_sclk_conf`; here it is this block's own `sys_conf` — see
//!    [`S3Clock`].
//! 3. **`ch_tx_conf0.mem_size` is bits 16:19**, eight blocks addressable
//!    against the C6's four at 16:18.
//! 4. **`ch_rx_conf0.mem_size` is bits 24:27**, against the C6's 23:25. Not
//!    in the phase brief's list; found by reading both PACs field for field,
//!    and it is why the two chips' reset values differ (`0x317f_ff02` here,
//!    `0x30ff_ff02` there) at the same `mem_size = 1`.
//! 5. **The status words are laid out differently.** `mem_raddr_ex` and
//!    `mem_waddr_ex` are bits **0:9** here (ten bits, absolute over the whole
//!    384-word RAM, `s3_rmt.rs:35-37,111`), `state` is 22:24 and `mem_empty`
//!    is bit 25. The C6's are 0:8, 9:11 and 22, so a `read_pos` taken through
//!    the C6's mask would fold this chip's state bits into the pointer.
//!
//! ⚠️ **The RAM offset is `+0x800`, and two sources disagreed about it.** The
//! metadata says `rmt.ram_start = 1610704896`
//! (`esp-metadata-generated-0.4.0/src/_generated_esp32s3.rs:223-224`).
//! Re-derived here rather than copied: `0x6000_0000` is 1,610,612,736, and
//! `1_610_704_896 − 1_610_612_736 = 92_160 = 0x1_6800`, so `ram_start =
//! 0x6001_6800`; the block's base is `0x6001_6000` ([`memmap::periph::RMT`]),
//! so the offset is `0x800`. The planning pass converted the same decimal to
//! `0x6001_6400` — a mis-conversion — and the firmware's own `+0x800` has a
//! bench probe behind it (`s3_rmt.rs:11-25,95-99`). **The C6's is `+0x400`**,
//! and its driver's module doc says what carrying that constant across would
//! do: *"write into the tail of the register file and transmit whatever the
//! RAM happened to hold."*
//!
//! # RX is accepted, not modelled
//!
//! `CH0..=CH3` transmit and `CH4..=CH7` receive (`s3_rmt.rs:50-52`), and the
//! shipped firmware uses no receiver. So [`Config::model_rx`] is clear: every
//! RX register answers, is remembered and is graded, and `rx_en` set on a
//! channel is a `log::warn!` naming the channel rather than an engine whose
//! output nothing could check. ⚠️ This is the opposite of the C6, whose
//! `rmt_rx` payload does receive — the shared view keeps both behaviours and
//! this chip's table chooses.
//!
//! # Register grades
//!
//! | grade | registers |
//! |---|---|
//! | `measured` | none. A waveform is not a register's bit map, and no S3 silicon has been read at all. |
//! | `documented` | `ch0_tx_conf0`…`ch3_tx_conf0`, `ch0_tx_status`…`ch3_tx_status`, `ch0_tx_lim`…`ch3_tx_lim`, `ch4_rx_conf0`…`ch7_rx_conf0`, `ch4_rx_conf1`…`ch7_rx_conf1`, `ch0_rx_status`…`ch3_rx_status`, `ch0_rx_lim`…`ch3_rx_lim`, `int_raw`, `int_st`, `int_ena`, `int_clr`, `sys_conf`. The PAC's bit map is the source and every field is that bit map read out loud. |
//! | `modeled` | `ch0data`…`ch7data` (the APB FIFO, not modelled), `ch0carrier_duty`…`ch3carrier_duty`, `ch0_rx_carrier_rm`…`ch3_rx_carrier_rm` (carrier modulation and demodulation, not modelled), `tx_sim`, `ref_cnt_rst`, `date`. Accept-and-remember, at the PAC's reset value. |

use lp_emu_esp_common::SignalId;
pub use lp_emu_esp_common::ip::rmt::{
    Clock, LAG_BUCKETS, PULSE_LOG_CAP, Pulse, RefillStats, Rmt, TxState, WORD_LOG_CAP, lag_bucket,
};
use lp_emu_esp_common::ip::rmt::{ClockLine, Config, Dir, IntKind};

use crate::memmap;
use crate::regs;
use crate::regs::output_signals::{RMT_RX_SIG_0, RMT_SIG_0};
use crate::regs::source;

// Register offsets (`regs::RMT`).
//
// As on the C6, the PAC numbers the two halves of the block differently and
// the names below are its names: the *configuration* registers carry the
// absolute channel number (`ch4_rx_conf0` … `ch7_rx_conf1`) while the status,
// limit and carrier registers carry the receiver's index among the receivers
// (`ch0_rx_status` is channel 4's).
const CH_DATA_END: u32 = 0x020;
const CH_TX_CONF0: [u32; TX_CHANNELS] = [0x020, 0x024, 0x028, 0x02c];
const CH_RX_CONF0: [u32; RX_CHANNELS] = [0x030, 0x038, 0x040, 0x048];
const CH_RX_CONF1: [u32; RX_CHANNELS] = [0x034, 0x03c, 0x044, 0x04c];
const CH_TX_STATUS: [u32; TX_CHANNELS] = [0x050, 0x054, 0x058, 0x05c];
const CH_RX_STATUS: [u32; RX_CHANNELS] = [0x060, 0x064, 0x068, 0x06c];
const INT_RAW: u32 = 0x070;
const INT_ST: u32 = 0x074;
const INT_ENA: u32 = 0x078;
const INT_CLR: u32 = 0x07c;
const CH_RX_CARRIER_RM: [u32; RX_CHANNELS] = [0x090, 0x094, 0x098, 0x09c];
const CH_TX_LIM: [u32; TX_CHANNELS] = [0x0a0, 0x0a4, 0x0a8, 0x0ac];
const CH_RX_LIM: [u32; RX_CHANNELS] = [0x0b0, 0x0b4, 0x0b8, 0x0bc];
const SYS_CONF: u32 = 0x0c0;
const REF_CNT_RST: u32 = 0x0c8;

/// The PAC register block: `ch0data` through `date` (`+0x0cc`).
pub const REGS_LEN: u32 = 0x0d0;

/// The RMT RAM: `+0x800`. See the module header for the arithmetic — this is
/// the one constant whose being wrong transmits whatever the register file
/// happened to hold, and **the C6's is `+0x400`**.
pub const RAM_OFFSET: u32 = 0x800;

/// Eight channels of 48 words (`rmt.channel_ram_size = 48`,
/// `_generated_esp32s3.rs:229-230`; `for_each_rmt_channel!` lists 0..=7).
pub const BLOCK_WORDS: u32 = 48;
pub const RAM_WORDS: usize = 384;

/// The whole window the bus maps: registers, the gap, and the RAM at
/// `+0x800..+0xe00`.
pub const LEN: u32 = RAM_OFFSET + 4 * RAM_WORDS as u32;

/// TX channels: 0…3 (`for_each_rmt_channel!`'s `tx(0, 0), (1, 1), (2, 2),
/// (3, 3)`).
pub const TX_CHANNELS: usize = 4;
/// RX channels: 4…7, indexed here by their **receiver index** — 0 is channel
/// 4 — because that is how the PAC's status registers index them
/// (`rx(4, 0), (5, 1), (6, 2), (7, 3)`).
pub const RX_CHANNELS: usize = 4;
/// The absolute channel number of receiver index 0.
pub const RX_CH_BASE: usize = 4;

/// `ch_tx_conf0.mem_size`, bits **16:19** — eight blocks addressable. ⚠️ The
/// C6's is 16:18.
const CONF_MEM_SIZE_SHIFT: u32 = 16;
const CONF_MEM_SIZE_MASK: u32 = 0xf;
/// PAC reset: `div_cnt 2`, `mem_size 1`, the carrier bits set — the same word
/// as the C6's, which is a coincidence of the two field widths overlapping at
/// `mem_size = 1` rather than evidence that the layouts agree.
const CH_TX_CONF0_RESET: u32 = 0x0071_0200;
/// `ch_tx_lim.tx_lim`, bits 0:8 — nine bits, max 511 (`s3_rmt.rs:35-37`).
const TX_LIM_MASK: u32 = 0x1ff;

/// `ch_rx_conf0.mem_size`, bits **24:27**. ⚠️ The C6's is 23:25.
const RX_CONF0_MEM_SIZE_SHIFT: u32 = 24;
const RX_CONF0_MEM_SIZE_MASK: u32 = 0xf;
/// PAC reset: `div_cnt 2`, `idle_thres 0x7fff`, `mem_size 1`, carrier bits
/// set. Different from the C6's `0x30ff_ff02` **only** because `mem_size`
/// moved.
const CH_RX_CONF0_RESET: u32 = 0x317f_ff02;
/// PAC reset for `ch_rx_conf1`: `mem_owner 1`, `rx_filter_thres 15`.
const CH_RX_CONF1_RESET: u32 = 0x0000_01e8;
/// `ch_rx_lim.rx_lim`, bits 0:8.
const RX_LIM_MASK: u32 = 0x1ff;

// `ch_tx_status` fields (`rmt/ch_tx_status.rs`): `mem_raddr_ex` **0:9**,
// `apb_mem_waddr` 11:20, `state` **22:24**, `mem_empty` **25**,
// `apb_mem_wr_err` 26. ⚠️ The C6's are 0:8, 12:20, 9:11, 22 and 23.
const STATUS_RADDR_MASK: u32 = 0x3ff;
const STATUS_STATE_SHIFT: u32 = 22;
const STATUS_MEM_EMPTY: u32 = 1 << 25;

// `ch_rx_status` fields (`rmt/ch_rx_status.rs`): `mem_waddr_ex` **0:9**,
// `apb_mem_raddr` **11:20**, `state` 22:24, `mem_owner_err` 25, `mem_full`
// 26. ⚠️ The C6's `apb_mem_raddr` starts at 12.
const RX_STATUS_WADDR_MASK: u32 = 0x3ff;
const RX_STATUS_APB_RADDR_SHIFT: u32 = 11;
const RX_STATUS_STATE_SHIFT: u32 = 22;
const RX_STATUS_MEM_FULL: u32 = 1 << 26;

/// Every `int_raw` bit this chip has: 0–27 are the seven per-channel events,
/// 28 is `tx_ch3_dma_access_fail` and 29 `rx_ch7_dma_access_fail`. The DMA
/// bits are accepted and never raised — nothing here does DMA — so they stay
/// in the mask a guest may enable and out of everything else.
const INT_MASK: u32 = 0x3fff_ffff;

// The event groups, from `rmt/int_raw.rs`'s own bit list.
const INT_TX_END_SHIFT: u32 = 0;
const INT_TX_ERR_SHIFT: u32 = 4;
const INT_TX_THR_SHIFT: u32 = 8;
const INT_RX_END_SHIFT: u32 = 16;
const INT_RX_ERR_SHIFT: u32 = 20;
const INT_RX_THR_SHIFT: u32 = 24;

/// Which `int_raw` bit one interrupt of one channel is, on **this** chip.
///
/// Grouped by event: the bit is `<event base> + <index within the direction>`,
/// and the RX bases are their own rather than the TX bases plus the channel
/// number. A mask lifted from the C6 is wrong here even though the PAC
/// accessors have the same names — see the module header's delta 1.
fn int_bit(dir: Dir, kind: IntKind, index: usize) -> u32 {
    let base = match (dir, kind) {
        (Dir::Tx, IntKind::End) => INT_TX_END_SHIFT,
        (Dir::Tx, IntKind::Err) => INT_TX_ERR_SHIFT,
        (Dir::Tx, IntKind::Thr) => INT_TX_THR_SHIFT,
        (Dir::Rx, IntKind::End) => INT_RX_END_SHIFT,
        (Dir::Rx, IntKind::Err) => INT_RX_ERR_SHIFT,
        (Dir::Rx, IntKind::Thr) => INT_RX_THR_SHIFT,
    };
    base + index as u32
}

// `sys_conf`'s clock fields (`rmt/sys_conf.rs`) — the C6 has none of these,
// and reads `PCR.rmt_sclk_conf` instead.
const SYS_SCLK_DIV_NUM_SHIFT: u32 = 4;
const SYS_SCLK_DIV_NUM_MASK: u32 = 0xff;
const SYS_SCLK_DIV_A_SHIFT: u32 = 12;
const SYS_SCLK_DIV_A_MASK: u32 = 0x3f;
const SYS_SCLK_DIV_B_SHIFT: u32 = 18;
const SYS_SCLK_DIV_B_MASK: u32 = 0x3f;
const SYS_SCLK_SEL_SHIFT: u32 = 24;
/// `sclk_active`, bit 26 — the function clock's gate. **Not** `clk_en` (bit
/// 31), which forces the *register* clock on: esp-hal's `configure_clock`
/// clears `clk_en` on this chip while leaving `sclk_active` at its reset
/// value (`esp-hal-1.1.1/src/rmt.rs:2355-2370`), so a model that gated on
/// `clk_en` would stall the engine the moment the driver configured it.
const SYS_SCLK_ACTIVE: u32 = 1 << 26;

/// How often a stalled engine re-checks the clock: 1 ms of guest time.
pub const CLOCK_POLL_CYCLES: u64 = 1_000 * memmap::CYCLES_PER_US;

/// The APB clock the RX filter's threshold is counted in, and the `sclk_sel`
/// = 1 source: **80 MHz** ([`super::APB_HZ`]).
pub const APB_HZ: u64 = super::APB_HZ;

/// The GPIO-matrix output signal TX channel `ch` drives: `RMT_SIG_0 + ch`.
/// Whether any pad listens is the fabric's business, and the RMT never learns
/// the answer — it cannot see the GPIO block.
pub const fn signal_of(ch: usize) -> SignalId {
    SignalId(RMT_SIG_0 + ch as u16)
}

/// The GPIO-matrix **input** signal receiver `rxi` would read, if this chip's
/// receivers were modelled. Kept because the register file is real even where
/// the engine is not, and because the two signal spaces are two tables.
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
    stall_clock_hint: "(the engine resumes when sys_conf gives it a clock)",

    regs_len: REGS_LEN,
    ram_offset: RAM_OFFSET,
    ram_words: RAM_WORDS,
    block_words: BLOCK_WORDS,
    len: LEN,

    tx_channels: TX_CHANNELS,
    rx_channels: RX_CHANNELS,
    rx_ch_base: RX_CH_BASE,
    // `CH0..=CH3` transmit; the shipped firmware never receives.
    model_rx: false,

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

/// The S3's clock: the block's **own** `sys_conf`.
///
/// `sclk_div_num` 4:11, `sclk_div_a` 12:17, `sclk_div_b` 18:23, `sclk_sel`
/// 24:25, `sclk_active` 26. The source numbers are the metadata's
/// `for_each_rmt_clock_source!` for this chip — `(Apb, 1), (RcFast, 2),
/// (Xtal, 3)` (`_generated_esp32s3.rs:487-489`) — which happens to be the
/// same encoding the C6 uses in a different register.
///
/// **RC_FAST is refused rather than guessed at**, exactly as the C6 refuses
/// FOSC: this chip's RC_FAST is nominally 17.5 MHz and is calibrated at
/// runtime, so a model that picked a number would put every pulse in the
/// wrong place and say nothing. Nothing in the shipped firmware selects it.
#[derive(Debug)]
pub struct S3Clock;

impl ClockLine for S3Clock {
    fn decode(&self, sys_conf: u32, cpu_hz: u64) -> Result<Clock, &'static str> {
        decode_sys_conf(sys_conf, cpu_hz)
    }
}

/// Decode `sys_conf`, or say why there is no clock.
pub fn decode_sys_conf(sys_conf: u32, cpu_hz: u64) -> Result<Clock, &'static str> {
    if sys_conf & SYS_SCLK_ACTIVE == 0 {
        return Err("RMT sys_conf.sclk_active = 0");
    }
    let src_hz = match (sys_conf >> SYS_SCLK_SEL_SHIFT) & 3 {
        1 => APB_HZ,
        2 => return Err("RMT sys_conf.sclk_sel = 2 (RC_FAST): not modelled"),
        3 => super::XTAL_HZ,
        _ => return Err("RMT sys_conf.sclk_sel = 0: no clock"),
    };
    Ok(Clock {
        src_hz,
        div_num: (sys_conf >> SYS_SCLK_DIV_NUM_SHIFT) & SYS_SCLK_DIV_NUM_MASK,
        div_a: (sys_conf >> SYS_SCLK_DIV_A_SHIFT) & SYS_SCLK_DIV_A_MASK,
        div_b: (sys_conf >> SYS_SCLK_DIV_B_SHIFT) & SYS_SCLK_DIV_B_MASK,
        cpu_hz,
    })
}

/// The S3's RMT block: the shared view at [`CONFIG`], on its own `sys_conf`.
pub fn new() -> Rmt {
    Rmt::new(&CONFIG, Box::new(S3Clock))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::{Peripheral, Sandbox};

    /// `lp_ws281x::pulse_code`, locally: two level/duration pairs in one word.
    const fn word(l1: bool, d1: u16, l2: bool, d2: u16) -> u32 {
        ((l1 as u32) << 15)
            | (d1 as u32 & 0x7fff)
            | ((((l2 as u32) << 15) | (d2 as u32 & 0x7fff)) << 16)
    }

    /// The WS2812 codes at 80 MHz (`lp_ws281x::timing`): 100 ticks each.
    const ZERO: u32 = word(true, 32, false, 68);
    const ONE: u32 = word(true, 64, false, 36);

    /// `sys_conf` as esp-hal's `configure_clock` leaves it for an 80 MHz
    /// channel clock: `sclk_active` (its reset value, which the driver does
    /// not touch), `sclk_sel` = 1 (APB), `sclk_div_num` = 0 — `div = src /
    /// target - 1`, so an 80 MHz target off an 80 MHz source is zero — and
    /// `apb_fifo_mask` (bit 0) set. **Bit 0 is the FIFO mask, not the
    /// divider**: the divider starts at bit 4.
    const SYS_CONF_80MHZ: u32 = SYS_SCLK_ACTIVE | (1 << SYS_SCLK_SEL_SHIFT) | 1;

    /// The same with `sclk_div_num` = 1, which halves it to 40 MHz.
    const SYS_CONF_40MHZ: u32 = SYS_CONF_80MHZ | (1 << SYS_SCLK_DIV_NUM_SHIFT);

    fn rig() -> (Sandbox, Rmt) {
        let mut r = new();
        r.attached(9);
        r.set_keep_logs(true);
        let mut sb = Sandbox::new();
        sb.write(&mut r, SYS_CONF, SYS_CONF_80MHZ);
        (sb, r)
    }

    fn ram_off(word_index: u32) -> u32 {
        RAM_OFFSET + 4 * word_index
    }

    /// esp-hal's `configure_tx` on channel `ch`: `div_cnt 1`, idle low,
    /// `mem_size` blocks, then start.
    fn start(sb: &mut Sandbox, r: &mut Rmt, ch: usize, mem_size: u32) {
        use lp_emu_esp_common::ip::rmt::{
            CONF_APB_MEM_RST, CONF_CONF_UPDATE, CONF_DIV_CNT_SHIFT, CONF_MEM_RD_RST,
            CONF_MEM_TX_WRAP_EN, CONF_TX_START,
        };
        let conf =
            (1 << CONF_DIV_CNT_SHIFT) | (mem_size << CONF_MEM_SIZE_SHIFT) | CONF_MEM_TX_WRAP_EN;
        sb.write(r, CH_TX_CONF0[ch], conf);
        sb.write(r, CH_TX_CONF0[ch], conf | CONF_CONF_UPDATE);
        sb.write(
            r,
            CH_TX_CONF0[ch],
            conf | CONF_MEM_RD_RST | CONF_APB_MEM_RST | CONF_TX_START,
        );
        sb.write(r, CH_TX_CONF0[ch], conf | CONF_CONF_UPDATE);
    }

    /// The PAC's own reset values, and the aperture.
    #[test]
    fn the_reset_state_is_the_pac() {
        let mut sb = Sandbox::new();
        let mut r = new();
        for ch in 0..TX_CHANNELS {
            assert_eq!(sb.read(&mut r, CH_TX_CONF0[ch]), 0x0071_0200, "ch{ch}");
            assert_eq!(sb.read(&mut r, CH_TX_LIM[ch]), 0x80, "ch{ch}");
        }
        for rxi in 0..RX_CHANNELS {
            assert_eq!(sb.read(&mut r, CH_RX_CONF0[rxi]), 0x317f_ff02, "rx{rxi}");
            assert_eq!(sb.read(&mut r, CH_RX_CONF1[rxi]), 0x0000_01e8, "rx{rxi}");
        }
        assert_eq!(sb.read(&mut r, SYS_CONF), 0x0500_0010);
        assert_eq!(r.reg_name(0x050), Some("ch0_tx_status"));
        assert_eq!(r.reg_name(0x0cc), Some("date"));
        assert_eq!(r.reg_name(RAM_OFFSET), None, "the RAM has no register name");
        // Each of the four TX channels starts at its own block.
        for ch in 0..TX_CHANNELS {
            assert_eq!(
                sb.read(&mut r, CH_TX_STATUS[ch]) & STATUS_RADDR_MASK,
                BLOCK_WORDS * ch as u32,
                "ch{ch}'s window"
            );
        }
    }

    /// ⚠️ **Delta: the RAM offset.** `+0x800`, re-derived from the metadata's
    /// decimal, and the C6's `+0x400` lands inside the register file's gap —
    /// which this block drops with a note rather than storing.
    #[test]
    fn the_ram_is_at_0x800_and_the_c6s_0x400_is_the_gap() {
        let (mut sb, mut r) = rig();
        sb.write(&mut r, ram_off(0), 0xdead_beef);
        assert_eq!(r.ram()[0], 0xdead_beef);
        assert_eq!(sb.read(&mut r, ram_off(0)), 0xdead_beef);
        // The last word of the 384-word RAM.
        sb.write(&mut r, ram_off(RAM_WORDS as u32 - 1), 0x1234_5678);
        assert_eq!(r.ram()[RAM_WORDS - 1], 0x1234_5678);
        assert_eq!(LEN, 0xe00, "registers, the gap, and 384 words");

        // The C6's offset is between the register block and the RAM here.
        sb.write(&mut r, 0x400, 0xffff_ffff);
        assert_eq!(sb.read(&mut r, 0x400), 0, "dropped, not stored");
        assert!(r.ram().iter().all(|&w| w != 0xffff_ffff));
    }

    /// ⚠️ **Delta 1: the interrupt bits are grouped by event.** A mask lifted
    /// from the C6 — where `end` on receiver 0 is bit 2 — names
    /// `ch2_tx_end` here.
    #[test]
    fn the_interrupt_bits_are_grouped_by_event_and_not_by_channel() {
        assert_eq!(int_bit(Dir::Tx, IntKind::End, 0), 0);
        assert_eq!(int_bit(Dir::Tx, IntKind::End, 3), 3);
        assert_eq!(int_bit(Dir::Tx, IntKind::Err, 0), 4);
        assert_eq!(int_bit(Dir::Tx, IntKind::Thr, 0), 8);
        assert_eq!(int_bit(Dir::Tx, IntKind::Thr, 3), 11);
        // The RX bases are their own, not `tx_base + rx_ch_base`.
        assert_eq!(int_bit(Dir::Rx, IntKind::End, 0), 16);
        assert_eq!(int_bit(Dir::Rx, IntKind::Err, 0), 20);
        assert_eq!(int_bit(Dir::Rx, IntKind::Thr, 0), 24);
        assert_eq!(int_bit(Dir::Rx, IntKind::Thr, 3), 27);
        // The C6's rule would put an `rx_end` on receiver 0 at bit
        // `0 + RX_CH_BASE` = 4, which here is `ch0_tx_err`.
        assert_ne!(
            int_bit(Dir::Rx, IntKind::End, 0),
            INT_TX_END_SHIFT + RX_CH_BASE as u32
        );
    }

    /// The same delta, seen from a running frame: a `tx_end` on channel 3 is
    /// bit 3, and `tx_thr_event` on channel 3 is bit 11.
    #[test]
    fn a_frame_on_channel_three_raises_its_own_end_and_threshold_bits() {
        let (mut sb, mut r) = rig();
        let base = BLOCK_WORDS * 3;
        for i in 0..48 {
            sb.write(&mut r, ram_off(base + i), ZERO);
        }
        // A STOP word four in, and the threshold at word 2.
        sb.write(&mut r, ram_off(base + 4), 0);
        sb.write(&mut r, CH_TX_LIM[3], 2);
        sb.write(&mut r, INT_ENA, INT_MASK);
        start(&mut sb, &mut r, 3, 1);
        sb.run_to(&mut r, 100_000);
        assert_eq!(r.frames_ended(3), 1);
        assert_eq!(r.sticky() & (1 << 3), 1 << 3, "ch3_tx_end is bit 3");
        assert_eq!(
            r.sticky() & (1 << 11),
            1 << 11,
            "ch3_tx_thr_event is bit 11"
        );
        assert_eq!(r.sticky() & 0b111, 0, "no other channel ended");
        assert!(sb.irq.level(source::RMT));
        let sticky = r.sticky();
        sb.write(&mut r, INT_CLR, sticky);
        assert!(!sb.irq.level(source::RMT));
    }

    /// ⚠️ **Delta 2: the divider is in `sys_conf`.** The C6 reads PCR; here a
    /// write to this block's own register changes the tick rate, and clearing
    /// `sclk_active` stalls the engine.
    #[test]
    fn sys_conf_carries_the_divider_and_the_gate() {
        let c = decode_sys_conf(SYS_CONF_80MHZ, memmap::CPU_HZ).unwrap();
        assert_eq!(c.hz(), 80_000_000, "div_num 0 is the undivided source");
        // ⚠️ At 240 MHz a 100-tick WS2812 symbol is **300** cycles, not the
        // C6's 200 — the tick arithmetic is on `CPU_HZ` and never on a ratio.
        assert_eq!(c.cycles_for(100, 1), 300);
        // `sclk_div_num` is bits 4:11, so bit 0 (the FIFO mask) does not
        // divide anything and bit 4 halves it.
        let c = decode_sys_conf(SYS_CONF_40MHZ, memmap::CPU_HZ).unwrap();
        assert_eq!(c.hz(), 40_000_000, "div_num 1 halves the 80 MHz source");
        assert_eq!(c.cycles_for(100, 1), 600);
        // XTAL is source 3, RC_FAST (2) is refused rather than guessed.
        let c =
            decode_sys_conf(SYS_SCLK_ACTIVE | (3 << SYS_SCLK_SEL_SHIFT), memmap::CPU_HZ).unwrap();
        assert_eq!(c.hz(), 40_000_000);
        assert!(decode_sys_conf(SYS_SCLK_ACTIVE | (2 << SYS_SCLK_SEL_SHIFT), 1).is_err());
        assert!(decode_sys_conf(SYS_SCLK_ACTIVE, 1).is_err(), "sel 0");
        assert_eq!(
            decode_sys_conf(1 << SYS_SCLK_SEL_SHIFT, 1),
            Err("RMT sys_conf.sclk_active = 0")
        );
        // ⚠️ `clk_en` (bit 31) is not the gate: esp-hal clears it.
        assert!(decode_sys_conf(SYS_CONF_80MHZ, memmap::CPU_HZ).is_ok());
        assert!(decode_sys_conf(SYS_CONF_80MHZ | (1 << 31), memmap::CPU_HZ).is_ok());
    }

    /// ⚠️ **Delta 3: `ch_tx_conf0.mem_size` is four bits.** `mem_size = 8` is
    /// the whole RAM here; through the C6's three-bit mask it reads 0, which
    /// the view refuses to start on.
    #[test]
    fn tx_mem_size_is_four_bits_wide() {
        let (mut sb, mut r) = rig();
        for i in 0..RAM_WORDS as u32 {
            sb.write(&mut r, ram_off(i), ONE);
        }
        // A STOP word at word 200 — past the four blocks a three-bit
        // `mem_size` could have addressed.
        sb.write(&mut r, ram_off(200), 0);
        sb.write(&mut r, CH_TX_LIM[0], 0);
        start(&mut sb, &mut r, 0, 8);
        sb.run_to(&mut r, 10_000_000);
        assert_eq!(r.frames_ended(0), 1);
        assert_eq!(r.words(0).len(), 201, "the window really was 384 words");
    }

    /// ⚠️ **Delta 4: `ch_rx_conf0.mem_size` is bits 24:27.** The reset value
    /// is `mem_size = 1` read through *this* chip's field; read through the
    /// C6's it is 2, which would give every receiver twice the window.
    #[test]
    fn rx_mem_size_is_at_bit_twenty_four() {
        assert_eq!(
            (CH_RX_CONF0_RESET >> RX_CONF0_MEM_SIZE_SHIFT) & RX_CONF0_MEM_SIZE_MASK,
            1,
            "the PAC's reset is one block"
        );
        assert_eq!(
            (CH_RX_CONF0_RESET >> 23) & 0x7,
            2,
            "the C6's field would read two blocks out of the same word"
        );
        // And the idle threshold is still 15 bits at 8:22 either way, which
        // is why the mistake passes a register test.
        assert_eq!((CH_RX_CONF0_RESET >> 8) & 0x7fff, 0x7fff);
    }

    /// ⚠️ **Delta 5: `mem_raddr_ex` is ten bits and absolute.** Channel 3's
    /// window starts at word 144, which does not fit the C6's nine-bit mask
    /// once the pointer passes 511 — and `state` at 22:24 would land inside
    /// the C6's `apb_mem_waddr`.
    #[test]
    fn the_read_pointer_is_ten_bits_and_absolute_over_the_whole_ram() {
        let (mut sb, mut r) = rig();
        // The last channel's window is words 336..384, all of which need the
        // tenth bit.
        let base = BLOCK_WORDS * 3;
        for i in 0..48 {
            sb.write(&mut r, ram_off(base + i), ONE);
        }
        sb.write(&mut r, ram_off(base + 30), 0);
        sb.write(&mut r, CH_TX_LIM[3], 0);
        start(&mut sb, &mut r, 3, 1);
        let status = sb.read(&mut r, CH_TX_STATUS[3]);
        assert_eq!(
            status & STATUS_RADDR_MASK,
            base + 1,
            "absolute, and past nine bits"
        );
        assert!(base + 1 > 0x1ff / 4, "the pointer needs the tenth bit here");
        assert_eq!(
            status & (1 << STATUS_STATE_SHIFT),
            1 << STATUS_STATE_SHIFT,
            "state is bit 22, not bit 9"
        );
        assert_eq!(status & (1 << 9), 0, "the C6's state bit is clear here");
    }

    /// A frame comes out of the pump onto `RMT_SIG_0 + ch`, and the two
    /// halves of a word are two pulses.
    #[test]
    fn a_started_channel_pumps_its_signal() {
        let (mut sb, mut r) = rig();
        for i in 0..48 {
            sb.write(&mut r, ram_off(i), ZERO);
        }
        sb.write(&mut r, ram_off(3), 0);
        sb.write(&mut r, CH_TX_LIM[0], 0);
        start(&mut sb, &mut r, 0, 1);
        sb.run_to(&mut r, 100_000);
        assert_eq!(r.frames_ended(0), 1);
        assert_eq!(r.pulses(0).len(), 6, "three words, two pulses each");
        // 80 MHz channel clock at `div_cnt` 1 and a 240 MHz CPU: three
        // cycles a tick, so a 32-tick high is 96 cycles. ⚠️ On the C6 the
        // same word is 64 cycles — the CPU is 160 MHz there.
        assert_eq!(r.pulses(0)[0].ticks, 32);
        assert_eq!(r.pulses(0)[1].at - r.pulses(0)[0].at, 96);
        assert!(r.pulses(0)[0].level);
        assert!(!r.pulses(0)[1].level);
        assert_eq!(signal_of(0), SignalId(81));
        assert_eq!(signal_of(3), SignalId(84));
    }

    /// The receivers answer every register and model nothing (the module
    /// header's rule): `rx_en` writes back, no words appear, and the RX
    /// status still reads its own window.
    #[test]
    fn rx_en_is_accepted_and_writes_no_words() {
        use lp_emu_esp_common::ip::rmt::{
            RX_CONF1_CONF_UPDATE, RX_CONF1_MEM_OWNER, RX_CONF1_RX_EN,
        };
        let (mut sb, mut r) = rig();
        sb.write(
            &mut r,
            CH_RX_CONF1[0],
            CH_RX_CONF1_RESET | RX_CONF1_RX_EN | RX_CONF1_MEM_OWNER | RX_CONF1_CONF_UPDATE,
        );
        assert!(!r.rx_running(0), "not modelled on this chip");
        assert_eq!(r.rx_words_written(0), 0);
        // `conf_update` is a strobe and reads back 0; `rx_en` is remembered.
        assert_eq!(
            sb.read(&mut r, CH_RX_CONF1[0]) & RX_CONF1_CONF_UPDATE,
            0,
            "the strobe reads zero"
        );
        assert_eq!(
            sb.read(&mut r, CH_RX_CONF1[0]) & RX_CONF1_RX_EN,
            RX_CONF1_RX_EN
        );
        assert_eq!(
            sb.read(&mut r, CH_RX_STATUS[0]) & RX_STATUS_WADDR_MASK,
            BLOCK_WORDS * RX_CH_BASE as u32,
            "receiver 0 is channel 4"
        );
        assert!(r.ram().iter().all(|&w| w == 0));
    }

    /// The state blob round-trips a frame in flight, at this chip's channel
    /// counts and RAM width.
    #[test]
    fn the_state_round_trips_mid_frame() {
        let (mut sb, mut r) = rig();
        for i in 0..48 {
            sb.write(&mut r, ram_off(i), ONE);
        }
        sb.write(&mut r, CH_TX_LIM[0], 24);
        sb.now = 500;
        start(&mut sb, &mut r, 0, 1);
        sb.run_to(&mut r, 500 + 5 * 600 + 41);
        let blob = r.save_state();
        let mut other = new();
        other.load_state(&blob);
        assert_eq!(other.index(), 9);
        assert!(other.is_running(0));
        assert_eq!(other.tx_state(0), r.tx_state(0));
        assert_eq!(other.ram(), r.ram());
        assert_eq!(other.words(0), r.words(0));
        assert_eq!(other.pulses(0), r.pulses(0));
        assert_eq!(other.stored(CH_TX_LIM[0]), 24);
        assert_eq!(other.stored(SYS_CONF), SYS_CONF_80MHZ);
    }

    /// The two signal spaces are two constants even though they coincide.
    #[test]
    fn the_signal_tables_are_the_chips_own() {
        assert_eq!(RMT_SIG_0, 81);
        assert_eq!(RMT_RX_SIG_0, 81);
        assert_eq!(rx_signal_of(0), SignalId(81));
        // The C6's are 71 and 71; the classic's are 87 and 83.
        assert_ne!(RMT_SIG_0, 71);
    }
}
