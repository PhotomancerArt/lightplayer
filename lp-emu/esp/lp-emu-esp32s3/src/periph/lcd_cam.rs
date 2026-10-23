//! `LCD_CAM` at `0x6004_1000` — the **LCD half in i8080 mode**, shifting a
//! GDMA stream onto up to sixteen data pads (M9, the parallel WS281x output).
//!
//! The parallel LED path writes one frame buffer in which bit `j` of every
//! byte is lane `j`'s level for one bus clock, and LCD_CAM shifts it out of
//! `LCD_DATA_0..7` (or `0..15` on a 16-bit bus) with no CPU involvement
//! (`led-channels-spike/report.md`). This view plays that: the per-pad WS281x
//! decoders the machine already runs (the ones that read the RMT's pads) read
//! each lane back, so a lane's bytes are checked exactly as an RMT channel's
//! are.
//!
//! # What esp-hal's driver writes, and what this does with it
//!
//! `third_party/esp-hal/src/lcd_cam/lcd/i8080.rs` (1.1.1):
//!
//! - `apply_config` writes `lcd_clock` (source, divider, `clkcnt_n`),
//!   clears `lcd_ctrl.lcd_rgb_mode_en`, sets `lcd_misc` (setup/hold cycles,
//!   `lcd_bk_en`) and `lcd_data_dout_mode`, then `lcd_user.lcd_update`;
//! - `send` sets `lcd_reset` and `lcd_afifo_reset`, the command and dummy
//!   bits, `lcd_2byte_en`, then `lcd_always_out_en | lcd_dout`; starts the
//!   GDMA channel ([`super::gdma`]); clears `lc_dma_int_clr.trans_done`; and
//!   sets `lcd_update | lcd_start`;
//! - `wait` **spins on `lcd_start` reading clear** (the TRM: in continuous
//!   output the LCD stops when "all the data in GDMA is sent out", and the bit
//!   clears), then clears `trans_done`.
//!
//! So: `lcd_start` takes the stream GDMA holds for peripheral 5 (or waits for
//! it — the machine walks the chain at the boundary after `outlink_start`,
//! which may come after this store), then plays it: setup cycles, the command
//! and dummy phases if enabled, the data, the hold cycles. When the last slot
//! has been shifted out, `lcd_start` clears, `lcd_trans_done` raises (source
//! `LCD_CAM`, 24), and the GDMA channel's done and EOF bits raise.
//!
//! # Event-driven, never per cycle
//!
//! 8 lanes × 500 LEDs is 36,750 bus bytes a frame. Each slot's time is the
//! start plus `k` PCLK periods, computed exactly from the clock word (a
//! rational number of CPU cycles: [`ClockSetup::period`]), and the edges are
//! driven [`CHUNK`] slots at a time from a scheduled event, only on lanes
//! whose level changes. Nothing runs per emulated cycle.
//!
//! # The clock
//!
//! `f_LCD_CLK = f_src / (div_num + div_b/div_a)`, and `f_PCLK = f_LCD_CLK /
//! (clkcnt_n + 1)` unless `lcd_clk_equ_sysclk` (then `f_PCLK = f_LCD_CLK`) —
//! the PAC's field docs (`esp32s3-0.35.2/src/lcd_cam/lcd_clock.rs`). The
//! source: `lcd_clk_sel` 1 = XTAL (40 MHz), 2 = PLL_D2 (240 MHz), 3 =
//! PLL_F160M (160 MHz) — esp-hal's own table of the three
//! (`calculate_clkm`'s `[xtal, pll_d2, crypto_pwm]`, indexed `sel − 1`).
//! **Modeled**: the frequencies are the clock tree's nominal ones, and no
//! S3 silicon edge time has graded them. `div_num = 0` reads as 256 (the
//! family's convention for an 8-bit divider). **Timing is ungraded** until a
//! silicon capture grades it; the bytes are what transfers.
//!
//! # The CAM half is not modelled, and refuses
//!
//! `cam_ctrl`, `cam_ctrl1` and `cam_rgb_yuv` (`+0x04..+0x10`) are **not
//! mapped**: the block is registered as two windows, `lcd_clock` at `+0x00`
//! ([`LcdClock`]) and the LCD's registers from `+0x10` ([`Lcd`]). So under
//! `--strict-bus` a CAM access is a stop, as M9 §6c asks: the CAM is the
//! spike's analyzer, never product. The two windows share the clock word
//! through a [`ClockCell`].
//!
//! # What else is not modelled
//!
//! RGB mode (`lcd_rgb_mode_en`, a warning and no output), `lcd_next_frame_en`
//! (a warning), the per-bit `dout_mode` delays (accepted), PCLK, CS, DC and
//! the sync lines (never driven: a WS281x lane needs none of them).

