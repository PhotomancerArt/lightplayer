//! `DMA` at `0x6003_F000` — the S3's AHB GDMA, its **OUT** half (M9, the
//! parallel WS281x output).
//!
//! Five channels, each an IN and an OUT half, `0xC0` apart
//! (`esp32s3-0.35.2/src/dma.rs`, the `CH%s` cluster at `0x00..0x3c0`). The
//! one user this machine serves is esp-hal 1.1.1's LCD_CAM i8080 driver
//! feeding [`super::lcd_cam`], so the model is that path's:
//!
//! - **The registers it writes** (`third_party/esp-hal/src/dma/gdma/ahb_v1.rs`
//!   and `dma/mod.rs`'s `do_prepare` / `start_transfer`): `out_conf0`
//!   (`out_rst`, `out_auto_wrback`), `out_conf1` (`out_check_owner`),
//!   `out_link` (`outlink_addr`, `outlink_start`, `outlink_stop`), the
//!   `out_int` cluster (`raw`, `st`, `ena`, `clr`), `out_peri_sel`, and the
//!   `outfifo_status` word `start_transfer` **spins on**: it waits for
//!   `outfifo_empty_l3` to clear (or an interrupt to pend) before it returns.
//! - **The descriptor walk is the machine's**, not this view's: a peripheral
//!   has no way into guest RAM ([`lp_emu_esp_common::BusCx`] carries none), so
//!   `outlink_start` records a [`StartRequest`] and asks the machine for a
//!   boundary; the machine walks the chain out of internal RAM between
//!   windows (`Machine::service_gdma`, the C6's `drain_tx_log` shape) and
//!   hands the bytes back through [`Gdma::loaded`]. The FIFO then reads
//!   non-empty, which is what releases `start_transfer`'s spin.
//! - **The stream waits in the shared [`GdmaHandle`]** for the peripheral
//!   `out_peri_sel` names (`5` = LCD_CAM, `esp-metadata-generated-0.4.0`'s
//!   `(LCD_CAM, LcdCam, 5)`). The peripheral takes it when it starts and
//!   reports back when it has shifted the last byte out
//!   ([`GdmaState::complete`]): `out_done`, `out_eof` and `out_total_eof`
//!   raise then, the FIFO reads empty again, and `out_eof_des_addr` names the
//!   last descriptor.
//!
//! # What is not modelled, said so
//!
//! - **The IN half** (the CAM's capture side, `test_lcd_ws281x`'s analyzer):
//!   its registers are accept-and-remember, and an `inlink_start` is a
//!   `log::warn!`, once. The CAM is silicon-only test equipment (M9 §6c).
//! - **Reading as it goes.** The chain is read whole, at the boundary after
//!   `outlink_start`. A CPU that rewrote the buffer mid-transfer would be
//!   seen on silicon and not here; esp-hal's driver never does.
//! - **`outlink_restart`, burst modes, priorities, weights, FIFO depths and
//!   the external-memory blocks**: accepted and remembered.
//! - **PSRAM.** A descriptor or a buffer outside internal RAM
//!   ([`INTERNAL_RAM`]) is a descriptor error here, and a strict-bus stop
//!   under `--strict-bus` (the machine raises it). The S3's EDMA can read
//!   PSRAM, but the parallel LED design rules it out (an underrun during
//!   every flash write — `led-channels-spike/report.md`, "PSRAM and the LED
//!   drivers"), so an image that puts the bus buffer there is a finding.
//!
//! # Grades
//!
//! `documented` for the live registers above: the PAC's field docs are the
//! source and nothing here has been read off silicon. Everything else keeps
//! the PAC's grade through [`RegFile::with_pac_grades`]. The descriptor
//! layout (`size` 0:11, `length` 12:23, `suc_eof` 30, `owner` 31, then the
//! buffer and next words) is esp-hal's `DmaDescriptorFlags`
//! (`third_party/esp-hal/src/dma/mod.rs`), which is the TRM's.

use std::sync::{Arc, Mutex};

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::regs;

