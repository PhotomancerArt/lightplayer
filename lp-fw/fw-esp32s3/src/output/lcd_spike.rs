//! LCD_CAM parallel WS281x — the LED-channels spike's silicon proof.
//! **Spike code; never shipped.** Two users: the `test_lcd_ws281x` harness
//! (`tests/lcd_ws281x.rs`: the cases below, with the CPU masked or stalled in
//! a flash write) and the `lcd_ws281x_probe` app image (`lcd_probe.rs`: the
//! same check beside the server, Bluetooth and a Wi‑Fi scan loop).
//!
//! Up to eight WS281x lanes from one DMA stream: LCD_CAM's LCD half in
//! i8080 mode shifts one byte per bus clock onto eight pads, and the frame is
//! encoded into an internal-RAM buffer by [`lp_ws281x::parallel`]
//! (3 slots of 400 ns per WS281x bit). GDMA feeds the bus with no CPU
//! involvement, so there is no refill deadline at all.
//!
//! # The analyzer: the other half of the same peripheral
//!
//! LCD_CAM's CAM half samples a parallel bus into DMA too. Its master clock
//! (`CAM_CLK`, 8 MHz) goes out of a pad and back in as its own pixel clock,
//! VSYNC and DE are tied high inside the GPIO matrix, and its eight data
//! inputs read the same eight pads the LCD drives. So every frame is
//! captured by a free-running 125 ns sampler that also needs no CPU, and
//! [`lp_ws281x::parallel::decode_lane`] reads each lane back: bytes against
//! what was sent, pulse widths against the WS2812 window. A stall in the
//! output stream (an underrun) would show as a stretched period.
//!
//! Because neither side needs the CPU, the frame can be checked while the
//! CPU is held in a masked critical section, or stalled in a flash erase with
//! the cache off, which is what the stress cases do.
//!
//! # Pads (XIAO ESP32-S3 Plus)
//!
//! Lanes 0–7 on GPIO1, 2, 3, 4, 5, 6, 7, 10 (D0–D5, D8, and a back pad); the
//! sampler's clock loops through GPIO11. GPIO9 (D10, FS3's LED panel) and
//! GPIO8 (D9, the button harness's pin) are left alone. Nothing needs to be
//! attached.
//!
//! # Lines
//!
//! `LCD:` lines, one per case, `LCD: RESULT pass|fail` last (raw text in the
//! harness, log records in the app image).

#![cfg_attr(
    not(fw_harness),
    allow(dead_code, reason = "the app probe runs only `frame`")
)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_hal::Blocking;
use esp_hal::dma::{DmaDescriptor, DmaRxBuf, DmaTxBuf, RxChannelFor, TxChannelFor};
use esp_hal::gpio::{AnyPin, Flex, Level};
use esp_hal::lcd_cam::LcdCam;
use esp_hal::lcd_cam::cam::{Camera, Config as CamConfig, EofMode, VhdeMode};
use esp_hal::lcd_cam::lcd::i8080::{Command, Config as LcdConfig, I8080};
use esp_hal::peripherals::LCD_CAM;
use esp_hal::time::{Instant, Rate};
use lp_ws281x::parallel::{self, SlotPlan};

/// Raw text in the harness (no logger there), a log record in the app image
/// (raw text would tear the USB link's frames).
macro_rules! println {
    ($($arg:tt)*) => {{
        #[cfg(fw_harness)]
        esp_println::println!($($arg)*);
        #[cfg(not(fw_harness))]
        log::info!($($arg)*);
    }};
}

/// Bus clock: 2.5 MHz, so a slot is 400 ns and a WS281x bit 1.2 µs.
const BUS_HZ: u32 = 2_500_000;
const SLOT_NS: u32 = 1_000_000_000 / BUS_HZ;
/// The sampler: 8 MHz, 125 ns a sample.
const SAMPLE_HZ: u32 = 8_000_000;
const SAMPLE_NS: u32 = 1_000_000_000 / SAMPLE_HZ;
/// A high run longer than this many samples (625 ns) is a 1.
const THRESHOLD: u32 = 5;
/// A low run this long (50 µs) ends a frame in the decoder.
const IDLE: u32 = 400;
/// The latch, as zero bytes at the end of the buffer: 300 µs.
const LATCH_BYTES: usize = (300_000 / SLOT_NS) as usize;