use std::sync::{Arc, Mutex};

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, SignalId, Width, event_id, event_local};

use super::gdma::{GdmaHandle, PERI_LCD_CAM, Stream};
use crate::memmap;
use crate::regs;
use crate::regs::output_signals::LCD_DATA_0;

/// The whole block's span, for the name table.
pub const LEN: u32 = 0x100;
/// The `lcd_clock` window: one register.
pub const CLOCK_LEN: u32 = 0x4;
/// Where the LCD window starts: after the CAM's three registers.
pub const LCD_OFF: u32 = 0x10;
/// The LCD window's length.
pub const LCD_LEN: u32 = LEN - LCD_OFF;

// ---- block offsets
pub const LCD_CLOCK: u32 = 0x00;
pub const LCD_USER: u32 = 0x14;
pub const LCD_MISC: u32 = 0x18;
pub const LCD_CTRL: u32 = 0x1c;
pub const LCD_CMD_VAL: u32 = 0x28;
pub const LC_DMA_INT_ENA: u32 = 0x64;
pub const LC_DMA_INT_RAW: u32 = 0x68;
pub const LC_DMA_INT_ST: u32 = 0x6c;
pub const LC_DMA_INT_CLR: u32 = 0x70;

// ---- lcd_user
pub const LCD_DOUT_CYCLELEN: u32 = 0x1fff;
pub const LCD_ALWAYS_OUT_EN: u32 = 1 << 13;
pub const LCD_8BITS_ORDER: u32 = 1 << 19;
pub const LCD_UPDATE: u32 = 1 << 20;
pub const LCD_BIT_ORDER: u32 = 1 << 21;
pub const LCD_BYTE_ORDER: u32 = 1 << 22;
pub const LCD_2BYTE_EN: u32 = 1 << 23;
pub const LCD_DOUT: u32 = 1 << 24;
pub const LCD_DUMMY: u32 = 1 << 25;
pub const LCD_CMD: u32 = 1 << 26;
pub const LCD_START: u32 = 1 << 27;
pub const LCD_RESET: u32 = 1 << 28;
pub const LCD_DUMMY_CYCLELEN_SHIFT: u32 = 29;
pub const LCD_CMD_2_CYCLE_EN: u32 = 1 << 31;

// ---- lcd_misc
pub const LCD_VFK_SHIFT: u32 = 6;
pub const LCD_VBK_SHIFT: u32 = 12;
pub const LCD_NEXT_FRAME_EN: u32 = 1 << 25;
pub const LCD_BK_EN: u32 = 1 << 26;
pub const LCD_AFIFO_RESET: u32 = 1 << 27;

// ---- lcd_ctrl
pub const LCD_RGB_MODE_EN: u32 = 1 << 31;

// ---- lc_dma_int
pub const LCD_TRANS_DONE: u32 = 1 << 1;
/// The bits the four `lc_dma_int_*` registers define.
pub const LC_INT_MASK: u32 = 0xf;

/// Slots emitted per scheduled event.
pub const CHUNK: usize = 1024;

/// The two event tags.
const EV_CHUNK: u16 = 0;
const EV_DONE: u16 = 1;

/// The clock word, shared by the two windows.
pub type ClockCell = Arc<Mutex<u32>>;

/// A fresh cell at the PAC's reset (`lcd_clock` resets to 0: no source).
pub fn clock_cell() -> ClockCell {
    Arc::new(Mutex::new(0))
}

/// `lcd_clock`, decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockSetup {
    pub sel: u32,
    pub div_num: u32,
    pub div_a: u32,
    pub div_b: u32,
    pub clkcnt_n: u32,
    pub equ_sysclk: bool,
}

