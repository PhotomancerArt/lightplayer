//! `UART0` at `0x6000_0000` — the mask ROM's console, as a **view** over
//! [`lp_emu_esp_common::engine::uart::UartEngine`]: a real FIFO pair, a
//! shifter draining at the programmed baud in emulated time, the two
//! threshold levels, and a host byte stream on the outside.
//!
//! # Why this block exists on a chip whose console is USB-Serial-JTAG
//!
//! The shipped `fw-esp32s3` image touches **no** UART register and calls no
//! ROM UART routine (`m6/notes.md` §2.4, §2.5): its console is `esp-println`'s
//! `jtag-serial`, and its only link is USB-Serial-JTAG ([`super::usb_sj`]).
//! The mask ROM's own console is the first user of this block, on a ROM-up
//! boot: the reset banner, the `load:`/`entry` lines and the ESP-IDF
//! second-stage bootloader's log all come out through `ets_printf` →
//! `ets_write_char_uart` (`0x4004_3ce8`) → `uart_tx_one_char`
//! (`0x4004_8c30`) → `uart_tx_one_char_uart` (`0x4004_8804`). So this view
//! is **what the ROM's console needs, and no more** — the classic's M3 P6
//! shape, at the S3's offsets.
//!
//! ⚠️ **The ROM prints to two places.** `uart_tx_one_char` sends the byte
//! to USB-Serial-JTAG first when `g_usb_print` (`0x3fce_ffb8`) is set
//! (`40048c43: beq …; movi a10, 4; call8 uart_tx_one_char_uart` → channel
//! 4 → `usb_uart_device_tx_one_char` `0x4004_903c`), and then to UART0 when
//! `g_uart_print` (`0x3fce_ffb9`) is set. Both flags are `.bss.interface`
//! bytes the ROM's `uart_usb_print_status_update` (`0x4004_8b80`) derives
//! from eFuses on every boot. A ROM-up boot log therefore appears on the
//! `usb-sj` stream as well as here, and the two are one console said twice.
//!
//! # What the ROM does to this block, from the disassembly
//!
//! | ROM routine | registers |
//! |---|---|
//! | `uart_tx_one_char_uart(uart_no, ch)` `0x4004_8804` | base = `(uart_no + 0x6000) << 16` (+ `0xe000` for UART2, `40048820..2e`); spins on `status`(`+0x1c`) bits 16:25 & `0x380` — `txfifo_cnt` ≥ 128 (`40048833: movi a9, 0x380` / `4004883b: extui a2, a2, 16, 16` / `and`) — then stores the byte to `fifo`(`+0x00`) (`40048847`) |
//! | `uart_tx_flush(uart_no)` `0x4004_8bf4` | spins on `status & 0x03ff_0000` — `txfifo_cnt` — until zero (`40048c20: l32r a2, 0x3ff0000`) |
//! | `uartAttach` `0x4004_8860` | writes `int_clr`(`+0x10`) = 1 on UART0 **and UART1** (`4004888f: 60000010`, `40048897: 60010010`) — UART1 gets an accept block for that one store, as the classic's did |
//! | `Uart_Init(uart_no, baud)` `0x4004_89f0` | reads `clk_conf`(`+0x78`), programs the divider through `uart_div_modify`, and touches `IO_MUX` (`600090b0/b4`), `SYSTEM` (`perip_clk_en0`/`perip_rst_en0`) and the interrupt map (`600c206c`) on the way |
//!
//! # Register facts (the `esp32s3` PAC 0.35.2 `uart0` block)
//!
//! The first six registers sit at the C6's offsets and the `int_raw` bit
//! numbers agree; from `0x18` on the layout is the S3's own and every
//! constant below is read off [`crate::regs::UART0`] and the PAC's field
//! docs:
//!
//! | | S3 | C6 |
//! |---|---|---|
//! | clock select | **in the block**: `clk_conf`(`+0x78`) `sclk_sel` bits 21:20 ("1: 80Mhz, 2: 8Mhz, 3: XTAL"), `sclk_div_num` 19:12, `tx_sclk_en` 24, `rx_sclk_en` 25 | PCR's `uart(n).clk_conf` |
//! | FIFO resets | `conf0` bits **17** (`rxfifo_rst`) / **18** (`txfifo_rst`) — the classic's, not the C6's 22/23 | bits 22 / 23 |
//! | receive timeout | `conf1.rx_tout_en` (bit 23) + `mem_conf.rx_tout_thrhd` (bits 26:17), in bit times | a whole `tout_conf` register |
//! | thresholds | `conf1` **10-bit** fields (`rxfifo_full_thrhd` 9:0, `txfifo_empty_thrhd` 19:10) | 8-bit |
//! | `clkdiv` | integer bits 11:0, fraction 23:20 | the same |
//! | `status` | `rxfifo_cnt` 9:0, `txfifo_cnt` 25:16, `txd` 31 | `rxfifo_cnt` 7:0 |
//! | register sync | **none** — no `reg_update` on the S3 | `reg_update` |
//!
//! **Timing.** One symbol takes `symbol_bits × CPU_HZ × (clkdiv·16 + frag)
//! / (sclk × 16)` cycles with `sclk` = the selected source over
//! `sclk_div_num + 1`. The PAC's resets — `clk_conf` `0x0370_1000` (XTAL,
//! `div_num` 1, both halves clocked) and `clkdiv` `0x2b6` = 694 — describe
//! `40 MHz / 2 × 16 / (694 × 16) = 28,818` baud until the ROM's `Uart_Init`
//! reprograms them for its console rate; nothing here is a flag, and the
//! view recomputes from the registers on every access. ⚠️ This machine has
//! no clock **tree**: the 80 MHz source is [`super::APB_HZ`] and the crystal
//! [`super::XTAL_HZ`] whatever the PLL is doing at that moment. Reported,
//! not hidden.
//!
//! # What is not here
//!
//! The auto-baud counters (`rxd_cnt`, `lowpulse`, `highpulse`, `pospulse`,
//! `negpulse`) and `conf0.autobaud_en` (bit 27): the ROM's UART download
//! console measures the host's baud through them before it reads a byte
//! (`main` `40043b7e: call8 uart_baudrate_detect`, only on strap nibble 7),
//! and that console is not on this milestone's path — the S3's download
//! link is USB-Serial-JTAG. They read the PAC's resets and are remembered,
//! **not** modelled; a boot that reaches the detector will spin on
//! `rxd_cnt` and say so in the trace rather than be answered by a number
//! nobody derived. `at_cmd_char_det` is a detector that never fires, as on
//! both siblings.
//!
//! # Grades
//!
//! `documented` for `fifo`, `int_raw`, `int_st`, `int_ena`, `int_clr`,
//! `clkdiv`, `status`, `conf0`, `conf1`, `mem_conf`, `clk_conf`,
//! `fsm_status`, `mem_tx_status`, `mem_rx_status` — the PAC states what
//! each holds and this file implements that statement against the ROM's
//! own driver. Everything else is `modeled`: stored and read back at the
//! PAC's reset with no behaviour. **Nothing is `measured`**: no S3 silicon
//! has been read (P09).

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::engine::uart::{
    RxDeliver, TxPush, UartConfig, UartEngine, UartEventIds, UartEvents,
};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{
    BusCx, Peripheral, RegFile, RegGrade, RegGrades, StreamId, Width, event_id, event_local,
};

