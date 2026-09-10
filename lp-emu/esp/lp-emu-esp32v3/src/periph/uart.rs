//! `UART0` at `0x3FF4_0000` and `UART1` at `0x3FF5_0000` — the classic's
//! **view** of the UART block: a real FIFO pair, a shifter draining at the
//! programmed baud in emulated time, the thresholds, the receive timeout, and
//! a host byte stream on the outside.
//!
//! Register facts are the `esp32` PAC 0.40.2 `uart0` block (offsets and reset
//! values in [`regs::UART0`], generated; bit positions cited per register
//! below) and esp-hal 1.1.1's `src/uart/mod.rs`.
//!
//! # This file is a *view*
//!
//! Everything under the registers — the FIFO pair, the shifter, the source
//! poll, the receive-timeout arm, the sticky events as **names** — is
//! [`lp_emu_esp_common::engine::uart::UartEngine`], shared with the C6 and
//! whatever comes next (M2 D2; the common crate's README, "Engines and
//! views"). What lives here is what is a *classic* fact: the offsets, the bit
//! positions, the reset values, the clock selection, the auto-baud counters,
//! the three local event numbers and the `Peripheral` impl.
//!
//! # Where the classic differs from the C6, register by register
//!
//! The first six registers sit at the same offsets and the `int_raw` bit
//! numbers agree for every bit the C6 models. **That is a coincidence of two
//! generations of one IP and this file does not rely on it**: every constant
//! below is read off the `esp32` PAC. From `0x18` the two parts diverge, and
//! the differences are the reason this is a separate view rather than a
//! parameter:
//!
//! | | classic | C6 |
//! |---|---|---|
//! | clock select | `conf0.tick_ref_always_on` (bit 27): 1 = APB, 0 = REF_TICK | PCR's `uart(n).clk_conf` |
//! | per-block clock gate | none in the block (DPORT's `perip_clk_en`, P4's) | `clk_conf.tx_sclk_en` / `rx_sclk_en` |
//! | FIFO resets | `conf0` bits **17** (`rxfifo_rst`) / **18** (`txfifo_rst`) | bits 22 / 23 |
//! | receive timeout | `conf1.rx_tout_thrhd` (24:30) + `rx_tout_en` (31), counted in **symbols of 8 bits** | a whole `TOUT_CONF` register, counted in bit times |
//! | thresholds | `conf1` 7-bit fields + `mem_conf`'s three high bits | `conf1` 8-bit fields |
//! | `clkdiv` | integer bits 0:**19** | bits 0:11 |
//! | FSM state | `status.st_urx_out` / `st_utx_out` | a separate `fsm_status` |
//! | register sync | none (`sync_regs` is a no-op on esp32) | `reg_update` |
//! | RX count | **not** `status.rxfifo_cnt` — see the errata below | `status.rxfifo_cnt` |
//!
//! ## The RX-count errata, which is load-bearing
//!
//! esp-hal does not read `status.rxfifo_cnt` on this chip. `rx_fifo_count`
//! (`third_party/esp-hal/src/uart/mod.rs:3637-3658`) computes the real count
//! from `mem_rx_status.mem_rx_rd_addr` (bits 2:12) and `mem_rx_wr_addr`
//! (bits 13:23), citing the Espressif errata *"UART FIFO_CNT indicates data
//! length incorrectly"*:
//!
//! ```text
//! wr > rd → wr - rd      wr < rd → wr + 128 - rd
//! wr == rd → 128 if status.rxfifo_cnt > 0 else 0
//! ```
//!
//! A model that left `mem_rx_status` at the PAC's reset would answer *zero
//! bytes available* to the async reader for ever. So this view publishes the
//! pair — **modeled**, and the choice is stated rather than dressed up: the
//! read address is reported as 0 and the write address as the number of bytes
//! in the FIFO, which is the pair the errata's own formula turns back into
//! the true count. The real part's addresses are RAM offsets that wrap; no
//! document in this repo states where the classic's RX block starts, and
//! inventing a base would be inventing a number.
//!
//! ## The receive timeout cannot be cleared while the FIFO has bytes
//!
//! esp-hal, `uart/mod.rs:1180-1183`: *"On ESP32 and S2, the timeout interrupt
//! can't be cleared unless the FIFO is empty, so listening could cause an
//! infinite loop here."* So an `int_clr` write naming `rxfifo_tout` clears it
//! only when the receive FIFO is empty. That is a **third** category beside
//! the engine's levels and sticky events, it belongs to this chip, and it
//! lives here rather than in the engine for exactly that reason.
//!
//! # Who drives it
//!
//! Three writers share the block, and all three reach
//! [`UartEngine::push_tx`]:
//!
//! - the **mask ROM's** `uart_tx_one_char` (`0x4000_9200`), which spins while
//!   `status & 0x0080_0000` — bit 23, the top bit of `txfifo_cnt`, i.e. "128
//!   bytes queued" — and then stores a **word** to `fifo`;
//! - **`esp-println`'s `uart` feature**, which calls that same ROM routine:
//!   every `[INIT]` line of the boot comes out this way, bypassing the async
//!   driver entirely;
//! - **esp-hal's async driver** on the io_task (swi2 `InterruptExecutor` at
//!   Priority2, paced by TIMG0 `timer1` at 1 ms): `write` fills the FIFO
//!   against `Info::UART_FIFO_SIZE - tx_fifo_count`, `read_async` sets
//!   `conf1.rxfifo_full_thrhd` and awaits `rxfifo_full | rxfifo_tout | …`,
//!   and the ISR reads `int_st`, clears the fired bits out of `int_ena`, and
//!   wakes.
//!
//! # Timing, and the two bauds
//!
//! `sclk` is APB (80 MHz, [`super::APB_HZ`]) when `conf0.tick_ref_always_on`
//! is set and REF_TICK (1 MHz, [`REF_TICK_HZ`]) when it is clear. One symbol
//! takes `symbol_bits × CPU_HZ × (clkdiv·16 + frag) / (sclk × 16)` cycles,
//! and `symbol_bits` comes from `conf0` (start + data + parity + stop).
//!
//! **Nothing here is a flag.** The block starts at the PAC's `clkdiv` of
//! `0x2b6` = 694 against APB, which is `80,000,000 / 694 = 115,273` baud —
//! the ROM console's rate, to 0.06 % — and the shipped image reprograms
//! `clkdiv` for 921,600 in `board/esp32v3/init.rs`. The change mid-stream is
//! automatic because [`Uart::config`] recomputes from the registers on every
//! access. L0 captured the desk board at both rates for this reason
//! (`../bench.md`: "ROM banner + bootloader at 115200, app console at
//! 921600").
//!
//! ⚠️ This machine has no clock **tree**: `sclk` is [`super::APB_HZ`]
//! whenever the guest selects APB, whatever the PLL is actually doing at that
//! moment. A ROM-up boot therefore computes its banner baud against 80 MHz
//! from the first instruction, where silicon runs the first milliseconds from
//! the crystal. Reported, not hidden.
//!
//! # Levels versus sticky events
//!
//! `rxfifo_full` (bit 0) and `txfifo_empty` (bit 1) are **levels**: they hold
//! while `rx_len > rxfifo_full_thrhd` / `tx_len < txfifo_empty_thrhd`, and an
//! `int_clr` write cannot clear one while the condition holds. `rxfifo_ovf`
//! (4), `rxfifo_tout` (8) and `tx_done` (14) are sticky events `int_clr`
//! does clear — `rxfifo_tout` subject to the empty-FIFO rule above.
//! `at_cmd_char_det` (18) is a detector that never fires, exactly as on the
//! C6: the async read only ever *adds* it to the set it waits on, so a silent
//! detector changes nothing. Stated so it is not mistaken for modelled.
//!
//! # Per-register grades (`--strict-grade`)
//!
//! | grade | registers | why |
//! |---|---|---|
//! | `documented` | `fifo`, `int_raw`, `int_st`, `int_ena`, `int_clr`, `clkdiv`, `status`, `conf0`, `conf1`, `mem_conf`, `mem_cnt_status`, `autobaud`, `lowpulse`, `highpulse`, `rxd_cnt`, `pospulse`, `negpulse` | the PAC states what each holds and this file implements that statement, against esp-hal's driver and the mask ROM's disassembly. `status`'s two FIFO counts are the PAC's; its `st_urx_out`/`st_utx_out` implement a *subset* of the PAC's own enumeration (idle versus not idle — esp-hal's `is_tx_idle` tests exactly that, `uart/mod.rs:906-912`) and its five line-level bits read the PAC's reset of zero |
//! | `modeled` | `mem_rx_status`, `mem_tx_status`, and everything else | `mem_rx_status`'s address pair is the modelled choice described above; `mem_tx_status` has no named sub-fields on this part at all (the PAC declares one opaque 24-bit field), so it reads its reset; the rest — `flow_conf`, `sleep_conf`, `swfc_conf`, `idle_conf`, `rs485_conf`, `at_cmd_*`, `date`, `id` — is stored and read back at the PAC's reset with no behaviour |
//!
//! **Nothing here is `measured`.** L0 captured the desk board's console at
//! both bauds but no register-level transcript of this block exists for the
//! classic; M5 is the milestone that can raise a grade, and it will do it
//! with a transcript pair, not with a boot that looked right.