impl ClockSetup {
    pub fn decode(word: u32) -> Self {
        Self {
            clkcnt_n: word & 0x3f,
            equ_sysclk: word & (1 << 6) != 0,
            div_num: (word >> 9) & 0xff,
            div_b: (word >> 17) & 0x3f,
            div_a: (word >> 23) & 0x3f,
            sel: (word >> 29) & 0x3,
        }
    }

    /// The source clock, or `None` for `sel = 0` (disabled).
    pub fn source_hz(&self) -> Option<u64> {
        match self.sel {
            1 => Some(40_000_000),
            2 => Some(240_000_000),
            3 => Some(160_000_000),
            _ => None,
        }
    }

    /// One PCLK period in CPU cycles, as an exact fraction `(num, den)`.
    pub fn period(&self) -> Option<(u64, u64)> {
        let src = self.source_hz()?;
        let div_num = if self.div_num == 0 {
            256
        } else {
            u64::from(self.div_num)
        };
        let (a, b) = if self.div_a == 0 {
            (1, 0)
        } else {
            (u64::from(self.div_a), u64::from(self.div_b))
        };
        let cnt = if self.equ_sysclk {
            1
        } else {
            u64::from(self.clkcnt_n) + 1
        };
        // cycles = CPU_HZ * cnt * (div_num + b/a) / src
        Some((memmap::CPU_HZ * cnt * (div_num * a + b), src * a))
    }

    /// PCLK in Hz, for a report.
    pub fn pclk_hz(&self) -> Option<f64> {
        let (num, den) = self.period()?;
        Some(memmap::CPU_HZ as f64 * den as f64 / num as f64)
    }
}

/// The `lcd_clock` window at `+0x00`.
#[derive(Debug)]
pub struct LcdClock {
    cell: ClockCell,
}

impl LcdClock {
    pub fn new(cell: ClockCell) -> Self {
        Self { cell }
    }
}

impl Peripheral for LcdClock {
    fn name(&self) -> &'static str {
        "LCD_CAM_CLOCK"
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        lane_of(*self.cell.lock().expect("lcd clock poisoned"), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, _cx: &mut BusCx<'_>) {
        let mut cell = self.cell.lock().expect("lcd clock poisoned");
        *cell = merge_lane(*cell, off, width, value);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::LCD_CAM.name(LCD_CLOCK + (off & !3))
    }

    fn reg_grade(&self, _off: u32) -> Option<RegGrade> {
        Some(RegGrade::Documented)
    }

    fn save_state(&self) -> Vec<u8> {
        self.cell
            .lock()
            .expect("lcd clock poisoned")
            .to_le_bytes()
            .to_vec()
    }

    fn load_state(&mut self, bytes: &[u8]) {
        if let Some(b) = bytes.get(..4) {
            *self.cell.lock().expect("lcd clock poisoned") =
                u32::from_le_bytes(b.try_into().expect("4 bytes"));
        }
    }
}

/// A frame being shifted out.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    stream: Stream,
    /// Every slot's value, setup and hold included: 8 or 16 lanes wide.
    slots: Vec<u16>,
    lanes: usize,
    t0: Cycles,
    num: u64,
    den: u64,
    /// The next slot to drive.
    next: usize,
}

impl Run {
    fn at(&self, k: usize) -> Cycles {
        self.t0 + (k as u64 * self.num) / self.den
    }
}

/// The LCD window from `+0x10`.
#[derive(Debug)]
pub struct Lcd {
    regs: RegFile,
    clock: ClockCell,
    gdma: GdmaHandle,
    index: usize,
    /// `lcd_start` is set and no stream has arrived yet.
    waiting: bool,
    run: Option<Run>,
    /// The level last driven on each `LCD_DATA_n`.
    levels: [bool; 16],
    /// Frames shifted out, for a report.
    frames: u64,
    warned_rgb: bool,
    warned_next_frame: bool,
}

impl Lcd {
    pub fn new(clock: ClockCell, gdma: GdmaHandle) -> Self {
        Self {
            regs: RegFile::new("LCD_CAM", LEN)
                .with_names(regs::LCD_CAM)
                .with_pac_grades(),
            clock,
            gdma,
            index: 0,
            waiting: false,
            run: None,
            levels: [false; 16],
            frames: 0,
            warned_rgb: false,
            warned_next_frame: false,
        }
    }

