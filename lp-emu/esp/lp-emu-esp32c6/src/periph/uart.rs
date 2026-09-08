//! `UART0` / `UART1` at `0x6000_0000` / `0x6000_1000` — a real FIFO pair,
//! a shifter that drains at the configured baud in emulated time, the
//! thresholds and the receive timeout, and a host stream on the outside.
//!
//! Register facts are the esp32c6 PAC 0.23.2 `uart0` block (offsets in
//! `regs::UART0`; bit positions and reset values cited per register below)
//! and esp-hal 1.1.1's driver (`src/uart/mod.rs`, discovery §5).
//!
//! # Who drives it
//!
//! Two very different clients share this block, and both are the reason it
//! has to be a model rather than an accept table:
//!
//! - the **mask ROM's** `uart_tx_one_char` (`0x4002_2CDA`, reached through the
//!   `0x4000_0058` trampoline): `uart_serial_tx_one_char(0, byte)` spins on
//!   `status.txfifo_cnt` until it is at most 127 and then does a **word**
//!   store to `fifo`. The channel byte it routes by (`ets_printf_uart`, a ROM
//!   `.bss` byte at `0x4087_FA98`) is zero after a direct load, so the real
//!   ROM reaches UART0 with no hook. The spike image's tee and esp-println's
//!   `uart` printer come in this way.
//! - **esp-hal's async driver** (the spike image's `io_task`): `write_async`
//!   fills the FIFO and awaits `txfifo_empty`; `read_async` sets
//!   `conf1.rxfifo_full_thrhd` to the buffer size and awaits `rxfifo_full |
//!   rxfifo_tout | …`; the ISR reads `int_st`, clears the fired bits out of
//!   `int_ena`, and wakes.
//!
//! # Timing (the time class — reported, never compared)
//!
//! One symbol takes `symbol_bits × CPU_HZ × (clkdiv·16 + frag) / (sclk × 16)`
//! cycles: `sclk` is the source PCR selects (`pcr::ClockLine`: 1 = 80 MHz,
//! 2 = RC_FAST 17.5 MHz, 3 = XTAL 40 MHz, divided by `sclk_div_num + 1`) and
//! `symbol_bits` comes from `conf0` (start + data + parity + stop). Bytes
//! leave the shifter one symbol apart, and are written to the host sink as
//! they leave, so a transcript's byte order is the guest's order at the
//! guest's rate. That is what reproduces the ROM console's 7 ms per 81-byte
//! line at 115,200 (spike report §11.1) — silicon spends it, esp-emu did not.
//!
//! **Reset values are "as the ROM boot leaves them", grade modeled.** The PAC
//! resets `clkdiv` to `0x02B6` (the pre-boot value); the ROM's `uartAttach`/
//! `Uart_Init` then programs 115,200 from XTAL — `(40 MHz << 4) / 115200 =
//! 5555` → `clkdiv = 347, frag = 3` — and a direct load never runs the ROM
//! boot, so this block starts there ([`CLKDIV_RESET`]). PCR's
//! `uart(n).clk_conf` reset is the PAC's `0x0070_0000` (XTAL, enabled), which
//! agrees. Every *other* register in the block starts at the PAC's own
//! reset, seeded from the generated table — `status` included, which is why
//! a boot trace shows `status = 0xe000c000` (`txd`, `rtsn`, `dtrn`, `rxd`,
//! `ctsn` all high) before the driver touches it.
//!
//! # Interrupts (source 43 / 44, level = `int_raw & int_ena != 0`)
//!
//! `rxfifo_full` (bit 0) and `txfifo_empty` (bit 1) are **levels**: set while
//! `rxfifo_cnt > rxfifo_full_thrhd` / `txfifo_cnt < txfifo_empty_thrhd`, and an
//! `int_clr` write cannot clear them while the condition holds — esp-hal
//! says as much about `rxfifo_full` ("not cleared until the FIFO actually
//! drops below the threshold", `uart/mod.rs:3697`), and `int_raw`'s PAC reset
//! of `0x02` (`txfifo_empty` set on an empty FIFO) says it about the other.
//! `rxfifo_ovf` (4), `rxfifo_tout` (8), `tx_done` (14) are sticky events
//! cleared by `int_clr`. `at_cmd_char_det` (18) is implemented as a detector
//! that never fires: the async read listens for it whenever `at_cmd_char.
//! char_num > 0` (the PAC reset is `0x032B`: three `+`), but it only ever
//! *adds* the event to the set it waits on, never waits on it alone, so a
//! silent detector changes nothing on the read path. Stated here so it is
//! not mistaken for modelled.
//!
//! # The outside
//!
//! TX bytes go to one host stream (`HostSinks`: stdout, a file, a TCP
//! listener, or memory); RX bytes come from the same stream's source. A
//! scripted source ([`lp_emu_esp_common::ScriptedSource`]) delivers its
//! chunk at a declared cycle, one byte per symbol from there; a live socket
//! is polled every [`LIVE_POLL_CYCLES`] and its bytes arrive at the cycle
//! the poll landed on — deterministic only with the script.
//!
//! # Auto-baud: the mask ROM's UART0 download console (RD9, M3 P1)
//!
//! The ROM's download console will not read a byte from UART0 until it has
//! measured the host's baud. `UartConnCheck(0)` (`0x4002_3388`) calls
//! `uart_baudrate_detect(0, 1)` → `uart_serial_baudrate_detect`
//! (`0x4002_2B20`), which enables the glitch filter (`rx_filt = 0x104`: on,
//! four clocks) and `conf0.autobaud_en`, then polls `rxd_cnt` until more than
//! `0x7f` edges have been counted, reads `lowpulse` and `highpulse`, disables
//! auto-baud and returns `((low + high) << 3) + 16` — a clock divisor in
//! sixteenths that `uart_div_modify` writes to `clkdiv`. Until that returns
//! non-zero the console byte at `0x4087_F580` stays on whichever port is being
//! probed and UART0's ring buffer is drained unread on every probe, so a
//! model with counters that read zero is a console that never answers on
//! UART0 (M7 deviation 4). The USB console never reaches this code: F19,
//! `uart_baudrate_detect` returns 1 at once for any port above 1.
//!
//! What the counters hold is the PAC's statement (`uart0::rxd_cnt`,
//! `lowpulse`, `highpulse`, `pospulse`, `negpulse`: "used in baud
//! rate-detect process"), and this model computes exactly that from two
//! inputs it is given rather than guesses: **the bytes the host actually
//! sent** and **the host's stated baud** ([`Uart::with_host_baud`], the CLI's
//! `--uart0-baud`, default [`DEFAULT_HOST_BAUD`]). The counters count in the
//! block's source clock (`sclk`, PCR's selection — XTAL 40 MHz after the ROM's
//! `uart_hal_clk_source_sel(0, 3)`), so one bit at the host's rate is
//! `sclk / host_baud` clocks, floored, because a counter cannot see the
//! fraction:
//!
//! ```text
//! 40,000,000 / 115,200 = 347.2 → 347 clocks per bit
//! a 0x55 frame is idle 1, start 0, 1 0 1 0 1 0 1 0, stop 1: ten edges, every pulse one bit wide
//! lowpulse = highpulse = 347
//! ROM: ((347 + 347) << 3) + 16 = 5568 sixteenths → uart_div_modify(0, 5568)
//!      clkdiv = 5568 >> 4 = 348, frag = 5568 & 0xf = 0 → 40e6 × 16 / 5568 = 114,943 baud
//! ```
//!
//! Nothing here is chosen to make that land: change `--uart0-baud` and the
//! divisor the ROM computes follows the host, which is the whole point of
//! the feature on silicon. Per frame the model counts every level change
//! along idle → start → data (LSB first) → parity → stop (the stop bit's
//! rising edge included, the next frame's start counted with that frame);
//! `lowpulse` is the shortest low run, `highpulse` the shortest high run
//! that is closed by a falling edge — a run inside the frame, or the stop
//! bit(s) when the next frame follows back to back, which is how a scripted
//! chunk delivers. An idle gap between chunks is not a pulse this model
//! measures. `pospulse` / `negpulse` are the shortest distance between two
//! rising / two falling edges within one frame. Pulses narrower than
//! `rx_filt.glitch_filt` clocks are ignored while the filter is enabled
//! (never the case at any baud this model is given: the ROM's four clocks
//! against 347). The counters restart on the enable's rising edge —
//! *modeled*: the PAC states no rule, and on the ROM's path each is read
//! once between an enable and a disable, so the choice is invisible there.
//! `rxd_cnt` saturates at its ten bits, for the same reason and with the
//! same caveat.
//!
//! **What the ROM then does with the first SYNC on UART0 is the ROM's.**
//! `uart_div_modify` resets both FIFOs, and the detector returns while the
//! frame that fed it is still on the wire, so the SYNC that carries the
//! 128th edge is consumed by detection and never parsed; the next SYNC is
//! answered. esptool sends up to seven per attempt for exactly this reason,
//! and `scripts/rom-download-sync.uart0` sends two.
//!
//! # Per-register grades (`--strict-grade`)
//!
//! | grade | registers | why |
//! |---|---|---|
//! | `measured` | `fifo`, `int_raw`, `int_st`, `int_ena`, `int_clr`, `clkdiv`, `status`, `conf0`, `conf1`, `tout_conf` | the UART0-link transcript pairs under `lp-emu/transcripts/esp32c6/` — `boot-idle` and `shader-compile-stress` at `d6cfaa205` (the `spike_uart0_link` image) and the `upload-walk` replay — carry the whole conversation over this block byte for byte against silicon at 115,200: the ROM's word stores and `txfifo_cnt` spins, esp-hal's `write_async` / `read_async` with their thresholds, the receive timeout that ends every `M!` read, the interrupt quartet, and the 7 ms an 81-byte line takes (`clkdiv`) |
//! | `documented` | `rx_filt`, `mem_tx_status`, `mem_rx_status`, `fsm_status`, `pospulse`, `negpulse`, `lowpulse`, `highpulse`, `rxd_cnt`, `clk_conf`, `reg_update` | the PAC states what each holds or does and this file implements that statement: the auto-baud section above; `reg_update` "write 1 … cleared by hardware after synchronization", answered at once; `fsm_status` busy while a symbol is on the wire; the two memory-status counters; `clk_conf`'s `tx_sclk_en` / `rx_sclk_en` (bits 24 / 25, "enable UART Tx / Rx clock") gate the shifter and the receiver — the ROM's `uart_hal_clk_enable` writes exactly those two (`0x0300_0000`) and the PAC reset has them set, while the source select and divider the PAC still lists in this register are PCR's on the C6 ([`ClockLine`]) and the `*_rst_core` pulses are stored, not acted on; no transcript has measured any of these |
//! | `modeled` | everything else | stored and read back at the PAC's reset, behaviour not modelled: `at_cmd_char` (a detector that never fires, see above), `hwfc_conf`, `sleep_conf*`, `swfc_conf*`, `txbrk_conf`, `idle_conf`, `rs485_conf`, `at_cmd_precnt` / `postcnt` / `gaptout`, `mem_conf`, `afifo_status`, `date`, `id` |
//!
//! The mask ROM's console path — both consoles, since the detector probes
//! UART0 on every loop — touches `fifo`, `status`, `fsm_status`, `int_st`,
//! `int_ena`, `int_clr`, `clkdiv`, `rx_filt`, `conf0`, `conf1`, `clk_conf`,
//! `reg_update`, `rxd_cnt`, `lowpulse` and `highpulse`, so the highest level
//! a download-console run passes on this block is **`documented`**: the
//! auto-baud counters it reads are the PAC's statement, computed, and no
//! transcript has measured them (`tests/rom_download_console.rs`).