/// The block's aperture: the five `CH` clusters (`0x000..0x3c0`), the misc
/// and external-memory registers, and `date` at `+0x40c`.
pub const LEN: u32 = 0x410;
/// Channels: `soc_has_dma_ch0..4`.
pub const CHANNELS: usize = 5;
/// One `CH` cluster.
pub const CH_STRIDE: u32 = 0xC0;

// ---- per-channel offsets (within a cluster), `esp32s3-0.35.2/src/dma/ch.rs`
pub const IN_LINK: u32 = 0x20;
pub const OUT_CONF0: u32 = 0x60;
pub const OUT_CONF1: u32 = 0x64;
pub const OUT_INT_RAW: u32 = 0x68;
pub const OUT_INT_ST: u32 = 0x6c;
pub const OUT_INT_ENA: u32 = 0x70;
pub const OUT_INT_CLR: u32 = 0x74;
pub const OUTFIFO_STATUS: u32 = 0x78;
pub const OUT_LINK: u32 = 0x80;
pub const OUT_STATE: u32 = 0x84;
pub const OUT_EOF_DES_ADDR: u32 = 0x88;
pub const OUT_EOF_BFR_DES_ADDR: u32 = 0x8c;
pub const OUT_DSCR: u32 = 0x90;
pub const OUT_DSCR_BF0: u32 = 0x94;
pub const OUT_DSCR_BF1: u32 = 0x98;
pub const OUT_PERI_SEL: u32 = 0xa8;

// ---- fields
/// `out_conf0.out_rst`: reset the channel's FSM and FIFO pointer.
pub const OUT_RST: u32 = 1 << 0;
/// `out_conf0.out_auto_wrback`: hand each descriptor back to the CPU.
pub const OUT_AUTO_WRBACK: u32 = 1 << 2;
/// `out_conf1.out_check_owner`.
pub const OUT_CHECK_OWNER: u32 = 1 << 12;
/// `out_link.outlink_addr`: the descriptor's address, low 20 bits.
pub const OUTLINK_ADDR: u32 = 0x000F_FFFF;
pub const OUTLINK_STOP: u32 = 1 << 20;
pub const OUTLINK_START: u32 = 1 << 21;
pub const OUTLINK_RESTART: u32 = 1 << 22;
/// `outlink_park`: the descriptor FSM is idle.
pub const OUTLINK_PARK: u32 = 1 << 23;
/// `in_link.inlink_start`.
pub const INLINK_START: u32 = 1 << 22;
/// `out_int` bits.
pub const INT_OUT_DONE: u32 = 1 << 0;
pub const INT_OUT_EOF: u32 = 1 << 1;
pub const INT_OUT_DSCR_ERR: u32 = 1 << 2;
pub const INT_OUT_TOTAL_EOF: u32 = 1 << 3;
/// Every `out_int` bit the PAC defines (0..=7).
pub const INT_OUT_MASK: u32 = 0xff;
/// `outfifo_status` at reset (the PAC's): the three `empty` bits (1, 3, 5)
/// set, the `remain_under` bits (23..=26) set.
pub const OUTFIFO_STATUS_RESET: u32 = 0x0780_002a;
/// The three `outfifo_empty_l1/l2/l3` bits.
pub const OUTFIFO_EMPTY: u32 = (1 << 1) | (1 << 3) | (1 << 5);

/// `out_peri_sel` for LCD_CAM (`esp-metadata-generated-0.4.0`'s
/// `_for_each_inner_peripheral!((LCD_CAM, LcdCam, 5))`).
pub const PERI_LCD_CAM: u8 = 5;

/// The DMA's view of internal RAM's high bits. `outlink_addr` holds only the
/// low 20 bits of the descriptor's address, and esp-hal writes a whole data-bus
/// pointer into it; internal SRAM's data-bus window is `0x3FC8_8000..
/// 0x3FD0_0000`, all of which shares the top twelve bits `0x3FC`. **Modeled**:
/// read off the field width and the memory map, not off silicon.
pub const DMA_ADDR_BASE: u32 = 0x3FC0_0000;