    /// Frames this view has shifted out completely.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The machine loaded a GDMA stream: start it if `lcd_start` is waiting.
    pub fn data_ready(&mut self, cx: &mut BusCx<'_>) {
        if self.waiting {
            self.begin(cx);
        }
    }

    fn user(&self) -> u32 {
        self.regs.stored(LCD_USER)
    }

    fn begin(&mut self, cx: &mut BusCx<'_>) {
        let user = self.user();
        let misc = self.regs.stored(LCD_MISC);
        if self.regs.stored(LCD_CTRL) & LCD_RGB_MODE_EN != 0 {
            if !self.warned_rgb {
                self.warned_rgb = true;
                log::warn!("LCD_CAM: RGB mode is not modelled; lcd_start drives nothing");
            }
            return;
        }
        if misc & LCD_NEXT_FRAME_EN != 0 && !self.warned_next_frame {
            self.warned_next_frame = true;
            log::warn!("LCD_CAM: lcd_next_frame_en is not modelled; one frame is sent");
        }
        let Some(stream) = self
            .gdma
            .lock()
            .expect("gdma poisoned")
            .take_stream(PERI_LCD_CAM)
        else {
            self.waiting = true;
            return;
        };
        self.waiting = false;
        let setup = ClockSetup::decode(*self.clock.lock().expect("lcd clock poisoned"));
        let Some((num, den)) = setup.period() else {
            log::warn!(
                "LCD_CAM: lcd_start with lcd_clk_sel = 0 (no clock): the frame is dropped and \
                 the transfer ends at once"
            );
            self.finish(stream, cx);
            return;
        };
        let wide = user & LCD_2BYTE_EN != 0;
        let lanes = if wide { 16 } else { 8 };
        let slots = Self::slots(&stream.bytes, user, misc, self.regs.stored(LCD_CMD_VAL));
        let run = Run {
            stream,
            slots,
            lanes,
            t0: cx.now,
            num,
            den,
            next: 0,
        };
        cx.sched.schedule_at(cx.now, event_id(self.index, EV_CHUNK));
        self.run = Some(run);
    }

    /// Every slot of one frame: setup, command, dummy, data, hold.
    fn slots(bytes: &[u8], user: u32, misc: u32, cmd: u32) -> Vec<u16> {
        let wide = user & LCD_2BYTE_EN != 0;
        let blank = misc & LCD_BK_EN != 0;
        let mut out = Vec::new();
        if blank {
            let setup = ((misc >> LCD_VFK_SHIFT) & 0x3f) + 1;
            out.extend(std::iter::repeat_n(0, setup as usize));
        }
        if user & LCD_CMD != 0 {
            out.push((cmd & 0xffff) as u16);
            if user & LCD_CMD_2_CYCLE_EN != 0 {
                out.push((cmd >> 16) as u16);
            }
        }
        if user & LCD_DUMMY != 0 {
            let dummy = ((user >> LCD_DUMMY_CYCLELEN_SHIFT) & 0x3) + 1;
            out.extend(std::iter::repeat_n(0, dummy as usize));
        }
        if user & LCD_DOUT != 0 {
            let mut data: Vec<u8> = bytes.to_vec();
            if !wide && user & LCD_8BITS_ORDER != 0 {
                for pair in data.chunks_exact_mut(2) {
                    pair.swap(0, 1);
                }
            }
            let mut values: Vec<u16> = if wide {
                data.chunks(2)
                    .map(|p| {
                        let (lo, hi) = (p[0], p.get(1).copied().unwrap_or(0));
                        if user & LCD_BYTE_ORDER != 0 {
                            u16::from_le_bytes([hi, lo])
                        } else {
                            u16::from_le_bytes([lo, hi])
                        }
                    })
                    .collect()
            } else {
                data.iter().map(|&b| u16::from(b)).collect()
            };
            if user & LCD_BIT_ORDER != 0 {
                for v in &mut values {
                    *v = if wide {
                        v.reverse_bits()
                    } else {
                        u16::from((*v as u8).reverse_bits())
                    };
                }
            }
            if user & LCD_ALWAYS_OUT_EN == 0 {
                let cycles = (user & LCD_DOUT_CYCLELEN) as usize + 1;
                values.truncate(cycles);
            }
            out.extend(values);
        }
        if blank {
            let hold = ((misc >> LCD_VBK_SHIFT) & 0x1fff) + 1;
            out.extend(std::iter::repeat_n(0, hold as usize));
        }
        out
    }