use std::collections::VecDeque;

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{
    BusCx, Peripheral, RegFile, RegGrade, RegGrades, StreamId, Width, event_id, event_local,
};

use super::pcr::ClockLine;
use super::systimer::Reader;
use crate::memmap;
use crate::regs::{self, source};

// Register offsets (`regs::UART0`).
const FIFO: u32 = 0x00;
const INT_RAW: u32 = 0x04;
const INT_ST: u32 = 0x08;
const INT_ENA: u32 = 0x0c;
const INT_CLR: u32 = 0x10;
const CLKDIV: u32 = 0x14;
const RX_FILT: u32 = 0x18;
const STATUS: u32 = 0x1c;
const CONF0: u32 = 0x20;
const CONF1: u32 = 0x24;
const AT_CMD_CHAR: u32 = 0x5c;
const TOUT_CONF: u32 = 0x64;
const MEM_TX_STATUS: u32 = 0x68;
const MEM_RX_STATUS: u32 = 0x6c;
const FSM_STATUS: u32 = 0x70;
const POSPULSE: u32 = 0x74;
const NEGPULSE: u32 = 0x78;
const LOWPULSE: u32 = 0x7c;
const HIGHPULSE: u32 = 0x80;
const RXD_CNT: u32 = 0x84;
const AFIFO_STATUS: u32 = 0x90;
const REG_UPDATE: u32 = 0x98;

/// FIFO depth, both directions (`esp-metadata` `uart.ram_size` = 128).
pub const FIFO_DEPTH: usize = 128;

/// The host's baud on the wire when a run states none (`--uart0-baud`'s
/// default): what esptool and the ROM console both start at.
pub const DEFAULT_HOST_BAUD: u64 = 115_200;

/// `conf0.autobaud_en` (bit 19): the ROM's `uart_hal_enable_autobaud`.
const CONF0_AUTOBAUD_EN: u32 = 1 << 19;
/// `rx_filt`: `glitch_filt` bits 0:7, `glitch_filt_en` bit 8.
const RX_FILT_EN: u32 = 1 << 8;
/// The four pulse minima are 12-bit counters that reset to all ones (the
/// PAC's `0x0fff` for `pospulse`, `negpulse`, `lowpulse`, `highpulse`).
const PULSE_MIN_RESET: u32 = 0xfff;
/// `rxd_cnt.rxd_edge_cnt` is ten bits.
const RXD_CNT_MAX: u32 = 0x3ff;

/// A pulse count as a 12-bit counter holds it.
fn clamp12(clocks: u64) -> u32 {
    clocks.min(u64::from(PULSE_MIN_RESET)) as u32
}

/// `clk_conf` (`0x88`): bit 24 `tx_sclk_en`, bit 25 `rx_sclk_en` — the
/// C6's own two bits in this register (the source select and divider the
/// PAC still lists here are PCR's on this part, [`ClockLine`]). The ROM's
/// `uart_hal_clk_enable` writes `0x0300_0000`; the PAC reset has both set.
const CLK_CONF: u32 = 0x88;
const CLK_CONF_TX_SCLK_EN: u32 = 1 << 24;
const CLK_CONF_RX_SCLK_EN: u32 = 1 << 25;

/// `clkdiv` as the ROM boot leaves it: 115,200 from XTAL (see the module
/// docs). `frag` is bits 20:23, `clkdiv` bits 0:11.
///
/// **The one register in this block that does not start where the PAC says.**
/// Everything else — `conf0`, `conf1`, `tout_conf`, `at_cmd_*`, `idle_conf`,
/// `clk_conf`, `afifo_status`, `id`, `rx_filt`, `status`, and the pulse
/// counters — is seeded from the generated table by `with_names`.
pub const CLKDIV_RESET: u32 = (3 << 20) | 347;
/// `status` PAC reset: `txd`, `rtsn`, `dtrn` high; `rxd`, `ctsn` high.
const STATUS_IDLE: u32 = 0xe000_c000;
const STATUS_TXD: u32 = 1 << 31;

// `int_raw` bits.
const INT_RXFIFO_FULL: u32 = 1 << 0;
const INT_TXFIFO_EMPTY: u32 = 1 << 1;
const INT_RXFIFO_OVF: u32 = 1 << 4;
const INT_RXFIFO_TOUT: u32 = 1 << 8;
const INT_TX_DONE: u32 = 1 << 14;
const INT_MASK: u32 = 0x000f_ffff;
const INT_LEVEL_BITS: u32 = INT_RXFIFO_FULL | INT_TXFIFO_EMPTY;