/// Where a descriptor or a buffer may live: internal SRAM on the data bus
/// (SRAM1 `0x3FC8_8000`, through the end of SRAM2's data window).
pub const INTERNAL_RAM: core::ops::Range<u32> = 0x3FC8_8000..0x3FD0_0000;

/// A chain longer than this is treated as a loop and ends there: 4,096
/// descriptors of at most 4,095 bytes is 16 MB, past any internal RAM.
pub const MAX_DESCRIPTORS: usize = 4096;

/// The interrupt source of OUT channel `ch`: `DMA_OUT_CH0..4` = 71..=75.
pub fn out_source(ch: usize) -> u16 {
    regs::source::DMA_OUT_CH0 + ch as u16
}

/// A DMA descriptor as esp-hal lays it out (`DmaDescriptor`: flags, buffer,
/// next).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    /// Its own address.
    pub addr: u32,
    /// `size` (0:11): the buffer's capacity.
    pub size: u32,
    /// `length` (12:23): the bytes in it to send.
    pub length: u32,
    /// `suc_eof` (30): the last descriptor of the frame.
    pub suc_eof: bool,
    /// `owner` (31): 1 = the DMA may use it.
    pub owner_dma: bool,
    pub buffer: u32,
    /// The next descriptor, 0 at the end.
    pub next: u32,
}

/// `owner`, bit 31 of the first word.
pub const DESC_OWNER: u32 = 1 << 31;
/// `suc_eof`, bit 30.
pub const DESC_SUC_EOF: u32 = 1 << 30;

impl Descriptor {
    /// The three words at `addr`.
    pub fn parse(addr: u32, words: [u32; 3]) -> Self {
        Self {
            addr,
            size: words[0] & 0xfff,
            length: (words[0] >> 12) & 0xfff,
            suc_eof: words[0] & DESC_SUC_EOF != 0,
            owner_dma: words[0] & DESC_OWNER != 0,
            buffer: words[1],
            next: words[2],
        }
    }
}

/// `outlink_start`, as the machine needs it to walk the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartRequest {
    pub ch: usize,
    /// The first descriptor's address: [`DMA_ADDR_BASE`] over `outlink_addr`.
    pub first: u32,
    pub check_owner: bool,
    pub auto_wrback: bool,
    pub peri: u8,
    /// The cycle of the store.
    pub at: Cycles,
}

/// What the machine read out of RAM for one start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chain {
    pub bytes: Vec<u8>,
    /// The last descriptor (the one with `suc_eof`, or the one whose `next`
    /// ended the chain).
    pub last: u32,
    /// The one before it, for `out_eof_bfr_des_addr`.
    pub before_last: u32,
    pub descriptors: usize,
}

/// Why a walk stopped short.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkFault {
    /// The address that could not be used.
    pub address: u32,
    /// Outside internal RAM (a strict-bus stop), rather than an owner error.
    pub outside_ram: bool,
    pub why: String,
}

/// A stream waiting for (or taken by) its peripheral.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stream {
    pub ch: usize,
    pub peri: u8,
    pub bytes: Vec<u8>,
    pub last: u32,
    pub before_last: u32,
}

/// One OUT channel's live state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutChannel {
    pub conf0: u32,
    pub conf1: u32,
    pub link: u32,
    pub peri_sel: u32,
    pub int_raw: u32,
    pub int_ena: u32,
    pub fifo_status: u32,
    pub eof_des_addr: u32,
    pub eof_bfr_des_addr: u32,
    pub state: u32,
    pub start: Option<StartRequest>,
    pub stream: Option<Stream>,
}

impl OutChannel {
    fn at_reset() -> Self {
        Self {
            conf0: 0x08,
            conf1: 0,
            link: OUTLINK_PARK,
            peri_sel: 0x3f,
            fifo_status: OUTFIFO_STATUS_RESET,
            ..Self::default()
        }
    }
}