/// Samples of margin past the encoded frame (whose own tail is the latch).
const CAPTURE_MARGIN: usize = 2_000;

/// The bus buffer for lanes of at most `max_lane_bytes` wire bytes.
pub const fn tx_bytes(max_lane_bytes: usize) -> usize {
    parallel::encoded_len(SlotPlan::THREE, max_lane_bytes, LATCH_BYTES)
}

/// The capture for a bus buffer of `bus_bytes` bytes.
const fn capture_len(bus_bytes: usize) -> usize {
    (bus_bytes * SLOT_NS as usize) / SAMPLE_NS as usize + CAPTURE_MARGIN
}

/// A scratch sector past the end of the partition table (`partitions.csv`
/// ends at 0x790000 of the 8 MB floor). Used only if it reads blank.
const SCRATCH_SECTOR: u32 = 0x7F_F000;

/// Window the decoded widths must fall in, in samples (asynchronous
/// sampling gives ±1 sample of 125 ns around the nominal).
const T0H_WINDOW: (u32, u32) = (3, 4); // 375–500 ns, nominal 400
const T1H_WINDOW: (u32, u32) = (6, 7); // 750–875 ns, nominal 800
const PERIOD_WINDOW: (u32, u32) = (9, 10); // 1125–1250 ns, nominal 1200

#[derive(Clone, Copy, PartialEq)]
pub enum Stress {
    /// The CPU waits for the frame, interrupts on.
    None,
    /// The CPU spins with interrupts masked for the whole frame.
    Masked,
    /// The CPU erases and programs a flash sector (cache off) during the frame.
    FlashWrite,
}