// `conf0` bits.
const CONF0_PARITY_EN: u32 = 1 << 1;
const CONF0_BIT_NUM_SHIFT: u32 = 2;
const CONF0_STOP_SHIFT: u32 = 4;
const CONF0_RXFIFO_RST: u32 = 1 << 22;
const CONF0_TXFIFO_RST: u32 = 1 << 23;

// `tout_conf` bits.
const TOUT_EN: u32 = 1;
const TOUT_THRHD_SHIFT: u32 = 2;
const TOUT_THRHD_MASK: u32 = 0x3ff;

// PCR `uart(n).clk_conf` fields (`pcr/uart/clk_conf.rs`).
const PCR_SCLK_SEL_SHIFT: u32 = 20;
const PCR_SCLK_DIV_NUM_SHIFT: u32 = 12;

/// How often a live (socket) source is polled when it has nothing queued:
/// 1 ms of guest time. A byte that arrives between polls waits at most this
/// long, which is under the 2 ms the firmware's own `io_task` sleeps.
pub const LIVE_POLL_CYCLES: u64 = 1_000 * memmap::CYCLES_PER_US;

const EV_TX: u16 = 0;
const EV_RX_POLL: u16 = 1;
const EV_RX_TOUT: u16 = 2;

/// The auto-baud counters (module docs, "Auto-baud"). Live while
/// `conf0.autobaud_en` is set; restarted on its rising edge.
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
    /// The previous frame: the cycle it landed and how many half-bits of
    /// high it ended with (its last data bit if high, plus the stop bits).
    /// A frame landing exactly one symbol later closes that pulse.
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

/// One UART instance.
#[derive(Debug)]
pub struct Uart {
    name: &'static str,
    index: usize,
    irq_source: u16,
    regs: RegFile,
    grades: RegGrades,
    clock: ClockLine,
    stream: Option<StreamId>,
    /// The rate the host at the other end of the wire sends at (RD9): what
    /// the auto-baud counters measure. Not the rate this block is programmed
    /// for — the two agree only once the ROM's detection has run.
    host_baud: u64,
    autobaud: Autobaud,
    tx: VecDeque<u8>,
    rx: VecDeque<u8>,
    /// The byte on the wire, if any.
    shifter: Option<u8>,
    /// The cycle the byte on the wire leaves it. The next symbol is timed
    /// from here, never from the cycle the event happened to be dispatched
    /// at: a slice ends *at or after* an event, and a chain scheduled from
    /// dispatch time would drift by the lateness of every link.
    tx_due: u64,
    /// The cycle the next host byte is delivered (or the source polled).
    /// Same rule.
    rx_due: u64,
    /// Sticky `int_raw` bits (the level bits are derived).
    sticky: u32,
    tx_pushed: u32,
    tx_popped: u32,
    rx_pushed: u32,
    rx_popped: u32,
    /// Bytes the guest wrote into a full TX FIFO, dropped.
    tx_dropped: u64,
    warned_at_cmd: bool,
}

impl Uart {
    fn new(
        name: &'static str,
        irq_source: u16,
        clock: ClockLine,
        stream: Option<StreamId>,
    ) -> Self {
        Self {
            name,
            index: 0,
            irq_source,
            regs: RegFile::new(name, 0x100)
                .with_names(regs::UART0)
                // The one deviation from the PAC in this block: see
                // `CLKDIV_RESET` and the module docs.
                .with_reset(CLKDIV, CLKDIV_RESET),
            grades: Self::grades(),
            clock,
            stream,
            host_baud: DEFAULT_HOST_BAUD,
            autobaud: Autobaud::RESET,
            tx: VecDeque::with_capacity(FIFO_DEPTH),
            rx: VecDeque::with_capacity(FIFO_DEPTH),
            shifter: None,
            tx_due: 0,
            rx_due: 0,
            sticky: 0,
            tx_pushed: 0,
            tx_popped: 0,
            rx_pushed: 0,
            rx_popped: 0,
            tx_dropped: 0,
            warned_at_cmd: false,
        }
    }

    /// `UART0`, the console: source 43, the `uart0` host stream.
    pub fn uart0(clock: ClockLine, stream: Option<StreamId>) -> Self {
        Self::new("UART0", source::UART0, clock, stream)
    }

    /// `UART1`: source 44. The firmware never opens it; it gets the same
    /// model so a guest that does is not answered by a table.
    pub fn uart1(clock: ClockLine, stream: Option<StreamId>) -> Self {
        Self::new("UART1", source::UART1, clock, stream)
    }

    /// State the host's baud on the wire (`--uart0-baud`; default
    /// [`DEFAULT_HOST_BAUD`]). The auto-baud counters measure this rate; the
    /// bytes themselves are delivered at the block's own symbol time, as
    /// before, so nothing about an existing transcript moves.
    pub fn with_host_baud(mut self, baud: u64) -> Self {
        self.host_baud = baud;
        self
    }

    pub fn host_baud(&self) -> u64 {
        self.host_baud
    }

    /// The per-register grade table (the module docs' table).
    pub fn grades() -> RegGrades {
        RegGrades::new()
            // The UART0-link transcript pairs exercise these end to end.
            .with_grade(FIFO, RegGrade::Measured)
            .with_grade(INT_RAW, RegGrade::Measured)
            .with_grade(INT_ST, RegGrade::Measured)
            .with_grade(INT_ENA, RegGrade::Measured)
            .with_grade(INT_CLR, RegGrade::Measured)
            .with_grade(CLKDIV, RegGrade::Measured)
            .with_grade(STATUS, RegGrade::Measured)
            .with_grade(CONF0, RegGrade::Measured)
            .with_grade(CONF1, RegGrade::Measured)
            .with_grade(TOUT_CONF, RegGrade::Measured)
            // The PAC's statement, implemented; nothing measured it.
            .with_grade(RX_FILT, RegGrade::Documented)
            .with_grade(MEM_TX_STATUS, RegGrade::Documented)
            .with_grade(MEM_RX_STATUS, RegGrade::Documented)
            .with_grade(FSM_STATUS, RegGrade::Documented)
            .with_grade(POSPULSE, RegGrade::Documented)
            .with_grade(NEGPULSE, RegGrade::Documented)
            .with_grade(LOWPULSE, RegGrade::Documented)
            .with_grade(HIGHPULSE, RegGrade::Documented)
            .with_grade(RXD_CNT, RegGrade::Documented)
            .with_grade(CLK_CONF, RegGrade::Documented)
            .with_grade(REG_UPDATE, RegGrade::Documented)
    }

