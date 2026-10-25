//! The product image's own frame check (feature `ws281x_frame_check`, never
//! shipped): LCD_CAM's CAM half samples every lane's pad while the LCD half
//! sends, and each frame is decoded against the bytes that went in. This is
//! how a walk states a truncation rate for this backend the way the RMT
//! states guard trips.
//!
//! The sampler runs at 4 MHz on its own clock, looped out of GPIO11 and back
//! in as its pixel clock; VSYNC is GPIO12, toggled here. A lane's CAM input is
//! bound to the lane's pad when the lane opens. The sampler's start (the VSYNC
//! edge) and the LCD's start sit inside one short critical section, so radio
//! work cannot move one against the other and shorten the capture window.
//!
//! A frame counts as **failed** when any lane's decode differs from what was
//! sent: a missing or extra bit, a byte that differs, a high time outside
//! its window (250–500 ns for a 0, 750–1000 ns for a 1, at 250 ns a sample),
//! or a stretched period, which is what an underrun would look like. A frame the check could not arm (no buffer, a capture still busy)
//! counts as **skipped**, never as passed.
//!
//! Costs, all test-only: GDMA channel 1, two back pads, a capture buffer of
//! ~1.6 B per bus byte from internal RAM, and a decode per frame on the
//! render thread.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use core::alloc::Layout;

use esp_alloc::MemoryCapability;
use esp_hal::dma::{DmaDescriptor, DmaRxBuf};
use esp_hal::gpio::{AnyPin, Flex, Level};
use esp_hal::lcd_cam::cam::{Cam, Camera, CameraTransfer, Config, EofMode};
use esp_hal::time::{Instant, Rate};
use lp_ws281x::ColorOrder;
use lp_ws281x::parallel::{self, LANES};

/// 4 MHz: 250 ns a sample, 1.6 samples a 400 ns slot. Coarse on purpose:
/// at 8 MHz a 2 × 310-LED frame's capture (76 KB) does not fit internal RAM
/// beside the frame buffers on the radio image (FS3, 2026-10-23); at 4 MHz a
/// 0's high is 1–2 samples and a 1's 3–4, which still decodes every bit.
const SAMPLE_HZ: u32 = 4_000_000;
/// Samples a bus byte lasts, ×10.
const SAMPLES_PER_BUS_BYTE_X10: usize = 16;
/// Margin past the frame, in samples (the latch is inside the frame).
const MARGIN: usize = 1_000;
/// A high longer than 2 samples (500 ns) is a 1.
const THRESHOLD: u32 = 2;
/// A low of 200 samples (50 µs) ends a lane's frame.
const IDLE: u32 = 200;
/// Measured widths, in samples, that pass: a 0's high 1–2 (250–500 ns), a
/// 1's 3–4 (750–1000 ns), a period 4–5 (1000–1250 ns).
const T0H: (u32, u32) = (1, 2);
const T1H: (u32, u32) = (3, 4);
const PERIOD: (u32, u32) = (4, 5);
/// The lead between the VSYNC edge and the LCD start.
const LEAD_US: u64 = 20;
/// Checked frames between log lines.
const LOG_EVERY: u32 = 250;

struct Job {
    transfer: CameraTransfer<'static, DmaRxBuf>,
    rx_ptr: *const u8,
    len: usize,
    mask: u8,
    expected: [Vec<u8>; LANES],
}

pub struct FrameCheck {
    cam: Option<Camera<'static>>,
    rx: Option<DmaRxBuf>,
    rx_cap: usize,
    vsync: Flex<'static>,
    job: Option<Job>,
    pub checked: u32,
    pub failed: u32,
    pub skipped: u32,
}