/// The eight lane pads, the sampler's clock loop pad and its VSYNC pad.
pub struct Pads {
    pub lanes: [AnyPin<'static>; 8],
    pub clk: AnyPin<'static>,
    pub vsync: AnyPin<'static>,
}

pub struct Rig {
    lcd: Option<I8080<'static, Blocking>>,
    cam: Option<Camera<'static>>,
    tx: Option<DmaTxBuf>,
    rx: Option<DmaRxBuf>,
    rx_cap: usize,
    max_lane_bytes: usize,
    flash: Option<esp_storage::FlashStorage<'static>>,
    vsync: Flex<'static>,
    /// The VSYNC level a capture starts from; the edge is to its opposite.
    vsync_idle_high: bool,
    cam_config: CamConfig,
    scratch_ok: bool,
    pub failures: u32,
}

pub struct FrameReport {
    pub encode_us: u64,
    pub frame_us: u64,
    pub bus_bytes: usize,
    pub ok: bool,
    /// Worst widths over the lanes, in samples.
    t0h: (u32, u32),
    t1h: (u32, u32),
    period: (u32, u32),
}

impl Rig {
    /// Build the LCD (eight lanes) and the CAM sampler over the same pads,
    /// with buffers for lanes of up to `max_lane_bytes` wire bytes.
    pub fn new(
        lcd_cam: LCD_CAM<'static>,
        tx_dma: impl TxChannelFor<LCD_CAM<'static>>,
        rx_dma: impl RxChannelFor<LCD_CAM<'static>>,
        pads: Pads,
        flash: Option<esp_storage::FlashStorage<'static>>,
        max_lane_bytes: usize,
    ) -> Rig {
        let lcd_cam = LcdCam::new(lcd_cam);
        let [p0, p1, p2, p3, p4, p5, p6, p7] = pads.lanes;
        let (in0, out0) = Flex::new(p0).split();
        let (in1, out1) = Flex::new(p1).split();
        let (in2, out2) = Flex::new(p2).split();
        let (in3, out3) = Flex::new(p3).split();
        let (in4, out4) = Flex::new(p4).split();
        let (in5, out5) = Flex::new(p5).split();
        let (in6, out6) = Flex::new(p6).split();
        let (in7, out7) = Flex::new(p7).split();
        let (clk_in, clk_out) = Flex::new(pads.clk).split();
        // VSYNC from a pad the rig toggles: the sampler starts a capture on
        // a VSYNC edge, not on a level.
        let mut vsync = Flex::new(pads.vsync);
        vsync.apply_output_config(&esp_hal::gpio::OutputConfig::default());
        vsync.set_output_enable(true);
        vsync.set_input_enable(true);
        vsync.set_low();
        let vsync_in = vsync.peripheral_input();

        let lcd_config = LcdConfig::default().with_frequency(Rate::from_hz(BUS_HZ));
        let lcd = match I8080::new(lcd_cam.lcd, tx_dma, lcd_config) {
            Ok(lcd) => lcd,
            Err(error) => fatal_dbg("lcd_new", &error),
        }
        .with_data0(out0)
        .with_data1(out1)
        .with_data2(out2)
        .with_data3(out3)
        .with_data4(out4)
        .with_data5(out5)
        .with_data6(out6)
        .with_data7(out7);

        let cam_config = CamConfig::default()
            .with_frequency(Rate::from_hz(SAMPLE_HZ))
            .with_eof_mode(EofMode::ByteLen(u16::MAX));
        let cam = match Camera::new(lcd_cam.cam, rx_dma, cam_config.clone()) {
            Ok(cam) => cam,
            Err(error) => fatal_dbg("cam_new", &error),
        }
        .with_master_clock(clk_out)
        .with_pixel_clock(clk_in)
        .with_vsync(vsync_in)
        .with_hsync(Level::High)
        .with_h_enable(Level::High)
        .with_data0(in0)
        .with_data1(in1)
        .with_data2(in2)
        .with_data3(in3)
        .with_data4(in4)
        .with_data5(in5)
        .with_data6(in6)
        .with_data7(in7);

        let tx_cap = tx_bytes(max_lane_bytes);
        let rx_cap = capture_len(tx_cap);
        let tx = DmaTxBuf::new(leak_descriptors(tx_cap), leak_bytes(tx_cap))
            .unwrap_or_else(|error| fatal_dbg("tx_buf", &error));
        let rx = DmaRxBuf::new(leak_descriptors(rx_cap), leak_bytes(rx_cap))
            .unwrap_or_else(|error| fatal_dbg("rx_buf", &error));

        let mut flash = flash;
        let scratch_ok = flash.as_mut().is_some_and(scratch_is_blank);

        println!(
            "LCD: config bus_hz={BUS_HZ} slot_ns={SLOT_NS} sample_hz={SAMPLE_HZ} \
             tx_buffer={tx_cap} rx_buffer={rx_cap} latch_bytes={LATCH_BYTES} \
             heap_free={} scratch_sector=0x{SCRATCH_SECTOR:x} scratch_blank={scratch_ok}",
            esp_alloc::HEAP.free(),
        );

        let mut rig = Rig {
            lcd: Some(lcd),
            cam: Some(cam),
            tx: Some(tx),
            rx: Some(rx),
            rx_cap,
            max_lane_bytes,
            flash,
            vsync,
            vsync_idle_high: false,
            cam_config,
            scratch_ok,
            failures: 0,
        };
        rig.probe_cam();
        rig
    }
}

impl Rig {
    /// Which control mode and VSYNC edge make the sampler capture: try
    /// each on 4,000 bytes of idle bus and keep the first that fills.
    pub fn probe_cam(&mut self) {
        let modes = [
            (VhdeMode::De, false),
            (VhdeMode::De, true),
            (VhdeMode::VsyncHsync, false),
            (VhdeMode::VsyncHsync, true),
        ];
        for (mode, idle_high) in modes {
            let mut cam = self.cam.take().unwrap();
            self.cam_config = self.cam_config.with_vh_de_mode(mode);
            if let Err(error) = cam.apply_config(&self.cam_config) {
                fatal_dbg("probe_config", &error);
            }
            self.vsync_idle_high = idle_high;
            self.set_vsync(false);
            let mut rx = self.rx.take().unwrap();
            rx.set_length(4_000);
            rx.as_mut_slice().fill(0xEE);
            let transfer = match cam.receive(rx) {
                Ok(transfer) => transfer,
                Err((error, _, _)) => fatal_dbg("probe_receive", &error),
            };
            spin_us(5);
            self.set_vsync(true);
            let (toggles, ctrl, ctrl1) = pad_diag();
            let t = Instant::now();
            while !transfer.is_done() && t.elapsed().as_millis() < 50 {}
            let done = transfer.is_done();
            let (cam, rx) = transfer.stop();
            let changed = rx.as_slice().iter().filter(|&&b| b != 0xEE).count();
            println!(
                "LCD: probe mode={mode:?} vsync_idle_high={idle_high} done={done} \
                 bytes_changed={changed} clk_pad_toggles={toggles} cam_ctrl=0x{ctrl:08x} \
                 cam_ctrl1=0x{ctrl1:08x} first={:02x?}",
                &rx.as_slice()[..8]
            );
            self.set_vsync(false);
            self.cam = Some(cam);
            self.rx = Some(rx);
            if changed > 0 {
                return;
            }
        }
        fatal_dbg("probe", &"no mode captured");
    }