use crate::memmap;
use crate::regs;
use crate::regs::source;

// Register offsets (`regs::UART0`, generated from the PAC).
const FIFO: u32 = 0x00;
const INT_RAW: u32 = 0x04;
const INT_ST: u32 = 0x08;
const INT_ENA: u32 = 0x0c;
const INT_CLR: u32 = 0x10;
const CLKDIV: u32 = 0x14;
const STATUS: u32 = 0x1c;
const CONF0: u32 = 0x20;
const CONF1: u32 = 0x24;
const LOWPULSE: u32 = 0x28;
const HIGHPULSE: u32 = 0x2c;
const RXD_CNT: u32 = 0x30;
const AT_CMD_CHAR: u32 = 0x5c;
const MEM_CONF: u32 = 0x60;
const MEM_TX_STATUS: u32 = 0x64;
const MEM_RX_STATUS: u32 = 0x68;
const FSM_STATUS: u32 = 0x6c;
const POSPULSE: u32 = 0x70;
const NEGPULSE: u32 = 0x74;
const CLK_CONF: u32 = 0x78;

/// The block's aperture: the generated table runs to `+0x80` (`id`); UART1
/// is a whole `0x10000` above.
pub const UART_LEN: u32 = 0x100;

/// FIFO depth, both directions: **128 bytes** — `mem_conf`'s reset
/// `0x0014_0012` allocates one 128-byte block to each direction
/// (`rx_size` bits 3:1 = 1, `tx_size` bits 6:4 = 1; "The default number is
/// 128 bytes"), and the ROM's own full test is `txfifo_cnt & 0x380`, i.e.
/// 128 or more.
pub const FIFO_DEPTH: usize = 128;

