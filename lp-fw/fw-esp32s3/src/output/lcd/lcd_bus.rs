//! The LCD_CAM bus behind every lane: two frame buffers in internal RAM, the
//! lanes' pending frames, and the one transfer in flight.
//!
//! # A frame's life
//!
//! 1. Each open lane's [`LcdBus::start`] records where its RGB bytes are.
//!    The caller keeps them alive until that lane's next
//!    [`LcdBus::wait_complete`] (`Ws281xOutput::start`'s contract).
//! 2. When every open lane has started, or a lane starts again, or a lane
//!    waits, the frame is **kicked**: all pending lanes are encoded in one pass
//!    ([`lp_ws281x::parallel::encode_frame`], GRB on the wire) into the buffer
//!    not in flight, the previous transfer is waited out, and GDMA starts
//!    feeding the bus.
//! 3. From there to the latch the CPU is not involved: no refill interrupt,
//!    no deadline. A lane that did not start this frame is held low, and its
//!    strip keeps showing what it last latched.
//!
//! # Where the buffers live
//!
//! Internal RAM, by capability (`MemoryCapability::Internal`), never by
//! default placement. Two places are ruled out:
//!
//! - **`dram2_seg`.** It carries no tag, so an `Internal` request can never
//!   land there. The bootloader takes that RAM back on a warm reset, and a DMA
//!   target must not be in it (the radio's rule, `crate::dram2_heap`).
//! - **PSRAM.** GDMA reads PSRAM through the cache, and the cache is off for
//!   every flash write, which is exactly the moment this backend exists to
//!   survive.
//!
//! The buffers are sized by the longest open lane, not by the per-port cap,
//! and grow on `open`/`resize`. When they cannot be had, the open fails with
//! a sentence saying how much was asked for and how much is free.

use alloc::format;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::ptr::NonNull;

use esp_hal::Blocking;
use esp_hal::dma::{DmaDescriptor, DmaTxBuf};
use esp_hal::lcd_cam::lcd::i8080::{Command, I8080, I8080Transfer};
use esp_hal::time::{Duration, Instant};
use esp_alloc::MemoryCapability;
use lp_ws281x::ColorOrder;
use lp_ws281x::parallel::{self, LANES, SlotPlan};
use lpc_hardware::{HardwareEndpointError, OutputError};

/// Bus clock: 2.5 MHz, so a slot is 400 ns and a WS281x bit (three slots)
/// 1.2 µs: T0H 400 ns, T1H 800 ns. Divides 160 MHz exactly.
pub const BUS_HZ: u32 = 2_500_000;
const SLOT_NS: usize = 1_000_000_000 / BUS_HZ as usize;
/// The latch, as low bytes at the end of every frame: 300 µs.
pub const LATCH_BYTES: usize = 300_000 / SLOT_NS;
/// A frame not done in this long has hung: cancel and report. The longest
/// frame the per-port cap allows (1,024 LEDs) is ~29.8 ms on this bus.
const FRAME_TIMEOUT: Duration = Duration::from_millis(50);
/// Bytes one GDMA descriptor carries (it can do 4,095).
const DESCRIPTOR_CHUNK: usize = 4_000;

/// One lane's state.
#[derive(Clone, Copy, Default)]
struct Lane {
    open: bool,
    /// Bytes per frame (a whole number of pixels).
    bytes: usize,
    /// The frame `start` recorded and no kick has encoded yet.
    pending: Option<(*const u8, usize)>,
}

enum Bus {
    Idle(I8080<'static, Blocking>),
    Sending {
        transfer: I8080Transfer<'static, DmaTxBuf, Blocking>,
        started: Instant,
        /// The lanes this transfer carries.
        lanes: u8,
    },
    /// A transfer could not be got back from (only on a start error).
    Gone,
}

pub struct LcdBus {
    bus: Bus,
    /// Buffers not in flight: two when idle, one while sending.
    free: Vec<FrameBuffer>,
    /// Bytes each buffer holds.
    capacity: usize,
    /// The sizes of the buffer inside the transfer in flight.
    in_flight_sizes: Option<(usize, usize)>,
    lanes: [Lane; LANES],
    pub stats: BusStats,
}

/// Counters since boot, for the log and the walk.
#[derive(Clone, Copy, Default, Debug)]
pub struct BusStats {
    pub frames: u32,
    pub timeouts: u32,
    pub encode_us_max: u64,
    pub encode_us_last: u64,
}

/// A frame buffer and its descriptors, both from internal RAM.
struct FrameBuffer {
    buf: DmaTxBuf,
    bytes: usize,
    descriptors: usize,
}

impl LcdBus {
    pub fn new(lcd: I8080<'static, Blocking>) -> Self {
        Self {
            bus: Bus::Idle(lcd),
            free: Vec::with_capacity(2),
            capacity: 0,
            in_flight_sizes: None,
            lanes: [Lane::default(); LANES],
            stats: BusStats::default(),
        }
    }