    /// Drive VSYNC to its idle level (`active = false`) or away from it.
    fn set_vsync(&mut self, active: bool) {
        if active != self.vsync_idle_high {
            self.vsync.set_high();
        } else {
            self.vsync.set_low();
        }
    }

    /// Every harness case once, `LCD: RESULT` last.
    pub fn run_all(&mut self) {
        let rig = self;
        // The skirt: five lanes, ragged lengths (the short lanes pad).
        let skirt = [124, 124, 100, 124, 60];
        rig.case("five_lanes", &skirt, 0, Stress::None);
        rig.case("eight_lanes_124", &[124; 8], 1, Stress::None);
        rig.case("five_lanes_masked", &skirt, 2, Stress::Masked);
        if rig.scratch_ok {
            for n in 0..5 {
                rig.case("five_lanes_flash_write", &skirt, 3 + n, Stress::FlashWrite);
            }
        } else {
            println!("LCD: case=five_lanes_flash_write skipped=scratch_sector_not_blank");
        }
        if rig.max_lane_bytes >= 1_500 {
            rig.case("eight_lanes_500", &[500; 8], 9, Stress::None);
            rig.case("eight_lanes_500_masked", &[500; 8], 10, Stress::Masked);
        }

        // The soak: every frame decoded, content changing every frame.
        let started = Instant::now();
        let mut soak_fail = 0u32;
        let mut worst_us = 0u64;
        const SOAK: u32 = 1_000;
        for frame in 0..SOAK {
            let report = rig.frame(&skirt, 100 + frame, Stress::None);
            if !report.ok {
                soak_fail += 1;
            }
            worst_us = worst_us.max(report.frame_us);
        }
        println!(
            "LCD: case=soak_five_lanes frames={SOAK} failed={soak_fail} worst_frame_us={worst_us} \
         elapsed_ms={}",
            started.elapsed().as_millis()
        );
        rig.failures += soak_fail;

        println!(
            "LCD: RESULT {} failures={}",
            if rig.failures == 0 { "pass" } else { "fail" },
            rig.failures
        );
    }

    fn case(&mut self, name: &str, leds: &[usize], seed: u32, stress: Stress) {
        let r = self.frame(leds, seed, stress);
        if !r.ok {
            self.failures += 1;
        }
        let ns = |s: u32| s * SAMPLE_NS;
        println!(
            "LCD: case={name} lanes={} leds={:?} stress={} ok={} bus_bytes={} frame_us={} \
             expected_frame_us={} encode_us={} t0h_ns={}..{} t1h_ns={}..{} period_ns={}..{}",
            leds.len(),
            leds,
            match stress {
                Stress::None => "none",
                Stress::Masked => "masked",
                Stress::FlashWrite => "flash_write",
            },
            r.ok,
            r.bus_bytes,
            r.frame_us,
            r.bus_bytes as u64 * SLOT_NS as u64 / 1_000,
            r.encode_us,
            ns(r.t0h.0),
            ns(r.t0h.1),
            ns(r.t1h.0),
            ns(r.t1h.1),
            ns(r.period.0),
            ns(r.period.1),
        );
    }