impl FrameCheck {
    pub fn new(
        cam: Cam<'static>,
        dma: esp_hal::peripherals::DMA_CH1<'static>,
        clk: AnyPin<'static>,
        vsync: AnyPin<'static>,
    ) -> Option<Self> {
        let (clk_in, clk_out) = Flex::new(clk).split();
        let mut vsync = Flex::new(vsync);
        vsync.apply_output_config(&esp_hal::gpio::OutputConfig::default());
        vsync.set_output_enable(true);
        vsync.set_input_enable(true);
        vsync.set_low();
        let vsync_in = vsync.peripheral_input();
        let config = Config::default()
            .with_frequency(Rate::from_hz(SAMPLE_HZ))
            .with_eof_mode(EofMode::VsyncSignal);
        let cam = match Camera::new(cam, dma, config) {
            Ok(cam) => cam,
            Err(error) => {
                log::warn!("[ws281x-check] camera init failed: {error:?}; no frame check");
                return None;
            }
        }
        .with_master_clock(clk_out)
        .with_pixel_clock(clk_in)
        .with_vsync(vsync_in)
        .with_hsync(Level::High)
        .with_h_enable(Level::High);
        log::info!("[ws281x-check] armed: CAM sampling the lane pads at {SAMPLE_HZ} Hz");
        Some(Self {
            cam: Some(cam),
            rx: None,
            rx_cap: 0,
            vsync,
            job: None,
            checked: 0,
            failed: 0,
            skipped: 0,
        })
    }

    /// Point the sampler's input `lane` at the lane's pad.
    pub fn bind(&mut self, lane: u8, gpio: u8) {
        let Some(cam) = self.cam.take() else { return };
        // SAFETY: the lane's lease holds this pad; the sampler only reads it.
        let pin = unsafe { AnyPin::steal(gpio) };
        self.cam = Some(match lane {
            0 => cam.with_data0(pin),
            1 => cam.with_data1(pin),
            2 => cam.with_data2(pin),
            3 => cam.with_data3(pin),
            4 => cam.with_data4(pin),
            5 => cam.with_data5(pin),
            6 => cam.with_data6(pin),
            _ => cam.with_data7(pin),
        });
    }

    /// Prepare a capture of a `bus_len`-byte frame carrying `lanes`; `false`
    /// when this frame goes unchecked. The capture starts at [`Self::trigger`].
    pub fn arm(&mut self, lanes: &[&[u8]; LANES], mask: u8, bus_len: usize) -> bool {
        if self.job.is_some() {
            self.skipped += 1;
            return false;
        }
        let len = bus_len * SAMPLES_PER_BUS_BYTE_X10 / 10 + MARGIN;
        if len > self.rx_cap && !self.grow(len) {
            self.skipped += 1;
            return false;
        }
        let (Some(cam), Some(mut rx)) = (self.cam.take(), self.rx.take()) else {
            self.skipped += 1;
            return false;
        };
        let mut expected: [Vec<u8>; LANES] = Default::default();
        for (index, slice) in lanes.iter().enumerate() {
            if mask & (1 << index) != 0 {
                expected[index] = wire_order(slice);
            }
        }
        rx.set_length(len);
        rx.as_mut_slice().fill(0xEE);
        let rx_ptr = rx.as_slice().as_ptr();
        self.vsync.set_low();
        match cam.receive(rx) {
            Ok(transfer) => {
                self.job = Some(Job {
                    transfer,
                    rx_ptr,
                    len,
                    mask,
                    expected,
                });
                true
            }
            Err((error, cam, rx)) => {
                log::warn!("[ws281x-check] receive failed: {error:?}");
                self.cam = Some(cam);
                self.rx = Some(rx);
                self.skipped += 1;
                false
            }
        }
    }

    /// The VSYNC edge that starts the capture, then the lead. Called inside
    /// the critical section that also starts the LCD.
    pub fn trigger(&mut self) {
        if self.job.is_none() {
            return;
        }
        self.vsync.set_high();
        let t = Instant::now();
        while t.elapsed().as_micros() < LEAD_US {}
    }