use lp_emu_esp_common::engine::uart::{
    RxDeliver, TxPush, UartConfig, UartEngine, UartEventIds, UartEvents,
};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{
    BusCx, Peripheral, RegFile, RegGrade, RegGrades, StreamId, Width, event_id, event_local,
};
use lp_emu_core::sched::EventId;

use crate::memmap;
use crate::regs;

// Register offsets (`regs::UART0`, generated from the PAC).
const FIFO: u32 = 0x00;
const INT_RAW: u32 = 0x04;
const INT_ST: u32 = 0x08;
const INT_ENA: u32 = 0x0c;
const INT_CLR: u32 = 0x10;
const CLKDIV: u32 = 0x14;
const AUTOBAUD: u32 = 0x18;
const STATUS: u32 = 0x1c;
const CONF0: u32 = 0x20;
const CONF1: u32 = 0x24;
const LOWPULSE: u32 = 0x28;
const HIGHPULSE: u32 = 0x2c;
const RXD_CNT: u32 = 0x30;
const AT_CMD_CHAR: u32 = 0x54;
const MEM_CONF: u32 = 0x58;
const MEM_TX_STATUS: u32 = 0x5c;
const MEM_RX_STATUS: u32 = 0x60;
const MEM_CNT_STATUS: u32 = 0x64;
const POSPULSE: u32 = 0x68;
const NEGPULSE: u32 = 0x6c;

/// The block's aperture: the generated table runs to `+0x7c` (`id`).
pub const UART_LEN: u32 = 0x80;

/// FIFO depth, both directions: **128 bytes** (`esp-metadata`
/// `uart.ram_size` for esp32 = 128, and `mem_conf`'s reset `0x88` allocates
/// one 128-byte block to each direction — `rx_size` bits 3:6 = 1, `tx_size`
/// bits 7:10 = 1).
///
/// A guest that re-allocated the shared memory would change the real depth;
/// this model remembers the fields and keeps the depth. Nothing on the boot
/// path writes them to anything but the reset, and a write that did is
/// reported through the trace rather than silently ignored.
pub const FIFO_DEPTH: usize = 128;

/// **REF_TICK, 1 MHz** — the other half of `conf0.tick_ref_always_on`.
///
/// Not a datasheet transcription: it is what the PAC's own reset values for
/// the tick dividers make it. `APB_CTRL.xtal_tick_conf` resets to `0x27` = 39
/// against a 40 MHz crystal ([`super::XTAL_HZ`]) and `pll_tick_conf` to
/// `0x4f` = 79 against the 80 MHz APB ([`super::APB_HZ`]); both are
/// `divisor + 1` and both land on exactly 1 MHz
/// (`src/regs/apb_ctrl.rs`, and esp-hal's `configure_ref_tick_*_impl`,
/// `soc/esp32/clocks.rs:456-530`, which is what writes those registers).
pub const REF_TICK_HZ: u64 = 1_000_000;

/// The host's baud on the wire when a run states none (`--uart0-baud`'s
/// default): what the mask ROM's console and esptool both start at, and what
/// L0's `cap_115200_a.bin` capture was taken at.
pub const DEFAULT_HOST_BAUD: u64 = 115_200;

/// How often a live (socket) source is polled when it has nothing queued:
/// 1 ms of guest time — under the 2 ms the firmware's own `io_task` sleeps.
pub const LIVE_POLL_CYCLES: u64 = 1_000 * memmap::CYCLES_PER_US;

// `int_raw` bit numbers (`esp32-0.40.2/src/uart0/int_raw.rs`).
const INT_RXFIFO_FULL: u32 = 1 << 0;
const INT_TXFIFO_EMPTY: u32 = 1 << 1;
const INT_RXFIFO_OVF: u32 = 1 << 4;
const INT_RXFIFO_TOUT: u32 = 1 << 8;
const INT_TX_DONE: u32 = 1 << 14;
/// Bits 0..=18 are named on this part; `at_cmd_char_det` (18) is the last.
const INT_MASK: u32 = 0x0007_ffff;
const INT_LEVEL_BITS: u32 = INT_RXFIFO_FULL | INT_TXFIFO_EMPTY;

// `conf0` bits (`uart0/conf0.rs`).
const CONF0_PARITY: u32 = 1 << 0;
const CONF0_PARITY_EN: u32 = 1 << 1;
const CONF0_BIT_NUM_SHIFT: u32 = 2;
const CONF0_STOP_SHIFT: u32 = 4;
/// Bit 17 — **not** the C6's 22.
const CONF0_RXFIFO_RST: u32 = 1 << 17;
/// Bit 18 — **not** the C6's 23.
const CONF0_TXFIFO_RST: u32 = 1 << 18;
/// Bit 27: "used to select the clock. 1: apb clock 0: ref_tick".
const CONF0_TICK_REF_ALWAYS_ON: u32 = 1 << 27;

// `conf1` fields (`uart0/conf1.rs`): seven bits each, with three more in
// `mem_conf` (which the PAC documents as "refer to the … description").
const CONF1_RXFIFO_FULL_THRHD_MASK: u32 = 0x7f;
const CONF1_TXFIFO_EMPTY_THRHD_SHIFT: u32 = 8;
const CONF1_RX_TOUT_THRHD_SHIFT: u32 = 24;
const CONF1_RX_TOUT_THRHD_MASK: u32 = 0x7f;
const CONF1_RX_TOUT_EN: u32 = 1 << 31;

// `mem_conf` fields (`uart0/mem_conf.rs`).
const MEM_CONF_RX_SIZE_SHIFT: u32 = 3;
const MEM_CONF_TX_SIZE_SHIFT: u32 = 7;
const MEM_CONF_SIZE_MASK: u32 = 0xf;
const MEM_CONF_RX_TOUT_THRHD_H3_SHIFT: u32 = 18;
const MEM_CONF_RXFIFO_FULL_THRHD_H3_SHIFT: u32 = 25;
const MEM_CONF_TXFIFO_EMPTY_THRHD_H3_SHIFT: u32 = 28;
const MEM_CONF_H3_MASK: u32 = 0x7;

/// `conf1`'s reset (`0x6060`) allocates one 128-byte block to each direction
/// and the size fields say so; anything else is a guest re-allocating the
/// shared memory, which this model does not follow.
const MEM_CONF_ONE_BLOCK: u32 = 1;

// `status` fields (`uart0/status.rs`).
const STATUS_RXFIFO_CNT_MASK: u32 = 0xff;
const STATUS_ST_URX_OUT_SHIFT: u32 = 8;
const STATUS_TXFIFO_CNT_SHIFT: u32 = 16;
const STATUS_ST_UTX_OUT_SHIFT: u32 = 24;
/// `st_utx_out`'s `TX_IDLE`, which esp-hal's `is_tx_idle` tests for
/// (`uart/mod.rs:906-912`).
const TX_IDLE: u32 = 0;
/// `st_utx_out`'s `TX_STRT` — the first non-idle state of the PAC's
/// enumeration. **Modeled**: this view reports "a symbol is on the wire", not
/// which of the fifteen states it is in.
const TX_STRT: u32 = 1;