    fn emit_chunk(&mut self, cx: &mut BusCx<'_>) {
        let Some(run) = self.run.as_mut() else {
            return;
        };
        let end = (run.next + CHUNK).min(run.slots.len());
        for k in run.next..end {
            let t = run.at(k);
            let v = run.slots[k];
            for lane in 0..run.lanes {
                let level = v & (1 << lane) != 0;
                if level != self.levels[lane] {
                    self.levels[lane] = level;
                    cx.pins.drive(SignalId(LCD_DATA_0 + lane as u16), level, t);
                }
            }
        }
        run.next = end;
        let next_at = run.at(end);
        let tag = if end < run.slots.len() {
            EV_CHUNK
        } else {
            EV_DONE
        };
        cx.sched
            .schedule_at(next_at.max(cx.now), event_id(self.index, tag));
    }

    fn finish(&mut self, stream: Stream, cx: &mut BusCx<'_>) {
        let user = self.user();
        self.regs.poke(LCD_USER, user & !LCD_START);
        let raw = self.regs.stored(LC_DMA_INT_RAW) | LCD_TRANS_DONE;
        self.regs.poke(LC_DMA_INT_RAW, raw);
        self.sync_irq(cx);
        self.gdma
            .lock()
            .expect("gdma poisoned")
            .complete(&stream, cx);
        self.frames += 1;
    }

    /// Stop on the spot (`lcd_reset`, or `lcd_start` cleared by the CPU).
    fn abort(&mut self, cx: &mut BusCx<'_>) {
        self.waiting = false;
        if self.run.take().is_some() {
            cx.sched.cancel(event_id(self.index, EV_CHUNK));
            cx.sched.cancel(event_id(self.index, EV_DONE));
        }
    }

    fn sync_irq(&self, cx: &mut BusCx<'_>) {
        let st = self.regs.stored(LC_DMA_INT_RAW) & self.regs.stored(LC_DMA_INT_ENA) & LC_INT_MASK;
        cx.irq.set_level(regs::source::LCD_CAM, st != 0);
    }
}