    /// After the LCD finished: wait for the capture, decode, count.
    pub fn finish(&mut self) {
        let Some(job) = self.job.take() else { return };
        let last = job.len - 1;
        // SAFETY: a volatile read of a byte the DMA writes; the buffer is
        // ours and `last` is inside the capture.
        let filled = || unsafe { core::ptr::read_volatile(job.rx_ptr.add(last)) != 0xEE };
        let t = Instant::now();
        while !filled() && t.elapsed().as_millis() < 60 {}
        let complete = filled();
        let (cam, rx) = job.transfer.stop();
        self.vsync.set_low();
        let mut ok = complete;
        if complete {
            let samples = &rx.as_slice()[..job.len];
            for (lane, expected) in job.expected.iter().enumerate() {
                if job.mask & (1 << lane) == 0 {
                    continue;
                }
                let mut got = vec![0u8; expected.len() + 1];
                match parallel::decode_lane(samples, lane as u8, THRESHOLD, IDLE, &mut got) {
                    Ok(d) => {
                        let within =
                            |m: (u32, u32), w: (u32, u32)| m.1 == 0 || (m.0 >= w.0 && m.1 <= w.1);
                        let widths =
                            within(d.t0h, T0H) && within(d.t1h, T1H) && within(d.period, PERIOD);
                        if d.bits != expected.len() * 8
                            || got[..expected.len()] != expected[..]
                            || !widths
                        {
                            ok = false;
                            if self.failed < 8 {
                                log::warn!(
                                    "[ws281x-check] lane {lane}: bits {} of {}, t0h {:?} t1h {:?} \
                                     period {:?} first_edge {}",
                                    d.bits,
                                    expected.len() * 8,
                                    d.t0h,
                                    d.t1h,
                                    d.period,
                                    d.first_edge
                                );
                            }
                        }
                    }
                    Err(error) => {
                        ok = false;
                        if self.failed < 8 {
                            log::warn!("[ws281x-check] lane {lane}: {error:?}");
                        }
                    }
                }
            }
        }
        self.cam = Some(cam);
        self.rx = Some(rx);
        if complete {
            self.checked += 1;
            if !ok {
                self.failed += 1;
            }
        } else {
            self.skipped += 1;
        }
        if complete && self.checked % LOG_EVERY == 0 {
            log::info!(
                "[ws281x-check] driver=lcd checked={} failed={} skipped={}",
                self.checked,
                self.failed,
                self.skipped
            );
        }
    }

    /// A capture buffer of `len` bytes from internal RAM, replacing the old.
    fn grow(&mut self, len: usize) -> bool {
        // The old buffer and descriptors are leaked: a test image grows this
        // once or twice per boot.
        let descriptors = len.div_ceil(4_000) + 1;
        // SAFETY: plain allocation requests, checked for null; the memory is
        // handed to the DMA buffer for the life of the image.
        let (data, desc) = unsafe {
            let data = esp_alloc::HEAP.alloc_caps(
                MemoryCapability::Internal.into(),
                Layout::from_size_align(len, 4).unwrap(),
            );
            let desc = esp_alloc::HEAP.alloc_caps(
                MemoryCapability::Internal.into(),
                Layout::array::<DmaDescriptor>(descriptors).unwrap(),
            );
            (data, desc.cast::<DmaDescriptor>())
        };
        if data.is_null() || desc.is_null() {
            log::warn!("[ws281x-check] no {len} B of internal RAM for the capture");
            return false;
        }
        // SAFETY: fresh, exclusively ours, sized and aligned for their types.
        let (data, desc) = unsafe {
            for i in 0..descriptors {
                desc.add(i).write(DmaDescriptor::EMPTY);
            }
            (
                core::slice::from_raw_parts_mut(data, len),
                core::slice::from_raw_parts_mut(desc, descriptors),
            )
        };
        match DmaRxBuf::new(desc, data) {
            Ok(rx) => {
                self.rx = Some(rx);
                self.rx_cap = len;
                true
            }
            Err(_) => false,
        }
    }
}

/// A lane's RGB frame as its wire bytes (GRB), what the decoder reads back.
fn wire_order(rgb: &[u8]) -> Vec<u8> {
    (0..rgb.len())
        .map(|i| {
            let slot = i % 3;
            rgb.get(i - slot + ColorOrder::Grb.source_index(slot))
                .copied()
                .unwrap_or(rgb[i])
        })
        .collect()
}