// `mem_rx_status` fields (`uart0/mem_rx_status.rs`).
const MEM_RX_RD_ADDR_SHIFT: u32 = 2;
const MEM_RX_WR_ADDR_SHIFT: u32 = 13;

// `autobaud` fields (`uart0/autobaud.rs`).
const AUTOBAUD_EN: u32 = 1 << 0;
const AUTOBAUD_GLITCH_FILT_SHIFT: u32 = 8;
const AUTOBAUD_GLITCH_FILT_MASK: u32 = 0xff;

// `clkdiv` fields (`uart0/clkdiv.rs`): the integer part is **twenty** bits on
// this part, the fraction four.
const CLKDIV_INT_MASK: u32 = 0x000f_ffff;
const CLKDIV_FRAG_SHIFT: u32 = 20;
const CLKDIV_FRAG_MASK: u32 = 0xf;

/// The four pulse minima are 20-bit counters that reset to all ones (the
/// PAC's `0x000f_ffff` for `lowpulse`, `highpulse`, `pospulse`, `negpulse`).
const PULSE_MIN_RESET: u32 = 0x000f_ffff;
/// `rxd_cnt.rxd_edge_cnt` is ten bits.
const RXD_CNT_MAX: u32 = 0x3ff;

/// The `esp32` PAC's `Interrupt` enum: `UART0 = 34`, `UART1 = 35`
/// (`esp32-0.40.2/src/lib.rs:279-283`). P4's DPORT matrix is what turns these
/// peripheral sources into a CPU interrupt; until it lands the level is set
/// and nothing reads it, which is the shape TIMG's view already has.
const SOURCE_UART0: u16 = 34;
const SOURCE_UART1: u16 = 35;

const EV_TX: u16 = 0;
const EV_RX_POLL: u16 = 1;
const EV_RX_TOUT: u16 = 2;

/// A pulse count as a 20-bit counter holds it.
fn clamp20(clocks: u64) -> u32 {
    clocks.min(u64::from(PULSE_MIN_RESET)) as u32
}

/// The auto-baud counters. Live while `autobaud.autobaud_en` is set;
/// restarted on its rising edge.
///
/// The algorithm is the C6's (`lp-emu-esp32c6/src/periph/uart.rs`, M3 P1),
/// which computes what the PAC says each counter holds from two inputs the
/// run states — the bytes the host actually sent and the host's baud
/// ([`Uart::with_host_baud`]) — rather than guessing. It is repeated here
/// rather than shared because every register it reads is a classic offset;
/// M2 owns the engine and M3 may only read it.
///
/// ⚠️ **One wording difference from the C6's PAC, unresolved and left for
/// P7.** The `esp32` PAC describes `lowpulse` as "the **minimum** duration
/// time for the low level pulse" and `highpulse` as "the **maxinum** [sic]
/// duration time for the high level pulse"; the C6's PAC says minimum for
/// both, and the mask ROM's `uart_baudrate_detect` adds the two and treats
/// the sum as one bit time each way, which only works if both are minima.
/// This model computes **minima for both**, as the ROM's arithmetic and the
/// C6 require. P7 runs the classic's download console and is the phase that
/// can settle it against the real ROM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Autobaud {
    /// `rxd_cnt.rxd_edge_cnt`: level changes seen on RXD, ten bits.
    rxd_cnt: u32,
    /// `lowpulse`: the shortest low pulse, in sclk clocks.
    low_min: u32,
    /// `highpulse`: the shortest closed high pulse, in sclk clocks.
    high_min: u32,
    /// `pospulse`: the shortest distance between two rising edges.
    pos_min: u32,
    /// `negpulse`: the shortest distance between two falling edges.
    neg_min: u32,
    /// The previous frame: the cycle it landed and how many half-bits of high
    /// it ended with. A frame landing exactly one symbol later closes it.
    trailing: Option<(u64, u64)>,
}

impl Autobaud {
    const RESET: Self = Self {
        rxd_cnt: 0,
        low_min: PULSE_MIN_RESET,
        high_min: PULSE_MIN_RESET,
        pos_min: PULSE_MIN_RESET,
        neg_min: PULSE_MIN_RESET,
        trailing: None,
    };
}

/// One UART instance: the classic's **view** of the block (module docs).
#[derive(Debug)]
pub struct Uart {
    name: &'static str,
    index: usize,
    irq_source: u16,
    regs: RegFile,
    grades: RegGrades,
    stream: Option<StreamId>,
    /// The rate the host at the other end of the cable sends at — what the
    /// auto-baud counters measure. Not the rate this block is programmed for.
    host_baud: u64,
    autobaud: Autobaud,
    /// The FIFO pair, the shifter and the host stream's schedule.
    engine: UartEngine,
    warned_at_cmd: bool,
    warned_mem_size: bool,
}

impl Uart {
    fn new(name: &'static str, irq_source: u16, stream: Option<StreamId>) -> Self {
        Self {
            name,
            index: 0,
            irq_source,
            // Every reset in this block is the PAC's, seeded from the
            // generated table. Unlike the C6 there is **no** exception: the
            // classic's `clkdiv` reset of 0x2b6 already is the ROM console's
            // divisor against APB (module docs), so nothing has to be
            // re-stated as "what the ROM boot leaves behind".
            regs: RegFile::new(name, UART_LEN).with_names(regs::UART0),
            grades: Self::grades(),
            stream,
            host_baud: DEFAULT_HOST_BAUD,
            autobaud: Autobaud::RESET,
            engine: UartEngine::new(FIFO_DEPTH),
            warned_at_cmd: false,
            warned_mem_size: false,
        }
    }

    /// `UART0`, the console: source 34, the `uart0` host stream.
    pub fn uart0(stream: Option<StreamId>) -> Self {
        Self::new("UART0", SOURCE_UART0, stream)
    }

    /// `UART1`: source 35. The application never opens it; the mask ROM's
    /// `uartAttach` touches `UART1 +0x10` on every boot, so it gets the same
    /// model rather than a table (P3's ledger §4.3).
    pub fn uart1(stream: Option<StreamId>) -> Self {
        Self::new("UART1", SOURCE_UART1, stream)
    }

    /// State the host's baud on the wire (`--uart0-baud`; default
    /// [`DEFAULT_HOST_BAUD`]). The auto-baud counters measure this rate; the
    /// bytes themselves are delivered at the block's own symbol time.
    pub fn with_host_baud(mut self, baud: u64) -> Self {
        self.host_baud = baud;
        self
    }

    /// [`with_host_baud`](Self::with_host_baud) after the block is in the
    /// bus, for the machine builder — which receives the UART from
    /// [`super::boot_set`] rather than constructing it.
    pub fn set_host_baud(&mut self, baud: u64) {
        self.host_baud = baud;
    }

    pub fn host_baud(&self) -> u64 {
        self.host_baud
    }

    /// The per-register grade table (the module docs' table).
    pub fn grades() -> RegGrades {
        RegGrades::new()
            .with_grade(FIFO, RegGrade::Documented)
            .with_grade(INT_RAW, RegGrade::Documented)
            .with_grade(INT_ST, RegGrade::Documented)
            .with_grade(INT_ENA, RegGrade::Documented)
            .with_grade(INT_CLR, RegGrade::Documented)
            .with_grade(CLKDIV, RegGrade::Documented)
            .with_grade(AUTOBAUD, RegGrade::Documented)
            .with_grade(STATUS, RegGrade::Documented)
            .with_grade(CONF0, RegGrade::Documented)
            .with_grade(CONF1, RegGrade::Documented)
            .with_grade(LOWPULSE, RegGrade::Documented)
            .with_grade(HIGHPULSE, RegGrade::Documented)
            .with_grade(RXD_CNT, RegGrade::Documented)
            .with_grade(MEM_CONF, RegGrade::Documented)
            .with_grade(MEM_CNT_STATUS, RegGrade::Documented)
            .with_grade(POSPULSE, RegGrade::Documented)
            .with_grade(NEGPULSE, RegGrade::Documented)
    }