/// How often a live (socket) source is polled when it has nothing queued:
/// 1 ms of guest time.
pub const LIVE_POLL_CYCLES: u64 = 1_000 * memmap::CYCLES_PER_US;

// `int_raw` bit numbers (`esp32s3-0.35.2/src/uart0/int_raw.rs`).
const INT_RXFIFO_FULL: u32 = 1 << 0;
const INT_TXFIFO_EMPTY: u32 = 1 << 1;
const INT_RXFIFO_OVF: u32 = 1 << 4;
const INT_RXFIFO_TOUT: u32 = 1 << 8;
const INT_TX_DONE: u32 = 1 << 14;
/// Bits 0..=19 are named on this part; `wakeup` (19) is the last.
const INT_MASK: u32 = 0x000f_ffff;
const INT_LEVEL_BITS: u32 = INT_RXFIFO_FULL | INT_TXFIFO_EMPTY;

// `conf0` bits (`uart0/conf0.rs`).
const CONF0_PARITY_EN: u32 = 1 << 1;
const CONF0_BIT_NUM_SHIFT: u32 = 2;
const CONF0_STOP_SHIFT: u32 = 4;
/// Bit 17 — the classic's number, **not** the C6's 22.
const CONF0_RXFIFO_RST: u32 = 1 << 17;
/// Bit 18 — **not** the C6's 23.
const CONF0_TXFIFO_RST: u32 = 1 << 18;

// `conf1` fields (`uart0/conf1.rs`): ten bits each.
const CONF1_RXFIFO_FULL_THRHD_MASK: u32 = 0x3ff;
const CONF1_TXFIFO_EMPTY_THRHD_SHIFT: u32 = 10;
/// "Bit 23 - This is the enble bit for uart receiver's timeout function."
const CONF1_RX_TOUT_EN: u32 = 1 << 23;

// `mem_conf` fields (`uart0/mem_conf.rs`): "Bits 17:26 - … the threshold
// time that receiver takes to receive one byte".
const MEM_CONF_RX_TOUT_THRHD_SHIFT: u32 = 17;
const MEM_CONF_RX_TOUT_THRHD_MASK: u32 = 0x3ff;

// `clk_conf` fields (`uart0/clk_conf.rs`).
const CLK_CONF_SCLK_DIV_NUM_SHIFT: u32 = 12;
const CLK_CONF_SCLK_SEL_SHIFT: u32 = 20;
const CLK_CONF_TX_SCLK_EN: u32 = 1 << 24;
const CLK_CONF_RX_SCLK_EN: u32 = 1 << 25;