impl Peripheral for Lcd {
    fn name(&self) -> &'static str {
        "LCD_CAM"
    }

    fn attached(&mut self, index: usize) {
        self.index = index;
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let block = off + LCD_OFF;
        match block & !3 {
            LC_DMA_INT_ST => lane_of(
                self.regs.stored(LC_DMA_INT_RAW) & self.regs.stored(LC_DMA_INT_ENA) & LC_INT_MASK,
                block,
                width,
            ),
            LC_DMA_INT_CLR => 0,
            _ => self.regs.read(block, width, cx),
        }
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let block = off + LCD_OFF;
        match block & !3 {
            LCD_USER => {
                let old = self.user();
                let word = merge_lane(old, block, width, value);
                // `lcd_update` and `lcd_reset` are strobes the hardware
                // clears; `lcd_start` stays set until the frame is out.
                self.regs.poke(LCD_USER, word & !(LCD_UPDATE | LCD_RESET));
                if word & LCD_RESET != 0 {
                    self.abort(cx);
                    self.regs.poke(LCD_USER, self.user() & !LCD_START);
                    return;
                }
                let starting = word & LCD_START != 0 && old & LCD_START == 0;
                let stopping = word & LCD_START == 0 && old & LCD_START != 0;
                if stopping {
                    self.abort(cx);
                } else if starting {
                    self.begin(cx);
                }
            }
            LCD_MISC => {
                let word = merge_lane(self.regs.stored(LCD_MISC), block, width, value);
                self.regs.poke(LCD_MISC, word & !LCD_AFIFO_RESET);
            }
            LC_DMA_INT_ENA => {
                let word = merge_lane(self.regs.stored(LC_DMA_INT_ENA), block, width, value);
                self.regs.poke(LC_DMA_INT_ENA, word & LC_INT_MASK);
                self.sync_irq(cx);
            }
            LC_DMA_INT_CLR => {
                let clear = merge_lane(0, block, width, value);
                let raw = self.regs.stored(LC_DMA_INT_RAW) & !clear;
                self.regs.poke(LC_DMA_INT_RAW, raw);
                self.sync_irq(cx);
            }
            // The hardware's.
            LC_DMA_INT_RAW | LC_DMA_INT_ST => {}
            _ => self.regs.write(block, width, value, cx),
        }
    }

    fn on_event(&mut self, id: lp_emu_core::EventId, cx: &mut BusCx<'_>) {
        match event_local(id) {
            EV_CHUNK => self.emit_chunk(cx),
            EV_DONE => {
                if let Some(run) = self.run.take() {
                    self.finish(run.stream, cx);
                }
            }
            _ => {}
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::LCD_CAM.name((off + LCD_OFF) & !3)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        match (off + LCD_OFF) & !3 {
            LCD_USER | LCD_MISC | LCD_CTRL | LCD_CMD_VAL | LC_DMA_INT_ENA | LC_DMA_INT_RAW
            | LC_DMA_INT_ST | LC_DMA_INT_CLR => Some(RegGrade::Documented),
            block => self.regs.reg_grade(block),
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }

    fn save_state(&self) -> Vec<u8> {
        // A snapshot is taken between windows; a frame in flight is not
        // carried (no path this machine runs snapshots mid-frame).
        let mut out = Vec::new();
        out.extend_from_slice(&self.frames.to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let Some(head) = bytes.get(..8) else {
            log::warn!("LCD_CAM: load_state blob too short, ignored");
            return;
        };
        self.frames = u64::from_le_bytes(head.try_into().expect("8 bytes"));
        self.run = None;
        self.waiting = false;
        self.levels = [false; 16];
        self.regs.load_state(&bytes[8..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::periph::gdma::{self, Chain, Gdma, StartRequest};
    use lp_emu_esp_common::Sandbox;
    use lp_emu_esp_common::pins::{PadId, RouteSource};

    /// What esp-hal writes for 2.5 MHz from XTAL: LCD_CLK = 5 MHz (div 8),
    /// PCLK = LCD_CLK / 2.
    fn clock_2_5mhz() -> u32 {
        (1 << 31) | (1 << 29) | (8 << 9) | 1
    }

    #[test]
    fn the_clock_decodes_to_esp_hals_pclk() {
        let s = ClockSetup::decode(clock_2_5mhz());
        assert_eq!(s.sel, 1);
        assert_eq!(s.div_num, 8);
        assert_eq!(s.clkcnt_n, 1);
        // 240 MHz / 2.5 MHz = 96 CPU cycles per slot, exactly.
        let (num, den) = s.period().unwrap();
        assert_eq!(num / den, 96);
        assert_eq!(num % den, 0);
        assert!((s.pclk_hz().unwrap() - 2_500_000.0).abs() < 1.0);
        assert_eq!(ClockSetup::decode(0).period(), None, "sel 0: no clock");
    }

    #[test]
    fn slots_follow_the_user_bits() {
        // One setup and one hold cycle, 8-bit, always-out.
        let misc = LCD_BK_EN;
        let user = LCD_DOUT | LCD_ALWAYS_OUT_EN;
        assert_eq!(
            Lcd::slots(&[0xA5, 0x0F], user, misc, 0),
            vec![0, 0xA5, 0x0F, 0]
        );
        // Bit order reversed.
        assert_eq!(Lcd::slots(&[0x01], user | LCD_BIT_ORDER, 0, 0), vec![0x80]);
        // 16-bit, little-endian pairs.
        assert_eq!(
            Lcd::slots(&[0x34, 0x12], user | LCD_2BYTE_EN, 0, 0),
            vec![0x1234]
        );
        // Without always-out, DOUT cycles = cyclelen + 1.
        assert_eq!(Lcd::slots(&[1, 2, 3], LCD_DOUT | 1, 0, 0), vec![1, 2]);
    }

    #[test]
    fn a_start_plays_the_stream_onto_the_pads_and_ends_the_transfer() {
        let mut sb = Sandbox::new();
        let gdma_state = gdma::handle();
        let cell = clock_cell();
        let mut clock = LcdClock::new(cell.clone());
        let mut lcd = Lcd::new(cell, gdma_state.clone());
        let mut g = Gdma::new(gdma_state.clone());
        lcd.attached(0);
        // Lane 0 → pad 1, lane 2 → pad 3.
        sb.pins.route(
            PadId(1),
            RouteSource::Signal(SignalId(LCD_DATA_0), false),
            false,
            0,
        );
        sb.pins.route(
            PadId(3),
            RouteSource::Signal(SignalId(LCD_DATA_0 + 2), false),
            false,
            0,
        );
        // esp-hal enables the pad's output when it connects the signal.
        sb.pins.set_gpio_enable(PadId(1), true, 0);
        sb.pins.set_gpio_enable(PadId(3), true, 0);
        sb.write(&mut clock, 0, clock_2_5mhz());
        sb.write(&mut lcd, LCD_USER - LCD_OFF, LCD_DOUT | LCD_ALWAYS_OUT_EN);
        // GDMA first (as esp-hal orders it), then the LCD start.
        sb.write(&mut g, gdma::OUT_PERI_SEL, PERI_LCD_CAM as u32);
        sb.write(&mut g, gdma::OUT_LINK, gdma::OUTLINK_START);
        let req: StartRequest = g.take_starts()[0];
        let mut cx = sb.cx();
        g.loaded(
            &req,
            Ok(Chain {
                bytes: vec![0b101, 0b001, 0b100, 0],
                last: 0x3FC9_0000,
                before_last: 0,
                descriptors: 1,
            }),
            &mut cx,
        );
        drop(cx);
        sb.write(&mut lcd, LC_DMA_INT_ENA - LCD_OFF, LCD_TRANS_DONE);
        sb.write(
            &mut lcd,
            LCD_USER - LCD_OFF,
            LCD_DOUT | LCD_ALWAYS_OUT_EN | LCD_UPDATE | LCD_START,
        );
        assert_ne!(
            sb.read(&mut lcd, LCD_USER - LCD_OFF) & LCD_START,
            0,
            "running"
        );
        sb.run_to(&mut lcd, 10_000);
        assert_eq!(sb.read(&mut lcd, LCD_USER - LCD_OFF) & LCD_START, 0, "done");
        assert_ne!(
            sb.read(&mut lcd, LC_DMA_INT_RAW - LCD_OFF) & LCD_TRANS_DONE,
            0
        );
        assert!(sb.irq.level(regs::source::LCD_CAM));
        assert_eq!(lcd.frames(), 1);
        // Pad 1 (bit 0): high for slots 0 and 1, low at slot 2.
        // Pad 3 (bit 2): high at slot 0, low at 1, high at 2, low at 3.
        let edges = sb.pins.take_edges();
        let on = |pad: u8| -> Vec<(u64, bool)> {
            edges
                .iter()
                .filter(|e| e.pad == PadId(pad))
                .map(|e| (e.at, e.level))
                .collect()
        };
        assert_eq!(on(1), vec![(0, true), (192, false)]);
        assert_eq!(
            on(3),
            vec![(0, true), (96, false), (192, true), (288, false)]
        );
        // GDMA's side of the end.
        let raw = sb.read(&mut g, gdma::OUT_INT_RAW);
        assert_eq!(raw & gdma::INT_OUT_TOTAL_EOF, gdma::INT_OUT_TOTAL_EOF);
    }

    #[test]
    fn a_start_before_the_stream_waits_for_it() {
        let mut sb = Sandbox::new();
        let gdma_state = gdma::handle();
        let cell = clock_cell();
        *cell.lock().unwrap() = clock_2_5mhz();
        let mut lcd = Lcd::new(cell, gdma_state.clone());
        let mut g = Gdma::new(gdma_state);
        lcd.attached(0);
        sb.write(
            &mut lcd,
            LCD_USER - LCD_OFF,
            LCD_DOUT | LCD_ALWAYS_OUT_EN | LCD_START,
        );
        sb.write(&mut g, gdma::OUT_PERI_SEL, PERI_LCD_CAM as u32);
        sb.write(&mut g, gdma::OUT_LINK, gdma::OUTLINK_START);
        let req = g.take_starts()[0];
        let mut cx = sb.cx();
        g.loaded(
            &req,
            Ok(Chain {
                bytes: vec![1; 8],
                last: 0,
                before_last: 0,
                descriptors: 1,
            }),
            &mut cx,
        );
        lcd.data_ready(&mut cx);
        drop(cx);
        sb.run_to(&mut lcd, 100_000);
        assert_eq!(lcd.frames(), 1);
        assert_eq!(sb.read(&mut lcd, LCD_USER - LCD_OFF) & LCD_START, 0);
    }
}