    /// Encode, send and capture one frame of `leds[i]` LEDs on lane `i`, and
    /// check every lane.
    pub fn frame(&mut self, leds: &[usize], seed: u32, stress: Stress) -> FrameReport {
        let lanes: Vec<Vec<u8>> = leds
            .iter()
            .enumerate()
            .map(|(lane, &n)| pattern(lane, n, seed))
            .collect();
        let refs: Vec<&[u8]> = lanes.iter().map(|lane| lane.as_slice()).collect();

        let mut tx = self.tx.take().unwrap();
        let t0 = Instant::now();
        let len = parallel::encode_frame(SlotPlan::THREE, &refs, LATCH_BYTES, tx.as_mut_slice());
        let encode_us = t0.elapsed().as_micros();
        tx.set_length(len);

        let mut rx = self.rx.take().unwrap();
        let capture = capture_len(len).min(self.rx_cap);
        rx.set_length(capture);
        rx.as_mut_slice().fill(0xEE);
        let rx_ptr = rx.as_slice().as_ptr();

        let cam = self.cam.take().unwrap();
        self.set_vsync(false);
        let cam_transfer = match cam.receive(rx) {
            Ok(transfer) => transfer,
            Err((error, _, _)) => fatal_dbg("cam_receive", &error),
        };
        spin_us(5);
        self.set_vsync(true);
        // Let the sampler see some idle low before the first edge.
        let lead = Instant::now();
        while lead.elapsed().as_micros() < 50 {}

        let lcd = self.lcd.take().unwrap();
        let started = Instant::now();
        let lcd_transfer = match lcd.send(Command::<u8>::None, 0, tx) {
            Ok(transfer) => transfer,
            Err((error, _, _)) => fatal_dbg("lcd_send", &error),
        };
        match stress {
            Stress::None => {}
            Stress::Masked => critical_section::with(|_| while !lcd_transfer.is_done() {}),
            Stress::FlashWrite => {
                let block = [0x5Au8; 256];
                let flash = self.flash.as_mut().expect("flash stress without flash");
                let erase = flash.erase(SCRATCH_SECTOR, SCRATCH_SECTOR + 4096);
                let write = flash.write(SCRATCH_SECTOR, &block);
                if erase.is_err() || write.is_err() {
                    println!("LCD: flash_write erase={:?} write={:?}", erase, write);
                }
            }
        }
        let lcd_deadline = Instant::now();
        while !lcd_transfer.is_done() && lcd_deadline.elapsed().as_millis() < 200 {}
        let lcd_hung = !lcd_transfer.is_done();
        let (lcd_result, lcd, tx) = if lcd_hung {
            let (lcd, tx) = lcd_transfer.cancel();
            (Err(esp_hal::dma::DmaError::DescriptorError), lcd, tx)
        } else {
            lcd_transfer.wait()
        };
        let frame_us = started.elapsed().as_micros();
        // The sampler fills its buffer and keeps its start bit set (stop on a
        // full FIFO does not fire at the end of the descriptor chain), so the
        // capture is done when its last byte has been written: the bus is
        // idle (0x00) there, never the 0xEE sentinel.
        let cam_deadline = Instant::now();
        let last = capture - 1;
        // SAFETY: a volatile read of a byte the DMA writes; the buffer is
        // leaked and `last` is inside the capture.
        let filled = || unsafe { core::ptr::read_volatile(rx_ptr.add(last)) != 0xEE };
        while !filled() && cam_deadline.elapsed().as_millis() < 200 {}
        let cam_hung = !filled();
        let (cam, rx) = cam_transfer.stop();
        let cam_result: Result<(), esp_hal::dma::DmaError> = if cam_hung {
            Err(esp_hal::dma::DmaError::DescriptorError)
        } else {
            Ok(())
        };
        if lcd_hung || cam_hung {
            let written = rx.as_slice().iter().filter(|&&b| b != 0xEE).count();
            println!(
                "LCD: hung lcd={lcd_hung} cam={cam_hung} cam_bytes_changed={written} of {}",
                rx.as_slice().len()
            );
        }
        if lcd_result.is_err() || cam_result.is_err() {
            println!("LCD: dma lcd={:?} cam={:?}", lcd_result, cam_result);
        }

        let mut ok = lcd_result.is_ok() && cam_result.is_ok();
        let mut worst = FrameReport {
            encode_us,
            frame_us,
            bus_bytes: len,
            ok: false,
            t0h: (u32::MAX, 0),
            t1h: (u32::MAX, 0),
            period: (u32::MAX, 0),
        };
        let samples = rx.as_slice();
        let mut got = vec![0u8; self.max_lane_bytes + 1];
        for (lane, expected) in lanes.iter().enumerate() {
            match parallel::decode_lane(samples, lane as u8, THRESHOLD, IDLE, &mut got) {
                Ok(d) => {
                    let bytes_ok =
                        d.bits == expected.len() * 8 && got[..expected.len()] == **expected;
                    let widths_ok = within(d.t0h, T0H_WINDOW)
                        && within(d.t1h, T1H_WINDOW)
                        && (expected.len() < 2 || within(d.period, PERIOD_WINDOW));
                    if !(bytes_ok && widths_ok) {
                        ok = false;
                        let first_bad = got[..expected.len()]
                            .iter()
                            .zip(expected.iter())
                            .position(|(a, b)| a != b);
                        println!(
                            "LCD: lane={lane} bits={} want_bits={} first_bad_byte={first_bad:?} \
                             t0h={:?} t1h={:?} period={:?} first_edge={} tail_low={}",
                            d.bits,
                            expected.len() * 8,
                            d.t0h,
                            d.t1h,
                            d.period,
                            d.first_edge,
                            d.tail_low
                        );
                    }
                    worst.t0h = (worst.t0h.0.min(d.t0h.0), worst.t0h.1.max(d.t0h.1));
                    worst.t1h = (worst.t1h.0.min(d.t1h.0), worst.t1h.1.max(d.t1h.1));
                    worst.period = (
                        worst.period.0.min(d.period.0),
                        worst.period.1.max(d.period.1),
                    );
                }
                Err(error) => {
                    ok = false;
                    println!("LCD: lane={lane} decode_error={error:?}");
                }
            }
        }
        worst.ok = ok;

        self.lcd = Some(lcd);
        self.cam = Some(cam);
        self.tx = Some(tx);
        self.rx = Some(rx);
        worst
    }
}

/// Known-answer bytes, distinct per lane and per frame, every bit used.
fn pattern(lane: usize, leds: usize, seed: u32) -> Vec<u8> {
    (0..leds * 3)
        .map(|i| (i as u32 * 37 + lane as u32 * 11 + seed * 7) as u8 ^ (lane as u8).rotate_left(5))
        .collect()
}

/// Read GPIO_IN 4,000 times and count level changes on the sampler's clock
/// pad (GPIO11), and snapshot the CAM control registers.
fn pad_diag() -> (u32, u32, u32) {
    let gpio = esp_hal::peripherals::GPIO::regs();
    let mut last = gpio.in_().read().bits() & (1 << 11);
    let mut toggles = 0;
    for _ in 0..4_000 {
        let now = gpio.in_().read().bits() & (1 << 11);
        if now != last {
            toggles += 1;
        }
        last = now;
    }
    let cam = esp_hal::peripherals::LCD_CAM::regs();
    (
        toggles,
        cam.cam_ctrl().read().bits(),
        cam.cam_ctrl1().read().bits(),
    )
}

fn spin_us(us: u64) {
    let t = Instant::now();
    while t.elapsed().as_micros() < us {}
}

fn within(measured: (u32, u32), window: (u32, u32)) -> bool {
    measured.0 >= window.0 && measured.1 <= window.1
}

fn scratch_is_blank(flash: &mut esp_storage::FlashStorage<'static>) -> bool {
    let mut buf = [0u8; 256];
    for chunk in 0..16 {
        if flash.read(SCRATCH_SECTOR + chunk * 256, &mut buf).is_err() {
            return false;
        }
        // Blank, or what an earlier run of this harness wrote.
        if !(buf.iter().all(|&b| b == 0xFF) || (chunk == 0 && buf.iter().all(|&b| b == 0x5A))) {
            return false;
        }
    }
    true
}

/// A DMA buffer in internal RAM, word aligned, for the life of the harness.
fn leak_bytes(len: usize) -> &'static mut [u8] {
    let words: &'static mut [u32] = Box::leak(vec![0u32; len.div_ceil(4)].into_boxed_slice());
    // SAFETY: the allocation is `len.div_ceil(4) * 4 >= len` bytes, leaked,
    // and this is the only reference to it.
    unsafe { core::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), len) }
}

fn leak_descriptors(len: usize) -> &'static mut [DmaDescriptor] {
    Box::leak(vec![DmaDescriptor::EMPTY; len.div_ceil(4000) + 1].into_boxed_slice())
}

fn fatal_dbg(what: &str, error: &dyn core::fmt::Debug) -> ! {
    panic!("LCD: FATAL {what} {error:?}");
}