// `clkdiv` fields (`uart0/clkdiv.rs`): "Bits 0:11 - The integral part",
// "Bits 20:23 - The decimal part".
const CLKDIV_INT_MASK: u32 = 0x0fff;
const CLKDIV_FRAG_SHIFT: u32 = 20;
const CLKDIV_FRAG_MASK: u32 = 0xf;

// `status` fields (`uart0/status.rs`): "Bits 0:9 - … valid data in
// Rx-FIFO", "Bits 16:25 - … data in Tx-FIFO", "Bit 31 - … txd signal".
const STATUS_IDLE: u32 = 0xe000_c000;
const STATUS_CNT_MASK: u32 = 0x3ff;
const STATUS_TXFIFO_CNT_SHIFT: u32 = 16;
const STATUS_TXD: u32 = 1 << 31;

/// RC_FAST, the `sclk_sel = 2` source: "8Mhz" in the PAC's own words for
/// this block. Nominal; nothing on the ROM-up path selects it.
const RC_FAST_HZ: u64 = 8_000_000;

const EV_TX: u16 = 0;
const EV_RX_POLL: u16 = 1;
const EV_RX_TOUT: u16 = 2;

/// One UART instance: the S3's **view** of the block (module docs).
#[derive(Debug)]
pub struct Uart {
    name: &'static str,
    index: usize,
    irq_source: u16,
    regs: RegFile,
    grades: RegGrades,
    stream: Option<StreamId>,
    /// The FIFO pair, the shifter and the host stream's schedule.
    engine: UartEngine,
    warned_at_cmd: bool,
}

impl Uart {
    fn new(name: &'static str, irq_source: u16, stream: Option<StreamId>) -> Self {
        Self {
            name,
            index: 0,
            irq_source,
            // Every reset in this block is the PAC's, seeded from the
            // generated table: the ROM's `Uart_Init` programs the divider
            // itself on a ROM-up boot, and a direct load never opens the
            // block.
            regs: RegFile::new(name, UART_LEN).with_names(regs::UART0),
            grades: Self::grades(),
            stream,
            engine: UartEngine::new(FIFO_DEPTH),
            warned_at_cmd: false,
        }
    }

    /// `UART0`, the ROM's console: source 27, the `uart0` host stream.
    pub fn uart0(stream: Option<StreamId>) -> Self {
        Self::new("UART0", source::UART0, stream)
    }

    /// `UART1`: source 28. Nothing opens it; the ROM's `uartAttach` writes
    /// its `int_clr` on every boot, so it gets the same model rather than a
    /// table.
    pub fn uart1(stream: Option<StreamId>) -> Self {
        Self::new("UART1", source::UART1, stream)
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
            .with_grade(STATUS, RegGrade::Documented)
            .with_grade(CONF0, RegGrade::Documented)
            .with_grade(CONF1, RegGrade::Documented)
            .with_grade(MEM_CONF, RegGrade::Documented)
            .with_grade(CLK_CONF, RegGrade::Documented)
            .with_grade(FSM_STATUS, RegGrade::Documented)
            .with_grade(MEM_TX_STATUS, RegGrade::Documented)
            .with_grade(MEM_RX_STATUS, RegGrade::Documented)
    }

    // ---- clocking ---------------------------------------------------------

    /// The function clock feeding this block, in Hz: `clk_conf.sclk_sel`
    /// over `sclk_div_num + 1`. `0` when the field selects no source.
    fn sclk_hz(&self) -> u64 {
        let word = self.regs.stored(CLK_CONF);
        let base: u64 = match (word >> CLK_CONF_SCLK_SEL_SHIFT) & 3 {
            1 => super::APB_HZ,
            2 => RC_FAST_HZ,
            3 => super::XTAL_HZ,
            _ => 0,
        };
        let div_num = u64::from((word >> CLK_CONF_SCLK_DIV_NUM_SHIFT) & 0xff);
        base / (div_num + 1)
    }