/// The OUT channels, shared between the register view and the peripherals
/// that consume their streams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GdmaState {
    pub out: Vec<OutChannel>,
    /// `inlink_start` was warned about.
    in_warned: bool,
}

impl Default for GdmaState {
    fn default() -> Self {
        Self {
            out: (0..CHANNELS).map(|_| OutChannel::at_reset()).collect(),
            in_warned: false,
        }
    }
}

/// The handle both sides hold.
pub type GdmaHandle = Arc<Mutex<GdmaState>>;

/// A fresh handle at reset.
pub fn handle() -> GdmaHandle {
    Arc::new(Mutex::new(GdmaState::default()))
}

impl GdmaState {
    /// Take the stream waiting for peripheral `peri`, if one is.
    pub fn take_stream(&mut self, peri: u8) -> Option<Stream> {
        self.out
            .iter_mut()
            .find(|c| c.stream.as_ref().is_some_and(|s| s.peri == peri))
            .and_then(|c| c.stream.take())
    }

    /// The consuming peripheral shifted the last byte of `stream` out at
    /// `cx.now`: the channel's done, EOF and total-EOF raise, its FIFO reads
    /// empty, and the EOF descriptor addresses name the chain's end.
    pub fn complete(&mut self, stream: &Stream, cx: &mut BusCx<'_>) {
        let Some(c) = self.out.get_mut(stream.ch) else {
            return;
        };
        c.int_raw |= INT_OUT_DONE | INT_OUT_EOF | INT_OUT_TOTAL_EOF;
        c.fifo_status |= OUTFIFO_EMPTY;
        c.eof_des_addr = stream.last;
        c.eof_bfr_des_addr = stream.before_last;
        c.link |= OUTLINK_PARK;
        Self::sync_irq(c, stream.ch, cx);
    }

    fn sync_irq(c: &OutChannel, ch: usize, cx: &mut BusCx<'_>) {
        cx.irq
            .set_level(out_source(ch), c.int_raw & c.int_ena & INT_OUT_MASK != 0);
    }
}

/// The register view.
#[derive(Debug)]
pub struct Gdma {
    regs: RegFile,
    state: GdmaHandle,
}

impl Gdma {
    pub fn new(state: GdmaHandle) -> Self {
        Self {
            regs: RegFile::new("DMA", LEN)
                .with_names(regs::DMA)
                .with_pac_grades(),
            state,
        }
    }

    /// The `outlink_start`s since the last call, for the machine to walk.
    pub fn take_starts(&mut self) -> Vec<StartRequest> {
        let mut st = self.state.lock().expect("gdma poisoned");
        st.out.iter_mut().filter_map(|c| c.start.take()).collect()
    }

    /// The machine walked `req`'s chain. On success the bytes wait for
    /// `out_peri_sel`'s peripheral and the FIFO reads non-empty; a fault is
    /// `out_dscr_err`.
    pub fn loaded(
        &mut self,
        req: &StartRequest,
        result: Result<Chain, WalkFault>,
        cx: &mut BusCx<'_>,
    ) {
        let mut st = self.state.lock().expect("gdma poisoned");
        let Some(c) = st.out.get_mut(req.ch) else {
            return;
        };
        match result {
            Ok(chain) => {
                c.fifo_status &= !OUTFIFO_EMPTY;
                c.link &= !OUTLINK_PARK;
                c.state = chain.last & 0x3_ffff;
                if req.peri != PERI_LCD_CAM {
                    log::warn!(
                        "GDMA: OUT channel {} feeds peripheral {} — only LCD_CAM ({PERI_LCD_CAM}) \
                         consumes a stream on this machine; {} bytes wait unread",
                        req.ch,
                        req.peri,
                        chain.bytes.len()
                    );
                }
                c.stream = Some(Stream {
                    ch: req.ch,
                    peri: req.peri,
                    bytes: chain.bytes,
                    last: chain.last,
                    before_last: chain.before_last,
                });
            }
            Err(fault) => {
                log::warn!(
                    "GDMA: OUT channel {} descriptor error at {:#010x}: {}",
                    req.ch,
                    fault.address,
                    fault.why
                );
                c.int_raw |= INT_OUT_DSCR_ERR;
                c.link |= OUTLINK_PARK;
            }
        }
        GdmaState::sync_irq(c, req.ch, cx);
    }