    /// Every register this block grades `Modeled`, in offset order — the
    /// README's and `--strict-grade`'s "modeled registers".
    pub fn modeled_registers() -> Vec<&'static str> {
        let grades = Self::grades();
        (0..UART_LEN)
            .step_by(4)
            .filter(|off| grades.grade(*off) == RegGrade::Modeled)
            .filter_map(|off| regs::UART0.name(off))
            .collect()
    }

    // ---- clocking ---------------------------------------------------------

    /// The function clock feeding this block, in Hz: APB or REF_TICK, chosen
    /// by `conf0.tick_ref_always_on` (module docs).
    fn sclk_hz(&self) -> u64 {
        if self.regs.stored(CONF0) & CONF0_TICK_REF_ALWAYS_ON != 0 {
            super::APB_HZ
        } else {
            REF_TICK_HZ
        }
    }

    /// `clkdiv·16 + frag`, never zero.
    fn divider16(&self) -> u64 {
        let w = self.regs.stored(CLKDIV);
        let integral = u64::from(w & CLKDIV_INT_MASK);
        let frag = u64::from((w >> CLKDIV_FRAG_SHIFT) & CLKDIV_FRAG_MASK);
        (integral * 16 + frag).max(1)
    }

    /// The baud rate the registers describe, for the log and the diagnostics.
    pub fn baud(&self) -> u64 {
        self.sclk_hz() * 16 / self.divider16()
    }

    /// Cycles per bit on the wire.
    fn bit_cycles(&self) -> Option<u64> {
        let sclk = self.sclk_hz();
        if sclk == 0 {
            return None;
        }
        let num = u128::from(memmap::CPU_HZ) * u128::from(self.divider16());
        let den = u128::from(sclk) * 16;
        Some((num.div_ceil(den)).max(1) as u64)
    }

    /// Bits per symbol from `conf0`, in half-bits: start + data + parity +
    /// stop (`stop_bit_num` 1 → 1, 2 → 1.5, 3 → 2).
    fn symbol_half_bits(&self) -> u64 {
        let conf0 = self.regs.stored(CONF0);
        let data = ((conf0 >> CONF0_BIT_NUM_SHIFT) & 3) + 5;
        let parity = (conf0 & CONF0_PARITY_EN != 0) as u32;
        let stop_half = match (conf0 >> CONF0_STOP_SHIFT) & 3 {
            2 => 3,
            3 => 4,
            _ => 2,
        };
        u64::from(2 * (1 + data + parity) + stop_half)
    }

    fn symbol_cycles(&self) -> Option<u64> {
        let bit = u128::from(self.bit_cycles()?);
        Some(((bit * u128::from(self.symbol_half_bits())).div_ceil(2)).max(1) as u64)
    }

    // ---- what the engine is told ------------------------------------------

    /// This block's three local event numbers, packed with its peripheral
    /// index. The engine never packs one itself.
    fn ids(&self) -> UartEventIds {
        UartEventIds {
            tx: event_id(self.index, EV_TX),
            rx_poll: event_id(self.index, EV_RX_POLL),
            rx_tout: event_id(self.index, EV_RX_TOUT),
        }
    }

    /// Everything the engine needs, read out of this chip's registers.
    fn config(&self) -> UartConfig {
        UartConfig {
            symbol_cycles: self.symbol_cycles(),
            bit_cycles: self.bit_cycles(),
            rx_full_thrhd: self.rx_full_thrhd(),
            tx_empty_thrhd: self.tx_empty_thrhd(),
            // The classic's threshold is in **symbols of eight bits**, not in
            // bit times: esp-hal, `uart/mod.rs:3296-3299` — "the esp32 counts
            // directly in number of symbols (symbol len fixed to 8)". The
            // engine counts bit times, so the view multiplies.
            tout_bits: self.tout_enabled().then(|| self.tout_symbols() * 8),
            // The classic has no per-direction clock enable inside the block
            // (the C6's `clk_conf.tx_sclk_en` / `rx_sclk_en`). What gates this
            // block's clock is `DPORT.perip_clk_en`, which is P4's view; until
            // it can answer, both halves are clocked.
            tx_clocked: true,
            rx_clocked: true,
            baud: self.baud(),
        }
    }

    // ---- thresholds and the timeout ---------------------------------------

    /// `conf1.rxfifo_full_thrhd` (0:6) with `mem_conf`'s three high bits.
    fn rx_full_thrhd(&self) -> usize {
        let low = self.regs.stored(CONF1) & CONF1_RXFIFO_FULL_THRHD_MASK;
        let high = (self.regs.stored(MEM_CONF) >> MEM_CONF_RXFIFO_FULL_THRHD_H3_SHIFT)
            & MEM_CONF_H3_MASK;
        (low | (high << 7)) as usize
    }

    /// `conf1.txfifo_empty_thrhd` (8:14) with `mem_conf`'s three high bits.
    fn tx_empty_thrhd(&self) -> usize {
        let low = (self.regs.stored(CONF1) >> CONF1_TXFIFO_EMPTY_THRHD_SHIFT)
            & CONF1_RXFIFO_FULL_THRHD_MASK;
        let high = (self.regs.stored(MEM_CONF) >> MEM_CONF_TXFIFO_EMPTY_THRHD_H3_SHIFT)
            & MEM_CONF_H3_MASK;
        (low | (high << 7)) as usize
    }

    fn tout_enabled(&self) -> bool {
        self.regs.stored(CONF1) & CONF1_RX_TOUT_EN != 0
    }

    /// `conf1.rx_tout_thrhd` (24:30) with `mem_conf`'s three high bits, in
    /// **symbols** (see [`Uart::config`]).
    fn tout_symbols(&self) -> u64 {
        let low = (self.regs.stored(CONF1) >> CONF1_RX_TOUT_THRHD_SHIFT) & CONF1_RX_TOUT_THRHD_MASK;
        let high =
            (self.regs.stored(MEM_CONF) >> MEM_CONF_RX_TOUT_THRHD_H3_SHIFT) & MEM_CONF_H3_MASK;
        u64::from(low | (high << 7))
    }

    // ---- interrupt state ---------------------------------------------------

    /// The engine's named sticky events in *this* block's `int_raw` bits.
    fn sticky_word(&self) -> u32 {
        let ev = self.engine.sticky();
        let mut word = 0;
        if ev.rx_overflow {
            word |= INT_RXFIFO_OVF;
        }
        if ev.rx_timeout {
            word |= INT_RXFIFO_TOUT;
        }
        if ev.tx_done {
            word |= INT_TX_DONE;
        }
        word
    }

    /// The inverse: which named events an `int_clr` word names.
    fn sticky_events(word: u32) -> UartEvents {
        UartEvents {
            rx_overflow: word & INT_RXFIFO_OVF != 0,
            rx_timeout: word & INT_RXFIFO_TOUT != 0,
            tx_done: word & INT_TX_DONE != 0,
        }
    }

    /// `int_raw` as the guest reads it: the sticky events plus the levels.
    fn int_raw(&self) -> u32 {
        let mut raw = self.sticky_word();
        let levels = self.engine.levels(&self.config());
        if levels.rx_over_threshold {
            raw |= INT_RXFIFO_FULL;
        }
        if levels.tx_under_threshold {
            raw |= INT_TXFIFO_EMPTY;
        }
        raw
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.int_raw() & self.regs.stored(INT_ENA);
        cx.irq.set_level(self.irq_source, st != 0);
    }

    // ---- the transmit side -------------------------------------------------

    fn push_tx(&mut self, byte: u8, cx: &mut BusCx<'_>) {
        let cfg = self.config();
        if let TxPush::Dropped { first } = self.engine.push_tx(byte, &cfg, self.ids(), self.name, cx)
        {
            if first {
                let line = format!(
                    "cyc={} pc=0x{:08x} {} TX FIFO full: byte 0x{byte:02x} dropped (the guest \
                     wrote past txfifo_cnt = {FIFO_DEPTH})",
                    cx.now, cx.pc, self.name
                );
                cx.trace.note(&line);
                log::warn!("{}: TX FIFO overflow, byte dropped", self.name);
            }
            return;
        }
        self.update_lines(cx);
    }

    // ---- the receive side --------------------------------------------------

    fn pop_rx(&mut self, cx: &mut BusCx<'_>) -> u8 {
        let byte = self.engine.pop_rx(self.ids(), cx);
        self.update_lines(cx);
        byte
    }

    /// Ask the host source for its next byte, delivered at cycle `at`.
    fn poll_source(&mut self, at: u64, cx: &mut BusCx<'_>) {
        let cfg = self.config();
        let arrival =
            self.engine
                .poll_source(self.stream, at, &cfg, LIVE_POLL_CYCLES, self.ids(), cx);
        let Some((byte, outcome)) = arrival else {
            return;
        };
        if self.autobaud_enabled() {
            self.autobaud_observe(byte, at);
        }
        match outcome {
            RxDeliver::NotSampled => {
                // The classic has no in-block receive clock gate, so this
                // arm is unreachable today; it is answered rather than
                // ignored so that P4's DPORT clock gate has somewhere to
                // land.
                cx.trace.note(&format!(
                    "cyc={at} {} RX byte 0x{byte:02x} not sampled: the receiver has no clock",
                    self.name
                ));
                return;
            }
            RxDeliver::Overflowed { first: true } => {
                cx.trace.note(&format!(
                    "cyc={at} {} RX FIFO overflow: {FIFO_DEPTH} bytes unread and another \
                     arrived; it is dropped, as the part drops it. The host is sending faster \
                     than the guest is reading — at {} baud, {} cycles a symbol",
                    self.name,
                    cfg.baud,
                    cfg.symbol_cycles.unwrap_or(0),
                ));
            }
            RxDeliver::Overflowed { first: false } | RxDeliver::Queued => {}
        }
        self.update_lines(cx);
    }

    // ---- auto-baud ---------------------------------------------------------

    fn autobaud_enabled(&self) -> bool {
        self.regs.stored(AUTOBAUD) & AUTOBAUD_EN != 0
    }

    /// One bit at the host's rate, in sclk clocks — what the pulse counters
    /// measure a one-bit pulse as. `None` with no clock or no host.
    fn host_bit_clocks(&self) -> Option<u64> {
        let sclk = self.sclk_hz();
        if sclk == 0 || self.host_baud == 0 {
            return None;
        }
        Some((sclk / self.host_baud).max(1))
    }

    /// The glitch filter's threshold in clocks: a pulse narrower than this is
    /// not seen. The classic's `autobaud.glitch_filt` (bits 8:15) has no
    /// separate enable — a value of zero filters nothing.
    fn glitch_filter(&self) -> u64 {
        u64::from((self.regs.stored(AUTOBAUD) >> AUTOBAUD_GLITCH_FILT_SHIFT) & AUTOBAUD_GLITCH_FILT_MASK)
    }

    /// The pulse counters' view of the frame that carried `byte`, landing at
    /// cycle `at`. Called for every byte the wire delivers while
    /// `autobaud_en` is set, before the FIFO sees it: the line toggles
    /// whether or not there is room.
    fn autobaud_observe(&mut self, byte: u8, at: u64) {
        let Some(bit) = self.host_bit_clocks() else {
            return;
        };
        let filter = self.glitch_filter();
        let conf0 = self.regs.stored(CONF0);
        let data_bits = ((conf0 >> CONF0_BIT_NUM_SHIFT) & 3) + 5;
        let stop_half_bits = match (conf0 >> CONF0_STOP_SHIFT) & 3 {
            2 => 3u64,
            3 => 4,
            _ => 2,
        };

        // The line after the idle, one entry per bit: start low, data LSB
        // first, parity. The stop bit(s) follow, high.
        let mut line: Vec<bool> = Vec::with_capacity(11);
        line.push(false);
        for i in 0..data_bits {
            line.push((byte >> i) & 1 != 0);
        }
        if conf0 & CONF0_PARITY_EN != 0 {
            let mask = ((1u16 << data_bits) - 1) as u8;
            let ones_odd = (byte & mask).count_ones() % 2 == 1;
            // `conf0.parity` (bit 0): 0 even, 1 odd.
            let odd = conf0 & CONF0_PARITY != 0;
            line.push(ones_odd != odd);
        }

        // Runs of equal level, `(level, bits)`. The idle before the start bit
        // is high, so the first run is the start bit's low and every run
        // begins with an edge.
        let mut runs: Vec<(bool, u64)> = Vec::with_capacity(11);
        for &level in &line {
            match runs.last_mut() {
                Some((l, n)) if *l == level => *n += 1,
                _ => runs.push((level, 1)),
            }
        }
        // The stop bit(s) are high and open-ended: a high last bit runs into
        // them; a low last bit is closed by their rising edge.
        let trailing_half_bits = match runs.last() {
            Some(&(true, n)) => {
                runs.pop();
                2 * n + stop_half_bits
            }
            _ => stop_half_bits,
        };
        let visible = |bits: u64| bits * bit >= filter;

        // The previous frame's trailing high is a pulse if this frame came
        // one symbol after it — back to back, as a scripted chunk delivers.
        if let Some((prev_at, half_bits)) = self.autobaud.trailing
            && let Some(symbol) = self.symbol_cycles()
            && at == prev_at.saturating_add(symbol)
        {
            let clocks = half_bits * bit / 2;
            if clocks >= filter {
                self.autobaud.high_min = self.autobaud.high_min.min(clamp20(clocks));
            }
        }
        self.autobaud.trailing = Some((at, trailing_half_bits));

        // Every closed run is one edge (the one it starts with); the open
        // trailing high is one more (the rising edge into it).
        let mut edges = 1u32;
        let mut pos = 0u64;
        let mut last_rising: Option<u64> = None;
        let mut last_falling: Option<u64> = None;
        for &(level, bits) in &runs {
            if visible(bits) {
                edges += 1;
                let clocks = clamp20(bits * bit);
                if level {
                    self.autobaud.high_min = self.autobaud.high_min.min(clocks);
                    if let Some(prev) = last_rising {
                        let gap = clamp20((pos - prev) * bit);
                        self.autobaud.pos_min = self.autobaud.pos_min.min(gap);
                    }
                    last_rising = Some(pos);
                } else {
                    self.autobaud.low_min = self.autobaud.low_min.min(clocks);
                    if let Some(prev) = last_falling {
                        let gap = clamp20((pos - prev) * bit);
                        self.autobaud.neg_min = self.autobaud.neg_min.min(gap);
                    }
                    last_falling = Some(pos);
                }
            }
            pos += bits;
        }
        if let Some(prev) = last_rising {
            let gap = clamp20((pos - prev) * bit);
            self.autobaud.pos_min = self.autobaud.pos_min.min(gap);
        }
        self.autobaud.rxd_cnt = (self.autobaud.rxd_cnt + edges).min(RXD_CNT_MAX);
    }

    // ---- registers ---------------------------------------------------------

    /// `mem_rx_status`'s address pair, the errata's own input (module docs).
    fn mem_rx_status(&self) -> u32 {
        let rd = 0u32;
        let wr = self.engine.rx_len() as u32;
        (rd << MEM_RX_RD_ADDR_SHIFT) | (wr << MEM_RX_WR_ADDR_SHIFT)
    }

    fn status(&self) -> u32 {
        let rx = self.engine.rx_len() as u32;
        let tx = self.engine.tx_len() as u32;
        let utx = if self.engine.is_shifting() {
            TX_STRT
        } else {
            TX_IDLE
        };
        // `st_urx_out` stays `RX_IDLE`: a byte on this model's wire is
        // delivered whole, so the receiver's state machine is never caught
        // between two bits. Modeled, and nothing reads it.
        let urx = 0u32;
        (rx & STATUS_RXFIFO_CNT_MASK)
            | (urx << STATUS_ST_URX_OUT_SHIFT)
            | ((tx & STATUS_RXFIFO_CNT_MASK) << STATUS_TXFIFO_CNT_SHIFT)
            | (utx << STATUS_ST_UTX_OUT_SHIFT)
    }

    /// `mem_cnt_status`: the three most significant bits of each FIFO count
    /// (`rx_mem_cnt` 0:2, `tx_mem_cnt` 3:5), which `status` holds the low
    /// eight of.
    fn mem_cnt_status(&self) -> u32 {
        let rx = (self.engine.rx_len() as u32) >> 8;
        let tx = (self.engine.tx_len() as u32) >> 8;
        (rx & 0x7) | ((tx & 0x7) << 3)
    }

    fn read_word(&self, off: u32) -> u32 {
        match off {
            INT_RAW => self.int_raw(),
            INT_ST => self.int_raw() & self.regs.stored(INT_ENA),
            INT_CLR => 0,
            STATUS => self.status(),
            MEM_CNT_STATUS => self.mem_cnt_status(),
            MEM_RX_STATUS => self.mem_rx_status(),
            LOWPULSE => self.autobaud.low_min,
            HIGHPULSE => self.autobaud.high_min,
            RXD_CNT => self.autobaud.rxd_cnt,
            POSPULSE => self.autobaud.pos_min,
            NEGPULSE => self.autobaud.neg_min,
            other => self.regs.effective(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            FIFO => self.push_tx((value & 0xff) as u8, cx),
            // Read-only on this part (the generated table's `access` list).
            INT_RAW | INT_ST | STATUS | MEM_TX_STATUS | MEM_RX_STATUS | MEM_CNT_STATUS
            | LOWPULSE | HIGHPULSE | RXD_CNT | POSPULSE | NEGPULSE => {}
            INT_ENA => {
                self.regs.poke(INT_ENA, value & INT_MASK);
                self.update_lines(cx);
            }
            INT_CLR => {
                // Levels cannot be cleared while they hold. `rxfifo_tout` is
                // the classic's third case: it clears only with an empty
                // receive FIFO (module docs, esp-hal `uart/mod.rs:1180-1183`).
                let mut named = Self::sticky_events(value & !INT_LEVEL_BITS);
                if named.rx_timeout && self.engine.rx_len() > 0 {
                    named.rx_timeout = false;
                }
                self.engine.clear_sticky(named);
                self.update_lines(cx);
            }
            CONF0 => {
                let was_detecting = self.autobaud_enabled();
                self.regs.poke(CONF0, value);
                let _ = was_detecting;
                if value & CONF0_RXFIFO_RST != 0 {
                    self.engine.reset_rx(self.ids(), cx);
                }
                if value & CONF0_TXFIFO_RST != 0 {
                    self.engine.reset_tx();
                }
                // A divider or a frame-format change moves the symbol time;
                // a transmitter that was stalled picks up where it left off.
                let cfg = self.config();
                self.engine
                    .start_shifter_if_idle(cx.now, &cfg, self.ids(), self.name, cx);
                self.update_lines(cx);
            }
            CONF1 => {
                self.regs.poke(CONF1, value);
                let cfg = self.config();
                self.engine.rearm_tout(cx.now, &cfg, self.ids(), cx);
                self.update_lines(cx);
            }
            CLKDIV => {
                self.regs.poke(CLKDIV, value);
                let cfg = self.config();
                self.engine
                    .start_shifter_if_idle(cx.now, &cfg, self.ids(), self.name, cx);
            }
            AUTOBAUD => {
                let was_detecting = self.autobaud_enabled();
                self.regs.poke(AUTOBAUD, value);
                if !was_detecting && value & AUTOBAUD_EN != 0 {
                    // The counters restart on the enable's rising edge
                    // (modeled; the PAC states no rule, and on the ROM's path
                    // each is read once between an enable and a disable, so
                    // the choice is invisible there).
                    self.autobaud = Autobaud::RESET;
                    let line = format!(
                        "cyc={} pc=0x{:08x} {} auto-baud enabled: counting edges and pulse \
                         minima for a host at {} baud ({} sclk clocks per bit)",
                        cx.now,
                        cx.pc,
                        self.name,
                        self.host_baud,
                        self.host_bit_clocks().unwrap_or(0)
                    );
                    cx.trace.note(&line);
                }
            }
            MEM_CONF => {
                self.regs.poke(MEM_CONF, value);
                let rx_size = (value >> MEM_CONF_RX_SIZE_SHIFT) & MEM_CONF_SIZE_MASK;
                let tx_size = (value >> MEM_CONF_TX_SIZE_SHIFT) & MEM_CONF_SIZE_MASK;
                if !self.warned_mem_size
                    && (rx_size != MEM_CONF_ONE_BLOCK || tx_size != MEM_CONF_ONE_BLOCK)
                {
                    self.warned_mem_size = true;
                    let line = format!(
                        "cyc={} pc=0x{:08x} {} mem_conf re-allocates the shared FIFO memory \
                         (rx_size={rx_size}, tx_size={tx_size}); this model keeps its \
                         {FIFO_DEPTH}-byte depth both ways",
                        cx.now, cx.pc, self.name
                    );
                    cx.trace.note(&line);
                    log::warn!("{}: mem_conf FIFO sizes are remembered, not applied", self.name);
                }
                self.update_lines(cx);
            }
            AT_CMD_CHAR => {
                self.regs.poke(AT_CMD_CHAR, value);
                if !self.warned_at_cmd && (value >> 8) & 0xff != 0 {
                    self.warned_at_cmd = true;
                    let line = format!(
                        "cyc={} pc=0x{:08x} {} at_cmd_char = 0x{value:08x}: the AT-command \
                         detector is not modelled and never fires",
                        cx.now, cx.pc, self.name
                    );
                    cx.trace.note(&line);
                }
            }
            other => self.regs.poke(other, value),
        }
    }

    /// Bytes waiting in the TX FIFO plus the one on the wire — what a run
    /// that stops now has not yet delivered.
    pub fn tx_pending(&self) -> usize {
        self.engine.tx_pending()
    }

    pub fn tx_dropped(&self) -> u64 {
        self.engine.tx_dropped()
    }
}