    /// `clkdiv·16 + frag`, never zero.
    fn divider16(&self) -> u64 {
        let w = self.regs.stored(CLKDIV);
        let integral = u64::from(w & CLKDIV_INT_MASK);
        let frag = u64::from((w >> CLKDIV_FRAG_SHIFT) & CLKDIV_FRAG_MASK);
        (integral * 16 + frag).max(1)
    }

    /// The baud rate the registers describe, for the log and the run
    /// report.
    pub fn baud(&self) -> u64 {
        let sclk = self.sclk_hz();
        if sclk == 0 {
            return 0;
        }
        sclk * 16 / self.divider16()
    }

    /// Cycles per bit on the wire. With no clock the wire never moves.
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

    fn ids(&self) -> UartEventIds {
        UartEventIds {
            tx: event_id(self.index, EV_TX),
            rx_poll: event_id(self.index, EV_RX_POLL),
            rx_tout: event_id(self.index, EV_RX_TOUT),
        }
    }

    /// Everything the engine needs, read out of this chip's registers.
    fn config(&self) -> UartConfig {
        let clk_conf = self.regs.stored(CLK_CONF);
        UartConfig {
            symbol_cycles: self.symbol_cycles(),
            bit_cycles: self.bit_cycles(),
            rx_full_thrhd: self.rx_full_thrhd(),
            tx_empty_thrhd: self.tx_empty_thrhd(),
            tout_bits: self.tout_enabled().then(|| self.tout_bits()),
            tx_clocked: clk_conf & CLK_CONF_TX_SCLK_EN != 0,
            rx_clocked: clk_conf & CLK_CONF_RX_SCLK_EN != 0,
            baud: self.baud(),
        }
    }

    fn rx_full_thrhd(&self) -> usize {
        (self.regs.stored(CONF1) & CONF1_RXFIFO_FULL_THRHD_MASK) as usize
    }

    fn tx_empty_thrhd(&self) -> usize {
        ((self.regs.stored(CONF1) >> CONF1_TXFIFO_EMPTY_THRHD_SHIFT) & CONF1_RXFIFO_FULL_THRHD_MASK)
            as usize
    }

    fn tout_enabled(&self) -> bool {
        self.regs.stored(CONF1) & CONF1_RX_TOUT_EN != 0
    }

    fn tout_bits(&self) -> u64 {
        u64::from(
            (self.regs.stored(MEM_CONF) >> MEM_CONF_RX_TOUT_THRHD_SHIFT)
                & MEM_CONF_RX_TOUT_THRHD_MASK,
        )
    }

    // ---- interrupt state ---------------------------------------------------

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

    fn sticky_events(word: u32) -> UartEvents {
        UartEvents {
            rx_overflow: word & INT_RXFIFO_OVF != 0,
            rx_timeout: word & INT_RXFIFO_TOUT != 0,
            tx_done: word & INT_TX_DONE != 0,
        }
    }

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
        if let TxPush::Dropped { first } =
            self.engine.push_tx(byte, &cfg, self.ids(), self.name, cx)
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

    fn poll_source(&mut self, at: u64, cx: &mut BusCx<'_>) {
        let cfg = self.config();
        let arrival =
            self.engine
                .poll_source(self.stream, at, &cfg, LIVE_POLL_CYCLES, self.ids(), cx);
        let Some((byte, outcome)) = arrival else {
            return;
        };
        match outcome {
            RxDeliver::NotSampled => {
                cx.trace.note(&format!(
                    "cyc={at} {} RX byte 0x{byte:02x} not sampled: clk_conf.rx_sclk_en is clear",
                    self.name
                ));
                return;
            }
            RxDeliver::Overflowed { first: true } => {
                cx.trace.note(&format!(
                    "cyc={at} {} RX FIFO overflow: {FIFO_DEPTH} bytes unread and another \
                     arrived; it is dropped, as the part drops it — at {} baud, {} cycles a \
                     symbol",
                    self.name,
                    cfg.baud,
                    cfg.symbol_cycles.unwrap_or(0),
                ));
            }
            RxDeliver::Overflowed { first: false } | RxDeliver::Queued => {}
        }
        self.update_lines(cx);
    }