    /// Open `lane` for frames of `bytes` bytes, growing the buffers if it is
    /// now the longest. `bind` connects the lane's pad (it needs the bus idle,
    /// which it is when it runs).
    pub fn open_lane(
        &mut self,
        lane: u8,
        bytes: usize,
        bind: impl FnOnce(I8080<'static, Blocking>) -> I8080<'static, Blocking>,
    ) -> Result<(), HardwareEndpointError> {
        self.ensure_capacity(self.longest_open().max(bytes))
            .map_err(|reason| HardwareEndpointError::UnsupportedConfig { reason })?;
        self.wait_idle().map_err(|error| HardwareEndpointError::Other {
            message: format!("LCD bus did not go idle to bind a lane: {error}"),
        })?;
        match core::mem::replace(&mut self.bus, Bus::Gone) {
            Bus::Idle(lcd) => self.bus = Bus::Idle(bind(lcd)),
            other => {
                self.bus = other;
                return Err(HardwareEndpointError::Other {
                    message: "LCD bus is gone".into(),
                });
            }
        }
        self.lanes[lane as usize] = Lane {
            open: true,
            bytes,
            pending: None,
        };
        Ok(())
    }

    /// A lane's frame size changes.
    pub fn resize_lane(&mut self, lane: u8, bytes: usize) -> Result<(), OutputError> {
        let longest = self
            .lanes
            .iter()
            .enumerate()
            .filter(|(index, l)| l.open && *index != lane as usize)
            .map(|(_, l)| l.bytes)
            .max()
            .unwrap_or(0)
            .max(bytes);
        self.ensure_capacity(longest)
            .map_err(|reason| OutputError::InvalidConfig { reason })?;
        let entry = &mut self.lanes[lane as usize];
        entry.bytes = bytes;
        entry.pending = None;
        Ok(())
    }

    /// Close a lane: whatever it had pending is dropped, a transfer carrying
    /// it is waited out, and `unbind` disconnects its pad. The buffers go back
    /// to the heap when the last lane closes.
    pub fn close_lane(&mut self, lane: u8, unbind: impl FnOnce()) {
        self.lanes[lane as usize].pending = None;
        let _ = self.wait_idle();
        self.lanes[lane as usize] = Lane::default();
        unbind();
        if self.lanes.iter().all(|l| !l.open) {
            self.release_buffers();
        }
    }

    /// Record `data` as `lane`'s next frame; kick the frame when every open
    /// lane has one.
    ///
    /// # Safety
    ///
    /// `data` stays alive and unmodified until this lane's next
    /// [`Self::wait_complete`] or [`Self::close_lane`].
    pub unsafe fn start(&mut self, lane: u8, data: &[u8]) -> Result<(), OutputError> {
        let expected = self.lanes[lane as usize].bytes;
        if data.len() != expected {
            return Err(OutputError::DataLengthMismatch {
                expected: expected as u32,
                actual: data.len(),
            });
        }
        // A lane starting again before its last frame went out: send what is
        // pending (this lane's included) first, so no frame waits more than one.
        if self.lanes[lane as usize].pending.is_some() {
            self.kick()?;
        }
        self.lanes[lane as usize].pending = Some((data.as_ptr(), data.len()));
        let all_started = self
            .lanes
            .iter()
            .all(|l| !l.open || l.pending.is_some());
        if all_started {
            self.kick()?;
        }
        Ok(())
    }

    /// Send `lane`'s pending frame if it has not gone yet, and wait out the
    /// transfer carrying it.
    pub fn wait_complete(&mut self, lane: u8) -> Result<(), OutputError> {
        if self.lanes[lane as usize].pending.is_some() {
            self.kick()?;
        }
        if let Bus::Sending { lanes, .. } = &self.bus {
            if lanes & (1 << lane) != 0 {
                self.wait_idle()?;
            }
        }
        Ok(())
    }

    /// Encode every pending lane into a free buffer and start it, after the
    /// transfer in flight (if any) is done.
    fn kick(&mut self) -> Result<(), OutputError> {
        let mut slices: [&[u8]; LANES] = [&[]; LANES];
        let mut mask = 0u8;
        for (index, lane) in self.lanes.iter_mut().enumerate() {
            if let Some((ptr, len)) = lane.pending.take() {
                // SAFETY: `start`'s contract keeps these bytes alive and
                // unmodified until this lane's next wait or close, and this
                // runs before either returns.
                slices[index] = unsafe { core::slice::from_raw_parts(ptr, len) };
                mask |= 1 << index;
            }
        }
        if mask == 0 {
            return Ok(());
        }
        // Single-buffered (two did not fit): the one buffer is in flight
        // until the bus is idle.
        if self.free.is_empty() {
            self.wait_idle()?;
        }
        let Some(mut frame) = self.free.pop() else {
            return Err(OutputError::Other {
                message: "LCD bus has no free frame buffer".into(),
            });
        };
        let started = Instant::now();
        let len = parallel::encode_frame(
            SlotPlan::THREE,
            &slices,
            ColorOrder::Grb,
            LATCH_BYTES,
            frame.buf.as_mut_slice(),
        );
        let encode_us = started.elapsed().as_micros();
        self.stats.encode_us_last = encode_us;
        self.stats.encode_us_max = self.stats.encode_us_max.max(encode_us);
        frame.buf.set_length(len);

        if let Err(error) = self.wait_idle() {
            self.free.push(frame);
            return Err(error);
        }
        let Bus::Idle(lcd) = core::mem::replace(&mut self.bus, Bus::Gone) else {
            self.free.push(frame);
            return Err(OutputError::Other {
                message: "LCD bus is gone".into(),
            });
        };
        let FrameBuffer {
            buf,
            bytes,
            descriptors,
        } = frame;
        match lcd.send(Command::<u8>::None, 0, buf) {
            Ok(transfer) => {
                self.bus = Bus::Sending {
                    transfer,
                    started: Instant::now(),
                    lanes: mask,
                };
                // The buffer travels inside the transfer; its sizes come back
                // with it in `wait_idle`.
                self.in_flight_sizes = Some((bytes, descriptors));
                self.stats.frames = self.stats.frames.wrapping_add(1);
                // One line per 1,000 frames (~30 s at 30 fps): what a walk
                // reads off the product image with no analyzer attached.
                if self.stats.frames % 1_000 == 1 {
                    log::info!(
                        "[lcd-ws281x] frames={} timeouts={} lanes={mask:#04x} bus_bytes={len} \
                         encode_us={} encode_us_max={}",
                        self.stats.frames,
                        self.stats.timeouts,
                        self.stats.encode_us_last,
                        self.stats.encode_us_max,
                    );
                }
                Ok(())
            }
            Err((error, lcd, buf)) => {
                self.bus = Bus::Idle(lcd);
                self.free.push(FrameBuffer {
                    buf,
                    bytes,
                    descriptors,
                });
                Err(OutputError::Other {
                    message: format!("LCD bus could not start a frame: {error:?}"),
                })
            }
        }
    }

    /// Wait out the transfer in flight, if any, and take its buffer back.
    fn wait_idle(&mut self) -> Result<(), OutputError> {
        let Bus::Sending { transfer, started, .. } = &self.bus else {
            return Ok(());
        };
        let started = *started;
        while !transfer.is_done() && started.elapsed() < FRAME_TIMEOUT {}
        let hung = !transfer.is_done();
        let Bus::Sending { transfer, .. } = core::mem::replace(&mut self.bus, Bus::Gone) else {
            unreachable!("matched above");
        };
        let (bytes, descriptors) = self.in_flight_sizes.take().unwrap_or((0, 0));
        let (result, lcd, buf) = if hung {
            let (lcd, buf) = transfer.cancel();
            (Err(()), lcd, buf)
        } else {
            let (result, lcd, buf) = transfer.wait();
            (result.map_err(|_| ()), lcd, buf)
        };
        self.bus = Bus::Idle(lcd);
        self.free.push(FrameBuffer {
            buf,
            bytes,
            descriptors,
        });
        match (hung, result) {
            (true, _) => {
                self.stats.timeouts = self.stats.timeouts.wrapping_add(1);
                Err(OutputError::Other {
                    message: format!(
                        "LCD WS281x frame did not finish within {} ms; cancelled",
                        FRAME_TIMEOUT.as_millis()
                    ),
                })
            }
            (false, Err(())) => Err(OutputError::Other {
                message: "LCD WS281x frame ended with a DMA descriptor error".into(),
            }),
            (false, Ok(())) => Ok(()),
        }
    }

    fn longest_open(&self) -> usize {
        self.lanes
            .iter()
            .filter(|l| l.open)
            .map(|l| l.bytes)
            .max()
            .unwrap_or(0)
    }

    /// Make both buffers hold a frame whose longest lane is `lane_bytes`.
    fn ensure_capacity(&mut self, lane_bytes: usize) -> Result<(), alloc::string::String> {
        let needed = parallel::encoded_len(SlotPlan::THREE, lane_bytes, LATCH_BYTES);
        if needed <= self.capacity {
            return Ok(());
        }
        self.wait_idle()
            .map_err(|error| format!("LCD bus did not go idle to grow its buffers: {error}"))?;
        self.release_buffers();
        // Two buffers, so the next frame encodes while this one sends; one
        // when two do not fit (the encode then waits for the bus).
        let mut fresh = Vec::with_capacity(2);
        for _ in 0..2 {
            match FrameBuffer::alloc(needed) {
                Some(frame) => fresh.push(frame),
                None => break,
            }
        }
        if fresh.is_empty() {
            return Err(format!(
                "the LED frame buffer needs {needed} B of internal RAM (longest lane \
                 {lane_bytes} B) and only {} B is free",
                esp_alloc::HEAP.free_caps(MemoryCapability::Internal.into())
            ));
        }
        log::info!(
            "[lcd-ws281x] frame buffers {} × {needed} B internal (longest lane {lane_bytes} B; \
             internal free after: {} B)",
            fresh.len(),
            esp_alloc::HEAP.free_caps(MemoryCapability::Internal.into()),
        );
        self.free = fresh;
        self.capacity = needed;
        Ok(())
    }

    fn release_buffers(&mut self) {
        let _ = self.wait_idle();
        while let Some(frame) = self.free.pop() {
            frame.free();
        }
        self.capacity = 0;
    }
}

impl FrameBuffer {
    /// A buffer of `bytes` and its descriptors, from internal RAM only.
    fn alloc(bytes: usize) -> Option<Self> {
        let descriptors = bytes.div_ceil(DESCRIPTOR_CHUNK) + 1;
        let data = internal_alloc(Layout::from_size_align(bytes, 4).ok()?)?;
        let Some(desc) = internal_alloc(Layout::array::<DmaDescriptor>(descriptors).ok()?) else {
            internal_free(data, Layout::from_size_align(bytes, 4).ok()?);
            return None;
        };
        // SAFETY: both allocations are fresh, exclusively ours, sized and
        // aligned for their types, and released only by `free`, after the
        // buffer is out of any transfer.
        let (data, desc) = unsafe {
            let desc = desc.cast::<DmaDescriptor>();
            for i in 0..descriptors {
                desc.as_ptr().add(i).write(DmaDescriptor::EMPTY);
            }
            (
                core::slice::from_raw_parts_mut(data.as_ptr(), bytes),
                core::slice::from_raw_parts_mut(desc.as_ptr(), descriptors),
            )
        };
        let buf = DmaTxBuf::new(desc, data).ok()?;
        Some(Self {
            buf,
            bytes,
            descriptors,
        })
    }

    fn free(self) {
        let (desc, data) = self.buf.split();
        internal_free(
            NonNull::from(data).cast(),
            Layout::from_size_align(self.bytes, 4).expect("layout it was allocated with"),
        );
        internal_free(
            NonNull::from(desc).cast(),
            Layout::array::<DmaDescriptor>(self.descriptors).expect("layout it was allocated with"),
        );
    }
}

fn internal_alloc(layout: Layout) -> Option<NonNull<u8>> {
    // SAFETY: a plain allocation request; the result is checked for null.
    NonNull::new(unsafe { esp_alloc::HEAP.alloc_caps(MemoryCapability::Internal.into(), layout) })
}

fn internal_free(ptr: NonNull<u8>, layout: Layout) {
    // SAFETY: `ptr` came from `internal_alloc` with this `layout` and is no
    // longer referenced by any transfer.
    unsafe { core::alloc::GlobalAlloc::dealloc(&esp_alloc::HEAP, ptr.as_ptr(), layout) };
}