    /// Every register this block grades `Modeled`, in offset order — the
    /// README's and `--strict-grade`'s "modeled registers".
    pub fn modeled_registers() -> Vec<&'static str> {
        let grades = Self::grades();
        (0..0x100u32)
            .step_by(4)
            .filter(|off| grades.grade(*off) == RegGrade::Modeled)
            .filter_map(|off| regs::UART0.name(off))
            .collect()
    }

    // ---- auto-baud --------------------------------------------------------

    fn autobaud_enabled(&self) -> bool {
        self.regs.stored(CONF0) & CONF0_AUTOBAUD_EN != 0
    }

    /// One bit at the host's rate, in sclk clocks — what the pulse counters
    /// measure a one-bit pulse as. `40_000_000 / 115_200 = 347` (347.2: the
    /// counter cannot see the fraction). `None` with no clock or no host.
    fn host_bit_clocks(&self) -> Option<u64> {
        let sclk = self.sclk_hz();
        if sclk == 0 || self.host_baud == 0 {
            return None;
        }
        Some((sclk / self.host_baud).max(1))
    }

    /// The glitch filter's threshold in clocks, when enabled: a pulse
    /// narrower than this is not seen.
    fn glitch_filter(&self) -> u64 {
        let w = self.regs.stored(RX_FILT);
        if w & RX_FILT_EN != 0 {
            u64::from(w & 0xff)
        } else {
            0
        }
    }

    /// The pulse counters' view of the frame that carried `byte`, landing
    /// at cycle `at`. Called for every byte the wire delivers while
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
            let odd = conf0 & 1 != 0;
            line.push(ones_odd != odd);
        }

        // Runs of equal level, `(level, bits)`. The idle before the start
        // bit is high, so the first run is the start bit's low and every
        // run begins with an edge.
        let mut runs: Vec<(bool, u64)> = Vec::with_capacity(11);
        for &level in &line {
            match runs.last_mut() {
                Some((l, n)) if *l == level => *n += 1,
                _ => runs.push((level, 1)),
            }
        }
        // The stop bit(s) are high and open-ended: a high last bit runs into
        // them; a low last bit is closed by their rising edge. Everything
        // left in `runs` is a closed pulse.
        let trailing_half_bits = match runs.last() {
            Some(&(true, n)) => {
                runs.pop();
                2 * n + stop_half_bits
            }
            _ => stop_half_bits,
        };
        // Pulses narrower than the glitch filter are not seen (module docs).
        let visible = |bits: u64| bits * bit >= filter;

        // The previous frame's trailing high is a pulse if this frame came
        // one symbol after it — back to back, as a scripted chunk delivers.
        if let Some((prev_at, half_bits)) = self.autobaud.trailing
            && let Some(symbol) = self.symbol_cycles()
            && at == prev_at.saturating_add(symbol)
        {
            let clocks = half_bits * bit / 2;
            if clocks >= filter {
                self.autobaud.high_min = self.autobaud.high_min.min(clamp12(clocks));
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
                let clocks = clamp12(bits * bit);
                if level {
                    self.autobaud.high_min = self.autobaud.high_min.min(clocks);
                    if let Some(prev) = last_rising {
                        let gap = clamp12((pos - prev) * bit);
                        self.autobaud.pos_min = self.autobaud.pos_min.min(gap);
                    }
                    last_rising = Some(pos);
                } else {
                    self.autobaud.low_min = self.autobaud.low_min.min(clocks);
                    if let Some(prev) = last_falling {
                        let gap = clamp12((pos - prev) * bit);
                        self.autobaud.neg_min = self.autobaud.neg_min.min(gap);
                    }
                    last_falling = Some(pos);
                }
            }
            pos += bits;
        }
        // The rising edge into the trailing high, against the last one.
        if let Some(prev) = last_rising {
            let gap = clamp12((pos - prev) * bit);
            self.autobaud.pos_min = self.autobaud.pos_min.min(gap);
        }
        self.autobaud.rxd_cnt = (self.autobaud.rxd_cnt + edges).min(RXD_CNT_MAX);
    }

    // ---- clocking -------------------------------------------------------

    /// The function clock PCR feeds this block, in Hz. `0` when PCR selects
    /// no source.
    fn sclk_hz(&self) -> u64 {
        let word = self.clock.get();
        let base: u64 = match (word >> PCR_SCLK_SEL_SHIFT) & 3 {
            1 => 80_000_000,
            // RC_FAST, nominal (`esp-metadata` `soc.rc_fast_clk_default`).
            2 => 17_500_000,
            3 => super::XTAL_HZ,
            _ => 0,
        };
        let div_num = u64::from((word >> PCR_SCLK_DIV_NUM_SHIFT) & 0xff);
        base / (div_num + 1)
    }

    /// `clkdiv·16 + frag`, never zero.
    fn divider16(&self) -> u64 {
        let w = self.regs.stored(CLKDIV);
        let integral = u64::from(w & 0xfff);
        let frag = u64::from((w >> 20) & 0xf);
        (integral * 16 + frag).max(1)
    }

    /// The baud rate the registers describe, for the log.
    pub fn baud(&self) -> u64 {
        let sclk = self.sclk_hz();
        if sclk == 0 {
            return 0;
        }
        sclk * 16 / self.divider16()
    }

    /// Cycles per bit on the wire. With no clock the wire never moves, and
    /// the FIFO fills and stays full — which is what the chip would do.
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

    // ---- interrupt state ------------------------------------------------

    fn rx_full_thrhd(&self) -> usize {
        (self.regs.stored(CONF1) & 0xff) as usize
    }

    fn tx_empty_thrhd(&self) -> usize {
        ((self.regs.stored(CONF1) >> 8) & 0xff) as usize
    }

    /// `int_raw` as the guest reads it: the sticky events plus the levels.
    fn int_raw(&self) -> u32 {
        let mut raw = self.sticky;
        if self.rx.len() > self.rx_full_thrhd() {
            raw |= INT_RXFIFO_FULL;
        }
        if self.tx.len() < self.tx_empty_thrhd() {
            raw |= INT_TXFIFO_EMPTY;
        }
        raw
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.int_raw() & self.regs.stored(INT_ENA);
        cx.irq.set_level(self.irq_source, st != 0);
    }

    // ---- the transmit side ----------------------------------------------

    /// Move the next FIFO byte onto the wire at cycle `start`: the guest's
    /// write cycle for an idle shifter, the previous byte's `tx_due` when
    /// chaining.
    fn start_shifter_if_idle(&mut self, start: u64, cx: &mut BusCx<'_>) {
        if self.shifter.is_some() {
            return;
        }
        if self.regs.stored(CLK_CONF) & CLK_CONF_TX_SCLK_EN == 0 {
            // `clk_conf.tx_sclk_en` clear: the transmitter has no clock and
            // the FIFO holds what it has (documented; nothing on this part
            // has been seen to clear it).
            return;
        }
        let Some(byte) = self.tx.pop_front() else {
            return;
        };
        self.tx_popped = self.tx_popped.wrapping_add(1);
        self.shifter = Some(byte);
        match self.symbol_cycles() {
            Some(cycles) => {
                self.tx_due = start.saturating_add(cycles);
                cx.sched
                    .schedule_at(self.tx_due, event_id(self.index, EV_TX));
            }
            None => {
                // No clock: the byte sits in the shifter until PCR gives it
                // one — which nothing in the firmware ever undoes, so say so.
                log::warn!(
                    "{}: no function clock; the TX shifter is stalled",
                    self.name
                );
            }
        }
    }

    fn push_tx(&mut self, byte: u8, cx: &mut BusCx<'_>) {
        if self.tx.len() >= FIFO_DEPTH {
            if self.tx_dropped == 0 {
                let line = format!(
                    "cyc={} pc=0x{:08x} {} TX FIFO full: byte 0x{byte:02x} dropped (the guest \
                     wrote past txfifo_cnt = 128)",
                    cx.now, cx.pc, self.name
                );
                cx.trace.note(&line);
                log::warn!("{}: TX FIFO overflow, byte dropped", self.name);
            }
            self.tx_dropped += 1;
            return;
        }
        self.tx.push_back(byte);
        self.tx_pushed = self.tx_pushed.wrapping_add(1);
        self.start_shifter_if_idle(cx.now, cx);
        self.update_lines(cx);
    }

    // ---- the receive side -----------------------------------------------

    fn tout_enabled(&self) -> bool {
        self.regs.stored(TOUT_CONF) & TOUT_EN != 0
    }

    fn tout_bits(&self) -> u64 {
        u64::from((self.regs.stored(TOUT_CONF) >> TOUT_THRHD_SHIFT) & TOUT_THRHD_MASK)
    }

    /// (Re)arm the receive timeout from `from` (the last byte's arrival): it
    /// fires `rx_tout_thrhd` bit-times later if nothing else arrives and the
    /// FIFO is still non-empty.
    fn rearm_tout(&mut self, from: u64, cx: &mut BusCx<'_>) {
        let ev = event_id(self.index, EV_RX_TOUT);
        cx.sched.cancel(ev);
        if !self.tout_enabled() || self.rx.is_empty() {
            return;
        }
        let Some(bit) = self.bit_cycles() else {
            return;
        };
        let delay = bit.saturating_mul(self.tout_bits().max(1));
        cx.sched.schedule_at(from.saturating_add(delay), ev);
    }

    /// A byte arrived on the wire at cycle `at`.
    fn push_rx(&mut self, byte: u8, at: u64, cx: &mut BusCx<'_>) {
        if self.autobaud_enabled() {
            self.autobaud_observe(byte, at);
        }
        if self.regs.stored(CLK_CONF) & CLK_CONF_RX_SCLK_EN == 0 {
            // `clk_conf.rx_sclk_en` clear: the receiver has no clock and the
            // byte on the wire is not sampled (documented; same caveat).
            cx.trace.note(&format!(
                "cyc={at} {} RX byte 0x{byte:02x} not sampled: clk_conf.rx_sclk_en is clear",
                self.name
            ));
            return;
        }
        if self.rx.len() >= FIFO_DEPTH {
            // A dropped byte is a corrupt line three layers up ("dropping
            // unparseable N B M! line"), so the first one says so here
            // rather than only as a sticky bit nobody reads.
            if self.sticky & INT_RXFIFO_OVF == 0 {
                cx.trace.note(&format!(
                    "cyc={at} {} RX FIFO overflow: {FIFO_DEPTH} bytes unread and another \
                     arrived; it is dropped, as the part drops it. The host is sending faster \
                     than the guest is reading — at {} baud, {} cycles a symbol",
                    self.name,
                    self.baud(),
                    self.symbol_cycles().unwrap_or(0),
                ));
            }
            self.sticky |= INT_RXFIFO_OVF;
        } else {
            self.rx.push_back(byte);
            self.rx_pushed = self.rx_pushed.wrapping_add(1);
        }
        self.rearm_tout(at, cx);
        self.update_lines(cx);
    }

    fn pop_rx(&mut self, cx: &mut BusCx<'_>) -> u8 {
        let byte = self.rx.pop_front().unwrap_or(0);
        if self.rx.len() < FIFO_DEPTH {
            self.rx_popped = self.rx_popped.wrapping_add(1);
        }
        if self.rx.is_empty() {
            cx.sched.cancel(event_id(self.index, EV_RX_TOUT));
        }
        self.update_lines(cx);
        byte
    }

    /// Ask the host source for its next byte, delivered at cycle `at`, and
    /// schedule the poll after.
    fn poll_source(&mut self, at: u64, cx: &mut BusCx<'_>) {
        let Some(id) = self.stream else {
            return;
        };
        let ev = event_id(self.index, EV_RX_POLL);
        let (byte, next_ready, live) = {
            let stream = cx.host.stream(id);
            (stream.next_byte(at), stream.next_ready(), stream.is_live())
        };
        match byte {
            Some(b) => {
                self.push_rx(b, at, cx);
                // The wire delivers at baud: the next byte, if there is one,
                // is one symbol behind this one.
                let gap = self.symbol_cycles().unwrap_or(LIVE_POLL_CYCLES);
                self.rx_due = at.saturating_add(gap);
                cx.sched.schedule_at(self.rx_due, ev);
            }
            None => match next_ready {
                Some(ready) => {
                    self.rx_due = ready.max(at + 1);
                    cx.sched.schedule_at(self.rx_due, ev);
                }
                None if live => {
                    // A socket: wall clock decides when bytes appear, so
                    // poll from the machine's actual time, not the chain's.
                    self.rx_due = cx.now.max(at).saturating_add(LIVE_POLL_CYCLES);
                    cx.sched.schedule_at(self.rx_due, ev);
                }
                None => {}
            },
        }
    }

    // ---- registers --------------------------------------------------------

    fn read_word(&self, off: u32) -> u32 {
        match off {
            INT_RAW => self.int_raw(),
            INT_ST => self.int_raw() & self.regs.stored(INT_ENA),
            INT_CLR | REG_UPDATE => 0,
            STATUS => {
                let mut v = STATUS_IDLE | (self.rx.len() as u32) | ((self.tx.len() as u32) << 16);
                if self.shifter.is_some() {
                    // The line is toggling (modeled: reported low while a
                    // symbol is on the wire).
                    v &= !STATUS_TXD;
                }
                v
            }
            FSM_STATUS => {
                if self.shifter.is_some() {
                    1 << 4
                } else {
                    0
                }
            }
            MEM_TX_STATUS => (self.tx_pushed & 0x7f) | ((self.tx_popped & 0x7f) << 9),
            // The RX SRAM starts at 0x80 (the PAC reset value `0x0001_0080`).
            MEM_RX_STATUS => {
                (0x80 | (self.rx_popped & 0x7f)) | ((0x80 | (self.rx_pushed & 0x7f)) << 9)
            }
            POSPULSE => self.autobaud.pos_min,
            NEGPULSE => self.autobaud.neg_min,
            LOWPULSE => self.autobaud.low_min,
            HIGHPULSE => self.autobaud.high_min,
            RXD_CNT => self.autobaud.rxd_cnt,
            other => self.regs.effective(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            FIFO => self.push_tx((value & 0xff) as u8, cx),
            POSPULSE | NEGPULSE | LOWPULSE | HIGHPULSE | RXD_CNT => {}
            INT_ENA => {
                self.regs.poke(INT_ENA, value & INT_MASK);
                self.update_lines(cx);
            }
            INT_CLR => {
                // Levels cannot be cleared while they hold; everything else
                // is write-one-to-clear.
                self.sticky &= !(value & !INT_LEVEL_BITS);
                self.update_lines(cx);
            }
            INT_RAW | INT_ST | STATUS | FSM_STATUS | MEM_TX_STATUS | MEM_RX_STATUS
            | AFIFO_STATUS => {}
            REG_UPDATE => {
                // Write 1 to synchronise, reads 0 once done — at once here.
            }
            CONF0 => {
                let was_detecting = self.autobaud_enabled();
                self.regs.poke(CONF0, value);
                if !was_detecting && value & CONF0_AUTOBAUD_EN != 0 {
                    // The counters restart on the enable's rising edge
                    // (modeled; see the module docs).
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
                if value & CONF0_RXFIFO_RST != 0 {
                    self.rx.clear();
                    cx.sched.cancel(event_id(self.index, EV_RX_TOUT));
                }
                if value & CONF0_TXFIFO_RST != 0 {
                    self.tx.clear();
                }
                self.update_lines(cx);
            }
            CONF1 => {
                self.regs.poke(CONF1, value);
                self.update_lines(cx);
            }
            CLK_CONF => {
                self.regs.poke(CLK_CONF, value);
                // A transmitter given its clock back picks up where the FIFO
                // left it.
                self.start_shifter_if_idle(cx.now, cx);
            }
            TOUT_CONF => {
                self.regs.poke(TOUT_CONF, value);
                self.rearm_tout(cx.now, cx);
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
        self.tx.len() + usize::from(self.shifter.is_some())
    }

    pub fn tx_dropped(&self) -> u64 {
        self.tx_dropped
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
            // A pop is a side effect: only lane 0 carries the byte.
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
            // A push is a side effect: the ROM stores a word, esp-hal a
            // byte; both put the byte in lane 0.
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
                let Some(byte) = self.shifter.take() else {
                    return;
                };
                if let Some(id) = self.stream {
                    cx.host.stream(id).write_byte(byte);
                }
                if self.tx.is_empty() {
                    self.sticky |= INT_TX_DONE;
                } else {
                    let due = self.tx_due;
                    self.start_shifter_if_idle(due, cx);
                }
                self.update_lines(cx);
            }
            EV_RX_POLL => {
                let due = self.rx_due;
                self.poll_source(due, cx);
            }
            EV_RX_TOUT => {
                if self.tout_enabled() && !self.rx.is_empty() {
                    self.sticky |= INT_RXFIFO_TOUT;
                    self.update_lines(cx);
                }
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

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x100 + 2 * FIFO_DEPTH + 128);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.sticky.to_le_bytes());
        out.extend_from_slice(&self.tx_pushed.to_le_bytes());
        out.extend_from_slice(&self.tx_popped.to_le_bytes());
        out.extend_from_slice(&self.rx_pushed.to_le_bytes());
        out.extend_from_slice(&self.rx_popped.to_le_bytes());
        out.extend_from_slice(&self.tx_dropped.to_le_bytes());
        out.extend_from_slice(&u32::from(self.warned_at_cmd).to_le_bytes());
        let shifter = self.shifter.map(|b| 0x100 | u32::from(b)).unwrap_or(0);
        out.extend_from_slice(&shifter.to_le_bytes());
        out.extend_from_slice(&self.tx_due.to_le_bytes());
        out.extend_from_slice(&self.rx_due.to_le_bytes());
        out.extend_from_slice(&(self.tx.len() as u32).to_le_bytes());
        out.extend(self.tx.iter());
        out.extend_from_slice(&(self.rx.len() as u32).to_le_bytes());
        out.extend(self.rx.iter());
        // The auto-baud state (M3 P1), between the FIFOs and the registers.
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
        let (Some(index), Some(sticky), Some(tx_pushed), Some(tx_popped), Some(rx_pushed)) =
            (r.u64(), r.u32(), r.u32(), r.u32(), r.u32())
        else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let (Some(rx_popped), Some(tx_dropped), Some(warned), Some(shifter)) =
            (r.u32(), r.u64(), r.u32(), r.u32())
        else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let (Some(tx_due), Some(rx_due), Some(tx_len)) = (r.u64(), r.u64(), r.u32()) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let tx_len = tx_len as usize;
        if r.0.len() < tx_len {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        }
        let (tx, rest) = r.0.split_at(tx_len);
        r.0 = rest;
        let Some(rx_len) = r.u32() else {
            return;
        };
        let rx_len = rx_len as usize;
        if r.0.len() < rx_len {
            return;
        }
        let (rx, rest) = r.0.split_at(rx_len);
        r.0 = rest;
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
        self.sticky = sticky;
        self.tx_pushed = tx_pushed;
        self.tx_popped = tx_popped;
        self.rx_pushed = rx_pushed;
        self.rx_popped = rx_popped;
        self.tx_dropped = tx_dropped;
        self.warned_at_cmd = warned != 0;
        self.shifter = (shifter & 0x100 != 0).then_some((shifter & 0xff) as u8);
        self.tx_due = tx_due;
        self.rx_due = rx_due;
        self.tx = tx.iter().copied().collect();
        self.rx = rx.iter().copied().collect();
        self.regs.load_state(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::{ByteLog, Sandbox, ScriptedSource};

    /// The PAC's reset for one register of this block, from the generated
    /// table — the same value `with_names` seeded, so a test can never
    /// drift from the model by carrying its own copy.
    fn pac(off: u32) -> u32 {
        regs::UART0
            .reset(off)
            .expect("the PAC gives this register a non-zero reset")
    }
    /// A sandbox with the `uart0` stream on a memory sink, a scripted
    /// source, and a UART0 on it.
    fn rig(script: ScriptedSource) -> (Sandbox, Uart, ByteLog) {
        let mut sb = Sandbox::new();
        let log = ByteLog::new();
        let id = sb.host.add(
            "uart0",
            Box::new(lp_emu_esp_common::host::MemorySink(log.clone())),
            Box::new(script),
        );
        let mut u = Uart::uart0(ClockLine::default(), Some(id));
        u.attached(3);
        u.started(&mut sb.cx());
        (sb, u, log)
    }

    /// 115,200 from XTAL: 40e6·16 / 5555 = 115,211; one bit is ceil(1388.75) = 1,389 cycles, ten bits 13,890.
    const SYMBOL_115200: u64 = 13_890;

    #[test]
    fn the_reset_state_is_the_rom_console_at_115200_with_an_empty_fifo() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new());
        assert_eq!(u.baud(), 115_211);
        assert_eq!(u.symbol_cycles(), Some(SYMBOL_115200));
        assert_eq!(
            sb.read(&mut u, STATUS),
            STATUS_IDLE,
            "txd high, both counts 0"
        );
        assert_eq!(
            sb.read(&mut u, INT_RAW),
            INT_TXFIFO_EMPTY,
            "the PAC reset, 0x02"
        );
        assert_eq!(sb.read(&mut u, CONF1), 0x6060);
        assert_eq!(sb.read(&mut u, TOUT_CONF), 0x28);
        assert_eq!(sb.read(&mut u, AT_CMD_CHAR), 0x032b);
        assert_eq!(sb.read(&mut u, MEM_RX_STATUS), 0x0001_0080);
        assert_eq!(sb.read(&mut u, FSM_STATUS), 0);
        assert_eq!(u.reg_name(0x1c), Some("status"));
        // reg_update: write 1, reads 0.
        sb.write(&mut u, REG_UPDATE, 1);
        assert_eq!(sb.read(&mut u, REG_UPDATE), 0);
        assert_eq!(
            sb.sched.live(),
            0,
            "a null-ish scripted source schedules nothing"
        );
    }

    #[test]
    fn the_rom_path_pushes_a_word_and_the_shifter_drains_at_baud() {
        let (mut sb, mut u, log) = rig(ScriptedSource::new());
        // `uart_hal_write_fifo`: a word store of the byte.
        sb.write(&mut u, FIFO, u32::from(b'A'));
        assert_eq!(
            (sb.read(&mut u, STATUS) >> 16) & 0xff,
            0,
            "straight into the shifter"
        );
        assert_eq!(sb.read(&mut u, STATUS) & STATUS_TXD, 0, "the line is busy");
        assert_eq!(sb.read(&mut u, FSM_STATUS) >> 4 & 0xf, 1);
        assert!(log.is_empty(), "nothing has left the wire yet");
        sb.write(&mut u, FIFO, u32::from(b'B'));
        assert_eq!(
            (sb.read(&mut u, STATUS) >> 16) & 0xff,
            1,
            "queued behind the shifter"
        );
        assert_eq!(sb.sched.next_deadline(), Some(SYMBOL_115200));

        sb.run_to(&mut u, SYMBOL_115200);
        assert_eq!(log.text(), "A");
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_TX_DONE,
            0,
            "B is on the wire"
        );
        sb.run_to(&mut u, 2 * SYMBOL_115200);
        assert_eq!(log.text(), "AB");
        assert_eq!(sb.read(&mut u, STATUS), STATUS_IDLE);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_TX_DONE, INT_TX_DONE);
        sb.write(&mut u, INT_CLR, INT_TX_DONE);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_TX_DONE, 0);
    }

    #[test]
    fn an_81_byte_line_takes_seven_milliseconds_to_leave_at_115200() {
        // Spike report §11.1: the 7,006–7,121 µs floor on silicon.
        let (mut sb, mut u, log) = rig(ScriptedSource::new());
        for i in 0..81u32 {
            sb.write(&mut u, FIFO, b'a' as u32 + (i % 26));
        }
        assert_eq!((sb.read(&mut u, STATUS) >> 16) & 0xff, 80);
        let done = 81 * SYMBOL_115200;
        sb.run_to(&mut u, done - 1);
        assert_eq!(log.len(), 80);
        sb.run_to(&mut u, done);
        assert_eq!(log.len(), 81);
        let us = done / memmap::CYCLES_PER_US;
        assert!((7_000..=7_100).contains(&us), "{us} us");
    }

    #[test]
    fn the_tx_fifo_holds_128_and_the_rom_wait_ends_when_one_leaves() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new());
        // One goes to the shifter; 128 fill the FIFO; the 130th is dropped.
        for i in 0..130u32 {
            sb.write(&mut u, FIFO, i & 0xff);
        }
        assert_eq!((sb.read(&mut u, STATUS) >> 16) & 0xff, 128);
        assert_eq!(u.tx_dropped(), 1);
        // `uart_serial_tx_one_char` loops while txfifo_cnt > 0x7f.
        sb.run_to(&mut u, SYMBOL_115200);
        assert_eq!((sb.read(&mut u, STATUS) >> 16) & 0xff, 127);
    }

    #[test]
    fn esp_hal_at_921600_from_pll_sees_txfifo_empty_as_a_level() {
        let (mut sb, mut u, log) = rig(ScriptedSource::new());
        // esp-hal for 921,600: PCR sclk_sel = 1 (80 MHz), div_num 0;
        // divider = (80e6 << 4) / 921600 = 1388 → clkdiv 86, frag 12.
        u.clock.set(0x0050_0000);
        sb.write(&mut u, CLKDIV, (12 << 20) | 86);
        assert_eq!(u.baud(), 922_190);
        // tx.fifo_empty_threshold = 10 (esp-hal's default TxConfig).
        sb.write(&mut u, CONF1, (10 << 8) | 120);
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_TXFIFO_EMPTY,
            INT_TXFIFO_EMPTY
        );
        // Cannot be cleared while the FIFO is below the threshold.
        sb.write(&mut u, INT_CLR, INT_TXFIFO_EMPTY);
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_TXFIFO_EMPTY,
            INT_TXFIFO_EMPTY
        );
        for i in 0..128u32 {
            sb.write(&mut u, FIFO, i);
        }
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_TXFIFO_EMPTY, 0, "127 queued");
        // The async write listens, then the ISR sees int_st once the FIFO
        // drains below ten.
        sb.write(&mut u, INT_ENA, INT_TXFIFO_EMPTY);
        assert!(!sb.irq.level(source::UART0));
        let symbol = u.symbol_cycles().unwrap();
        // One bit is ceil(160e6·1388 / (80e6·16)) = ceil(173.5) = 174 cycles.
        assert_eq!(symbol, 1_740, "10 bits at 922,190 baud, 160 MHz");
        // After k symbols the FIFO holds 127 - k (one byte was on the wire
        // from the first write).
        sb.run_to(&mut u, symbol * 117);
        assert!(!sb.irq.level(source::UART0), "10 left: not yet below 10");
        sb.run_to(&mut u, symbol * 118);
        assert!(sb.irq.level(source::UART0), "9 left");
        assert_eq!(sb.read(&mut u, INT_ST), INT_TXFIFO_EMPTY);
        // intr_handler: int_ena &= !int_st.
        sb.write(&mut u, INT_ENA, 0);
        assert!(!sb.irq.level(source::UART0));
        sb.run_to(&mut u, symbol * 200);
        assert_eq!(log.len(), 128);
    }

    #[test]
    fn scripted_bytes_arrive_at_their_cycle_one_symbol_apart_and_time_out_into_a_read() {
        let script = ScriptedSource::new().at(1_000_000, b"M!x\n");
        let (mut sb, mut u, _) = rig(script);
        assert_eq!(
            sb.sched.next_deadline(),
            Some(1_000_000),
            "the first poll is the chunk's cycle"
        );
        // esp-hal's read_async: threshold = buffer length, timeout 10 symbols on.
        sb.write(&mut u, CONF1, (10 << 8) | 64);
        sb.write(&mut u, TOUT_CONF, (100 << TOUT_THRHD_SHIFT) | TOUT_EN);
        sb.write(
            &mut u,
            INT_ENA,
            INT_RXFIFO_FULL | INT_RXFIFO_TOUT | INT_RXFIFO_OVF,
        );

        sb.run_to(&mut u, 1_000_000);
        assert_eq!(sb.read(&mut u, STATUS) & 0xff, 1, "M");
        sb.run_to(&mut u, 1_000_000 + 3 * SYMBOL_115200);
        assert_eq!(
            sb.read(&mut u, STATUS) & 0xff,
            4,
            "all four, one symbol apart"
        );
        assert!(
            !sb.irq.level(source::UART0),
            "4 < 64: not full, not timed out"
        );
        // 100 bit-times after the last byte: the timeout.
        let bit = u.bit_cycles().unwrap();
        let last = 1_000_000 + 3 * SYMBOL_115200;
        sb.run_to(&mut u, last + 100 * bit - 1);
        assert!(!sb.irq.level(source::UART0));
        sb.run_to(&mut u, last + 100 * bit);
        assert!(sb.irq.level(source::UART0));
        assert_eq!(sb.read(&mut u, INT_ST), INT_RXFIFO_TOUT);
        // read_buffered: pop what is counted, clear FifoFull; the future
        // clears the timeout it resolved on.
        let mut got = Vec::new();
        for _ in 0..4 {
            got.push(sb.read(&mut u, FIFO) as u8);
        }
        assert_eq!(got, b"M!x\n");
        assert_eq!(sb.read(&mut u, STATUS) & 0xff, 0);
        sb.write(&mut u, INT_CLR, INT_RXFIFO_TOUT | INT_RXFIFO_FULL);
        assert!(!sb.irq.level(source::UART0));
        assert_eq!(sb.read(&mut u, FIFO), 0, "an empty FIFO reads 0");
    }

    #[test]
    fn rxfifo_full_is_a_level_over_the_threshold_and_overflow_is_sticky() {
        let mut bytes = vec![0u8; 140];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        let (mut sb, mut u, _) = rig(ScriptedSource::new().at(10, bytes));
        sb.write(&mut u, CONF1, (10 << 8) | 120);
        sb.run_to(&mut u, 10 + 120 * SYMBOL_115200);
        assert_eq!(sb.read(&mut u, STATUS) & 0xff, 121);
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_RXFIFO_FULL,
            INT_RXFIFO_FULL,
            "121 > 120"
        );
        sb.write(&mut u, INT_CLR, INT_RXFIFO_FULL);
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_RXFIFO_FULL,
            INT_RXFIFO_FULL,
            "still over"
        );
        sb.run_to(&mut u, 10 + 139 * SYMBOL_115200);
        assert_eq!(sb.read(&mut u, STATUS) & 0xff, 128, "full");
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_RXFIFO_OVF,
            INT_RXFIFO_OVF,
            "12 dropped"
        );
        // A FIFO reset (esp-hal's rxfifo_reset on overflow) empties it.
        sb.write(&mut u, CONF0, pac(CONF0) | CONF0_RXFIFO_RST);
        sb.write(&mut u, CONF0, pac(CONF0));
        assert_eq!(sb.read(&mut u, STATUS) & 0xff, 0);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_RXFIFO_FULL, 0);
        sb.write(&mut u, INT_CLR, INT_RXFIFO_OVF);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_RXFIFO_OVF, 0);
    }

    #[test]
    fn the_state_round_trips_with_bytes_in_flight() {
        let (mut sb, mut u, _) = rig(ScriptedSource::new());
        sb.write(&mut u, FIFO, 0x41);
        sb.write(&mut u, FIFO, 0x42);
        sb.write(&mut u, CLKDIV, 0x0055);
        u.autobaud.rxd_cnt = 77;
        u.autobaud.low_min = 347;
        u.autobaud.trailing = Some((123, 2));
        let blob = u.save_state();
        let mut other = Uart::uart0(ClockLine::default(), None).with_host_baud(1);
        other.load_state(&blob);
        assert_eq!(other.index, 3);
        assert_eq!(other.shifter, Some(0x41));
        assert_eq!(other.tx, [0x42]);
        assert_eq!(other.regs.stored(CLKDIV), 0x0055);
        assert_eq!(other.tx_pending(), 2);
        assert_eq!(other.host_baud, DEFAULT_HOST_BAUD);
        assert_eq!(other.autobaud, u.autobaud);
    }

    /// esptool's SYNC, SLIP-framed: the bytes `scripts/rom-download-sync.*`
    /// send.
    fn sync_frame() -> Vec<u8> {
        let mut f = vec![0xc0, 0x00, 0x08, 0x24, 0, 0, 0, 0, 0, 0x07, 0x07, 0x12, 0x20];
        f.extend(std::iter::repeat_n(0x55u8, 32));
        f.push(0xc0);
        assert_eq!(f.len(), 46);
        f
    }

    /// The ROM's `uart_serial_baudrate_detect` (0x40022b20): enable the
    /// glitch filter at four clocks and `autobaud_en`.
    fn rom_enables_autobaud(sb: &mut Sandbox, u: &mut Uart) {
        sb.write(u, RX_FILT, 0x104);
        sb.write(u, CONF0, pac(CONF0) | CONF0_AUTOBAUD_EN);
    }

    #[test]
    fn the_auto_baud_counters_are_the_sync_frame_at_115200_from_xtal() {
        // Silent while disabled: the PAC resets, whatever arrives.
        let script = ScriptedSource::new().at(1_000_000, b"UU");
        let (mut sb, mut u, _) = rig(script);
        sb.run_to(&mut u, 1_000_000 + 3 * SYMBOL_115200);
        assert_eq!(sb.read(&mut u, RXD_CNT), 0);
        assert_eq!(sb.read(&mut u, LOWPULSE), PULSE_MIN_RESET);
        assert_eq!(sb.read(&mut u, HIGHPULSE), PULSE_MIN_RESET);

        // The ROM's sequence, then the SYNC.
        let script = ScriptedSource::new().at(1_000_000, sync_frame());
        let (mut sb, mut u, _) = rig(script);
        rom_enables_autobaud(&mut sb, &mut u);
        assert_eq!(u.host_bit_clocks(), Some(347), "40e6 / 115200, floored");
        // After the first byte only (0xc0: idle→start, then the two high
        // data bits into the stop): two edges, one 7-bit low, no closed high.
        sb.run_to(&mut u, 1_000_000);
        assert_eq!(sb.read(&mut u, RXD_CNT), 2);
        assert_eq!(sb.read(&mut u, LOWPULSE), 7 * 347);
        assert_eq!(sb.read(&mut u, HIGHPULSE), PULSE_MIN_RESET);
        // The whole frame, one symbol apart. Per byte: c0 2, 00 2, 08 4,
        // 24 6, 00×5 10, 07 4, 07 4, 12 6, 20 4, 55×32 320, c0 2 = 364.
        sb.run_to(&mut u, 1_000_000 + 46 * SYMBOL_115200);
        assert_eq!(sb.read(&mut u, RXD_CNT), 364);
        assert_eq!(sb.read(&mut u, LOWPULSE), 347, "a 0x55's one-bit lows");
        assert_eq!(sb.read(&mut u, HIGHPULSE), 347, "and its one-bit highs");
        assert_eq!(sb.read(&mut u, POSPULSE), 2 * 347, "rising edges two bits apart");
        assert_eq!(sb.read(&mut u, NEGPULSE), 2 * 347);
        // The ROM's arithmetic on what it read (notes F21): the divisor in
        // sixteenths, then `uart_hal_clk_set_div`.
        let low = sb.read(&mut u, LOWPULSE);
        let high = sb.read(&mut u, HIGHPULSE);
        let div16 = ((low + high) << 3) + 16;
        assert_eq!(div16, 5568);
        assert_eq!((div16 >> 4, div16 & 0xf), (348, 0));
        sb.write(&mut u, CLKDIV, ((div16 & 0xf) << 20) | (div16 >> 4));
        assert_eq!(u.baud(), 114_942, "40e6 × 16 / 5568, truncated");
        // Disable, as the ROM does: the counters hold; re-enable restarts.
        sb.write(&mut u, CONF0, pac(CONF0));
        assert_eq!(sb.read(&mut u, RXD_CNT), 364);
        sb.write(&mut u, CONF0, pac(CONF0) | CONF0_AUTOBAUD_EN);
        assert_eq!(sb.read(&mut u, RXD_CNT), 0);
        assert_eq!(sb.read(&mut u, LOWPULSE), PULSE_MIN_RESET);
    }

    #[test]
    fn the_auto_baud_counters_follow_the_stated_host_baud() {
        // RD9: the host's rate is an input, never a constant chosen to land
        // the ROM's arithmetic. At 921,600 one bit is 40e6 / 921600 = 43.4 →
        // 43 clocks, and the ROM would compute ((43 + 43) << 3) + 16 = 704
        // sixteenths → clkdiv 44, frag 0 → 909,090 baud.
        let script = ScriptedSource::new().at(1_000_000, sync_frame());
        let (mut sb, mut u, _) = rig(script);
        u = u.with_host_baud(921_600);
        rom_enables_autobaud(&mut sb, &mut u);
        sb.run_to(&mut u, 1_000_000 + 46 * SYMBOL_115200);
        assert_eq!(sb.read(&mut u, RXD_CNT), 364, "the edges do not depend on the rate");
        assert_eq!(sb.read(&mut u, LOWPULSE), 43);
        assert_eq!(sb.read(&mut u, HIGHPULSE), 43);
        let div16 = ((43 + 43) << 3) + 16;
        assert_eq!((div16 >> 4, div16 & 0xf), (44, 0));

        // And a rate the twelve-bit counters cannot hold saturates rather
        // than wrapping.
        let script = ScriptedSource::new().at(1_000_000, b"\x00");
        let (mut sb, mut u, _) = rig(script);
        u = u.with_host_baud(1_200);
        rom_enables_autobaud(&mut sb, &mut u);
        sb.run_to(&mut u, 1_000_000);
        // 40e6 / 1200 = 33,333 clocks a bit; a 9-bit low is far past 0xfff.
        assert_eq!(sb.read(&mut u, LOWPULSE), PULSE_MIN_RESET);
        assert_eq!(sb.read(&mut u, RXD_CNT), 2);
    }

    #[test]
    fn the_uart_block_publishes_its_grade_table() {
        let u = Uart::uart0(ClockLine::default(), None);
        assert_eq!(u.reg_grade(FIFO), Some(RegGrade::Measured));
        assert_eq!(u.reg_grade(RXD_CNT), Some(RegGrade::Documented));
        assert_eq!(u.reg_grade(CLK_CONF), Some(RegGrade::Documented));
        assert_eq!(u.reg_grade(0x48), Some(RegGrade::Modeled), "idle_conf");
        assert_eq!(
            Uart::modeled_registers(),
            [
                "hwfc_conf",
                "sleep_conf0",
                "sleep_conf1",
                "sleep_conf2",
                "swfc_conf0",
                "swfc_conf1",
                "txbrk_conf",
                "idle_conf",
                "rs485_conf",
                "at_cmd_precnt",
                "at_cmd_postcnt",
                "at_cmd_gaptout",
                "at_cmd_char",
                "mem_conf",
                "date",
                "afifo_status",
                "id",
            ]
        );
    }

    #[test]
    fn the_sclk_enables_gate_the_shifter_and_the_receiver() {
        let script = ScriptedSource::new().at(1_000_000, b"x");
        let (mut sb, mut u, log) = rig(script);
        let clk_conf = pac(CLK_CONF);
        assert_eq!(clk_conf & (CLK_CONF_TX_SCLK_EN | CLK_CONF_RX_SCLK_EN), 0x0300_0000);
        // The ROM's `uart_hal_clk_enable` value: both enables, nothing else.
        sb.write(&mut u, CLK_CONF, 0x0300_0000);
        sb.write(&mut u, FIFO, u32::from(b'A'));
        sb.run_to(&mut u, SYMBOL_115200);
        assert_eq!(log.text(), "A");
        // TX clock off: the byte waits in the FIFO.
        sb.write(&mut u, CLK_CONF, CLK_CONF_RX_SCLK_EN);
        sb.write(&mut u, FIFO, u32::from(b'B'));
        sb.run_to(&mut u, 4 * SYMBOL_115200);
        assert_eq!(log.text(), "A");
        assert_eq!((sb.read(&mut u, STATUS) >> 16) & 0xff, 1);
        // RX clock off: the wire's byte is not sampled.
        sb.write(&mut u, CLK_CONF, CLK_CONF_TX_SCLK_EN);
        sb.run_to(&mut u, 1_000_000 + SYMBOL_115200);
        assert_eq!(sb.read(&mut u, STATUS) & 0xff, 0, "nothing received");
        assert_eq!(log.text(), "AB", "and the TX clock is back");
    }
}