    // ---- registers ---------------------------------------------------------

    fn status(&self) -> u32 {
        let rx = self.engine.rx_len() as u32 & STATUS_CNT_MASK;
        let tx = self.engine.tx_len() as u32 & STATUS_CNT_MASK;
        let mut v = STATUS_IDLE | rx | (tx << STATUS_TXFIFO_CNT_SHIFT);
        if self.engine.is_shifting() {
            // The line is toggling (modeled: reported low while a symbol is
            // on the wire).
            v &= !STATUS_TXD;
        }
        v
    }

    fn read_word(&self, off: u32) -> u32 {
        match off {
            INT_RAW => self.int_raw(),
            INT_ST => self.int_raw() & self.regs.stored(INT_ENA),
            INT_CLR => 0,
            STATUS => self.status(),
            FSM_STATUS => {
                if self.engine.is_shifting() {
                    1 << 4
                } else {
                    0
                }
            }
            MEM_TX_STATUS => {
                (self.engine.tx_pushed() & 0x7f) | ((self.engine.tx_popped() & 0x7f) << 9)
            }
            // The RX SRAM starts at 0x80 (the PAC reset `0x0010_0200`
            // packs `rd_addr` 0x80 at bits 10:2 and `wr_addr` 0x80 at 19:11).
            MEM_RX_STATUS => {
                ((0x80 | (self.engine.rx_popped() & 0x7f)) << 2)
                    | ((0x80 | (self.engine.rx_pushed() & 0x7f)) << 11)
            }
            other => self.regs.effective(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            FIFO => self.push_tx((value & 0xff) as u8, cx),
            // Read-only on this part (the generated table's `access` list
            // and the PAC).
            INT_RAW | INT_ST | STATUS | FSM_STATUS | MEM_TX_STATUS | MEM_RX_STATUS | LOWPULSE
            | HIGHPULSE | RXD_CNT | POSPULSE | NEGPULSE => {}
            INT_ENA => {
                self.regs.poke(INT_ENA, value & INT_MASK);
                self.update_lines(cx);
            }
            INT_CLR => {
                // Levels cannot be cleared while they hold; everything else
                // is write-one-to-clear.
                self.engine
                    .clear_sticky(Self::sticky_events(value & !INT_LEVEL_BITS));
                self.update_lines(cx);
            }
            CONF0 => {
                self.regs.poke(CONF0, value);
                if value & CONF0_RXFIFO_RST != 0 {
                    self.engine.reset_rx(self.ids(), cx);
                }
                if value & CONF0_TXFIFO_RST != 0 {
                    self.engine.reset_tx();
                }
                // A frame-format change moves the symbol time; a transmitter
                // that was stalled picks up where it left off.
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
            MEM_CONF => {
                self.regs.poke(MEM_CONF, value);
                let cfg = self.config();
                self.engine.rearm_tout(cx.now, &cfg, self.ids(), cx);
                self.update_lines(cx);
            }
            CLKDIV | CLK_CONF => {
                self.regs.poke(off, value);
                // A divider or clock change: a transmitter given its clock
                // back picks up where the FIFO left it.
                let cfg = self.config();
                self.engine
                    .start_shifter_if_idle(cx.now, &cfg, self.ids(), self.name, cx);
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
            // A push is a side effect: the ROM's `uart_tx_one_char_uart`
            // stores a word with the byte in lane 0.
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
        let mut out = Vec::with_capacity(UART_LEN as usize + 2 * FIFO_DEPTH + 64);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.sticky_word().to_le_bytes());
        self.engine.save_counters(&mut out);
        out.extend_from_slice(&u32::from(self.warned_at_cmd).to_le_bytes());
        self.engine.save_stream(&mut out);
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = bytes;
        let take = |r: &mut &[u8], n: usize| -> Option<Vec<u8>> {
            if r.len() < n {
                return None;
            }
            let (head, rest) = r.split_at(n);
            *r = rest;
            Some(head.to_vec())
        };
        let (Some(index), Some(sticky)) = (take(&mut r, 8), take(&mut r, 4)) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let Some((counters, used)) = UartEngine::load_counters(r) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        r = &r[used..];
        let Some(flags) = take(&mut r, 4) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let Some((stream, used)) = UartEngine::load_stream(r) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        r = &r[used..];
        self.index = u64::from_le_bytes(index.try_into().expect("8 bytes")) as usize;
        let sticky = u32::from_le_bytes(sticky.try_into().expect("4 bytes"));
        self.engine.set_sticky(Self::sticky_events(sticky));
        self.engine.restore(counters, stream);
        self.warned_at_cmd = u32::from_le_bytes(flags.try_into().expect("4 bytes")) & 1 != 0;
        self.regs.load_state(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;
    use lp_emu_esp_common::host::{ByteLog, MemorySink, NullSource};

    fn rig() -> (Sandbox, Uart, ByteLog) {
        let mut sb = Sandbox::new();
        let log = ByteLog::new();
        let id = sb.host.add(
            "uart0",
            Box::new(MemorySink(log.clone())),
            Box::new(NullSource),
        );
        let mut u = Uart::uart0(Some(id));
        u.attached(3);
        (sb, u, log)
    }

    /// Run every event due at or before `now`.
    fn run_to(sb: &mut Sandbox, u: &mut Uart, now: u64) {
        sb.now = now;
        while let Some(id) = sb.sched.pop_due(now) {
            let mut cx = sb.cx();
            u.on_event(id, &mut cx);
        }
    }

    #[test]
    fn the_pac_resets_describe_a_clocked_block_at_the_roms_pre_boot_rate() {
        let (mut sb, mut u, _) = rig();
        assert_eq!(
            sb.read(&mut u, STATUS),
            STATUS_IDLE,
            "both FIFOs empty, lines high"
        );
        assert_eq!(
            sb.read(&mut u, CLK_CONF),
            0x0370_1000,
            "XTAL / 2, both halves clocked"
        );
        assert_eq!(sb.read(&mut u, CLKDIV), 0x2b6);
        assert_eq!(sb.read(&mut u, MEM_CONF), 0x0014_0012);
        // 40 MHz / (1 + 1) × 16 / (694 × 16) = 28,818 baud.
        assert_eq!(u.baud(), 28_818);
        assert_eq!(
            sb.read(&mut u, INT_RAW),
            INT_TXFIFO_EMPTY,
            "an empty TX FIFO is a level"
        );
    }

    /// `uart_tx_one_char_uart` (`0x4004_8804`), register for register: the
    /// full test on `status` bits 16:25, then the byte into `fifo`. And the
    /// byte leaves at the programmed rate, one symbol later.
    #[test]
    fn the_roms_console_write_lands_on_the_wire_one_symbol_later() {
        let (mut sb, mut u, log) = rig();
        // `clkdiv` at XTAL/2 for 115,200: 20 MHz × 16 / 115200 = 2777.7 →
        // integral 173, frag 9 (2777 = 173 × 16 + 9), which the registers
        // describe back as 320,000,000 / 2777 = 115,232 baud.
        sb.write(&mut u, CLKDIV, 173 | (9 << CLKDIV_FRAG_SHIFT));
        assert_eq!(u.baud(), 115_232);
        let status = sb.read(&mut u, STATUS);
        assert_eq!(
            (status >> 16) & 0x380,
            0,
            "`bany a2, 0x380` falls through: room"
        );
        sb.write(&mut u, FIFO, u32::from(b'e'));
        assert!(log.is_empty(), "the byte is on the wire, not yet delivered");
        // The default frame is 8N1: ten bits at ~2,083 CPU cycles a bit.
        let symbol = u.symbol_cycles().expect("clocked");
        assert!((20_000..21_500).contains(&symbol), "{symbol}");
        run_to(&mut sb, &mut u, symbol - 1);
        assert!(log.is_empty());
        run_to(&mut sb, &mut u, symbol);
        assert_eq!(log.text(), "e");
        // `uart_tx_flush`: `status & 0x3ff0000 == 0` once the FIFO drained.
        assert_eq!(sb.read(&mut u, STATUS) & 0x03ff_0000, 0);
    }

    #[test]
    fn a_full_fifo_is_what_the_rom_spins_on_and_the_129th_byte_is_dropped() {
        let (mut sb, mut u, _) = rig();
        // The first byte goes straight to the shifter; 128 more fill the FIFO.
        for i in 0..129u32 {
            sb.write(&mut u, FIFO, i);
        }
        let status = sb.read(&mut u, STATUS);
        assert_eq!((status >> 16) & 0x3ff, 128);
        assert_ne!((status >> 16) & 0x380, 0, "the ROM's full test holds");
        sb.write(&mut u, FIFO, 0xaa);
        assert_eq!(u.tx_dropped(), 1);
    }

    #[test]
    fn uart_attach_writes_int_clr_and_the_fifo_resets_are_bits_17_and_18() {
        let (mut sb, mut u, _) = rig();
        // `uartAttach` (`0x4004_8860`): int_clr = 1 on UART0 and UART1.
        sb.write(&mut u, INT_CLR, 1);
        assert_eq!(sb.read(&mut u, INT_CLR), 0);
        // The classic's bit numbers, not the C6's.
        sb.write(&mut u, FIFO, 1);
        sb.write(&mut u, FIFO, 2);
        assert_eq!((sb.read(&mut u, STATUS) >> 16) & 0x3ff, 1);
        let conf0 = sb.read(&mut u, CONF0);
        sb.write(&mut u, CONF0, conf0 | CONF0_TXFIFO_RST);
        assert_eq!(
            (sb.read(&mut u, STATUS) >> 16) & 0x3ff,
            0,
            "bit 18 empties the TX FIFO"
        );
        sb.write(&mut u, CONF0, conf0);
    }

    #[test]
    fn the_names_and_grades_come_from_the_generated_table() {
        let u = Uart::uart0(None);
        assert_eq!(u.reg_name(STATUS), Some("status"));
        assert_eq!(u.reg_name(CLK_CONF), Some("clk_conf"));
        assert_eq!(u.reg_name(MEM_CONF), Some("mem_conf"));
        assert_eq!(u.reg_grade(FIFO), Some(RegGrade::Documented));
        assert_eq!(
            u.reg_grade(RXD_CNT),
            Some(RegGrade::Modeled),
            "auto-baud is not modelled"
        );
        assert_eq!(UART_LEN, 0x100);
    }

    #[test]
    fn the_state_round_trips() {
        let (mut sb, mut u, _) = rig();
        sb.write(&mut u, CLKDIV, 173 | (9 << CLKDIV_FRAG_SHIFT));
        sb.write(&mut u, FIFO, u32::from(b'A'));
        sb.write(&mut u, FIFO, u32::from(b'B'));
        let blob = u.save_state();
        let (_, mut back, _) = rig();
        back.load_state(&blob);
        assert_eq!(back.tx_pending(), 2);
        assert_eq!(back.baud(), u.baud());
        assert_eq!(sb.read(&mut back, STATUS), sb.read(&mut u, STATUS));
    }
}