impl Peripheral for Uart {
    fn name(&self) -> &'static str {
        self.name
    }

    fn attached(&mut self, index: usize) {
        self.index = index;
    }

    fn started(&mut self, cx: &mut BusCx<'_>) {
        // The first poll of the host source; every later one schedules the
        // next. A null source has nothing ready and is not live, so this
        // schedules nothing.
        self.poll_source(cx.now, cx);
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let word = off & !3;
        if word == FIFO {
            // A pop is a side effect: only lane 0 carries the byte. The ROM
            // reads a word, esp-hal reads `fifo.rxfifo_rd_byte` — also a word
            // load on this part (`uart/mod.rs:3590-3600`).
            if off & 3 == 0 {
                let byte = self.pop_rx(cx);
                return lane_of(u32::from(byte), off, width);
            }
            return 0;
        }
        lane_of(self.read_word(word), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        if word == FIFO {
            // A push is a side effect: the ROM's `uart_tx_one_char` stores a
            // word and esp-hal's `write_byte` writes the same register; both
            // put the byte in lane 0.
            if off & 3 == 0 {
                self.push_tx((value & 0xff) as u8, cx);
            }
            return;
        }
        let merged = merge_lane(self.read_word(word), off, width, value);
        self.write_word(word, merged, cx);
    }

    fn on_event(&mut self, id: EventId, cx: &mut BusCx<'_>) {
        match event_local(id) {
            EV_TX => {
                let cfg = self.config();
                self.engine
                    .on_tx_due(self.stream, &cfg, self.ids(), self.name, cx);
                self.update_lines(cx);
            }
            EV_RX_POLL => {
                let due = self.engine.rx_due();
                self.poll_source(due, cx);
            }
            EV_RX_TOUT => {
                let cfg = self.config();
                self.engine.on_rx_timeout(&cfg);
                self.update_lines(cx);
            }
            _ => {}
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::UART0.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        Some(self.grades.grade(off))
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn save_state(&self) -> Vec<u8> {
        // The byte format is pinned by the round-trip test below.
        let mut out = Vec::with_capacity(UART_LEN as usize + 2 * FIFO_DEPTH + 128);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.sticky_word().to_le_bytes());
        self.engine.save_counters(&mut out);
        let flags = u32::from(self.warned_at_cmd) | (u32::from(self.warned_mem_size) << 1);
        out.extend_from_slice(&flags.to_le_bytes());
        self.engine.save_stream(&mut out);
        out.extend_from_slice(&self.host_baud.to_le_bytes());
        let ab = &self.autobaud;
        for v in [ab.rxd_cnt, ab.low_min, ab.high_min, ab.pos_min, ab.neg_min] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let (trail_at, trail_half) = ab.trailing.unwrap_or((u64::MAX, 0));
        out.extend_from_slice(&trail_at.to_le_bytes());
        out.extend_from_slice(&trail_half.to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(index), Some(sticky)) = (r.u64(), r.u32()) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let Some((counters, used)) = UartEngine::load_counters(r.0) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        r.0 = &r.0[used..];
        let Some(flags) = r.u32() else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let Some((stream, used)) = UartEngine::load_stream(r.0) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        r.0 = &r.0[used..];
        let (
            Some(host_baud),
            Some(rxd_cnt),
            Some(low_min),
            Some(high_min),
            Some(pos_min),
            Some(neg_min),
            Some(trail_at),
            Some(trail_half),
        ) = (
            r.u64(),
            r.u32(),
            r.u32(),
            r.u32(),
            r.u32(),
            r.u32(),
            r.u64(),
            r.u64(),
        )
        else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let rest = r.0;
        self.host_baud = host_baud;
        self.autobaud = Autobaud {
            rxd_cnt,
            low_min,
            high_min,
            pos_min,
            neg_min,
            trailing: (trail_at != u64::MAX).then_some((trail_at, trail_half)),
        };
        self.index = index as usize;
        self.engine.set_sticky(Self::sticky_events(sticky));
        self.engine.restore(counters, stream);
        self.warned_at_cmd = flags & 1 != 0;
        self.warned_mem_size = flags & 2 != 0;
        self.regs.load_state(rest);
    }
}

/// A little-endian cursor over a state blob.
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

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::{ByteLog, Sandbox, ScriptedSource};

    /// The PAC's reset for one register of this block, from the generated
    /// table — the same value `with_names` seeded, so a test can never drift
    /// from the model by carrying its own copy.
    fn pac(off: u32) -> u32 {
        regs::UART0
            .reset(off)
            .expect("the PAC gives this register a non-zero reset")
    }

    /// A sandbox with the `uart0` stream on a memory sink, a scripted source,
    /// and a UART0 on it.
    fn rig(script: ScriptedSource) -> (Sandbox, Uart, ByteLog) {
        let mut sb = Sandbox::new();
        let log = ByteLog::new();
        let id = sb.host.add(
            "uart0",
            Box::new(lp_emu_esp_common::host::MemorySink(log.clone())),
            Box::new(script),
        );
        let mut u = Uart::uart0(Some(id));
        u.attached(7);
        u.started(&mut sb.cx());
        (sb, u, log)
    }

    /// 115,200-ish from APB: `clkdiv` 0x2b6 = 694, `80e6 / 694 = 115,273`
    /// baud; one bit is `ceil(240e6 × 11104 / 1.28e9)` = 2,082 cycles and a
    /// 10-bit symbol is 20,820.
    const SYMBOL_ROM: u64 = 20_820;

    #[test]
    fn the_reset_state_is_the_rom_console_at_115200_with_an_empty_fifo() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new());
        assert_eq!(
            u.regs.stored(CLKDIV),
            pac(CLKDIV),
            "no register in this block deviates from the PAC"
        );
        assert_eq!(u.baud(), 115_273);
        assert_eq!(u.symbol_cycles(), Some(SYMBOL_ROM));
        assert_eq!(
            sb.read(&mut u, STATUS),
            0,
            "the classic PAC resets `status` to zero: both counts 0, every line bit 0"
        );
        // `conf0`'s reset is 8N1 with the APB clock selected.
        assert_eq!(pac(CONF0), 0x0800_001c);
        assert_eq!(u.symbol_half_bits(), 20, "start + 8 data + 1 stop");
        assert_eq!(u.sclk_hz(), super::super::APB_HZ);
        // `conf1`'s reset is 96 both ways, so an empty TX FIFO is under its
        // threshold and `txfifo_empty` is set before anything is written.
        assert_eq!(u.rx_full_thrhd(), 96);
        assert_eq!(u.tx_empty_thrhd(), 96);
        assert_eq!(sb.read(&mut u, INT_RAW), INT_TXFIFO_EMPTY);
    }

    /// The reference derivation for the shipped image's rate: what
    /// `board/esp32v3/init.rs` programs, and what it costs per byte.
    #[test]
    fn the_921600_divisor_the_image_programs_gives_the_right_symbol_time() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new());
        // 80e6 × 16 / 921,600 = 1388.9 → the integer divider esp-hal writes.
        sb.write(&mut u, CLKDIV, 1_388 / 16 | ((1_388 % 16) << CLKDIV_FRAG_SHIFT));
        assert_eq!(u.divider16(), 1_388);
        assert_eq!(u.baud(), 922_190, "80e6 × 16 / 1388, 0.06 % over 921,600");
        // 240e6 / 922,190 = 260.25 → 261 cycles a bit, 2,610 a 10-bit symbol.
        assert_eq!(u.bit_cycles(), Some(261));
        assert_eq!(u.symbol_cycles(), Some(2_610));
    }

    #[test]
    fn a_byte_written_to_the_fifo_leaves_the_wire_one_symbol_later() {
        let (mut sb, mut u, log) = rig(ScriptedSource::new());
        sb.write(&mut u, FIFO, u32::from(b'A'));
        assert_eq!(
            sb.read(&mut u, STATUS) >> STATUS_TXFIFO_CNT_SHIFT & 0xff,
            0,
            "the byte went straight to the shifter, not to the FIFO"
        );
        assert_eq!(
            sb.read(&mut u, STATUS) >> STATUS_ST_UTX_OUT_SHIFT & 0xf,
            TX_STRT,
            "and the transmitter's state machine is not idle"
        );
        assert!(log.is_empty());
        sb.run_to(&mut u, SYMBOL_ROM);
        assert_eq!(log.text(), "A");
        assert!(u.engine.sticky().tx_done);
        assert_eq!(
            sb.read(&mut u, STATUS) >> STATUS_ST_UTX_OUT_SHIFT & 0xf,
            TX_IDLE
        );
    }

    /// The ROM's own spin: `uart_tx_one_char` waits while `status` bit 23 is
    /// set, which is `txfifo_cnt >= 128`. A model whose count never reaches
    /// 128 is P3's accept block, and it is why this phase exists.
    #[test]
    fn the_rom_sees_bit_23_when_the_transmit_fifo_is_full() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new());
        // The first byte goes to the shifter; 128 more fill the FIFO.
        for i in 0..=FIFO_DEPTH {
            sb.write(&mut u, FIFO, u32::from(i as u8));
        }
        assert_eq!(u.engine.tx_len(), FIFO_DEPTH);
        let status = sb.read(&mut u, STATUS);
        assert_eq!(
            (status >> STATUS_TXFIFO_CNT_SHIFT) & 0xff,
            FIFO_DEPTH as u32
        );
        assert_ne!(
            status & 0x0080_0000,
            0,
            "the bit the ROM's `uart_tx_one_char` tests"
        );
        assert_eq!(u.tx_dropped(), 0, "nothing has been dropped yet");
        // The count's three high bits live in `mem_cnt_status`, and 128 fits
        // in the low eight, so it is still zero.
        assert_eq!(sb.read(&mut u, MEM_CNT_STATUS), 0);
    }

    /// esp-hal does not read `status.rxfifo_cnt` on this part. The pair in
    /// `mem_rx_status` must turn back into the true count through the
    /// errata's own formula, or the async reader waits for ever.
    #[test]
    fn the_rx_count_esp_hal_actually_computes_is_the_number_of_bytes_waiting() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new().at(0, b"hello"));
        // Five bytes, one symbol apart.
        sb.run_to(&mut u, 5 * SYMBOL_ROM);
        assert_eq!(u.engine.rx_len(), 5);

        // esp-hal's `rx_fifo_count`, `uart/mod.rs:3637-3658`.
        let esp_hal_rx_count = |sb: &mut Sandbox, u: &mut Uart| -> u32 {
            let fifo_cnt = sb.read(u, STATUS) & STATUS_RXFIFO_CNT_MASK;
            let status = sb.read(u, MEM_RX_STATUS);
            let rd = (status >> MEM_RX_RD_ADDR_SHIFT) & 0x7ff;
            let wr = (status >> MEM_RX_WR_ADDR_SHIFT) & 0x7ff;
            if wr > rd {
                wr - rd
            } else if wr < rd {
                wr + FIFO_DEPTH as u32 - rd
            } else if fifo_cnt > 0 {
                FIFO_DEPTH as u32
            } else {
                0
            }
        };
        assert_eq!(esp_hal_rx_count(&mut sb, &mut u), 5);
        assert_eq!(sb.read(&mut u, FIFO) & 0xff, u32::from(b'h'));
        assert_eq!(esp_hal_rx_count(&mut sb, &mut u), 4);
        for _ in 0..4 {
            sb.read(&mut u, FIFO);
        }
        assert_eq!(esp_hal_rx_count(&mut sb, &mut u), 0, "and empty reads empty");
    }

    #[test]
    fn the_levels_are_derived_and_an_int_clr_cannot_clear_one_that_holds() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new().at(0, b"abcd"));
        // Drop the receive threshold to two so four bytes are over it.
        let conf1 = (pac(CONF1) & !CONF1_RXFIFO_FULL_THRHD_MASK) | 2;
        sb.write(&mut u, CONF1, conf1);
        sb.run_to(&mut u, 4 * SYMBOL_ROM);
        assert_eq!(u.engine.rx_len(), 4);
        assert_ne!(sb.read(&mut u, INT_RAW) & INT_RXFIFO_FULL, 0);
        sb.write(&mut u, INT_CLR, u32::MAX);
        assert_ne!(
            sb.read(&mut u, INT_RAW) & INT_RXFIFO_FULL,
            0,
            "a level is not sticky and an int_clr write cannot clear one"
        );
        // Reading it down to two clears it: the level is `len > thrhd`.
        sb.read(&mut u, FIFO);
        sb.read(&mut u, FIFO);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_RXFIFO_FULL, 0);
    }

    /// The classic's third category, beside a level and a sticky event.
    #[test]
    fn the_receive_timeout_clears_only_once_the_fifo_is_empty() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new().at(0, b"ab"));
        // One symbol of timeout, enabled.
        let conf1 = pac(CONF1) | CONF1_RX_TOUT_EN | (1 << CONF1_RX_TOUT_THRHD_SHIFT);
        sb.write(&mut u, CONF1, conf1);
        assert_eq!(u.tout_symbols(), 1);
        sb.run_to(&mut u, 2 * SYMBOL_ROM);
        assert_eq!(u.engine.rx_len(), 2);
        // Eight bit-times after the last byte.
        sb.run_to(&mut u, 2 * SYMBOL_ROM + 8 * 2_082);
        assert_ne!(sb.read(&mut u, INT_RAW) & INT_RXFIFO_TOUT, 0);

        sb.write(&mut u, INT_CLR, INT_RXFIFO_TOUT);
        assert_ne!(
            sb.read(&mut u, INT_RAW) & INT_RXFIFO_TOUT,
            0,
            "esp-hal: on ESP32 the timeout cannot be cleared unless the FIFO is empty"
        );
        sb.read(&mut u, FIFO);
        sb.read(&mut u, FIFO);
        sb.write(&mut u, INT_CLR, INT_RXFIFO_TOUT);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_RXFIFO_TOUT, 0);
    }

    #[test]
    fn a_fifo_reset_bit_is_the_classics_own_bit_number() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new().at(0, b"xy"));
        sb.run_to(&mut u, 2 * SYMBOL_ROM);
        assert_eq!(u.engine.rx_len(), 2);
        // The C6's bit 22 does nothing here.
        sb.write(&mut u, CONF0, pac(CONF0) | (1 << 22));
        assert_eq!(u.engine.rx_len(), 2, "bit 22 is not this chip's rxfifo_rst");
        sb.write(&mut u, CONF0, pac(CONF0) | CONF0_RXFIFO_RST);
        assert_eq!(u.engine.rx_len(), 0);
    }

    /// The clock selection is a `conf0` bit, not a separate register.
    #[test]
    fn clearing_tick_ref_always_on_moves_the_block_onto_ref_tick() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new());
        sb.write(&mut u, CONF0, pac(CONF0) & !CONF0_TICK_REF_ALWAYS_ON);
        assert_eq!(u.sclk_hz(), REF_TICK_HZ);
        assert_eq!(u.baud(), 1_000_000 * 16 / (694 * 16), "1 MHz / 694");
    }

    /// The auto-baud counters, against a host at a stated rate: a `0x55`
    /// frame is ten one-bit pulses, so both minima are one bit in sclk
    /// clocks. `80e6 / 115200 = 694` (694.4: a counter cannot see the
    /// fraction).
    #[test]
    fn the_autobaud_counters_measure_the_host_rate_the_run_states() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new().at(0, b"\x55\x55"));
        assert_eq!(sb.read(&mut u, LOWPULSE), PULSE_MIN_RESET, "reset: all ones");
        assert_eq!(sb.read(&mut u, RXD_CNT), 0);
        sb.write(&mut u, AUTOBAUD, pac(AUTOBAUD) | AUTOBAUD_EN);
        sb.run_to(&mut u, 2 * SYMBOL_ROM);
        assert_eq!(u.host_bit_clocks(), Some(694));
        assert_eq!(sb.read(&mut u, LOWPULSE), 694);
        assert_eq!(sb.read(&mut u, HIGHPULSE), 694);
        assert!(sb.read(&mut u, RXD_CNT) >= 10, "ten edges in a 0x55 frame");
        // Changing the stated host rate changes what the counters report,
        // which is the whole point of the feature on silicon.
        let mut faster = Uart::uart0(None).with_host_baud(230_400);
        faster.regs.poke(CONF0, pac(CONF0));
        assert_eq!(faster.host_bit_clocks(), Some(347));
    }

    #[test]
    fn the_state_round_trips() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new().at(0, b"r"));
        sb.write(&mut u, FIFO, u32::from(b'A'));
        sb.write(&mut u, FIFO, u32::from(b'B'));
        sb.run_to(&mut u, 1);
        let blob = u.save_state();

        let mut other = Uart::uart0(None);
        other.load_state(&blob);
        assert_eq!(other.index, u.index);
        assert_eq!(other.engine.tx_len(), u.engine.tx_len());
        assert_eq!(other.engine.shifter(), u.engine.shifter());
        assert_eq!(other.host_baud, u.host_baud);
        assert_eq!(other.autobaud, u.autobaud);
        assert_eq!(other.regs.stored(CLKDIV), u.regs.stored(CLKDIV));

        let mut short = Uart::uart0(None);
        short.load_state(&blob[..3]);
        assert_eq!(
            short.engine.tx_len(),
            0,
            "a short blob is refused, not half-applied"
        );
    }

    /// UART1 is the same view at its own base and its own source number.
    #[test]
    fn uart1_is_the_same_model_with_its_own_interrupt_source() {
        let u = Uart::uart1(None);
        assert_eq!(u.name(), "UART1");
        assert_eq!(u.irq_source, SOURCE_UART1);
        assert_eq!(Uart::uart0(None).irq_source, SOURCE_UART0);
    }

    /// The grade table answers for every offset in the window, and nothing in
    /// this block claims to be measured.
    #[test]
    fn no_register_in_this_block_is_graded_measured() {
        let grades = Uart::grades();
        for off in (0..UART_LEN).step_by(4) {
            assert_ne!(
                grades.grade(off),
                RegGrade::Measured,
                "{off:#x} — no register-level transcript of this block exists for the classic"
            );
        }
        assert!(
            Uart::modeled_registers().contains(&"mem_rx_status"),
            "the address pair is a modelled choice and says so"
        );
    }
}