    /// Which channel and which live register `off` is, if it is one.
    fn live(off: u32) -> Option<(usize, u32)> {
        let w = off & !3;
        if w >= CH_STRIDE * CHANNELS as u32 {
            return None;
        }
        let ch = (w / CH_STRIDE) as usize;
        let reg = w % CH_STRIDE;
        matches!(
            reg,
            IN_LINK
                | OUT_CONF0
                | OUT_CONF1
                | OUT_INT_RAW
                | OUT_INT_ST
                | OUT_INT_ENA
                | OUT_INT_CLR
                | OUTFIFO_STATUS
                | OUT_LINK
                | OUT_STATE
                | OUT_EOF_DES_ADDR
                | OUT_EOF_BFR_DES_ADDR
                | OUT_PERI_SEL
        )
        .then_some((ch, reg))
    }
}

impl Peripheral for Gdma {
    fn name(&self) -> &'static str {
        "DMA"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let Some((ch, reg)) = Self::live(off) else {
            return self.regs.read(off, width, cx);
        };
        if reg == IN_LINK {
            return self.regs.read(off, width, cx);
        }
        let st = self.state.lock().expect("gdma poisoned");
        let c = &st.out[ch];
        let word = match reg {
            OUT_CONF0 => c.conf0,
            OUT_CONF1 => c.conf1,
            OUT_INT_RAW => c.int_raw,
            OUT_INT_ST => c.int_raw & c.int_ena,
            OUT_INT_ENA => c.int_ena,
            OUT_INT_CLR => 0,
            OUTFIFO_STATUS => c.fifo_status,
            OUT_LINK => c.link,
            OUT_STATE => c.state,
            OUT_EOF_DES_ADDR => c.eof_des_addr,
            OUT_EOF_BFR_DES_ADDR => c.eof_bfr_des_addr,
            OUT_PERI_SEL => c.peri_sel,
            _ => 0,
        };
        lane_of(word, off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let Some((ch, reg)) = Self::live(off) else {
            self.regs.write(off, width, value, cx);
            return;
        };
        if reg == IN_LINK {
            let word = merge_lane(self.regs.stored(off & !3), off, width, value);
            if word & INLINK_START != 0 {
                let mut st = self.state.lock().expect("gdma poisoned");
                if !st.in_warned {
                    st.in_warned = true;
                    log::warn!(
                        "GDMA: inlink_start on channel {ch} — the IN half (the CAM's capture) is \
                         not modelled here; nothing will be written"
                    );
                }
            }
            self.regs.write(off, width, value & !INLINK_START, cx);
            return;
        }
        let mut st = self.state.lock().expect("gdma poisoned");
        let c = &mut st.out[ch];
        let merge = |old: u32| merge_lane(old, off, width, value);
        match reg {
            OUT_CONF0 => {
                let word = merge(c.conf0);
                if word & OUT_RST != 0 {
                    c.start = None;
                    c.stream = None;
                    c.fifo_status = OUTFIFO_STATUS_RESET;
                    c.link |= OUTLINK_PARK;
                }
                c.conf0 = word;
            }
            OUT_CONF1 => c.conf1 = merge(c.conf1),
            OUT_INT_ENA => c.int_ena = merge(c.int_ena) & INT_OUT_MASK,
            OUT_INT_CLR => c.int_raw &= !merge(0),
            OUT_PERI_SEL => c.peri_sel = merge(c.peri_sel) & 0x3f,
            OUT_LINK => {
                let word = merge(c.link);
                c.link = (word & OUTLINK_ADDR) | (c.link & OUTLINK_PARK);
                if word & OUTLINK_STOP != 0 {
                    c.start = None;
                    c.stream = None;
                    c.fifo_status |= OUTFIFO_EMPTY;
                    c.link |= OUTLINK_PARK;
                }
                if word & OUTLINK_RESTART != 0 {
                    log::warn!(
                        "GDMA: outlink_restart on channel {ch} is not modelled; the chain is \
                         not resumed"
                    );
                }
                if word & OUTLINK_START != 0 {
                    c.int_raw &= !(INT_OUT_DONE | INT_OUT_EOF | INT_OUT_TOTAL_EOF);
                    c.link &= !OUTLINK_PARK;
                    c.start = Some(StartRequest {
                        ch,
                        first: DMA_ADDR_BASE | (word & OUTLINK_ADDR),
                        check_owner: c.conf1 & OUT_CHECK_OWNER != 0,
                        auto_wrback: c.conf0 & OUT_AUTO_WRBACK != 0,
                        peri: (c.peri_sel & 0x3f) as u8,
                        at: cx.now,
                    });
                    // The walk is the machine's, at the next boundary — and
                    // `start_transfer` spins on the FIFO until it is done.
                    cx.yield_to_machine();
                }
            }
            // `raw`, `st`, the FIFO status, the state and the EOF addresses
            // are the hardware's.
            _ => {}
        }
        GdmaState::sync_irq(c, ch, cx);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::DMA.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        if Self::live(off).is_some_and(|(_, reg)| reg != IN_LINK) {
            return Some(RegGrade::Documented);
        }
        self.regs.reg_grade(off)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn save_state(&self) -> Vec<u8> {
        let st = self.state.lock().expect("gdma poisoned");
        let mut out = Vec::new();
        for c in &st.out {
            for w in [
                c.conf0,
                c.conf1,
                c.link,
                c.peri_sel,
                c.int_raw,
                c.int_ena,
                c.fifo_status,
                c.eof_des_addr,
                c.eof_bfr_des_addr,
                c.state,
            ] {
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        const PER: usize = 10 * 4;
        if bytes.len() < PER * CHANNELS {
            log::warn!("DMA: load_state blob too short, ignored");
            return;
        }
        let mut st = self.state.lock().expect("gdma poisoned");
        for (i, c) in st.out.iter_mut().enumerate() {
            let w = |k: usize| {
                let at = i * PER + k * 4;
                u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
            };
            *c = OutChannel {
                conf0: w(0),
                conf1: w(1),
                link: w(2),
                peri_sel: w(3),
                int_raw: w(4),
                int_ena: w(5),
                fifo_status: w(6),
                eof_des_addr: w(7),
                eof_bfr_des_addr: w(8),
                state: w(9),
                // A snapshot is taken between windows, where no start waits
                // and no stream is in flight on any path this machine runs.
                start: None,
                stream: None,
            };
        }
        drop(st);
        self.regs.load_state(&bytes[PER * CHANNELS..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    const CH1: u32 = CH_STRIDE;

    #[test]
    fn a_start_records_the_chain_yields_and_releases_the_fifo_once_loaded() {
        let mut sb = Sandbox::new();
        let state = handle();
        let mut g = Gdma::new(state.clone());
        assert_eq!(sb.read(&mut g, CH1 + OUTFIFO_STATUS), OUTFIFO_STATUS_RESET);
        assert_eq!(sb.read(&mut g, CH1 + OUT_PERI_SEL), 0x3f);
        sb.write(&mut g, CH1 + OUT_PERI_SEL, PERI_LCD_CAM as u32);
        sb.write(&mut g, CH1 + OUT_CONF0, OUT_RST);
        sb.write(&mut g, CH1 + OUT_CONF0, 0);
        sb.write(&mut g, CH1 + OUT_LINK, 0x3FC9_1230 & OUTLINK_ADDR);
        assert!(!sb.yield_now, "setting the address is not a start");
        sb.write(
            &mut g,
            CH1 + OUT_LINK,
            (0x3FC9_1230 & OUTLINK_ADDR) | OUTLINK_START,
        );
        assert!(sb.yield_now, "a start asks for a boundary");
        let starts = g.take_starts();
        assert_eq!(starts.len(), 1);
        let req = starts[0];
        assert_eq!(
            (req.ch, req.first, req.peri),
            (1, 0x3FC9_1230, PERI_LCD_CAM)
        );
        // The FIFO stays empty until the machine has read the chain.
        assert_ne!(sb.read(&mut g, CH1 + OUTFIFO_STATUS) & (1 << 5), 0);
        let chain = Chain {
            bytes: vec![1, 2, 3],
            last: 0x3FC9_1230,
            before_last: 0,
            descriptors: 1,
        };
        let mut cx = sb.cx();
        g.loaded(&req, Ok(chain), &mut cx);
        drop(cx);
        assert_eq!(sb.read(&mut g, CH1 + OUTFIFO_STATUS) & (1 << 5), 0);
        let stream = state.lock().unwrap().take_stream(PERI_LCD_CAM).unwrap();
        assert_eq!(stream.bytes, vec![1, 2, 3]);
        // The consumer finishes: done, EOF and total EOF, the FIFO empty.
        sb.write(&mut g, CH1 + OUT_INT_ENA, INT_OUT_TOTAL_EOF);
        let mut cx = sb.cx();
        state.lock().unwrap().complete(&stream, &mut cx);
        drop(cx);
        let raw = sb.read(&mut g, CH1 + OUT_INT_RAW);
        assert_eq!(raw & 0xf, INT_OUT_DONE | INT_OUT_EOF | INT_OUT_TOTAL_EOF);
        assert_eq!(sb.read(&mut g, CH1 + OUT_INT_ST), INT_OUT_TOTAL_EOF);
        assert!(sb.irq.level(out_source(1)), "DMA_OUT_CH1 raised");
        assert_eq!(sb.read(&mut g, CH1 + OUT_EOF_DES_ADDR), 0x3FC9_1230);
        sb.write(&mut g, CH1 + OUT_INT_CLR, 0xff);
        assert_eq!(sb.read(&mut g, CH1 + OUT_INT_RAW), 0);
        assert!(!sb.irq.level(out_source(1)));
    }

    #[test]
    fn a_walk_fault_is_a_descriptor_error() {
        let mut sb = Sandbox::new();
        let mut g = Gdma::new(handle());
        sb.write(&mut g, OUT_LINK, OUTLINK_START);
        let req = g.take_starts()[0];
        let mut cx = sb.cx();
        g.loaded(
            &req,
            Err(WalkFault {
                address: 0x3C00_0000,
                outside_ram: true,
                why: "outside internal RAM".into(),
            }),
            &mut cx,
        );
        drop(cx);
        assert_ne!(sb.read(&mut g, OUT_INT_RAW) & INT_OUT_DSCR_ERR, 0);
    }

    #[test]
    fn descriptors_parse_as_esp_hal_writes_them() {
        // size 4092, length 300, suc_eof, owner = DMA.
        let w0 = 4092 | (300 << 12) | DESC_SUC_EOF | DESC_OWNER;
        let d = Descriptor::parse(0x3FC9_0000, [w0, 0x3FC9_1000, 0]);
        assert_eq!((d.size, d.length), (4092, 300));
        assert!(d.suc_eof && d.owner_dma);
        assert_eq!((d.buffer, d.next), (0x3FC9_1000, 0));
    }

    #[test]
    fn the_state_survives_a_snapshot() {
        let mut sb = Sandbox::new();
        let mut a = Gdma::new(handle());
        sb.write(&mut a, CH1 + OUT_INT_ENA, 0x5);
        sb.write(&mut a, CH1 + OUT_PERI_SEL, 5);
        let blob = a.save_state();
        let mut b = Gdma::new(handle());
        b.load_state(&blob);
        assert_eq!(sb.read(&mut b, CH1 + OUT_INT_ENA), 0x5);
        assert_eq!(sb.read(&mut b, CH1 + OUT_PERI_SEL), 5);
    }
}
