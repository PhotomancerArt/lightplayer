//! `Esp32C6Machine` — the hart, the bus and the schedule, driven as one.
//!
//! # The run loop (plan PD5)
//!
//! ```text
//! loop {
//!   deadline = min(next scheduler event, stop cycle, next probe, slice cap)
//!   match hart.run_slice(bus, deadline - now) {
//!     BudgetExhausted => fire every due event, then resample the matrix and poll
//!     Wfi             => jump guest time to the next event (or the stop cycle), then the same
//!     Ebreak { pc }   => the ROM hook table gets first refusal; otherwise deliver the breakpoint
//!     Fault(f)        => stop
//!   }
//! }
//! ```
//!
//! **Wall clock never enters the machine.** Guest time is
//! `lp_emu_core::sched::Scheduler` and the hart's cycle model, and nothing
//! else, so two runs of the same image with the same scripted host input are
//! byte-identical. `--wall-timeout` is the single exception and it is a
//! safety net, not an input: it can only *end* a run, never change one.
//!
//! # Time
//!
//! The CPU runs at 160 MHz, so `micros = cycles / 160`. Three grades. The
//! first two are just the hart's cycle model; the third adds what an
//! access's *address* costs:
//!
//! - `t1` (`lp-emu:esp32c6:t1`) — [`CycleModel::InstructionCount`], what the
//!   vendor emulator does.
//! - `t2` (`lp-emu:esp32c6:t2`) — [`CycleModel::Esp32C6`], a per-class model
//!   taken from a blog post.
//! - `t3` (`lp-emu:esp32c6:t3`) — [`CycleModel::Esp32C6Kernels`], the same
//!   table with the four classes the `cycle-probe` payload measured on
//!   silicon corrected, **plus** [`crate::cache::CacheCost`] installed on
//!   the bus: the flash cache's fills and the APB's wait states. It is the
//!   first grade in which two instructions with the same opcode can cost
//!   different amounts because they are at different addresses.
//!
//! `t1` and `t2` install no memory-cost model at all, so their cycle counts
//! are exactly what they were before `t3` existed.
//!
//! Neither is a claim about milliseconds on silicon; see vision "Time is a
//! graded ladder, never a promise".
//!
//! # Peripheral registration order
//!
//! `event_id` packs a peripheral's **index** into the scheduler's event tags,
//! and the index is insertion order. So the order this builder registers
//! blocks in is part of the machine's contract, not a detail: re-sorting it
//! (by base address, say, for tidiness) would silently re-point every
//! already-scheduled event. [`PERIPHERAL_REGISTRATION_ORDER`] is that order
//! written down, and [`Esp32C6Builder::build`] refuses a registration
//! sequence that is not a subsequence of it — so adding a block is an edit to
//! the list, which a reviewer sees, rather than a call site nobody diffs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use lp_emu_core::sched::Cycles;
use lp_emu_core::{Bus, CycleModel};
use lp_emu_esp_common::bus::StrictViolation;
use lp_emu_esp_common::periph::BoxedPeripheral;
use lp_emu_esp_common::pins::{PadId, RouteSource};
use lp_emu_esp_common::strip::ws281x::{Frame, Ws281xDecoder, unpermute};
use lp_emu_esp_common::{
    ByteLog, ByteSink, ByteSource, ElfImage, RamRegion, RegGrade, SocBus, Strap,
};
use lp_riscv_emu::mach::trigger::TRIGGER_COUNT;
use lp_riscv_emu::mach::{HartFault, MachineHart, SliceEnd};
use lp_ws281x::{ChannelTiming, ColorOrder};

use crate::control::{ControlCommand, ControlReply, HostReport, PadReport};
use crate::intmatrix::Esp32C6IntMatrix;
use crate::loader::{self, EfuseIdentity, LoadError, PlacedAppSegment, ResetCause};
use crate::memmap;
use crate::periph;
use crate::periph::gpio::Gpio;
use crate::periph::uart::LIVE_POLL_CYCLES;
use crate::periph::usb_sj::UsbSerialJtag;
use crate::pinscript::PinScript;
use crate::rom::{self, HookResult, HookTable, PlacedSegment, RomError};
use crate::snapshot::Snapshot;

/// The largest slice the machine ever asks for: 8,192 cycles, 51 µs.
///
/// A slice's deadline is fixed when it starts, from the schedule as it was
/// then. An MMIO write *inside* the slice can schedule an event sooner than
/// that deadline — esp-rtos arming a 1 ms tick, say — and nothing in the
/// hart looks at the schedule again until the slice ends. So the cap is the
/// worst-case lateness of any event a peripheral schedules mid-slice, and
/// 51 µs is 0.5 % of the 10 ms tick and 5 % of the 1 ms one. The cost is one
/// scheduler peek and one matrix resample per 8,192 cycles, which the
/// no-radio boot does not notice. (P4 had 1,000,000 here, when no peripheral
/// could schedule anything.)
/// `LP_CLKRST + 0x10`, the register the mask ROM reads before anything else.
const LP_CLKRST_RESET_CAUSE: u32 = 0x010;

pub const MAX_SLICE_CYCLES: u64 = 8_192;

/// The slice cap while strict mode is on.
///
/// A strict refusal reaches the guest as an ordinary access fault, so the
/// hart takes the trap and carries on — into `_pre_init_trap`, which is
/// `_default_abort`, which is `j _default_abort` forever. Short slices let
/// the machine notice the recorded violation and stop instead of spinning to
/// the end of the budget. Strict mode is a bring-up mode; the cost is one
/// scheduler peek per 1024 cycles.
const STRICT_SLICE_CYCLES: u64 = 1_024;

/// The order the C6's peripheral blocks are registered in. See the module
/// docs for why this is a contract.
///
/// The list is the blocks the M3 discovery reports name on the C6 boot and
/// runtime path, in the order the boot meets them. A block a later phase
/// needs is added **in its place in this list**, never appended for
/// convenience. [`crate::periph::boot_set`] registers exactly this order.
pub const PERIPHERAL_REGISTRATION_ORDER: &[&str] = &[
    // Reached inside `esp_hal::init`, in `init` order (discovery §4).
    "LP_APM",
    "LP_APM0",
    "HP_APM",
    "LP_AON",
    "PMU",
    "LP_CLKRST",
    "LP_WDT",
    "MODEM_SYSCON",
    "MODEM_LPCON",
    "I2C_ANA_MST",
    "LP_I2C_ANA_MST",
    "PCR",
    "TIMG0",
    "TIMG1",
    "EFUSE",
    "LP_TIMER",
    "APB_SARADC",
    "SYSTIMER",
    "ASSIST_DEBUG",
    // The interrupt path.
    "INTERRUPT_CORE0",
    "PLIC_MX",
    // M7: the mask ROM's `_init` writes the last word of BOTH PLIC
    // apertures before it has a stack. The user-mode one is nothing else's
    // business on this chip (`tee_enabled()` is a const false), so it is
    // accept-and-remember with the PAC's names.
    "PLIC_UX",
    "INTPRI",
    // The rest of what the no-radio image touches.
    "HP_SYS",
    "TEE",
    "LP_TEE",
    "LP_IO",
    "RNG",
    "EXTMEM",
    // Consoles (accept in P5, modelled in P6) and the product path (M4, M5).
    "UART0",
    "UART1",
    "USB_DEVICE",
    "IO_MUX",
    "GPIO",
    "SPI0",
    "SPI1",
    "RMT",
    // The radio window (P6): esp-radio's init runs after `Rmt::new` in
    // `main`, and nothing else in the boot reaches `0x600A_0000`. The PWR
    // gap is the first radio block the ROM's `tsf_hal_*` touches.
    "WIFI_MAC",
    "WIFI_PWR",
    // The analog I2C master's command memory (P6, G6-2 finding 1): libphy
    // fills it right after its first radio-window writes.
    "I2C_MST_MEM",
    // M7: the two SDIO-slave blocks only the mask ROM touches — `HINF`'s
    // device id and the one `SLC` word `ets_spi_download_disabled` reads.
    // Last, not first, even though a ROM-up boot meets them before
    // everything else: the list is read as "what the application's boot
    // walks through", and the ROM's own corner is an appendix to it.
    "LP_ANA",
    // The one block on the boot path that has to compute the right answer:
    // the bootloader hashes the image before it loads it.
    "SHA",
    "HINF",
    "SLC",
];

/// Which cycle model a run uses. Both grades are the same machine; only the
/// hart's per-instruction cost changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeGrade {
    /// `lp-emu:esp32c6:t1` — one cycle per instruction.
    #[default]
    T1,
    /// `lp-emu:esp32c6:t2` — the per-instruction-class model.
    T2,
    /// `lp-emu:esp32c6:t3` — the kernel-measured class table, and the cache
    /// and bus costs of the address ([`crate::cache::CacheCost`]).
    T3,
}

impl TimeGrade {
    pub const fn cycle_model(self) -> CycleModel {
        match self {
            TimeGrade::T1 => CycleModel::InstructionCount,
            TimeGrade::T2 => CycleModel::Esp32C6,
            TimeGrade::T3 => CycleModel::Esp32C6Kernels,
        }
    }

    /// What an access's address costs at this grade, or `None` for free.
    ///
    /// `t1` and `t2` are `None` **by construction**, which is the reason
    /// their counts cannot move: the bus's hook is not installed, so its
    /// drain is a constant zero.
    pub fn memory_cost(self) -> Option<Box<dyn lp_emu_core::MemoryCost + Send>> {
        match self {
            TimeGrade::T1 | TimeGrade::T2 => None,
            TimeGrade::T3 => Some(Box::new(crate::cache::CacheCost::new())),
        }
    }

    /// The validation system's configuration name (plan PD4).
    pub const fn configuration(self) -> &'static str {
        match self {
            TimeGrade::T1 => "lp-emu:esp32c6:t1",
            TimeGrade::T2 => "lp-emu:esp32c6:t2",
            TimeGrade::T3 => "lp-emu:esp32c6:t3",
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "t1" => Ok(TimeGrade::T1),
            "t2" => Ok(TimeGrade::T2),
            "t3" => Ok(TimeGrade::T3),
            other => Err(format!(
                "unknown time grade `{other}` (expected t1, t2 or t3)"
            )),
        }
    }
}

/// Where the mask ROM comes from. There is no "no ROM" variant: plan PD7
/// says the ROM is loaded in every configuration.
#[derive(Clone, Debug)]
pub enum RomSource {
    /// The image vendored in this repository. See `lp-emu/esp/roms/`.
    Vendored,
    Path(PathBuf),
    Bytes(Vec<u8>),
}

/// The application image to direct-load.
#[derive(Clone, Debug)]
pub enum AppSource {
    /// No app: a machine with the ROM in place and nothing else. What M7's
    /// ROM-up boot starts from, and what the ROM tests use.
    None,
    Path(PathBuf),
    Bytes(Vec<u8>),
}

/// How the machine arrives at the application: the plan's two boot paths.
///
/// They are not two implementations of one thing — they are two *different*
/// amounts of chip. [`BootMode::Direct`] is M3's: the app's segments are
/// placed by the host, the flash-resident half is staged at offsets the
/// loader computes, the ROM's chip-size word is written for it, and the hart
/// starts at the app's `_start` in the state the bootloader would have left.
/// [`BootMode::RomUp`] is M7's: **nothing** is placed. The chip holds a
/// merged image a flasher wrote, the hart starts at the mask ROM's reset
/// vector, and the ROM and the second-stage bootloader do every one of those
/// things themselves, for real, out of flash.
///
/// `loader`'s module documentation lists what direct load does not
/// reproduce; each line of it is a place the two paths can disagree, and
/// `tests/rom_up_boot.rs` is where they are made to agree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BootMode {
    /// Place the app and start at its entry point (M3, M4).
    #[default]
    Direct,
    /// Start at the mask ROM's reset vector and let the chip boot itself
    /// out of flash (M7).
    RomUp,
}

impl BootMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "direct" => Some(BootMode::Direct),
            "rom-up" => Some(BootMode::RomUp),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BootMode::Direct => "direct",
            BootMode::RomUp => "rom-up",
        }
    }
}

/// Where a run's UART0 bytes go — and, for `Tcp`, where its RX bytes come
/// from. Whatever the choice, the bytes are also kept in memory for
/// `--exit-on` and [`Esp32C6Machine::uart0`].
#[derive(Clone, Debug, Default)]
pub enum Uart0Sink {
    /// Collected in memory only (and matched against `--exit-on`).
    #[default]
    Memory,
    Stdout,
    File(PathBuf),
    /// **Listen** on this address (`127.0.0.1:5555`) for one client at a
    /// time — as esp-emu's `--uart-tcp` did for the spike's proxy, and what
    /// `lp-cli … serial:tcp://127.0.0.1:5555` connects to. The client's bytes
    /// are UART0's RX. A run with a live socket is not deterministic; see
    /// [`lp_emu_esp_common::TcpHost`].
    Tcp(String),
}

/// Where USB-Serial-JTAG's bytes go. Used twice: for the `usb-sj` stream —
/// what a **host received** from the IN endpoint (`--usb-sj`) — and for the
/// observation stream — what the guest handed over that no host took
/// (`--usb-sj-tried`). Both are always also kept in memory
/// ([`Esp32C6Machine::usb_sj`], [`Esp32C6Machine::usb_sj_tried`]).
#[derive(Clone, Debug, Default)]
pub enum UsbSjSink {
    #[default]
    Memory,
    Stderr,
    File(PathBuf),
    /// **Listen** on this address for one client at a time, exactly as
    /// `Uart0Sink::Tcp` does: the client receives what a host received, and
    /// its bytes are the OUT endpoint's live source. `lp-cli …
    /// serial:tcp://<addr>` connects to it unchanged.
    ///
    /// There is nothing to replay to a late client. With no client the host
    /// is `Attached { draining: false }` or `Absent`, so no packet is ever
    /// delivered and no backlog accumulates; the backlog exists only for a
    /// client that disconnects and reconnects while `draining` is held on by
    /// `--usb-sj-drain manual`.
    ///
    /// Only the `usb-sj` stream may be a socket — the observation stream is
    /// an observation, not a link.
    Tcp(String),
}

/// Whether connecting to the USB byte socket opens the port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UsbSjDrain {
    /// A client connecting is an application opening the port; disconnecting
    /// is it closing. What `lp-cli`'s readiness engine expects, and what a
    /// Web Serial `open()`/`close()` means.
    #[default]
    Auto,
    /// The control channel owns `open` and `close`; the socket only carries
    /// bytes. For a run that wants a host attached and *not* draining while a
    /// client watches the byte stream.
    Manual,
}

impl UsbSjDrain {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(UsbSjDrain::Auto),
            "manual" => Some(UsbSjDrain::Manual),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            UsbSjDrain::Auto => "auto",
            UsbSjDrain::Manual => "manual",
        }
    }
}

/// Where decoded WS281x frames go (`--dump-frames`).
///
/// One JSON line per frame, as it is decoded — a stream, not a report, so a
/// run that is killed still leaves the frames it had already seen.
#[derive(Clone, Debug, Default)]
pub enum FrameSink {
    /// Kept in memory only, for [`Esp32C6Machine::frames`].
    #[default]
    Memory,
    Stdout,
    File(PathBuf),
}

/// Where the raw pin log goes (`--pin-log`): one line per edge.
///
/// Never the default: a 256-LED frame is 12,288 edges, and 24 frames of the
/// chase are 295,000 lines.
#[derive(Clone, Debug, Default)]
pub enum PinLogSink {
    #[default]
    Off,
    File(PathBuf),
}

/// Lines the pin log writes before it stops, with a closing note.
pub const PIN_LOG_LINE_CAP: u64 = 2_000_000;

/// Where the radio TX log goes (`--tx-log`): one line per frame the WiFi
/// blob handed the MAC.
///
/// The pin log's shape, for the radio: an observation of what left the
/// guest, not a model of anything. **Nothing is delivered** — no machine
/// receives these bytes, no interrupt is raised, and a run with the log on
/// is the same run with it off. See [`Esp32C6Machine::drain_tx_log`] for
/// what each field is and how much of it is a reading rather than a fact.
#[derive(Clone, Debug, Default)]
pub enum TxLogSink {
    #[default]
    Off,
    Stderr,
    File(PathBuf),
}

/// Lines the radio TX log writes before it stops, with a closing note. A
/// frame a second on the observed image, so this is a runaway guard rather
/// than a real ceiling.
pub const TX_LOG_LINE_CAP: u64 = 100_000;

/// Bytes of a TX buffer the log will print for one frame. The observed frame
/// is 64 (an eight-byte header and a 56-byte 802.11 frame); the cap keeps a
/// descriptor whose length word has gone strange from writing a screenful.
pub const TX_LOG_BYTE_CAP: u32 = 2_048;

/// Frames kept in memory per pad, with a note when the cap is reached. The
/// `--dump-frames` stream itself is not capped.
pub const FRAMES_PER_PAD_CAP: usize = 8_192;

/// The strip a pad is decoded as: the wire timing, and the byte order the
/// record's `rgb` field is unpermuted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StripConfig {
    pub timing: ChannelTiming,
    pub order: ColorOrder,
}

impl Default for StripConfig {
    fn default() -> Self {
        Self {
            timing: ChannelTiming::WS2812,
            order: ColorOrder::Grb,
        }
    }
}

impl StripConfig {
    /// `ws2812` | `ws2811`.
    pub fn parse_timing(text: &str) -> Option<ChannelTiming> {
        match text {
            "ws2812" => Some(ChannelTiming::WS2812),
            "ws2811" => Some(ChannelTiming::WS2811),
            _ => None,
        }
    }

    /// `rgb` | `rbg` | `grb` | `gbr` | `brg` | `bgr`.
    pub fn parse_order(text: &str) -> Option<ColorOrder> {
        match text {
            "rgb" => Some(ColorOrder::Rgb),
            "rbg" => Some(ColorOrder::Rbg),
            "grb" => Some(ColorOrder::Grb),
            "gbr" => Some(ColorOrder::Gbr),
            "brg" => Some(ColorOrder::Brg),
            "bgr" => Some(ColorOrder::Bgr),
            _ => None,
        }
    }

    /// The timing with this record's byte order on it.
    pub fn timing(&self) -> ChannelTiming {
        self.timing.with_color_order(self.order)
    }
}

/// What the machine has decoded from the pads.
///
/// The sinks are deliberately not here: a snapshot is state, and a file
/// handle is not ([`crate::snapshot`]). A decoder caught **mid-frame** is
/// state — one restored without its half-shifted bits would resume a frame
/// that never existed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PinState {
    /// Per pad, its decoder.
    pub decoders: BTreeMap<u8, Ws281xDecoder>,
    /// Per pad, the frames it has completed.
    pub frames: BTreeMap<u8, Vec<Frame>>,
    /// Per pad, what it is routed to right now.
    pub routed: BTreeMap<u8, RouteSource>,
    /// Per pad, edges seen.
    pub edges: BTreeMap<u8, u64>,
    /// Pads whose frame list reached [`FRAMES_PER_PAD_CAP`].
    pub capped: BTreeSet<u8>,
}

/// The pads, online: one decoder per routed pad, fed from the fabric every
/// slice, plus the two host sinks.
struct PinObserver {
    strip: StripConfig,
    cpu_hz: u64,
    state: PinState,
    /// The last [`lp_emu_esp_common::pins::Fabric::route_epoch`] seen, so a
    /// slice that changed no routing walks no pads.
    epoch: u64,
    dump: Option<Box<dyn std::io::Write + Send>>,
    pin_log: Option<Box<dyn std::io::Write + Send>>,
    pin_log_lines: u64,
    pin_log_capped: bool,
}

/// The USB host's state at power-on: the builder's stand-in for M6 P3's
/// control channel. See [`crate::periph::usb_sj`] for what each state does
/// at the registers.
pub use crate::periph::usb_sj::HostState as UsbHost;

/// One frame this machine's radio handed the MAC, ready for an
/// [`Air`](lp_emu_esp_common::air::Air).
///
/// The `at` is the guest cycle of the arming write — the same cycle the
/// `--tx-log` line reports — and `bytes` are the 802.11 frame exactly as it
/// stood in guest RAM, with neither the descriptor's eight-byte header nor
/// the FCS the buffer does not carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadioFrame {
    pub at: Cycles,
    pub bytes: Vec<u8>,
}

/// How far the walk of the guest's RX descriptor chain may go before the
/// machine gives up.
///
/// The ring the blob posts has ten elements and ends in a NULL `next` (M4 P0
/// §2), so the walk normally stops on its own. This bound exists so that a
/// `next` chain the guest has corrupted — or one that loops — cannot spin
/// the emulator inside a slice boundary.
const RX_RING_WALK_CAP: u32 = 64;

/// `dw0[31]`, `owner`: the hardware's, and the one bit of the descriptor
/// word whose reading M4 P0 called consistent across both descriptors.
const RX_DESC_OWNER_MASK: u32 = 1 << 31;
/// `dw0[30]`, `eof`. Set on the TX side's single-descriptor chain.
const RX_DESC_EOF_MASK: u32 = 1 << 30;
/// `dw0[11:0]`: the buffer's size — 1,700 against a 1,708-byte stride.
const RX_DESC_SIZE_MASK: u32 = 0xfff;
/// `dw0[23:12]`, the twelve bits M4 P0 could not name. See [`AirDelivery`].
const RX_DESC_LEN_MASK: u32 = 0xfff << 12;
const RX_DESC_LEN_SHIFT: u32 = 12;

/// What became of one frame the air offered this machine.
///
/// # `[23:12]`, and why this type carries the argument
///
/// M4 P0 refuted `[23:12] = length` on both descriptors: 272 on a TX
/// descriptor whose buffer is 96 bytes, and 2,704 on a freshly posted RX one
/// whose buffer is 1,700. The swap (`length` low, `size` high) is refuted
/// too, because it would put 2,704-byte buffers 1,708 bytes apart.
///
/// A delivery still has to write *something* there, so this machine writes
/// the **byte count it wrote** (header, frame and the four FCS bytes), and
/// the experiment that settled the choice was run rather than argued
/// (`tests/air_delivery.rs`,
/// `the_descriptor_words_undetermined_bits_change_nothing`):
///
/// - **The frame arrives whatever is in `[23:12]`** — the byte count, the
///   2,704 the ring already held, or all ones. But the instruction counts
///   differ between them, so the guest **reads** the field and merely
///   tolerates every value. Undetermined, then; not ignored. The byte count
///   is written because that is what a reader would expect to find there,
///   and no claim is made about what the silicon puts in it.
/// - **`[28:24]` is load-bearing**, which M4 P0 could not have known:
///   clearing the 1 the blob posted stops the guest receiving. Its meaning
///   is still P0's U3; leaving it exactly as posted is now a rule with a
///   test behind it.
/// - **`owner` is our convention, not the guest's requirement**: leaving it
///   set delivers the frame just the same. It is cleared anyway, because
///   that is what `lldesc` hardware does and a ring that never gave a
///   descriptor back would look full after ten frames when it is not.
/// - **`eof` is demanded.** With it clear the guest does not take the frame.
///
/// The README carries the same four lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AirDelivery {
    /// Written into `desc`'s buffer, and the interrupt raised.
    Delivered {
        desc: u32,
        buf: u32,
        dw0_before: u32,
        dw0_after: u32,
        /// Header, frame and the four FCS bytes.
        len: u32,
    },
    /// This machine has no `WIFI_MAC` block — a machine built without one.
    NoBlock,
    /// The blob has not programmed an RX ring yet (nothing at
    /// `WIFI_MAC+0x4084`), or the chain does not read. Before radio init
    /// this is the ordinary answer.
    NoRing,
    /// **Every descriptor in the chain is the guest's**, or none that is
    /// still the hardware's has a big enough buffer. The ring ends rather
    /// than wrapping, so this is what "the eleventh frame" looks like. The
    /// frame is dropped, counted, and the first one is logged.
    RingFull,
    /// The walk hit [`RX_RING_WALK_CAP`] without finding an end — a chain
    /// that loops or was corrupted.
    RingWalkCap,
}

impl AirDelivery {
    /// The phrase the trace line and the one-shot warning use.
    pub fn why(&self) -> &'static str {
        match self {
            AirDelivery::Delivered { .. } => "delivered",
            AirDelivery::NoBlock => "dropped: no WIFI_MAC block",
            AirDelivery::NoRing => "dropped: no RX ring programmed yet",
            AirDelivery::RingFull => "dropped: the RX ring is full",
            AirDelivery::RingWalkCap => "dropped: the RX chain did not end",
        }
    }
}

/// What [`Esp32C6Machine::read_tx_frame`] made of one armed handoff.
struct ReadTxFrame {
    /// The `--tx-log` line, always.
    line: String,
    /// The frame's bytes, when the descriptor's reading held.
    frame: Option<Vec<u8>>,
}

impl ReadTxFrame {
    /// A handoff whose bytes this machine could not read as a frame. The
    /// line still says so; the air gets nothing rather than a guess.
    fn unreadable(line: String) -> Self {
        Self { line, frame: None }
    }
}

/// Why a run stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The `--exit-on` substring appeared on UART0.
    ExitMatched { cycle: Cycles },
    /// The emulated deadline was reached with no fault. The ordinary
    /// "it ran for the requested time" answer.
    Deadline { cycle: Cycles },
    /// The hart cannot continue.
    Fault {
        cycle: Cycles,
        pc: u32,
        fault: HartFault,
    },
    /// Strict mode refused an access. Carries the access itself, not where
    /// the hart ended up after taking the fault for it.
    StrictBus { violation: StrictViolation },
    /// A peripheral asked for a reset the machine cannot perform: the RWDT
    /// expired with a reset action, or the USB-Serial-JTAG block saw a
    /// host's reset dance. The chip would reboot into `strap`; the emulator
    /// reports it (M7 owns the boot chain). Exit code 2, like a fault —
    /// on silicon this is `rst:0x10 (RTCWDT_RTC_RST)` / `rst:0x15
    /// (USB_UART_HPSYS)` in the boot log.
    Reset {
        cycle: Cycles,
        source: &'static str,
        strap: Strap,
    },
    /// The wall-clock safety net fired. The only non-deterministic outcome,
    /// and it can only end a run.
    WallTimeout { cycle: Cycles },
    /// A `--break-at` symbol was reached; the guest is stopped at its first
    /// instruction with every register as the caller left it.
    Breakpoint { cycle: Cycles, pc: u32 },
}

impl Outcome {
    /// The CLI's exit code for this outcome.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Outcome::ExitMatched { .. } | Outcome::Deadline { .. } => 0,
            Outcome::Fault { .. } | Outcome::Reset { .. } => 2,
            Outcome::StrictBus { .. } => 3,
            Outcome::WallTimeout { .. } => 4,
            Outcome::Breakpoint { .. } => 5,
        }
    }
}

/// When to stop. Everything but `wall_timeout` is emulated time.
#[derive(Clone, Debug, Default)]
pub struct StopCondition {
    /// Absolute guest cycle to stop at. `None` means "until something else
    /// stops it", which for a machine with no scheduled events means the
    /// first fault.
    pub stop_cycle: Option<Cycles>,
    /// Stop when this appears in UART0's bytes.
    pub exit_on: Option<String>,
    /// Host-side safety net.
    pub wall_timeout: Option<Duration>,
    /// `(cycle, symbol)` — print the word at `symbol` when guest time
    /// reaches `cycle`.
    pub probes: Vec<(Cycles, String)>,
}

impl StopCondition {
    /// Stop after `micros` of emulated time from cycle zero.
    pub fn after_micros(micros: u64) -> Self {
        Self {
            stop_cycle: Some(micros * memmap::CYCLES_PER_US),
            ..Default::default()
        }
    }

    /// …or at the end of the first UART0 line containing `needle`, whichever
    /// comes first. The `--exit-on` flag's condition, for a caller that
    /// builds a [`StopCondition`] rather than parsing one.
    pub fn exit_on(mut self, needle: impl Into<String>) -> Self {
        self.exit_on = Some(needle.into());
        self
    }
}

/// Anything that stopped a machine from being built.
#[derive(Debug)]
pub enum BuildError {
    Rom(RomError),
    Load(LoadError),
    Io(String),
    /// A peripheral was registered out of [`PERIPHERAL_REGISTRATION_ORDER`].
    RegistrationOrder {
        name: String,
        after: String,
    },
    /// A peripheral name that is not in the declared order at all.
    UndeclaredPeripheral {
        name: String,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Rom(e) => write!(f, "{e}"),
            BuildError::Load(e) => write!(f, "{e}"),
            BuildError::Io(m) => write!(f, "{m}"),
            BuildError::RegistrationOrder { name, after } => write!(
                f,
                "peripheral `{name}` was registered after `{after}`, which contradicts \
                 PERIPHERAL_REGISTRATION_ORDER. Peripheral indices are packed into scheduler \
                 event ids; re-ordering them re-points already-scheduled events. Register in \
                 the declared order, or move the entry in that list."
            ),
            BuildError::UndeclaredPeripheral { name } => write!(
                f,
                "peripheral `{name}` is not in PERIPHERAL_REGISTRATION_ORDER — add it in the \
                 place the boot meets it, not at the end"
            ),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<RomError> for BuildError {
    fn from(e: RomError) -> Self {
        BuildError::Rom(e)
    }
}

impl From<LoadError> for BuildError {
    fn from(e: LoadError) -> Self {
        BuildError::Load(e)
    }
}

/// A sink that keeps a copy of everything it forwards, so `--exit-on` can
/// match on bytes that also went to stdout or a file.
struct TeeSink {
    log: ByteLog,
    inner: Box<dyn ByteSink>,
}

impl ByteSink for TeeSink {
    fn write(&mut self, bytes: &[u8]) {
        self.log.append(bytes);
        self.inner.write(bytes);
    }

    fn flush(&mut self) {
        self.inner.flush();
    }
}

/// Assemble a machine. Code-first, as the vision asks: start from the chip,
/// attach the ROM and the app, pick the time grade, build.
///
/// [`Esp32C6Builder::new`] is the C6 with its boot peripheral set
/// ([`crate::periph::boot_set`]); [`Esp32C6Builder::bare`] is the memory
/// map and the ROM with **no** peripherals, for tests that bring their own
/// and for reading what an unmodelled boot looks like.
pub struct Esp32C6Builder {
    rom: RomSource,
    app: AppSource,
    /// Direct load, or the chip booting itself out of flash.
    boot_mode: BootMode,
    /// What `LP_CLKRST.reset_cause` says, and so what the mask ROM's banner
    /// prints as `rst:0x..`.
    reset_cause: ResetCause,
    /// What `GPIO.strap` reads, and so what the banner prints as `boot:0x..`
    /// and which of the ROM's two paths runs.
    strap: Strap,
    /// Perform a reset request instead of reporting it. See
    /// [`Esp32C6Builder::reboot_on_reset`].
    reboot_on_reset: bool,
    /// Let the hart pre-decode blocks. See
    /// [`Esp32C6Builder::block_cache`].
    block_cache: bool,
    /// Let the hart run a translated core. See
    /// [`Esp32C6Builder::translate`].
    translate: bool,
    /// Print the translated core's report and the boot cost of building it.
    /// See [`Esp32C6Builder::jit_report`].
    jit_report: bool,
    efuse: EfuseIdentity,
    time_grade: TimeGrade,
    strict: bool,
    strict_grade: Option<RegGrade>,
    strict_grade_blocks: Option<Vec<&'static str>>,
    trace: Option<Box<dyn std::io::Write + Send>>,
    trace_blocks: Vec<String>,
    uart0: Uart0Sink,
    /// Scripted host input for UART0 (`--uart0-script`). Ignored when the
    /// sink is `Tcp`, whose client is the source.
    uart0_source: Option<Box<dyn ByteSource>>,
    /// The same, as a [`ScriptedSource`] the builder still has to hand the
    /// UART0 log to — the only way an `after "<line>"` step can see what the
    /// device said.
    uart0_script: Option<lp_emu_esp_common::ScriptedSource>,
    /// The rate a host on UART0 sends at ([`Esp32C6Builder::uart0_baud`]).
    uart0_baud: Option<u64>,
    usb_sj: UsbSjSink,
    usb_sj_tried: UsbSjSink,
    /// Scripted host input on the USB link: what the host sends to the OUT
    /// endpoint, at declared cycles.
    usb_sj_source: Option<Box<dyn ByteSource>>,
    usb_script_source: Option<lp_emu_esp_common::ScriptedSource>,
    usb_host: UsbHost,
    usb_sj_drain: UsbSjDrain,
    /// `--control tcp:<host:port>`: listen for the line protocol.
    control: Option<String>,
    /// Scripted control commands (`--usb-script`), in file order.
    usb_script: Vec<(Cycles, ControlCommand)>,
    /// Scripted pad levels (`--pin-script`), in file order.
    pin_script: PinScript,
    /// `--wire a:b`: pads tied in the fabric before the guest starts.
    wires: Vec<(PadId, PadId)>,
    seed: u64,
    /// Where the flash chip's bytes come from and whether they go back.
    flash: crate::flash::FlashBacking,
    /// The modelled chip's size. `--flash` a larger file and the build
    /// fails rather than truncating it.
    flash_len: u32,
    /// Register the boot set before `peripherals`.
    boot_set: bool,
    peripherals: Vec<(u32, u32, BoxedPeripheral)>,
    dump_frames: FrameSink,
    pin_log: PinLogSink,
    tx_log: TxLogSink,
    strip: StripConfig,
    /// Keep the RMT's pulse and word logs (`Rmt::keep_logs`).
    rmt_logs: bool,
}

impl Default for Esp32C6Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Esp32C6Builder {
    /// A C6 with its boot peripheral set.
    pub fn new() -> Self {
        Self {
            rom: RomSource::Vendored,
            app: AppSource::None,
            boot_mode: BootMode::default(),
            reset_cause: ResetCause::default(),
            strap: Strap::App,
            reboot_on_reset: false,
            block_cache: true,
            translate: true,
            jit_report: false,
            efuse: EfuseIdentity::default(),
            time_grade: TimeGrade::default(),
            strict: false,
            strict_grade: None,
            strict_grade_blocks: None,
            trace: None,
            trace_blocks: Vec::new(),
            uart0: Uart0Sink::default(),
            uart0_source: None,
            uart0_script: None,
            uart0_baud: None,
            usb_sj: UsbSjSink::default(),
            usb_sj_tried: UsbSjSink::default(),
            usb_sj_source: None,
            usb_script_source: None,
            usb_host: UsbHost::Absent,
            usb_sj_drain: UsbSjDrain::default(),
            control: None,
            usb_script: Vec::new(),
            pin_script: PinScript::new(),
            wires: Vec::new(),
            seed: 0,
            flash: crate::flash::FlashBacking::Blank,
            flash_len: crate::flash::DEFAULT_FLASH_LEN,
            boot_set: true,
            peripherals: Vec::new(),
            dump_frames: FrameSink::default(),
            pin_log: PinLogSink::default(),
            tx_log: TxLogSink::default(),
            strip: StripConfig::default(),
            rmt_logs: false,
        }
    }

    /// The map and the ROM with no peripherals at all. Every MMIO access is
    /// unmapped until [`peripheral`](Self::peripheral) adds a block.
    pub fn bare() -> Self {
        Self {
            boot_set: false,
            ..Self::new()
        }
    }

    pub fn rom(mut self, rom: RomSource) -> Self {
        self.rom = rom;
        self
    }

    pub fn app(mut self, app: AppSource) -> Self {
        self.app = app;
        self
    }

    /// Direct load (the default) or the ROM-up boot.
    ///
    /// Under [`BootMode::RomUp`] an [`AppSource`] is still accepted and is
    /// still parsed — it is just never *placed*. The ELF is the symbol table
    /// `--probe`, `--break-at` and the boot report read, and the image the
    /// cross-check compares the booted state against; the bytes the machine
    /// runs come from the chip.
    pub fn boot_mode(mut self, mode: BootMode) -> Self {
        self.boot_mode = mode;
        self
    }

    /// Why the chip is starting. Seeds `LP_CLKRST.reset_cause`, which the
    /// mask ROM reads before anything else and prints as `rst:0x..`; the
    /// firmware's recovery ledger reads the same register later.
    pub fn reset_cause(mut self, cause: ResetCause) -> Self {
        self.reset_cause = cause;
        self
    }

    /// Where the strapping pins were at reset. Seeds `GPIO.strap`, which the
    /// ROM prints as `boot:0x..` and uses to choose between the flash
    /// bootloader and its own download console.
    pub fn strap(mut self, strap: Strap) -> Self {
        self.strap = strap;
        self
    }

    /// **Perform** a reset request rather than reporting it: reboot the chip
    /// into the strap the request names, and carry on.
    ///
    /// A peripheral that asks for a reset (the RWDT's stage action, the
    /// USB-Serial-JTAG `chip_rst` a host's DTR/RTS dance drives) ends the run
    /// with [`Outcome::Reset`] by default, because until this milestone there
    /// was no boot chain to reboot **into**. There is one now, and this turns
    /// the request into the real thing: the machine goes back to the state it
    /// was built in, with the new strap and a `USB_UART_HPSYS` reset cause,
    /// and the mask ROM runs again.
    ///
    /// It is **off** by default and that is deliberate. Three merged M6
    /// scenarios read the exit code and the reported strap as their evidence,
    /// and a default that reboots would change what those transcripts mean.
    /// Only a run that asks for it gets it. What is *not* reset is everything
    /// outside the chip: the flash part keeps what the guest wrote to it, the
    /// USB host stays attached or absent as it was, and both consoles keep
    /// every byte from before the reset — a reboot is not a new process.
    pub fn reboot_on_reset(mut self, reboot: bool) -> Self {
        self.reboot_on_reset = reboot;
        self
    }

    /// Let the hart pre-decode runs of instructions and dispatch them without
    /// re-deciding what each one is. **On by default**; `--no-block-cache`
    /// turns it off.
    ///
    /// Off is the bring-up tool, the bisection tool and the identity oracle:
    /// the same binary with the cache off must produce the same `stopped
    /// after` line, the same UART bytes, the same decoded frames off the pad
    /// and the same trace. It is also the *only* way anything about a run
    /// should change, because the cache is not architectural state.
    ///
    /// It is forced off under [`BootMode::RomUp`] (M5 MD13): there the mask
    /// ROM and the real ESP-IDF second-stage bootloader run as guest code and
    /// copy segments into RAM without a `fence.i`, and we own neither, so
    /// there is nothing to hold them to the contract. RomUp is a
    /// boot-modelling path measured in hundreds of milliseconds, not a speed
    /// path, so refusing to cache there costs nothing worth having.
    pub fn block_cache(mut self, on: bool) -> Self {
        self.block_cache = on;
        self
    }

    /// May this machine install a **translated core** — `lp-emu-jit` turning
    /// the guest's own program into host code the engine runs directly?
    ///
    /// `false` is `--interpreter`, and it carries the same promise
    /// [`Esp32C6Builder::block_cache`] does and for the same reason:
    /// translated code is not architectural state, so the same binary with
    /// the translator off must produce the same `stopped after` line, the
    /// same UART bytes, the same decoded frames off the pad and the same
    /// trace. That is what makes the interpreter this milestone's differential
    /// oracle (M7 JD15).
    ///
    /// **Nothing installs a core yet.** M7's P1 lands the seam and this
    /// switch; P3 is what first puts something behind it. Until then the flag
    /// is honest and inert: it already turns off something that is not there.
    pub fn translate(mut self, on: bool) -> Self {
        self.translate = on;
        self
    }

    /// Print the translated core's own report line at the end of a run, plus
    /// the boot cost of building it (M7 JD20). Off by default; a probe, never
    /// a gate.
    pub fn jit_report(mut self, on: bool) -> Self {
        self.jit_report = on;
        self
    }

    pub fn efuse(mut self, efuse: EfuseIdentity) -> Self {
        self.efuse = efuse;
        self
    }

    pub fn time_grade(mut self, grade: TimeGrade) -> Self {
        self.time_grade = grade;
        self
    }

    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    pub fn trace(mut self, sink: Box<dyn std::io::Write + Send>, blocks: Vec<String>) -> Self {
        self.trace = Some(sink);
        self.trace_blocks = blocks;
        self
    }

    pub fn uart0(mut self, sink: Uart0Sink) -> Self {
        self.uart0 = sink;
        self
    }

    /// Deterministic host input for UART0: bytes at declared cycles
    /// ([`lp_emu_esp_common::ScriptedSource`], or anything else that is not
    /// a socket).
    pub fn uart0_source(mut self, source: Box<dyn ByteSource>) -> Self {
        self.uart0_source = Some(source);
        self
    }

    /// Deterministic host input written as a script
    /// ([`lp_emu_esp_common::ScriptedSource`]). Preferred over
    /// [`uart0_source`](Self::uart0_source) for a script, because the
    /// builder hands it the UART0 log — without which an `after "<line>"`
    /// step has nothing to watch and never fires.
    pub fn uart0_script(mut self, script: lp_emu_esp_common::ScriptedSource) -> Self {
        self.uart0_script = Some(script);
        self
    }

    /// The rate the host on UART0 sends at (`--uart0-baud`, default
    /// [`crate::periph::uart::DEFAULT_HOST_BAUD`]). UART0 carries no clock,
    /// so the auto-baud counters the mask ROM reads can only report a rate
    /// the run states; this states it. It changes what the ROM computes and
    /// writes to `UART0.clkdiv` and nothing else — a scripted byte still
    /// lands when the script says it does.
    pub fn uart0_baud(mut self, baud: u64) -> Self {
        self.uart0_baud = Some(baud);
        self
    }

    /// Where what a host **receives** on the USB link goes.
    pub fn usb_sj(mut self, sink: UsbSjSink) -> Self {
        self.usb_sj = sink;
        self
    }

    /// Where the bytes the guest handed to the USB IN endpoint that **no
    /// host took** go (the observation stream, `--usb-sj-tried`).
    pub fn usb_sj_tried(mut self, sink: UsbSjSink) -> Self {
        self.usb_sj_tried = sink;
        self
    }

    /// Deterministic host input on the USB link: bytes a host sends to the
    /// OUT endpoint at declared cycles. They land only while the host is
    /// attached and draining.
    pub fn usb_sj_source(mut self, source: Box<dyn ByteSource>) -> Self {
        self.usb_sj_source = Some(source);
        self
    }

    /// The same, as a [`lp_emu_esp_common::ScriptedSource`] the builder still
    /// has to hand the USB link's own log to.
    ///
    /// Preferred over [`usb_sj_source`](Self::usb_sj_source) for a script,
    /// because an `after "<line>"` step needs to watch what the device said
    /// — and on this link that is the USB byte stream, not UART0. A script
    /// handed over as a bare `ByteSource` sees nothing and every wait is
    /// unsatisfiable, which is a silent walk rather than an error.
    pub fn usb_script_source(mut self, script: lp_emu_esp_common::ScriptedSource) -> Self {
        self.usb_script_source = Some(script);
        self
    }

    /// The USB host's state at power-on (`--usb-host`). `Absent` is P6's
    /// machine and the default; the control channel and `--usb-script` move
    /// it from there.
    pub fn usb_host(mut self, host: UsbHost) -> Self {
        self.usb_host = host;
        self
    }

    /// Whether a client on the USB byte socket implies `open`/`close`
    /// (`--usb-sj-drain`).
    pub fn usb_sj_drain(mut self, drain: UsbSjDrain) -> Self {
        self.usb_sj_drain = drain;
        self
    }

    /// Listen for the control channel's line protocol on this address
    /// (`--control tcp:<host:port>`). See [`crate::control`].
    pub fn control(mut self, addr: impl Into<String>) -> Self {
        self.control = Some(addr.into());
        self
    }

    /// Scripted control commands at declared cycles — the deterministic
    /// twin of the control socket (`--usb-script`'s non-byte lines).
    pub fn usb_script(mut self, commands: Vec<(Cycles, ControlCommand)>) -> Self {
        self.usb_script = commands;
        self
    }

    /// Scripted pad levels at declared guest times — the deterministic twin
    /// of the `pin` control verb (`--pin-script`, plan RD6). Repeatable:
    /// a second file's steps queue behind the first's.
    pub fn pin_script(mut self, script: PinScript) -> Self {
        self.pin_script.extend(script);
        self
    }

    /// Tie two pads in the fabric before the guest starts (`--wire a:b`).
    /// `a` is the TX side. Repeatable, and transitive.
    pub fn wire(mut self, a: PadId, b: PadId) -> Self {
        self.wires.push((a, b));
        self
    }

    /// Refuse every register graded below `level` (`--strict-grade`). See
    /// [`SocBus::set_strict_grade`].
    pub fn strict_grade(mut self, level: Option<RegGrade>) -> Self {
        self.strict_grade = level;
        self
    }

    /// Narrow `--strict-grade` to a named set of blocks; `None` (the
    /// default) is every block that publishes a table. See
    /// [`SocBus::set_strict_grade_blocks`].
    pub fn strict_grade_blocks(mut self, blocks: Option<Vec<&'static str>>) -> Self {
        self.strict_grade_blocks = blocks;
        self
    }

    /// Where decoded WS281x frames go. They are always also kept in memory
    /// for [`Esp32C6Machine::frames`].
    pub fn dump_frames(mut self, sink: FrameSink) -> Self {
        self.dump_frames = sink;
        self
    }

    /// Where the raw per-edge pin log goes. Off by default.
    /// `--tx-log`: one line per frame the WiFi blob hands the MAC.
    pub fn tx_log(mut self, sink: TxLogSink) -> Self {
        self.tx_log = sink;
        self
    }

    pub fn pin_log(mut self, sink: PinLogSink) -> Self {
        self.pin_log = sink;
        self
    }

    /// How a routed pad is decoded: the wire timing and the byte order.
    pub fn strip(mut self, order: ColorOrder, timing: ChannelTiming) -> Self {
        self.strip = StripConfig { timing, order };
        self
    }

    /// Keep the RMT's per-channel pulse and word logs — the word-level
    /// oracle P1 built, which the gates compare the decoder against. Off by
    /// default: a 24-frame run holds 305,490 pulses.
    pub fn rmt_logs(mut self, keep: bool) -> Self {
        self.rmt_logs = keep;
        self
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Where the flash chip's bytes come from, and whether they go back
    /// ([`crate::flash::FlashBacking`]). The default is a blank chip that
    /// lives and dies with the machine.
    pub fn flash(mut self, backing: crate::flash::FlashBacking) -> Self {
        self.flash = backing;
        self
    }

    /// The modelled chip's size, in bytes. Defaults to
    /// [`crate::flash::DEFAULT_FLASH_LEN`], which is what the C6 boards
    /// carry and what `partitions.csv` fills exactly.
    pub fn flash_len(mut self, len: u32) -> Self {
        self.flash_len = len;
        self
    }

    /// Register a peripheral. Order matters — see
    /// [`PERIPHERAL_REGISTRATION_ORDER`].
    pub fn peripheral(mut self, base: u32, len: u32, periph: BoxedPeripheral) -> Self {
        self.peripherals.push((base, len, periph));
        self
    }

    /// A bus with the C6's regions and MMIO windows and nothing else.
    ///
    /// The memory map on its own, which is what the ROM loader's tests want
    /// and what `build` starts from.
    pub fn bare_bus() -> SocBus {
        let mut bus = SocBus::new();
        // The guest arena is flat from the lowest region base to the highest
        // region end, so declaring the whole map before adding anything is
        // what makes it one allocation that never moves. `RAM_SPANS` is
        // base-sorted and its overlap is asserted in `memmap`'s own tests.
        let lo = memmap::RAM_SPANS[0].base;
        let hi = memmap::RAM_SPANS[memmap::RAM_SPANS.len() - 1].end();
        bus.reserve_guest_span(lo, hi - lo);
        for span in memmap::RAM_SPANS {
            let region = RamRegion::new(span.name, span.base, span.len);
            let region = match span.name {
                // Mask ROM: executable code, and nothing the guest may store
                // to. A guest write here is a bug, and it should fault rather
                // than quietly take.
                "rom-mask" => region.executable().read_only(),
                "drom-mask" => region.read_only(),
                "hp-sram" | "lp-sram" => region.executable(),
                // The flash cache window holds the app's `.text` and
                // `.rodata`. Executable, and read-only until M4 gives it a
                // real controller behind it.
                "flash-cache" => region.executable().read_only(),
                "drom-window" => region.read_only(),
                other => unreachable!("memmap::RAM_SPANS gained `{other}` with no policy"),
            };
            bus.add_region(region);
        }
        for span in memmap::MMIO_WINDOWS {
            bus.add_mmio_window(span.base, span.len);
        }
        bus
    }

    pub fn build(self) -> Result<Esp32C6Machine, BuildError> {
        let Self {
            rom,
            app,
            boot_mode,
            reset_cause,
            strap,
            reboot_on_reset,
            block_cache,
            translate,
            jit_report,
            efuse,
            time_grade,
            strict,
            strict_grade,
            strict_grade_blocks,
            trace,
            trace_blocks,
            uart0,
            uart0_source,
            uart0_script,
            uart0_baud,
            usb_sj,
            usb_sj_tried,
            usb_sj_source,
            usb_script_source,
            usb_host,
            usb_sj_drain,
            control,
            usb_script,
            pin_script,
            wires,
            seed,
            flash,
            flash_len,
            boot_set,
            peripherals,
            dump_frames,
            pin_log,
            tx_log,
            strip,
            rmt_logs,
        } = self;

        let rom_image = match rom {
            RomSource::Vendored => rom::parse(rom::VENDORED_C6_ROM)?,
            RomSource::Path(p) => rom::parse_file(&p)?,
            RomSource::Bytes(b) => rom::parse(&b)?,
        };
        let app_image = match app {
            AppSource::None => None,
            AppSource::Path(p) => {
                let bytes = std::fs::read(&p)
                    .map_err(|e| BuildError::Io(format!("reading {}: {e}", p.display())))?;
                Some(rom::parse(&bytes)?)
            }
            AppSource::Bytes(b) => Some(rom::parse(&b)?),
        };

        let mut bus = Self::bare_bus();
        bus.set_matrix(Box::new(Esp32C6IntMatrix::new()));
        if let Some(sink) = trace {
            bus.trace = lp_emu_esp_common::Trace::to_sink(sink).with_block_filter(trace_blocks);
        }

        // The host streams first, so the peripherals that hold their ids can
        // be built. UART0's bytes are always tee'd into memory for
        // `--exit-on` and the snapshot, whatever else they go to.
        let uart0_log = ByteLog::new();
        let mut uart0_tcp: Option<lp_emu_esp_common::TcpHost> = None;
        // A script watches the same log the sink tees into, so an
        // `after "<line>"` step sees exactly what the device sent.
        let uart0_source = match uart0_script {
            Some(script) => {
                Some(Box::new(script.watching(uart0_log.clone())) as Box<dyn ByteSource>)
            }
            None => uart0_source,
        };
        let (inner, source): (Box<dyn ByteSink>, Box<dyn ByteSource>) = match &uart0 {
            Uart0Sink::Memory => (
                Box::new(lp_emu_esp_common::host::NullSink),
                uart0_source.unwrap_or_else(|| Box::new(lp_emu_esp_common::host::NullSource)),
            ),
            Uart0Sink::Stdout => (
                Box::new(lp_emu_esp_common::host::StdoutSink),
                uart0_source.unwrap_or_else(|| Box::new(lp_emu_esp_common::host::NullSource)),
            ),
            Uart0Sink::File(path) => {
                let file = std::fs::File::create(path)
                    .map_err(|e| BuildError::Io(format!("creating {}: {e}", path.display())))?;
                (
                    Box::new(FileSink::new(file)),
                    uart0_source.unwrap_or_else(|| Box::new(lp_emu_esp_common::host::NullSource)),
                )
            }
            Uart0Sink::Tcp(addr) => {
                let host = lp_emu_esp_common::TcpHost::listen(addr)
                    .map_err(|e| BuildError::Io(format!("listening on {addr}: {e}")))?;
                log::info!("UART0 listening on {}", host.local_addr());
                if uart0_source.is_some() {
                    log::warn!(
                        "--uart0-script is ignored with --uart0 tcp: the client is the source"
                    );
                }
                let halves = host.split();
                uart0_tcp = Some(host);
                halves
            }
        };
        let uart0_id = bus.host.add(
            "uart0",
            Box::new(TeeSink {
                log: uart0_log.clone(),
                inner,
            }),
            source,
        );

        // The USB link: `usb-sj` is what a host receives (and, from its
        // source, what it sends); `usb-sj-tried` is the observation stream.
        let usb_sink = |sink: &UsbSjSink| -> Result<Box<dyn ByteSink>, BuildError> {
            Ok(match sink {
                UsbSjSink::Memory => Box::new(lp_emu_esp_common::host::NullSink),
                UsbSjSink::Stderr => Box::new(StderrSink),
                UsbSjSink::File(path) => {
                    let file = std::fs::File::create(path)
                        .map_err(|e| BuildError::Io(format!("creating {}: {e}", path.display())))?;
                    Box::new(FileSink::new(file))
                }
                UsbSjSink::Tcp(addr) => {
                    return Err(BuildError::Io(format!(
                        "tcp:{addr} is only a destination for --usb-sj, never for \
                         --usb-sj-tried: the observation stream is an observation, not a link"
                    )));
                }
            })
        };
        let usb_sj_log = ByteLog::new();
        let mut usb_sj_tcp: Option<lp_emu_esp_common::TcpHost> = None;
        // A USB script watches the same log the USB sink tees into, so an
        // `after "<line>"` step sees exactly what a host on this link saw —
        // which is what lets one walk file replay on either link.
        let usb_sj_source = match usb_script_source {
            Some(script) => {
                Some(Box::new(script.watching(usb_sj_log.clone())) as Box<dyn ByteSource>)
            }
            None => usb_sj_source,
        };
        let (usb_inner, usb_source): (Box<dyn ByteSink>, Box<dyn ByteSource>) = match &usb_sj {
            UsbSjSink::Tcp(addr) => {
                let host = lp_emu_esp_common::TcpHost::listen(addr)
                    .map_err(|e| BuildError::Io(format!("listening on {addr}: {e}")))?;
                eprintln!("usb-sj listening on {}", host.local_addr());
                if usb_sj_source.is_some() {
                    log::warn!(
                        "--usb-script's byte lines are ignored with --usb-sj tcp: the client is \
                         the source (its control lines still apply)"
                    );
                }
                let halves = host.split();
                usb_sj_tcp = Some(host);
                halves
            }
            other => (
                usb_sink(other)?,
                usb_sj_source.unwrap_or_else(|| Box::new(lp_emu_esp_common::host::NullSource)),
            ),
        };
        let usb_sj_id = bus.host.add(
            "usb-sj",
            Box::new(TeeSink {
                log: usb_sj_log.clone(),
                inner: usb_inner,
            }),
            usb_source,
        );
        let usb_sj_tried_log = ByteLog::new();
        let usb_sj_tried_id = bus.host.add(
            "usb-sj-tried",
            Box::new(TeeSink {
                log: usb_sj_tried_log.clone(),
                inner: usb_sink(&usb_sj_tried)?,
            }),
            Box::new(lp_emu_esp_common::host::NullSource),
        );
        // UART1: the firmware never opens it; its bytes go nowhere, on
        // purpose, but the block is a real UART so a guest that does open it
        // is answered by a model rather than a table.
        let uart1_id = bus.host.add(
            "uart1",
            Box::new(lp_emu_esp_common::host::NullSink),
            Box::new(lp_emu_esp_common::host::NullSource),
        );
        let streams = crate::periph::HostStreams {
            uart0: Some(uart0_id),
            uart1: Some(uart1_id),
            usb_sj: Some(usb_sj_id),
            usb_sj_tried: Some(usb_sj_tried_id),
        };

        // Peripherals, in the declared order: the boot set first, then
        // whatever the caller added. The check is what stops a re-sort from
        // re-pointing scheduled events.
        // The flash chip and the cache MMU are shared state, not
        // peripherals: SPI1 executes commands against the chip, SPI0
        // programs the table, and the machine's fill reads both.
        let flash_handle: crate::flash::FlashHandle = std::sync::Arc::new(std::sync::Mutex::new(
            crate::flash::FlashImage::open(flash, flash_len)
                .map_err(|e| BuildError::Io(format!("opening the flash image: {e}")))?,
        ));
        let cache_handle: crate::cache::CacheHandle =
            std::sync::Arc::new(std::sync::Mutex::new(crate::cache::CacheMmu::new()));

        let mut all = if boot_set {
            crate::periph::boot_set(
                efuse,
                seed,
                streams,
                flash_handle.clone(),
                cache_handle.clone(),
                usb_host,
                reset_cause,
                strap,
            )
        } else {
            Vec::new()
        };
        all.extend(peripherals);
        check_registration_order(&all)?;
        for (base, len, periph) in all {
            bus.add_peripheral(base, len, periph);
        }

        // The ROM first, then the app on top of it: the ROM's `.bss` reaches
        // across what the app calls RAM, and the real bootloader overwrites
        // it the same way. Then the ROM's initialised data, as its startup
        // would have left it (`rom::seed_data`) — all of it above the app's
        // memory.
        let rom_segments = rom::load(&mut bus, &rom_image)?;
        let rom_data = rom::seed_data(&mut bus, &rom_image)?;
        // …and the mask ROM's own copy of those bytes, so that `_init`'s
        // unpack loop — which a ROM-up boot really runs — copies them rather
        // than the zeros the ELF leaves at their source addresses.
        let rom_data_image = rom::seed_data_image(&mut bus, &rom_image)?;
        let mut app_segments = Vec::new();
        let mut entry = rom_image.entry;
        let mut staging = loader::FlashStaging::default();
        let mut cache_fills = 0u64;
        // The ROM-up boot places nothing. The hart starts at the ROM's reset
        // vector, the chip holds a merged image, and every one of the four
        // steps below happens for real: the ROM reads the bootloader out of
        // flash over SPI1, the bootloader reads the partition table and the
        // app, programs the MMU, and jumps. An `app` given here is a symbol
        // table and a cross-check reference, never a load.
        if let Some(app) = &app_image
            && boot_mode == BootMode::Direct
        {
            app_segments = loader::load_app(&mut bus, app)?;
            loader::clear_dram2(&mut bus)?;
            entry = app.entry;
            // What the second-stage bootloader would have done: the
            // flash-resident half of the image into the chip, the cache MMU
            // programmed for it, and the ROM told how big the part is.
            staging =
                loader::stage_image_in_flash(&flash_handle, &cache_handle, &[&rom_image, app]);
            loader::seed_rom_flash_chip(&mut bus, staging.chip_size);
            // Everything the loader staged was already placed into the
            // window by `rom::load` / `load_app`; filling now proves the
            // table and the placement agree, and is what serves the window
            // from here on.
            cache_fills = crate::cache::fill(&mut bus, &flash_handle, &cache_handle) as u64;
            // The staging is what a flasher left behind, not a guest write:
            // the window has just been filled from it, so nothing is stale.
            flash_handle.lock().unwrap().take_written_blocks();
        }

        if rmt_logs {
            let index = bus.peripheral_index("RMT");
            let kept = index
                .and_then(|i| {
                    bus.with_peripheral::<crate::periph::rmt::Rmt, _>(i, |rmt, _| {
                        rmt.set_keep_logs(true)
                    })
                })
                .is_some();
            if !kept {
                return Err(BuildError::Io(
                    "rmt_logs was asked for and this machine has no RMT block".to_string(),
                ));
            }
        }

        let open_write = |path: &PathBuf| -> Result<Box<dyn std::io::Write + Send>, BuildError> {
            let file = std::fs::File::create(path)
                .map_err(|e| BuildError::Io(format!("creating {}: {e}", path.display())))?;
            Ok(Box::new(std::io::BufWriter::new(file)))
        };
        let dump: Option<Box<dyn std::io::Write + Send>> = match &dump_frames {
            FrameSink::Memory => None,
            FrameSink::Stdout => Some(Box::new(std::io::stdout())),
            FrameSink::File(path) => Some(open_write(path)?),
        };
        let pin_log_sink: Option<Box<dyn std::io::Write + Send>> = match &pin_log {
            PinLogSink::Off => None,
            PinLogSink::File(path) => Some(open_write(path)?),
        };
        let tx_log_sink: Option<Box<dyn std::io::Write + Send>> = match &tx_log {
            TxLogSink::Off => None,
            TxLogSink::Stderr => Some(Box::new(std::io::stderr())),
            TxLogSink::File(path) => Some(open_write(path)?),
        };

        bus.set_strict(strict);
        bus.set_strict_grade(strict_grade);
        bus.set_strict_grade_blocks(strict_grade_blocks);

        // Guest time is zero and the schedule is empty: the peripherals that
        // need a first event (a UART polling its host source) take it now.
        bus.set_time(0);
        bus.start_peripherals();

        let mut hart = MachineHart::new(0);
        loader::reset_hart(&mut hart, &mut bus, entry);
        hart.set_cycle_model(time_grade.cycle_model());
        // M5 MD13: a ROM-up boot runs the mask ROM and the real ESP-IDF
        // second-stage bootloader as GUEST code, and both copy segments into
        // hp-sram and jump into them with ordinary stores. Neither will ever
        // emit the `fence.i` the cache's invalidation rests on, and we own
        // neither, so the cache is off there.
        hart.set_block_cache(block_cache && boot_mode != BootMode::RomUp);
        // The address's cost, if this grade charges one. Installed after the
        // loader has placed the app and filled the window: the cache starts
        // cold, as it is at reset, and the host's placement of segments
        // costs the guest nothing.
        bus.set_memory_cost(time_grade.memory_cost());

        // `--wire a:b`, before the guest runs: a jumper is on the header
        // when the board powers up, not put there later.
        for (a, b) in &wires {
            bus.pins
                .wire(*a, *b, 0)
                .map_err(|e| BuildError::Io(format!("--wire: {e}")))?;
        }
        // A pin script's `after` needles watch the device's consoles — both
        // of them, because a pin script says nothing about which link the
        // payload prints on.
        let pin_script = pin_script
            .watching(uart0_log.clone())
            .watching(usb_sj_log.clone());
        let gpio_index = bus.peripheral_index("GPIO");
        // The other block that reads the edge stream: the RMT's receivers
        // sample the pad their input signal is routed to (M2 P3).
        let rmt_index = bus.peripheral_index("RMT");
        // The radio TX log's source block; `None` on a machine without one.
        let wifi_mac_index = bus.peripheral_index("WIFI_MAC");
        // Arm the recording only when something is listening: it ends a
        // slice early, and a machine with no `--tx-log` must run exactly the
        // run it ran before this block could record anything.
        if !matches!(tx_log, TxLogSink::Off)
            && let Some(i) = wifi_mac_index
        {
            bus.with_peripheral::<crate::periph::wifi_stub::WifiStub, _>(i, |w, _| w.arm_tx_log());
        }
        if gpio_index.is_none() && !pin_script.is_empty() {
            return Err(BuildError::Io(
                "--pin-script needs a GPIO block, and this machine has none".to_string(),
            ));
        }

        // The control channel's listener, and the block it drives. The index
        // is looked up once: it is the peripheral's identity for the whole
        // run (see PERIPHERAL_REGISTRATION_ORDER).
        let control = match control {
            Some(addr) => {
                let host = lp_emu_esp_common::TcpHost::listen(&addr)
                    .map_err(|e| BuildError::Io(format!("listening on {addr}: {e}")))?;
                eprintln!("control listening on {}", host.local_addr());
                Some(ControlChannel {
                    host,
                    partial: Vec::new(),
                })
            }
            None => None,
        };
        let usb_index = bus.peripheral_index("USB_DEVICE");
        if usb_index.is_none() && (control.is_some() || !usb_script.is_empty()) {
            return Err(BuildError::Io(
                "the control channel needs a USB_DEVICE block, and this machine has none \
                 (Esp32C6Builder::bare?)"
                    .to_string(),
            ));
        }

        let mut machine = Esp32C6Machine {
            harts: vec![hart],
            bus,
            time_grade,
            boot_mode,
            hooks: HookTable::new(),
            rom: rom_image,
            app: app_image,
            efuse,
            reset_cause,
            strap,
            reboot_on_reset,
            translate,
            jit_report,
            power_on: None,
            reboots: 0,
            seed,
            rng: seed,
            rom_segments,
            rom_data,
            rom_data_image,
            app_segments,
            uart0_log,
            usb_sj_log,
            usb_sj_tried_log,
            usb_host,
            uart0_tcp,
            usb_sj_tcp,
            usb_sj_drain,
            usb_client_connected: false,
            usb_index,
            control,
            script: usb_script.into(),
            pin_script,
            gpio_index,
            rmt_index,
            wifi_mac_index,
            tx_log: tx_log_sink,
            tx_log_lines: 0,
            tx_log_capped: false,
            air_participant: None,
            air_tx: Vec::new(),
            air_offered: 0,
            air_delivered: 0,
            air_undelivered: 0,
            air_undelivered_noted: false,
            next_pin_poll: 0,
            next_host_poll: 0,
            control_lines: 0,
            hook_calls: 0,
            idle_skips: 0,
            stop_at: None,
            flash: flash_handle,
            cache: cache_handle,
            staging,
            cache_fills,
            pins: PinObserver {
                strip,
                cpu_hz: memmap::CPU_HZ,
                state: PinState::default(),
                epoch: 0,
                dump,
                pin_log: pin_log_sink,
                pin_log_lines: 0,
                pin_log_capped: false,
            },
        };
        // The state a reboot goes back to, taken before a single
        // instruction runs. Only when a run asked to perform resets: it is
        // a whole copy of guest memory (~17 MiB), and a run that will never
        // reboot should not pay for it.
        // Before the power-on snapshot, so a reboot restores the stated
        // rate rather than the default: `--uart0-baud` describes the host on
        // the other end of the wire, and that host does not change when the
        // chip resets.
        if let Some(baud) = uart0_baud
            && let Some(i) = machine.bus.peripheral_index("UART0")
        {
            machine
                .bus
                .with_peripheral::<crate::periph::uart::Uart, _>(i, |u, _| u.set_host_baud(baud));
        }
        if reboot_on_reset {
            machine.power_on = Some(machine.snapshot());
        }
        machine.sync_translated_core();
        Ok(machine)
    }
}

/// One decoded frame as a JSON line — the `--dump-frames` record.
///
/// `wire` is what the wire carried and `rgb` is that unpermuted with the
/// configured order: the frame the *driver* was handed. Both are in the
/// record on purpose (M5 P2), so a wrong order assumption is a visible
/// difference between two fields rather than a silent one inside `rgb`.
pub fn frame_record(frame: &Frame, strip: StripConfig, routed: Option<&RouteSource>) -> String {
    let signal = match routed {
        Some(RouteSource::Signal(sig, _)) => crate::regs::output_signals::output_signal_name(sig.0)
            .map_or_else(|| format!("sig{}", sig.0), str::to_string),
        Some(RouteSource::GpioOut) => "GPIO_OUT".to_string(),
        None => "unrouted".to_string(),
    };
    let us = |cycles: Cycles| cycles as f64 / memmap::CYCLES_PER_US as f64;
    let reset = match frame.reset_cycles {
        Some(c) => format!("{:.3}", us(c)),
        None => "null".to_string(),
    };
    format!(
        "{{\"kind\":\"ws281x-frame\",\"pad\":{},\"signal\":\"{signal}\",\"n\":{},\
         \"start_us\":{:.3},\"end_us\":{:.3},\"bits\":{},\"leds\":{},\
         \"wire\":\"{}\",\"rgb\":\"{}\",\"errors\":{},\"trailing_bits\":{},\
         \"reset_us\":{reset},\"complete\":{}}}",
        frame.pad.0,
        frame.n,
        us(frame.start),
        us(frame.end),
        frame.bits,
        frame.leds(),
        hex(&frame.wire),
        hex(&unpermute(&frame.wire, strip.order)),
        frame.error_count,
        frame.trailing_bits,
        frame.is_complete(),
    )
}

/// Lowercase hex, no separators — the shape `[ORACLE] rgb=` and the S3's
/// frame-dump line already use.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// The control channel's listener plus the tail of a line that arrived in
/// pieces. One reply per command, `\n`-terminated, in [`crate::control`]'s
/// grammar — never an `M!` frame.
struct ControlChannel {
    host: lp_emu_esp_common::TcpHost,
    partial: Vec<u8>,
}

/// The longest control line the channel will assemble. A client that sends
/// more without a newline is answered once and resynchronised, rather than
/// growing a buffer for as long as it keeps typing.
const MAX_CONTROL_LINE: usize = 4 << 10;

/// A `ByteSink` over the host's stderr — the `--usb-sj stderr` observation
/// channel, kept off stdout so it never mixes with `--uart0 stdout`.
struct StderrSink;

impl ByteSink for StderrSink {
    fn write(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(bytes);
        let _ = err.flush();
    }
}

/// A `ByteSink` over a file handle. Buffered: UART0 drains one byte at a
/// time in emulated time, and a `write(2)` per byte was 4-5 % of a run.
/// `BufWriter` flushes on [`ByteSink::flush`] and when the machine drops.
struct FileSink(std::io::BufWriter<std::fs::File>);

impl FileSink {
    fn new(file: std::fs::File) -> Self {
        Self(std::io::BufWriter::new(file))
    }
}

impl ByteSink for FileSink {
    fn write(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let _ = self.0.write_all(bytes);
    }

    fn flush(&mut self) {
        use std::io::Write;
        let _ = self.0.flush();
    }
}

fn check_registration_order(peripherals: &[(u32, u32, BoxedPeripheral)]) -> Result<(), BuildError> {
    let mut cursor = 0usize;
    let mut previous = String::new();
    for (_, _, p) in peripherals {
        let name = p.name();
        let Some(offset) = PERIPHERAL_REGISTRATION_ORDER[cursor..]
            .iter()
            .position(|d| *d == name)
        else {
            return if PERIPHERAL_REGISTRATION_ORDER.contains(&name) {
                Err(BuildError::RegistrationOrder {
                    name: name.to_string(),
                    after: previous,
                })
            } else {
                Err(BuildError::UndeclaredPeripheral {
                    name: name.to_string(),
                })
            };
        };
        cursor += offset + 1;
        previous = name.to_string();
    }
    Ok(())
}

/// The ESP32-C6, as a machine.
///
/// `harts` is a slot list with one entry (plan PD6): the C6 has one core, and
/// the shape is what lets a second chip's machine reuse this loop without
/// reshaping it. `bus.sched` is the machine's schedule — it lives on the bus
/// because that is where the peripherals that write to it are.
pub struct Esp32C6Machine {
    pub harts: Vec<MachineHart<SocBus>>,
    pub bus: SocBus,
    time_grade: TimeGrade,
    boot_mode: BootMode,
    hooks: HookTable,
    rom: ElfImage,
    app: Option<ElfImage>,
    efuse: EfuseIdentity,
    reset_cause: ResetCause,
    strap: Strap,
    reboot_on_reset: bool,
    /// May a translated core be installed on this machine's hart
    /// ([`Esp32C6Builder::translate`])? `false` is `--interpreter`.
    translate: bool,
    /// Print the translated core's report line and the boot cost of building
    /// it ([`Esp32C6Builder::jit_report`]).
    jit_report: bool,
    /// The machine as it was built, kept only when
    /// [`Esp32C6Builder::reboot_on_reset`] asked for it — a reboot is a
    /// restore of this.
    power_on: Option<Snapshot>,
    /// How many reset requests this run performed.
    reboots: u64,
    seed: u64,
    rng: u64,
    rom_segments: Vec<PlacedSegment>,
    rom_data: Vec<rom::SeededSection>,
    rom_data_image: rom::DataImage,
    app_segments: Vec<PlacedAppSegment>,
    uart0_log: ByteLog,
    usb_sj_log: ByteLog,
    usb_sj_tried_log: ByteLog,
    /// The USB host's state at power-on (P3's control channel moves it from
    /// there; the machine records where it started).
    usb_host: UsbHost,
    /// The UART0 listener, when the sink is `Tcp`; held so it lives as long
    /// as the machine and so a runner can ask whether a client ever came.
    uart0_tcp: Option<lp_emu_esp_common::TcpHost>,
    /// The USB byte-socket listener, when `--usb-sj tcp:` was chosen.
    usb_sj_tcp: Option<lp_emu_esp_common::TcpHost>,
    usb_sj_drain: UsbSjDrain,
    /// Whether a client was on the byte socket at the last poll — the edge
    /// the coupling rule watches.
    usb_client_connected: bool,
    /// `USB_DEVICE`'s peripheral index, the control channel's target.
    usb_index: Option<usize>,
    control: Option<ControlChannel>,
    /// Scripted pad levels still to drive (`--pin-script`).
    pin_script: PinScript,
    /// `GPIO`'s peripheral index: the block the drained edges are handed to.
    gpio_index: Option<usize>,
    /// `RMT`'s peripheral index: the other block the drained edges are handed
    /// to, for its RX channels (M2 P3).
    rmt_index: Option<usize>,
    /// `WIFI_MAC`'s peripheral index: where the radio TX log's handoffs come
    /// from. `None` on a machine with no radio window.
    wifi_mac_index: Option<usize>,
    /// The radio TX log's writer, its line count and whether the cap has
    /// been announced.
    tx_log: Option<Box<dyn std::io::Write + Send>>,
    tx_log_lines: u64,
    tx_log_capped: bool,
    /// This machine's seat on an [`Air`](lp_emu_esp_common::air::Air), or
    /// `None` for a machine that is not in one.
    ///
    /// The whole off switch: nothing on the air path runs, and `WIFI_MAC`
    /// records nothing, until [`Esp32C6Machine::arm_air`] is called. See
    /// [`crate::lockstep`].
    air_participant: Option<lp_emu_esp_common::air::ParticipantId>,
    /// Frames armed since the runner last drained them.
    air_tx: Vec<RadioFrame>,
    /// Frames the air has offered this machine. **P1 counts them and does
    /// not deliver them**: writing one into the receiver's RX ring, filling
    /// an `rx_ctrl` header and raising the RX interrupt is M4 P2's, and this
    /// counter is what makes the pair runner testable before P2 exists.
    air_offered: u64,
    /// Frames actually written into the guest's RX ring, and frames the air
    /// offered that could not be. `air_undelivered_noted` keeps the warning
    /// to one line per run; the counts go in the run's summary.
    air_delivered: u64,
    air_undelivered: u64,
    air_undelivered_noted: bool,
    /// The guest-cycle grid a pin script's unresolved `after` is looked at
    /// on. Guest time, so two runs resolve at the same cycle.
    next_pin_poll: Cycles,
    /// Scripted control commands still to apply, in file order.
    script: std::collections::VecDeque<(Cycles, ControlCommand)>,
    /// The next guest cycle at which the sockets are polled. Guest time,
    /// like everything else: `--control` changes when a run notices the
    /// outside, never how fast the machine runs.
    next_host_poll: Cycles,
    /// Control lines applied so far, for the exit report.
    control_lines: u64,
    hook_calls: u64,
    idle_skips: u64,
    /// Set by a hook that answered [`HookResult::Stop`]; the run loop ends
    /// with [`Outcome::Breakpoint`] at that pc.
    stop_at: Option<u32>,
    /// The flash chip SPI1 drives.
    flash: crate::flash::FlashHandle,
    /// The cache MMU SPI0 programs, and the window's page table.
    cache: crate::cache::CacheHandle,
    /// What the direct load put into flash and mapped.
    staging: loader::FlashStaging,
    /// Pages refilled from flash since the machine was built.
    cache_fills: u64,
    /// The pads: a decoder per routed pad, the frames, and the two sinks.
    pins: PinObserver,
}

impl Esp32C6Machine {
    // ---- what it is made of -------------------------------------------

    /// Which of the plan's two boot paths this machine took.
    pub fn boot_mode(&self) -> BootMode {
        self.boot_mode
    }

    pub fn time_grade(&self) -> TimeGrade {
        self.time_grade
    }

    /// The flash chip SPI1 drives.
    pub fn flash(&self) -> &crate::flash::FlashHandle {
        &self.flash
    }

    /// The cache MMU behind the `0x4200_0000` window.
    pub fn cache(&self) -> &crate::cache::CacheHandle {
        &self.cache
    }

    /// What the direct load put into flash and mapped.
    pub fn flash_staging(&self) -> &loader::FlashStaging {
        &self.staging
    }

    /// Pages refilled from flash since the machine was built (the initial
    /// fill included).
    pub fn cache_fills(&self) -> u64 {
        self.cache_fills
    }

    /// What the guest asked the flash to do.
    pub fn flash_census(&self) -> crate::flash::FlashCensus {
        self.flash.lock().unwrap().command_census()
    }

    /// Write the flash image back if its backing says to.
    pub fn flush_flash(&mut self) -> std::io::Result<bool> {
        self.flash.lock().unwrap().flush()
    }

    /// Refill any window page whose mapping or backing bytes moved.
    ///
    /// Called once per slice. Almost always a `bool` check and nothing else:
    /// only a write to `mmu_item_content`, a page-mode change or a flash
    /// write under a mapped page puts anything in the list.
    fn refill_cache(&mut self) {
        // A flash write anywhere may fall under a mapped page.
        let written = self.flash.lock().unwrap().take_written_blocks();
        if !written.is_empty() {
            let mut cache = self.cache.lock().unwrap();
            let page_len = cache.page_len();
            for block in written {
                // A 64 KiB flash block can hold several pages at a finer
                // page mode; mark each.
                let base = block * crate::flash::BLOCK_LEN;
                let mut at = base;
                while at < base + crate::flash::BLOCK_LEN {
                    cache.invalidate_page_at(at);
                    at += page_len;
                }
            }
        }
        if !self.cache.lock().unwrap().has_dirty() {
            return;
        }
        self.cache_fills += crate::cache::fill(&mut self.bus, &self.flash, &self.cache) as u64;
    }

    /// What the hart's pre-decoded block cache did, or `None` when it was
    /// never built (`--no-block-cache`, or a `--boot rom-up` run).
    pub fn block_stats(&self) -> Option<lp_emu_core::BlockStats> {
        self.harts[0].block_stats()
    }

    /// Whether a translated core may be installed on the hart at all.
    ///
    /// `false` is `--interpreter`, the translator's free oracle: the same
    /// binary, the same image, every instruction interpreted, and a
    /// transcript that must match byte for byte (M7 JD15).
    ///
    /// Nothing installs a core yet — M7 P3 is what first does — so today this
    /// reports the policy and [`Esp32C6Machine::sync_translated_core`]
    /// enforces it.
    pub fn translate(&self) -> bool {
        self.translate
    }

    /// Whether `--jit-report` asked for the translated core's report line and
    /// the boot cost of building it (M7 JD20).
    pub fn jit_report(&self) -> bool {
        self.jit_report
    }

    /// The installed translated core's report line, if there is one and
    /// `--jit-report` asked for it.
    pub fn translated_core_report(&self) -> Option<String> {
        self.jit_report
            .then(|| self.harts[0].translated_core_report())
            .flatten()
    }

    /// Hold the hart to this machine's translation policy.
    ///
    /// With `--interpreter` there must be no core installed, whatever else
    /// happened — a snapshot restore, a reboot, or a translation event that
    /// should never have fired. Called wherever a core could have appeared,
    /// so the flag is enforced rather than merely consulted.
    pub fn sync_translated_core(&mut self) {
        if !self.translate {
            for hart in &mut self.harts {
                hart.clear_translated_core();
            }
        }
    }

    /// Whether the hart is allowed to pre-decode blocks at all.
    pub fn block_cache(&self) -> bool {
        self.harts[0].block_cache()
    }

    /// `fence.i` instructions the guest retired. One per JIT publish is what
    /// the firmware's own fence should produce
    /// (`lpvm_native::rt_jit::buffer::JitBuffer::from_code`); a run that
    /// compiled a shader and saw none means that fence is not reaching here.
    pub fn fence_i_count(&self) -> u64 {
        self.harts[0].fence_i_count()
    }

    pub fn efuse(&self) -> EfuseIdentity {
        self.efuse
    }

    pub fn reset_cause(&self) -> ResetCause {
        self.reset_cause
    }

    /// The strapping the chip was started with, or the one the last reboot
    /// used.
    pub fn strap(&self) -> Strap {
        self.strap
    }

    /// How many reset requests this run performed
    /// ([`Esp32C6Builder::reboot_on_reset`]).
    pub fn reboots(&self) -> u64 {
        self.reboots
    }

    /// Reboot into `strap`, as a chip does when something asserts its reset.
    ///
    /// The machine goes back to the state it was built in and the two
    /// registers the mask ROM reads before anything else are re-seeded: the
    /// strap it prints as `boot:0x..` and chooses a path with, and the reset
    /// cause it prints as `rst:0x..`, which is `USB_UART_HPSYS` because
    /// every producer of a reset request on this chip is the serial bridge or
    /// a watchdog and the ROM has one code for "not a power-on" that the
    /// firmware maps to `user-reset`.
    ///
    /// The consoles keep their bytes: a restore would put back the empty logs
    /// the machine was built with, and a boot log that lost everything before
    /// the reset would be a worse record than one that has both boots in it.
    pub fn reboot(&mut self, strap: Strap) -> bool {
        let Some(power_on) = self.power_on.clone() else {
            return false;
        };
        let (uart0, usb_sj, tried) = (
            self.uart0_log.bytes(),
            self.usb_sj_log.bytes(),
            self.usb_sj_tried_log.bytes(),
        );
        self.restore(&power_on);
        self.uart0_log.replace(&uart0);
        self.usb_sj_log.replace(&usb_sj);
        self.usb_sj_tried_log.replace(&tried);

        self.strap = strap;
        self.reset_cause = ResetCause::UsbUartHpSys;
        let strap_word = loader::strap_word(strap);
        let cause = self.reset_cause.rom_code();
        if let Some(i) = self.bus.peripheral_index("GPIO") {
            self.bus
                .with_peripheral::<periph::gpio::Gpio, _>(i, |g, _| g.set_strap(strap_word));
        }
        if let Some(i) = self.bus.peripheral_index("LP_CLKRST") {
            self.bus
                .with_peripheral::<lp_emu_esp_common::RegFile, _>(i, |r, _| {
                    r.poke(LP_CLKRST_RESET_CAUSE, cause)
                });
        }
        // The cache MMU is shared state outside the snapshot, like the flash
        // part — but unlike the part it is *inside* the chip, so a reset
        // clears it. The ROM's `Cache_MMU_Init` would rewrite every entry
        // anyway; doing it here is what makes the two statements agree.
        {
            let mut mmu = self.cache.lock().unwrap();
            for index in 0..crate::cache::ENTRIES as u32 {
                mmu.set_entry(index, 0);
            }
            mmu.take_dirty();
        }
        // The two host-poll deadlines are ABSOLUTE guest cycles and they live
        // outside the snapshot, so a restore that puts the clock back to zero
        // leaves them in the guest's future — for as long as the board had
        // been up. Until the guest re-earns those cycles the machine reads no
        // control line, writes no reply and moves no host byte.
        //
        // MEASURED (plan two M5, `emu serve` over the WebSocket door): a
        // board 33 s of guest time old went silent for ~6 s of wall after
        // esptool-js's download-mode dance; one 120 s old went silent for
        // ~3.5 minutes, and the replies then arrived in a burst stamped 1 ms
        // after the reset. The stall is proportional to uptime, which is the
        // signature of exactly this. A reset is the moment a flasher needs
        // the chip MOST, so the deadlines go back with the clock.
        self.next_host_poll = 0;
        self.next_pin_poll = 0;
        // Same class, one layer up: the port's open/closed state IS in the
        // snapshot (USB_DEVICE's `HostState`) and the byte client's
        // connectedness is not, and the coupling only fires on an EDGE. So a
        // restore can leave the two disagreeing with nothing to reconcile
        // them. Re-derive this from BOTH sides, so the next poll (which the
        // deadline above has just made due) sees an edge exactly when one is
        // needed and none when it is not.
        //
        // Both terms are load-bearing, and each is a bug that was actually
        // observed:
        //
        // * The PORT term. A board whose power-on state was "cable in, port
        //   closed" comes back closed. Without it this would still say a
        //   client is attached, no edge would fire, and nothing would re-open
        //   the port — host bytes stage forever and the chip is deaf to the
        //   flasher that just reset it.
        // * The CLIENT term. `--usb-host attached` — `emu serve`'s default —
        //   powers on with the port OPEN and no byte client at all. Without
        //   it a reboot would claim a client that does not exist, and the
        //   very next poll would see `connected=false` against it and issue
        //   the matching `close` — slamming shut the port the restore had
        //   just opened, on a board nobody had touched. That is a `reset`
        //   through the door going quiet afterwards, and it is a race with
        //   the guest's own boot: sometimes the hello beats the close out.
        //
        // Together they read: "the port is open BECAUSE a client has it
        // open", which is the only state the coupling is entitled to assume
        // it already applied.
        let client = self
            .usb_sj_tcp
            .as_ref()
            .is_some_and(|tcp| tcp.client_connected());
        self.usb_client_connected = client && self.usb_sj_open();
        self.reboots += 1;
        true
    }

    /// Is the USB-Serial-JTAG port open — a host attached AND draining the IN
    /// endpoint? That pair is what "an application has the port open" means
    /// on this chip, and it is the state the byte socket's coupling mirrors.
    fn usb_sj_open(&mut self) -> bool {
        let Some(index) = self.usb_index else {
            return false;
        };
        self.bus
            .with_peripheral::<UsbSerialJtag, _>(index, |u, _| {
                u.host().attached() && u.host().draining()
            })
            .unwrap_or(false)
    }

    pub fn rom(&self) -> &ElfImage {
        &self.rom
    }

    pub fn app(&self) -> Option<&ElfImage> {
        self.app.as_ref()
    }

    pub fn rom_segments(&self) -> &[PlacedSegment] {
        &self.rom_segments
    }

    /// The ROM's initialised-data sections seeded after its segments
    /// ([`rom::seed_data`]).
    pub fn rom_data(&self) -> &[rom::SeededSection] {
        &self.rom_data
    }

    /// What `rom::seed_data_image` reconstructed of the mask ROM's own
    /// data image.
    pub fn rom_data_image(&self) -> rom::DataImage {
        self.rom_data_image
    }

    pub fn app_segments(&self) -> &[PlacedAppSegment] {
        &self.app_segments
    }

    pub fn hooks(&self) -> &HookTable {
        &self.hooks
    }

    pub fn hooks_mut(&mut self) -> &mut HookTable {
        &mut self.hooks
    }

    /// Stop the run when `symbol` (app first, then ROM; resolved like a
    /// `--probe`) is entered, with the guest untouched. The bring-up
    /// question "who calls this, with what in `a0..a7`" answered without a
    /// debugger.
    pub fn break_at(&mut self, symbol: &str) -> Result<u32, RomError> {
        let address = self
            .resolve_symbol(symbol)
            .ok_or_else(|| RomError::NoSuchSymbol(symbol.to_string()))?;
        // The hook table wants a `'static` name for the `--hooks` listing.
        let name: &'static str = Box::leak(symbol.to_string().into_boxed_str());
        self.hooks
            .install_at(&mut self.bus, address, name, |_| HookResult::Stop)
    }

    /// The NUL-terminated printable string at `address`, up to `limit`
    /// bytes, or `None` if the bytes there are not text. For a break
    /// report: `a0` at `ets_printf` is a format string worth reading.
    pub fn peek_string(&mut self, address: u32, limit: usize) -> Option<String> {
        let mut out = Vec::new();
        for i in 0..limit as u32 {
            let word = self.peek_word(address.wrapping_add(i) & !3)?;
            let byte = (word >> (8 * (address.wrapping_add(i) & 3))) as u8;
            if byte == 0 {
                break;
            }
            if !(byte.is_ascii_graphic() || byte == b' ' || byte == b'\n' || byte == b'\t') {
                return None;
            }
            out.push(byte);
        }
        (!out.is_empty()).then(|| String::from_utf8_lossy(&out).into_owned())
    }

    /// `(mcause, mepc, mtval)` — what the last trap said. At a `--break-at
    /// ExceptionHandler` these name the instruction that faulted.
    pub fn trap_csrs(&self) -> (u32, u32, u32) {
        let csr = self.harts[0].csr();
        (csr.mcause, csr.mepc, csr.mtval)
    }

    /// The hart's integer registers, `x0..x31`.
    pub fn registers(&self) -> [u32; 32] {
        let regs = self.harts[0].regs();
        let mut out = [0u32; 32];
        for (i, r) in out.iter_mut().enumerate() {
            *r = regs[i] as u32;
        }
        out
    }

    /// How many times a ROM hook has stood in for a routine.
    pub fn hook_calls(&self) -> u64 {
        self.hook_calls
    }

    /// How many times the hart parked in `wfi` and the machine moved guest
    /// time to the next event — the deterministic idle skip. The count is
    /// the evidence that the idle hook was reached.
    pub fn idle_skips(&self) -> u64 {
        self.idle_skips
    }

    /// Everything UART0 has put on the wire — bytes that left the shifter,
    /// in the guest's order. Bytes still in the TX FIFO are not here yet.
    pub fn uart0(&self) -> &ByteLog {
        &self.uart0_log
    }

    /// Everything a **host received** on the USB-Serial-JTAG link: IN
    /// packets a draining host took, in the guest's order. With no host, or
    /// the port closed, this stays empty — see [`usb_sj_tried`](Self::usb_sj_tried).
    pub fn usb_sj(&self) -> &ByteLog {
        &self.usb_sj_log
    }

    /// Everything the guest handed to the USB IN endpoint that **no host
    /// took**: pushed with no host attached (esp-println's `[INIT] …`
    /// lines), dropped into a committed FIFO (io_task's probe), or dropped
    /// by a bus reset. An observation of what it *tried*, never guest output
    /// that reached anyone.
    pub fn usb_sj_tried(&self) -> &ByteLog {
        &self.usb_sj_tried_log
    }

    /// The USB host's state at power-on.
    pub fn usb_host(&self) -> UsbHost {
        self.usb_host
    }

    /// The UART0 TCP listener, when `Uart0Sink::Tcp` was chosen.
    pub fn uart0_tcp(&self) -> Option<&lp_emu_esp_common::TcpHost> {
        self.uart0_tcp.as_ref()
    }

    /// The RMT block, read-only, through the bus's peripheral downcast
    /// (`Peripheral::as_any`, the matrix precedent). `None` on a bare
    /// machine that registered no RMT.
    fn rmt(&self) -> Option<&periph::rmt::Rmt> {
        let index = self.bus.peripheral_index("RMT")?;
        self.bus.peripheral(index)?.as_any()?.downcast_ref()
    }

    /// Every pulse RMT TX channel `ch` has put on its signal, in guest
    /// time — the observation until P2's fabric routes it to a pad.
    pub fn rmt_pulses(&self, ch: usize) -> &[periph::rmt::Pulse] {
        self.rmt().map(|r| r.pulses(ch)).unwrap_or(&[])
    }

    /// Every word channel `ch` fetched from the RMT RAM, with its cycle;
    /// STOP words included, so a frame reads `data … latch STOP`.
    pub fn rmt_words(&self, ch: usize) -> &[(Cycles, u32)] {
        self.rmt().map(|r| r.words(ch)).unwrap_or(&[])
    }

    /// `tx_end`s raised on channel `ch`.
    pub fn rmt_frames_ended(&self, ch: usize) -> usize {
        self.rmt().map(|r| r.frames_ended(ch)).unwrap_or(0)
    }

    /// What channel `ch`'s refills cost, in words consumed — the emulator's
    /// own reading of the race the guest's `[WS281X]` line reports from the
    /// other side. Reported, never gated (D13/PD9); see
    /// [`periph::rmt::RefillStats`].
    pub fn rmt_refill_stats(&self, ch: usize) -> periph::rmt::RefillStats {
        self.rmt().map(|r| r.refill_stats(ch)).unwrap_or_default()
    }

    // ---- the pads ------------------------------------------------------

    /// Every pad the guest has routed, ascending, with what it is routed to.
    pub fn routed_pads(&self) -> Vec<(PadId, RouteSource)> {
        self.pins
            .state
            .routed
            .iter()
            .map(|(p, r)| (PadId(*p), *r))
            .collect()
    }

    /// The frames decoded on `pad`. Empty for a pad nothing routed.
    pub fn frames(&self, pad: u8) -> &[Frame] {
        self.pins
            .state
            .frames
            .get(&pad)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Edges seen on `pad`.
    pub fn pin_edges(&self, pad: u8) -> u64 {
        self.pins.state.edges.get(&pad).copied().unwrap_or(0)
    }

    /// What is decoded, for a snapshot and for a test that wants to compare
    /// two runs.
    pub fn pin_state(&self) -> &PinState {
        &self.pins.state
    }

    /// How each pad is being read.
    pub fn strip(&self) -> StripConfig {
        self.pins.strip
    }

    /// One line per routed pad: `pin gpio18: 22 frames, 22 complete, 0
    /// errors, 256 leds`. What the CLI prints at exit.
    pub fn pin_summaries(&self) -> Vec<String> {
        self.pins
            .state
            .routed
            .keys()
            .map(|pad| {
                let frames = self.frames(*pad);
                let complete = frames.iter().filter(|f| f.is_complete()).count();
                let errors: u64 = frames.iter().map(|f| f.error_count).sum();
                let leds = frames.last().map(Frame::leds).unwrap_or(0);
                format!(
                    "pin gpio{pad}: {} frames, {complete} complete, {errors} errors, {leds} leds",
                    frames.len()
                )
            })
            .collect()
    }

    /// End of the run: a frame still open on a pad is reported incomplete.
    ///
    /// Not part of [`run_until`](Self::run_until), because a run can be
    /// resumed and closing a frame that is still being transmitted would
    /// invent one. The CLI calls it before its summary; so does a test that
    /// wants the last frame.
    pub fn flush_frames(&mut self) {
        let at = self.cycles();
        let pads: Vec<u8> = self.pins.state.decoders.keys().copied().collect();
        for pad in pads {
            let Some(frame) = self
                .pins
                .state
                .decoders
                .get_mut(&pad)
                .and_then(|d| d.flush(at))
            else {
                continue;
            };
            self.record_frame(pad, frame);
        }
        if let Some(w) = self.pins.dump.as_mut() {
            let _ = w.flush();
        }
        if let Some(w) = self.pins.pin_log.as_mut() {
            let _ = w.flush();
        }
    }

    /// Take the slice's edges off the fabric and feed them to the pads'
    /// decoders and the pin log.
    ///
    /// Called at every slice boundary, in guest-cycle order. An edge can be
    /// stamped slightly ahead of the boundary (the RMT emits a word's two
    /// halves at the fetch, each at the cycle it starts); that is a timestamp
    /// the decoder reads, never a reordering.
    fn drain_pins(&mut self) {
        let epoch = self.bus.pins.route_epoch();
        if epoch != self.pins.epoch {
            self.pins.epoch = epoch;
            let routes: Vec<(PadId, RouteSource)> = self
                .bus
                .pins
                .routes()
                .map(|(pad, route)| (pad, route.source))
                .collect();
            self.pins.state.routed.clear();
            for (pad, source) in routes {
                self.pins.state.routed.insert(pad.0, source);
                let strip = self.pins.strip;
                let cpu_hz = self.pins.cpu_hz;
                self.pins
                    .state
                    .decoders
                    .entry(pad.0)
                    .or_insert_with(|| Ws281xDecoder::new(pad, strip.timing(), cpu_hz));
            }
        }
        let edges = self.bus.pins.take_edges();
        if edges.is_empty() {
            return;
        }
        // The GPIO block reads the same stream the pin log and the decoders
        // do, so `status` latches exactly the edges that were on the wire —
        // including two in one slice.
        if let Some(index) = self.gpio_index {
            self.bus
                .with_peripheral::<Gpio, _>(index, |g, cx| g.observe_edges(&edges, cx));
        }
        // …and so does the RMT: a receiver samples the pad its input signal
        // is routed to, out of the same stream, so a word in the RX RAM and a
        // line in the pin log can never disagree about the wire.
        if let Some(index) = self.rmt_index {
            self.bus
                .with_peripheral::<crate::periph::rmt::Rmt, _>(index, |r, cx| {
                    r.observe_edges(&edges, cx)
                });
        }
        for edge in edges {
            let pad = edge.pad.0;
            *self.pins.state.edges.entry(pad).or_default() += 1;
            self.write_pin_log(&edge);
            let Some(frame) = self
                .pins
                .state
                .decoders
                .get_mut(&pad)
                .and_then(|d| d.feed(&edge))
            else {
                continue;
            };
            self.record_frame(pad, frame);
        }
    }

    /// The radio TX log: the frames the WiFi blob handed the MAC, as bytes.
    ///
    /// Drained at a slice boundary, because the bytes live in guest RAM and
    /// a peripheral cannot read it. `WIFI_MAC` records the arming PLCP0
    /// write ([`wifi_stub::TxHandoff`]) and yields; this reads the
    /// descriptor and the buffer and writes one line.
    ///
    /// # What a line says, and how much of it is a reading
    ///
    /// ```text
    /// 1036433.581 tx desc=0x4081de88 dw0=0xc0110060 buf=0x4081defc next=0x00000000 \
    ///     size=96 len=60 hdr=3c00000000000000 frame=d0000000ffffffffffff…
    /// ```
    ///
    /// - `desc` is `0x4080_0000 | (plcp0 & 0xf_ffff)`
    ///   ([`wifi_stub::TX_DESC_PTR_MASK`]) — arithmetic, corroborated by
    ///   `lmacTxFrame`'s own argument on the observed run.
    /// - `dw0`/`buf`/`next` are the three words **as read**, no
    ///   interpretation. That the twelve bytes are a `{word, buffer, link}`
    ///   descriptor is established by the RX ring, whose ten elements chain
    ///   through `next` at twelve-byte spacing.
    /// - `size` is `dw0 & 0xfff`. *Modeled*: on the RX ring that field is
    ///   1,700 against a 1,708-byte buffer stride, which is what a capacity
    ///   would look like. The other twelve bits of `dw0` are **not** decoded
    ///   here, because no reading of them survives both descriptors.
    /// - `len` is the buffer's first word — 60 on the observed frame, which
    ///   is the 56 bytes of 802.11 frame plus a four-byte FCS the buffer
    ///   does not carry, and the same value `mac_tx_set_plcp1` wrote to
    ///   `WIFI_MAC+0x5488`. `hdr` is the eight bytes it sits in and `frame`
    ///   is what follows, `len - 4` of them.
    ///
    /// A descriptor whose `len` does not fit its buffer is not forced into
    /// that shape: the line becomes `raw=` with the first bytes verbatim, so
    /// a frame this reading does not cover is visible rather than silently
    /// mis-printed.
    fn drain_tx_log(&mut self) {
        if self.tx_log.is_none() && self.air_participant.is_none() {
            return;
        }
        let Some(index) = self.wifi_mac_index else {
            return;
        };
        let handoffs = self
            .bus
            .with_peripheral::<crate::periph::wifi_stub::WifiStub, _>(index, |w, _| {
                w.take_tx_handoffs()
            })
            .unwrap_or_default();
        for handoff in handoffs {
            // Read the descriptor and the buffer **once**: the same bytes
            // feed the `--tx-log` line and the air, so a pair run and a
            // logged run can never disagree about what was sent.
            let read = self.read_tx_frame(&handoff);
            if self.air_participant.is_some()
                && let Some(bytes) = read.frame
            {
                self.air_tx.push(RadioFrame {
                    at: handoff.at,
                    bytes,
                });
            }
            if self.tx_log.is_none() {
                continue;
            }
            let line = read.line;
            let Some(w) = self.tx_log.as_mut() else {
                return;
            };
            if self.tx_log_lines >= TX_LOG_LINE_CAP {
                if !self.tx_log_capped {
                    self.tx_log_capped = true;
                    let _ = writeln!(
                        w,
                        "# tx log cap ({TX_LOG_LINE_CAP} lines) reached; later frames are not logged"
                    );
                }
                return;
            }
            self.tx_log_lines += 1;
            let _ = writeln!(w, "{line}");
        }
    }

    /// One armed frame, read out of guest RAM: the log's line, and the
    /// frame's own bytes when the descriptor's reading holds.
    ///
    /// See [`drain_tx_log`](Self::drain_tx_log) for what each field of the
    /// line means and how much of it is a reading. `frame` is `None` for
    /// exactly the cases the line reports as `unreadable=` or `raw=`: an air
    /// carries a frame this machine could read *as a frame*, or it carries
    /// nothing, and it never invents a length.
    fn read_tx_frame(&mut self, handoff: &crate::periph::wifi_stub::TxHandoff) -> ReadTxFrame {
        let us = handoff.at as f64 / memmap::CYCLES_PER_US as f64;
        let desc = handoff.descriptor();
        let head = format!(
            "{us:.3} tx desc={desc:#010x} plcp0={:#010x} pc={:#010x}",
            handoff.plcp0, handoff.pc
        );
        let (Some(dw0), Some(buf), Some(next)) = (
            self.peek_word(desc),
            self.peek_word(desc.wrapping_add(4)),
            self.peek_word(desc.wrapping_add(8)),
        ) else {
            return ReadTxFrame::unreadable(format!("{head} unreadable=descriptor"));
        };
        let size = dw0 & 0xfff;
        let head = format!("{head} dw0={dw0:#010x} buf={buf:#010x} next={next:#010x} size={size}");
        let Some(len) = self.peek_word(buf) else {
            return ReadTxFrame::unreadable(format!("{head} unreadable=buffer"));
        };
        // The frame sits at buf + 8 and runs `len - 4` bytes: the length
        // word counts the FCS the MAC appends and the buffer does not hold.
        // Anything that does not fit is printed raw rather than shaped.
        let fits = (4..=size.min(TX_LOG_BYTE_CAP)).contains(&len) && len + 4 <= size;
        if !fits {
            let raw = self.peek_bytes(buf, size.min(64));
            return ReadTxFrame::unreadable(format!("{head} len={len} raw={}", hex(&raw)));
        }
        let hdr = self.peek_bytes(buf, 8);
        let frame = self.peek_bytes(buf + 8, len - 4);
        ReadTxFrame {
            line: format!("{head} len={len} hdr={} frame={}", hex(&hdr), hex(&frame)),
            frame: Some(frame),
        }
    }

    // ---- the air seam (M4 P1) -------------------------------------------
    //
    // Three calls, all driven from *outside* the machine by
    // [`crate::lockstep`] or by the `--air` socket: take this machine's seat,
    // drain what its radio armed, and offer it what someone else's did. The
    // machine never sees the air and the air never sees the machine, which is
    // the same seam `pins` draws for pads.

    /// Give this machine seat `id` on an air, and start `WIFI_MAC` recording
    /// the frames its blob arms.
    ///
    /// Idempotent, and the only thing that turns the air path on. A machine
    /// this is never called on is byte-for-byte the machine that came before
    /// this seam existed: [`drain_tx_log`](Self::drain_tx_log) returns at its
    /// first line and `WIFI_MAC` never asks for a slice boundary.
    pub fn arm_air(&mut self, id: lp_emu_esp_common::air::ParticipantId) {
        self.air_participant = Some(id);
        if let Some(index) = self.wifi_mac_index {
            self.bus
                .with_peripheral::<crate::periph::wifi_stub::WifiStub, _>(index, |w, _| {
                    w.arm_air()
                });
        }
    }

    /// This machine's seat, or `None` when it is not in an air.
    pub fn air_participant(&self) -> Option<lp_emu_esp_common::air::ParticipantId> {
        self.air_participant
    }

    /// The frames this machine's radio armed since the last call, in the
    /// order it armed them. Empty unless [`arm_air`](Self::arm_air) was.
    pub fn take_air_frames(&mut self) -> Vec<RadioFrame> {
        std::mem::take(&mut self.air_tx)
    }

    /// Offer this machine a frame from the air — and **deliver** it (M4 P2):
    /// take a descriptor off the guest's own RX ring, write an `rx_ctrl`
    /// header and the frame into its buffer, hand the descriptor back and
    /// raise the radio interrupt.
    ///
    /// This runs at a **slice boundary**, where the machine holds guest
    /// memory and the interrupt lines and no peripheral holds a borrow — the
    /// mirror of [`drain_tx_log`](Self::drain_tx_log), which reads guest RAM
    /// at the same place for the same reason.
    ///
    /// # What is the guest's and what is ours
    ///
    /// The **ring** is entirely the guest's: its base is the address the
    /// blob wrote to `WIFI_MAC+0x4084`, its descriptors are the twelve-byte
    /// records the blob chained, and the buffers are its own heap. Nothing
    /// here allocates anything or assumes an address.
    ///
    /// The **interrupt source** and its **clear** are the guest's too — M4
    /// P1 observed both. The **raise** is ours: no silicon has been watched
    /// doing it, and an air that raises an interrupt is originating an event
    /// rather than reproducing one. The README says so by name.
    ///
    /// Everything else — that the header sits at `buf[0]`, what goes in the
    /// descriptor's undetermined bits, which event bits to raise — is
    /// **chosen**, and [`crate::periph::wifi_stub`]'s docs say which of them
    /// the guest was observed checking.
    pub fn offer_air_frame(&mut self, frame: &lp_emu_esp_common::air::AirFrame) {
        self.air_offered += 1;
        let outcome = self.deliver_air_frame(&frame.bytes);
        match &outcome {
            AirDelivery::Delivered { .. } => self.air_delivered += 1,
            _ => {
                self.air_undelivered += 1;
                // Once, and never silently: a run whose ring filled up says
                // so in its summary, and the first one says why.
                if !self.air_undelivered_noted {
                    self.air_undelivered_noted = true;
                    log::warn!(
                        "air: {} could not be delivered into the RX ring ({}); \
                         later ones are counted, not logged",
                        frame.bytes.len(),
                        outcome.why()
                    );
                }
            }
        }
        if self.bus.trace.is_enabled() {
            let line = format!(
                "cyc={} AIR {} {} bytes from {} (sent at cyc={})",
                self.cycles(),
                outcome.why(),
                frame.bytes.len(),
                frame.from,
                frame.at
            );
            self.bus.trace.note(&line);
        }
    }

    /// The delivery itself. See [`offer_air_frame`](Self::offer_air_frame).
    ///
    /// # The walk
    ///
    /// From **the block's own write cursor** — `WifiStub::rx_write_cursor`,
    /// the descriptor the modelled DMA will fill next, which starts at the
    /// base the blob programmed and is stepped by every fill — follow `next`
    /// while the hardware still owns the descriptor (`dw0[31]`) and its
    /// buffer is big enough for the header and the frame (`dw0[11:0]`, the
    /// size field M4 P0 corroborated against the ring's 1,708-byte buffer
    /// stride). The chain **ends** — the tenth descriptor's `next` is NULL,
    /// it does not wrap — so a walk that runs out of owned descriptors is the
    /// ring being full, and that is counted rather than papered over.
    ///
    /// **Why a cursor and not the base** — this is
    /// `docs/debt/emu-c6-air-delivers-every-other-frame.md`, and it cost the
    /// guest every second frame. The blob recycles a descriptor it has
    /// consumed by moving it to the **tail** of its chain (`owner` back to 1,
    /// `next` to NULL, the old tail linked to it) and advancing
    /// `RX_DMA_BASE_OFFSET` one ISR **later**. For the width of that window
    /// the base still names a descriptor the guest has already read. A walk
    /// that starts at the base lands in it on every second frame: it writes
    /// into the ring's tail, publishes that tail's NULL as
    /// `RX_DSCR_NEXT_OFFSET`, and the guest — which M4 P2 had already
    /// recorded refusing a cursor of 0 — never surfaces the frame. Real DMA
    /// holds a pointer and steps it, so this does.
    ///
    /// A cursor that has gone stale is not trusted: if the walk from it
    /// reaches the chain's end without finding a descriptor to fill, the walk
    /// is retried once **from the base**, so a guest that re-posts its ring
    /// somewhere else is followed rather than stranded.
    ///
    /// [`RX_RING_WALK_CAP`] bounds the walk: a `next` chain the guest
    /// corrupted must not spin the emulator.
    ///
    /// # The descriptor's undetermined bits
    ///
    /// `owner` (`[31]`) is **cleared** — the `lldesc` convention is that the
    /// hardware gives a descriptor back that way, and it is the only bit in
    /// the word whose reading M4 P0 called consistent across both
    /// descriptors. `eof` (`[30]`) is **set**: one frame, one descriptor,
    /// and the TX side's single-descriptor chain had it set. `[23:12]` is
    /// written with the byte count — see [`AirDelivery`] and the README for
    /// what the guest did and did not check about that. `[11:0]` (the
    /// buffer's size) and `[28:24]` are **left exactly as the blob posted
    /// them**: the air has nothing to say about a buffer's capacity, and
    /// `[28:24]`'s meaning is M4 P0's U3, still open.
    fn deliver_air_frame(&mut self, bytes: &[u8]) -> AirDelivery {
        let Some(index) = self.wifi_mac_index else {
            return AirDelivery::NoBlock;
        };
        let Some(base) = self
            .bus
            .with_peripheral::<crate::periph::wifi_stub::WifiStub, _>(index, |w, _| w.rx_dma_base())
            .flatten()
        else {
            return AirDelivery::NoRing;
        };
        let micros = (self.cycles() / memmap::CYCLES_PER_US) as u32;
        let payload = crate::periph::wifi_stub::rx_ctrl::frame_with_header(bytes, micros);
        let need = payload.len() as u32;

        let cursor = self
            .bus
            .with_peripheral::<crate::periph::wifi_stub::WifiStub, _>(index, |w, _| {
                w.rx_write_cursor()
            })
            .flatten();
        // The cursor first, the base second: a cursor that has gone stale
        // (the guest re-posted its ring somewhere else) must not strand the
        // air, and a base that has not caught up with the guest's own
        // recycling must not be walked from while the cursor is good.
        let starts = match cursor {
            Some(c) if c != base => vec![c, base],
            _ => vec![base],
        };
        let mut outcome = AirDelivery::RingFull;
        for start in starts {
            let mut desc = start;
            let mut steps = 0;
            outcome = loop {
                if steps == RX_RING_WALK_CAP {
                    break AirDelivery::RingWalkCap;
                }
                steps += 1;
                let (Some(dw0), Some(buf), Some(next)) = (
                    self.peek_word(desc),
                    self.peek_word(desc.wrapping_add(4)),
                    self.peek_word(desc.wrapping_add(8)),
                ) else {
                    break AirDelivery::NoRing;
                };
                let owned_by_hardware = dw0 & RX_DESC_OWNER_MASK != 0;
                let size = dw0 & RX_DESC_SIZE_MASK;
                if owned_by_hardware && size >= need {
                    if !self.poke_bytes(buf, &payload) {
                        break AirDelivery::NoRing;
                    }
                    let dw0_after = (dw0 & !RX_DESC_OWNER_MASK & !RX_DESC_LEN_MASK)
                        | RX_DESC_EOF_MASK
                        | ((need << RX_DESC_LEN_SHIFT) & RX_DESC_LEN_MASK);
                    self.poke_word(desc, dw0_after);
                    // One call sets the two RX cursor registers *and* steps
                    // this block's own write cursor to `next` — one fact,
                    // one writer.
                    self.bus
                        .with_peripheral::<crate::periph::wifi_stub::WifiStub, _>(index, |w, _| {
                            w.raise_rx_interrupt(desc, next)
                        });
                    // The level: a peripheral can only reach `IrqLines`
                    // inside an access, and this is a slice boundary. The
                    // block put the bits in its event word; the machine holds
                    // the line up until the guest's write-one-to-clear
                    // empties that word.
                    self.bus.irq.set_level(crate::regs::source::WIFI_MAC, true);
                    break AirDelivery::Delivered {
                        desc,
                        buf,
                        dw0_before: dw0,
                        dw0_after,
                        len: need,
                    };
                }
                if next == 0 {
                    break AirDelivery::RingFull;
                }
                desc = next;
            };
            if !matches!(outcome, AirDelivery::RingFull) {
                break;
            }
        }
        if matches!(outcome, AirDelivery::RingFull) {
            // Nothing took it from either start, so the cursor names a
            // descriptor that is no use: park it and let the next delivery
            // begin at the base the guest has by then programmed.
            self.bus
                .with_peripheral::<crate::periph::wifi_stub::WifiStub, _>(index, |w, _| {
                    w.set_rx_write_cursor(None)
                });
        }
        outcome
    }

    /// Write `bytes` into guest memory through the bus's own decode, word by
    /// word, read-modify-writing the two ends so a delivery never disturbs a
    /// byte outside its own range. `false` if any word refused.
    fn poke_bytes(&mut self, address: u32, bytes: &[u8]) -> bool {
        let mut at = address;
        let mut rest = bytes;
        while !rest.is_empty() {
            let word_at = at & !3;
            let lane = (at - word_at) as usize;
            let take = rest.len().min(4 - lane);
            let mut word = if lane == 0 && take == 4 {
                0
            } else {
                match self.peek_word(word_at) {
                    Some(w) => w,
                    None => return false,
                }
            }
            .to_le_bytes();
            word[lane..lane + take].copy_from_slice(&rest[..take]);
            if !self.poke_word(word_at, u32::from_le_bytes(word)) {
                return false;
            }
            at += take as u32;
            rest = &rest[take..];
        }
        true
    }

    /// How many frames the air has offered this machine. See
    /// [`offer_air_frame`](Self::offer_air_frame).
    pub fn air_frames_offered(&self) -> u64 {
        self.air_offered
    }

    /// How many of those reached the guest's RX ring, and how many did not.
    /// A pair's summary prints both, because "offered" and "delivered" are
    /// different claims and the difference is the ring's end (RD10).
    pub fn air_frames_delivered(&self) -> u64 {
        self.air_delivered
    }

    pub fn air_frames_undelivered(&self) -> u64 {
        self.air_undelivered
    }

    /// `count` bytes of guest memory, word by word through the bus's decode.
    /// Short if a word does not answer.
    fn peek_bytes(&mut self, address: u32, count: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count.div_ceil(4) {
            match self.peek_word(address.wrapping_add(i * 4)) {
                Some(word) => out.extend_from_slice(&word.to_le_bytes()),
                None => break,
            }
        }
        out.truncate(count as usize);
        out
    }

    fn write_pin_log(&mut self, edge: &lp_emu_esp_common::pins::Edge) {
        let Some(w) = self.pins.pin_log.as_mut() else {
            return;
        };
        if self.pins.pin_log_lines >= PIN_LOG_LINE_CAP {
            if !self.pins.pin_log_capped {
                self.pins.pin_log_capped = true;
                let _ = writeln!(
                    w,
                    "# pin log cap ({PIN_LOG_LINE_CAP} lines) reached; later edges are not logged"
                );
            }
            return;
        }
        self.pins.pin_log_lines += 1;
        let us = edge.at as f64 / memmap::CYCLES_PER_US as f64;
        let _ = writeln!(w, "{us:.3} {} {}", edge.pad, u8::from(edge.level));
    }

    /// Keep a decoded frame, and write its record if a sink asked for one.
    fn record_frame(&mut self, pad: u8, frame: Frame) {
        if let Some(w) = self.pins.dump.as_mut() {
            let line = frame_record(&frame, self.pins.strip, self.pins.state.routed.get(&pad));
            let _ = writeln!(w, "{line}");
        }
        let frames = self.pins.state.frames.entry(pad).or_default();
        if frames.len() < FRAMES_PER_PAD_CAP {
            frames.push(frame);
        } else if self.pins.state.capped.insert(pad) {
            log::warn!(
                "pin gpio{pad}: {FRAMES_PER_PAD_CAP} frames kept in memory; later ones are \
                 decoded and dumped but not kept"
            );
        }
    }

    // ---- time ----------------------------------------------------------

    pub fn cycles(&self) -> Cycles {
        self.harts[0].cycle_count()
    }

    /// Emulated microseconds: the CPU is 160 MHz.
    pub fn micros(&self) -> u64 {
        self.cycles() / memmap::CYCLES_PER_US
    }

    pub fn instructions(&self) -> u64 {
        self.harts[0].instruction_count()
    }

    // ---- determinism ---------------------------------------------------

    /// The machine's seeded PRNG (SplitMix64). The only source of "random"
    /// in the machine, so a run with the same seed is the same run — plan
    /// PD5's determinism, extended to whatever P5's RNG peripheral needs.
    pub fn next_random(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    // ---- symbols --------------------------------------------------------

    /// What is at `pc`: the app's symbol if the app claims it, else the
    /// ROM's. Both are asked because a fault inside `memcpy` is in the ROM
    /// and a fault inside `esp_hal::init` is in the app.
    pub fn symbolize(&self, address: u32) -> Option<String> {
        let from = |image: &ElfImage| {
            image.symbol_at(address).map(|s| {
                if s.address == address {
                    s.name.clone()
                } else {
                    format!("{}+0x{:x}", s.name, address - s.address)
                }
            })
        };
        self.app.as_ref().and_then(from).or_else(|| from(&self.rom))
    }

    /// The hart's call chain, outermost last, walked through the `s0`
    /// frame-pointer chain the firmware keeps (`-C force-frame-pointers`,
    /// the ADR that keeps it for `lpc-shared`'s crash report): at every
    /// frame `ra` is at `s0 - 4` and the caller's `s0` at `s0 - 8`. Stops at
    /// the first frame pointer outside HP SRAM, or after 64 frames.
    ///
    /// What a fault report wants: a strict-bus refusal inside `core::fmt`
    /// says nothing until the frames above it say "the panic handler, called
    /// from `X`".
    pub fn backtrace(&mut self) -> Vec<(u32, String)> {
        let regs = self.harts[0].regs();
        let pc = self.harts[0].pc();
        let ra = regs[1] as u32;
        let mut s0 = regs[8] as u32;
        let mut frames = vec![pc, ra];
        for _ in 0..64 {
            let in_sram =
                s0 >= memmap::HP_SRAM_BASE + 8 && s0 <= memmap::HP_SRAM_BASE + memmap::HP_SRAM_LEN;
            if !in_sram {
                break;
            }
            let (Some(next_ra), Some(next_s0)) = (self.peek_word(s0 - 4), self.peek_word(s0 - 8))
            else {
                break;
            };
            if next_ra == 0 || next_s0 == s0 {
                break;
            }
            frames.push(next_ra);
            s0 = next_s0;
        }
        frames
            .into_iter()
            .map(|a| {
                let sym = self.symbolize(a).unwrap_or_else(|| "?".to_string());
                (a, sym)
            })
            .collect()
    }

    /// Read a word of guest memory from the host side (a `--probe`, a test).
    /// Uses the bus's own decode, so an address nothing claims answers the
    /// same way it would for the guest.
    pub fn peek_word(&mut self, address: u32) -> Option<u32> {
        let saved = self.bus.pc();
        self.bus.set_pc(0);
        let value = self.bus.read_word(address).ok().map(|v| v as u32);
        self.bus.set_pc(saved);
        value
    }

    /// Write a word of guest memory from the host side — the same decode
    /// [`peek_word`](Self::peek_word) reads through, so a register written
    /// here behaves exactly as it would for the guest.
    ///
    /// A bench's hand on the chip, and it is one on purpose: a gate that
    /// wants "this pad's input buffer is on" should turn it on the way the
    /// driver does, through `IO_MUX`, rather than by reaching into the
    /// fabric behind the block that owns the bit.
    pub fn poke_word(&mut self, address: u32, value: u32) -> bool {
        let saved = self.bus.pc();
        self.bus.set_pc(0);
        let ok = self.bus.write_word(address, value as i32).is_ok();
        self.bus.set_pc(saved);
        ok
    }

    /// Both sides of every pad the machine has anything to say about — the
    /// `pins` verb's answer, without a socket.
    pub fn pads(&self) -> Vec<PadReport> {
        self.pad_reports()
    }

    /// The word at a symbol, for `--probe <symbol>@<ms>`.
    ///
    /// Resolution: the exact ELF name first (a C symbol, or a mangled name
    /// pasted from `nm`); then the demangled path without its hash, so
    /// `esp_println::serial_jtag_printer::TIMED_OUT` finds
    /// `_ZN11esp_println19serial_jtag_printer9TIMED_OUT17h…E`; then, as a
    /// last resort, a **unique** symbol whose demangled path ends with the
    /// query, so a bare `TIMED_OUT` works when only one exists. An ambiguous
    /// short name is refused rather than guessed — the alternatives are in
    /// the log.
    pub fn peek_symbol(&mut self, name: &str) -> Option<(u32, u32)> {
        let address = self.resolve_symbol(name)?;
        self.peek_word(address).map(|v| (address, v))
    }

    /// See [`peek_symbol`](Self::peek_symbol).
    pub fn resolve_symbol(&self, name: &str) -> Option<u32> {
        if let Some(s) = self
            .app
            .as_ref()
            .and_then(|a| a.symbol(name))
            .or_else(|| self.rom.symbol(name))
        {
            return Some(s.address);
        }
        let images = self.app.iter().chain(std::iter::once(&self.rom));
        let mut path_matches = Vec::new();
        let mut suffix_matches = Vec::new();
        for image in images {
            for s in image.symbols() {
                let mut demangled = format!("{:#}", rustc_demangle::demangle(&s.name));
                // LLVM's renaming suffix on a static that was split or
                // duplicated (`…::TIMED_OUT.0`): not part of the path.
                while let Some((head, tail)) = demangled.rsplit_once('.')
                    && !tail.is_empty()
                    && tail.bytes().all(|b| b.is_ascii_digit())
                {
                    demangled = head.to_string();
                }
                if demangled == name {
                    path_matches.push((s.address, demangled));
                } else if demangled.ends_with(name)
                    && demangled[..demangled.len() - name.len()].ends_with("::")
                {
                    suffix_matches.push((s.address, demangled));
                }
            }
        }
        for (label, matches) in [("path", path_matches), ("suffix", suffix_matches)] {
            match matches.as_slice() {
                [] => {}
                [(address, _)] => return Some(*address),
                many => {
                    log::warn!(
                        "probe `{name}`: {} {label} matches, refusing to guess: {}",
                        many.len(),
                        many.iter()
                            .map(|(a, n)| format!("{n} @ {a:#010x}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    return None;
                }
            }
        }
        None
    }

    // ---- the run loop ---------------------------------------------------

    /// Run until `stop` says otherwise. See the module docs for the loop.
    pub fn run_until(&mut self, stop: &StopCondition) -> Outcome {
        let started = Instant::now();
        let stop_cycle = stop.stop_cycle.unwrap_or(u64::MAX);
        let mut probes = stop.probes.clone();
        probes.sort_by(|a, b| a.0.cmp(&b.0));
        let mut next_probe = 0usize;
        // One search anchor per console: UART0, then the USB link.
        let mut matched = [0usize; 2];

        loop {
            let now = self.cycles();

            while next_probe < probes.len() && probes[next_probe].0 <= now {
                let (at, name) = probes[next_probe].clone();
                self.report_probe(at, &name);
                next_probe += 1;
            }
            if now >= stop_cycle {
                return Outcome::Deadline { cycle: now };
            }

            let mut deadline = stop_cycle.min(now.saturating_add(MAX_SLICE_CYCLES));
            if let Some(event) = self.bus.sched.next_deadline() {
                deadline = deadline.min(event.max(now + 1));
            }
            if let Some((at, _)) = probes.get(next_probe) {
                deadline = deadline.min((*at).max(now + 1));
            }
            if let Some(at) = self.next_host_service(now) {
                deadline = deadline.min(at.max(now + 1));
            }
            if self.bus.strict() {
                deadline = deadline.min(now + STRICT_SLICE_CYCLES);
            }
            let budget = deadline.saturating_sub(now).max(1);

            self.bus.set_time(now);
            let end = self.harts[0].run_slice(&mut self.bus, budget);

            match end {
                SliceEnd::BudgetExhausted => {}
                // A peripheral asked for the machine's attention before the
                // next instruction — today, the cache MMU after an entry
                // write. Everything below the match is that attention.
                SliceEnd::BusYield => {}
                SliceEnd::Wfi => {
                    // The deterministic idle skip: nothing can happen before
                    // the next scheduled event, so move guest time there —
                    // or before the next scripted command or socket poll,
                    // which is the only thing that keeps a run with an idle
                    // guest and a live control channel able to hear it.
                    let mut wake = self
                        .bus
                        .sched
                        .next_deadline()
                        .or_else(|| self.bus.host.next_ready())
                        .unwrap_or(stop_cycle);
                    if let Some(at) = self.next_host_service(now) {
                        wake = wake.min(at);
                    }
                    let wake = wake.max(self.cycles() + 1).min(stop_cycle);
                    self.harts[0].advance_to_cycle(wake);
                    self.idle_skips += 1;
                }
                SliceEnd::Ebreak { pc } => {
                    if !self.serve_breakpoint(pc) {
                        self.harts[0].deliver_breakpoint(pc);
                    }
                }
                SliceEnd::Fault(fault) => {
                    // A strict refusal that turned into a double fault is
                    // still a strict refusal, and naming the access is more
                    // useful than naming the vector.
                    if let Some(violation) = self.bus.first_strict_violation() {
                        return Outcome::StrictBus { violation };
                    }
                    return Outcome::Fault {
                        cycle: self.cycles(),
                        pc: self.harts[0].pc(),
                        fault,
                    };
                }
            }

            // Fire everything due, then resample the matrix and poll: an
            // event may have raised a line, and only the machine can tell
            // the hart about one that happened between slices.
            let at = self.cycles();
            self.bus.run_due_events(at);
            // The pads: whatever the slice put on the wire, in cycle order.
            self.drain_pins();
            // The radio: any frame the blob armed during the slice, read out
            // of guest RAM now that a peripheral's borrow has ended.
            self.drain_tx_log();
            // The host's side, at a slice boundary and never inside one: a
            // scripted command due by now, then — on the poll cadence — the
            // byte socket's client edge and the control channel's lines.
            self.service_host(at);
            let external = self.bus.pending_cpu_interrupt();
            self.harts[0].set_external(external);
            self.harts[0].poll_interrupts();
            // A peripheral's event may also have written a register the
            // hart's side-band would have reported; consume it so the next
            // slice's entry poll is not answering a stale flag.
            let _ = self.bus.take_sideband();
            self.refill_cache();
            // The block cache's emulator-side funnel, drained where the
            // window refill it usually follows already is. Everything that
            // writes guest code from the HOST side comes through
            // `SocBus::load_image` — a flash-cache MMU page fill, a ROM-hook
            // `ebreak` patch, an ELF segment — and none of it is followed by
            // a guest `fence.i`, because none of it is the guest. Almost
            // every slice this is one `is_empty` test: P1 counted 98 cache
            // refills across a whole render run.
            if self.bus.code_writes_pending() {
                for (lo, hi) in self.bus.take_code_writes() {
                    self.harts[0].invalidate_block_range(lo, hi);
                }
            }

            if let Some(violation) = self.bus.first_strict_violation() {
                return Outcome::StrictBus { violation };
            }
            if let Some(pc) = self.stop_at.take() {
                return Outcome::Breakpoint {
                    cycle: self.cycles(),
                    pc,
                };
            }
            if let Some(lp_emu_esp_common::MachineRequest::Reset { source, at, strap }) =
                self.bus.take_request()
            {
                if self.reboot_on_reset && self.reboot(strap) {
                    log::info!("machine: {source} at cycle {at} — rebooting into strap {strap}");
                    matched = [0usize; 2];
                    continue;
                }
                return Outcome::Reset {
                    cycle: at,
                    source,
                    strap,
                };
            }
            if let Some(needle) = &stop.exit_on
                && let Some(cycle) = self.exit_on_match(needle, &mut matched)
            {
                return Outcome::ExitMatched { cycle };
            }
            if let Some(limit) = stop.wall_timeout
                && started.elapsed() >= limit
            {
                return Outcome::WallTimeout {
                    cycle: self.cycles(),
                };
            }
        }
    }

    // ---- the host's side (M6 P3) -----------------------------------------

    /// The next cycle at which [`service_host`](Self::service_host) has
    /// something to do, or `None` when nothing outside can reach this run.
    ///
    /// A scripted command's own cycle, or the socket poll cadence. It bounds
    /// the slice and the idle skip, which is what stops a guest sitting in
    /// `wfi` from jumping over the whole script.
    fn next_host_service(&self, now: Cycles) -> Option<Cycles> {
        let scripted = self.script.front().map(|(at, _)| *at);
        let polled =
            (self.control.is_some() || self.usb_sj_tcp.is_some()).then_some(self.next_host_poll);
        let pins = self
            .pin_script
            .next_service(now, LIVE_POLL_CYCLES)
            .map(|at| at.max(self.next_pin_poll.min(at)));
        [scripted, polled, pins].into_iter().flatten().min()
    }

    /// Apply everything the host has asked for by cycle `now`.
    ///
    /// Order is deliberate: the script first (its times are the contract),
    /// then the byte socket's coupling edge, then the control channel's
    /// lines. Called only at a slice boundary, so a command never lands
    /// between two instructions of one slice, and the reply names the cycle
    /// it was drained at.
    fn service_host(&mut self, now: Cycles) {
        self.service_pins(now);
        while self.script.front().is_some_and(|(at, _)| *at <= now) {
            let (_, command) = self.script.pop_front().expect("checked");
            let reply = self.apply_control(&command, now);
            if let ControlReply::Err(reason) = &reply {
                log::warn!("--usb-script: {reason}");
            }
        }

        if self.control.is_none() && self.usb_sj_tcp.is_none() {
            return;
        }
        if now < self.next_host_poll {
            return;
        }
        self.next_host_poll = now.saturating_add(LIVE_POLL_CYCLES);

        // The coupling rule: a client on the byte socket is an application
        // with the port open. A cable is a separate thing — `attach` and
        // `detach` are never implied by a socket.
        if let Some(connected) = self.usb_sj_tcp.as_ref().map(|t| t.client_connected())
            && connected != self.usb_client_connected
        {
            self.usb_client_connected = connected;
            if self.usb_sj_drain == UsbSjDrain::Auto {
                let command = if connected {
                    ControlCommand::Open
                } else {
                    ControlCommand::Close
                };
                if let ControlReply::Err(reason) = self.apply_control(&command, now) {
                    log::warn!("--usb-sj tcp: coupling: {reason}");
                }
            }
        }

        self.service_control(now);
    }

    /// Drive every pad a `--pin-script` line is due for, then hand the
    /// edges straight on.
    ///
    /// The drain is repeated here rather than left to the next slice
    /// boundary: an edge a script caused at cycle N should raise the GPIO
    /// interrupt at cycle N, not one slice later.
    fn service_pins(&mut self, now: Cycles) {
        if self.pin_script.is_empty() {
            return;
        }
        self.next_pin_poll = now.saturating_add(LIVE_POLL_CYCLES);
        let due = self.pin_script.take_due(now);
        if due.is_empty() {
            return;
        }
        // The edge is stamped with the SCRIPT's cycle, not the boundary the
        // machine noticed it at: the file's times are the contract, and a
        // pin log that read them back a cycle or two late could not be
        // checked against the file that produced it.
        for (at, event) in due {
            self.bus.pins.drive_pad(event.pad, event.level, at);
        }
        self.bus.set_time(now);
        self.drain_pins();
    }

    /// Both sides of every pad the machine has anything to say about — what
    /// the `pins` verb answers.
    ///
    /// "Anything to say about" is: routed, input-enabled, driven from
    /// outside, or tied by a `--wire`. Listing all 31 would bury the two a
    /// run cares about.
    fn pad_reports(&self) -> Vec<PadReport> {
        let pins = &self.bus.pins;
        (0..crate::periph::gpio::PAD_COUNT as u8)
            .filter_map(|n| {
                let pad = PadId(n);
                let wired_to: Vec<u8> = pins
                    .wired_group(pad)
                    .into_iter()
                    .filter(|p| *p != pad)
                    .map(|p| p.0)
                    .collect();
                let route = pins.route_of(pad).map(|r| match r.source {
                    RouteSource::GpioOut => "gpio-out".to_string(),
                    RouteSource::Signal(sig, invert) => {
                        let name = crate::regs::output_signals::output_signal_name(sig.0)
                            .map_or_else(|| format!("sig{}", sig.0), str::to_string);
                        if invert { format!("~{name}") } else { name }
                    }
                });
                let driven = pins.driven_level(pad);
                let input_enable = pins.pad_input_enable(pad);
                if route.is_none() && driven.is_none() && !input_enable && wired_to.is_empty() {
                    return None;
                }
                Some(PadReport {
                    pad: n,
                    route,
                    input_enable,
                    driven,
                    level: pins.pad_level(pad),
                    wired_to,
                })
            })
            .collect()
    }

    /// Read whole lines off the control socket, apply each, answer each.
    fn service_control(&mut self, now: Cycles) {
        let Some(channel) = self.control.as_mut() else {
            return;
        };
        let incoming = channel.host.take_inbound();
        if incoming.is_empty() && channel.partial.is_empty() {
            return;
        }
        channel.partial.extend_from_slice(&incoming);

        let mut lines: Vec<Result<String, String>> = Vec::new();
        while let Some(at) = channel.partial.iter().position(|b| *b == b'\n') {
            let raw: Vec<u8> = channel.partial.drain(..=at).collect();
            let text = String::from_utf8_lossy(&raw[..raw.len() - 1])
                .trim_end_matches('\r')
                .trim()
                .to_string();
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            lines.push(Ok(text));
        }
        if channel.partial.len() > MAX_CONTROL_LINE {
            channel.partial.clear();
            lines.push(Err(format!(
                "a line longer than {MAX_CONTROL_LINE} bytes with no newline — dropped, and \
                 the channel resynchronised at the next newline"
            )));
        }

        for line in lines {
            let reply = match line {
                Err(reason) => ControlReply::Err(reason),
                Ok(text) => match ControlCommand::parse(&text) {
                    Ok(command) => self.apply_control(&command, now),
                    Err(reason) => ControlReply::Err(reason),
                },
            };
            let channel = self.control.as_mut().expect("held for this run");
            let mut out = reply.to_string();
            out.push('\n');
            if !channel.host.write_to_client(out.as_bytes()) {
                log::warn!("control: `{}` had nobody left to answer", out.trim_end());
            }
        }
    }

    /// Apply one control command to the USB block at guest cycle `now`.
    ///
    /// Every precondition is checked here rather than swallowed by the
    /// model: a command that could not be applied answers `err <reason>` and
    /// changes nothing, so a script that has drifted out of step is visible
    /// rather than quietly ineffective.
    fn apply_control(&mut self, command: &ControlCommand, now: Cycles) -> ControlReply {
        // The pads are the bus's, not the USB block's: a machine with no USB
        // console still has pins, and a `pin` verb on one must work.
        match command {
            ControlCommand::Pin { pad, level } => {
                self.bus.set_time(now);
                self.bus.pins.drive_pad(PadId(*pad), *level, now);
                self.drain_pins();
                self.control_lines += 1;
                if self.bus.trace.is_enabled() {
                    let line = format!("cyc={now} CONTROL pin gpio{pad} {}", u8::from(*level));
                    self.bus.trace.note(&line);
                }
                return ControlReply::Ok {
                    verb: "pin",
                    cycle: now,
                };
            }
            ControlCommand::Pins => {
                self.control_lines += 1;
                return ControlReply::Pins {
                    cycle: now,
                    pads: self.pad_reports(),
                };
            }
            _ => {}
        }
        let Some(index) = self.usb_index else {
            return ControlReply::Err("no USB_DEVICE block in this machine".to_string());
        };
        self.bus.set_time(now);
        let outcome = self
            .bus
            .with_peripheral::<UsbSerialJtag, _>(index, |u, cx| match command {
                ControlCommand::Attach => {
                    if u.host().attached() {
                        return Err("attach: a host is already attached".to_string());
                    }
                    u.attach(cx);
                    Ok(None)
                }
                ControlCommand::Detach => {
                    if !u.host().attached() {
                        return Err("detach: no host is attached".to_string());
                    }
                    u.detach(cx);
                    Ok(None)
                }
                ControlCommand::Open => {
                    if !u.host().attached() {
                        return Err("open: no host is attached (attach first — a cable is not \
                                    a port open)"
                            .to_string());
                    }
                    if u.host().draining() {
                        return Err("open: the port is already open".to_string());
                    }
                    u.open(cx);
                    Ok(None)
                }
                ControlCommand::Close => {
                    if !u.host().draining() {
                        return Err("close: the port is not open".to_string());
                    }
                    u.close(cx);
                    Ok(None)
                }
                ControlCommand::Signals { dtr, rts } => {
                    u.set_signals(*dtr, *rts, cx);
                    Ok(None)
                }
                ControlCommand::Reset | ControlCommand::DownloadMode => {
                    let download = matches!(command, ControlCommand::DownloadMode);
                    if u.chip_reset_disabled() {
                        return Err(format!(
                            "{}: USB_DEVICE chip_rst bit 2 (disable_usb_serial_chip_reset) is \
                             set — the guest has disabled the chip reset the serial channel \
                             can ask for, so it is recorded and not performed",
                            if download { "download-mode" } else { "reset" }
                        ));
                    }
                    let performed = if download {
                        u.download_mode(cx)
                    } else {
                        u.reset(cx)
                    };
                    if performed {
                        Ok(None)
                    } else {
                        Err("the chip reset was suppressed".to_string())
                    }
                }
                ControlCommand::UsbWrite(bytes) => {
                    if !u.host().attached() {
                        return Err(
                            "usb-write: no host is attached, so nothing could have sent bytes"
                                .to_string(),
                        );
                    }
                    u.host_write(bytes, cx);
                    Ok(None)
                }
                ControlCommand::State => Ok(Some(HostReport {
                    attached: u.host().attached(),
                    draining: u.host().draining(),
                    sof: u.sof_running(),
                    in_pending: u.in_fifo().len(),
                    out_queued: u.out_pending(),
                })),
                ControlCommand::Wait(_) => Err(
                    "`wait` is a --usb-script command; a client on this socket waits by waiting"
                        .to_string(),
                ),
                // Handled above: the pads are not this block's.
                ControlCommand::Pin { .. } | ControlCommand::Pins => {
                    Err("unreachable: a pin verb never reaches USB_DEVICE".to_string())
                }
            });

        match outcome {
            None => ControlReply::Err("USB_DEVICE declined the control channel".to_string()),
            Some(Err(reason)) => ControlReply::Err(reason),
            Some(Ok(host)) => {
                self.control_lines += 1;
                if self.bus.trace.is_enabled() {
                    let line = format!("cyc={now} CONTROL {}", command.verb());
                    self.bus.trace.note(&line);
                }
                match host {
                    Some(host) => ControlReply::State { cycle: now, host },
                    None => ControlReply::Ok {
                        verb: command.verb(),
                        cycle: now,
                    },
                }
            }
        }
    }

    /// The USB byte-socket listener, when `UsbSjSink::Tcp` was chosen.
    pub fn usb_sj_tcp(&self) -> Option<&lp_emu_esp_common::TcpHost> {
        self.usb_sj_tcp.as_ref()
    }

    /// The control channel's listener, when `--control` was given.
    pub fn control_tcp(&self) -> Option<&lp_emu_esp_common::TcpHost> {
        self.control.as_ref().map(|c| &c.host)
    }

    /// How many control commands have been applied (scripted and socket).
    pub fn control_lines(&self) -> u64 {
        self.control_lines
    }

    /// Scripted control commands not yet due.
    pub fn scripted_commands_left(&self) -> usize {
        self.script.len()
    }

    /// The ROM hook table's first refusal. `true` when a hook served the
    /// `ebreak` and the machine performed the `ret`.
    fn serve_breakpoint(&mut self, pc: u32) -> bool {
        let Some(hook) = self.hooks.get(pc) else {
            return false;
        };
        log::trace!("HOOK {} at {:#010x}", hook.symbol, hook.address);
        if self.bus.trace.is_enabled() {
            let line = format!("cyc={} pc={pc:#010x} HOOK {}", self.cycles(), hook.symbol);
            self.bus.trace.note(&line);
        }
        self.hook_calls += 1;
        match (hook.call)(self) {
            HookResult::Ret => {
                // `ret` is `jalr x0, 0(ra)`.
                let ra = self.harts[0].regs()[1] as u32;
                self.harts[0].set_pc(ra);
                true
            }
            HookResult::Breakpoint => false,
            HookResult::Stop => {
                self.stop_at = Some(pc);
                true
            }
        }
    }

    fn report_probe(&mut self, at: Cycles, name: &str) {
        match self.peek_symbol(name) {
            Some((address, value)) => println!(
                "probe cyc={at} us={} {name} @ {address:#010x} = {value:#010x}",
                at / memmap::CYCLES_PER_US
            ),
            None => println!("probe cyc={at} {name}: no such symbol in the app or the ROM"),
        }
    }

    /// `--exit-on`: stop at the **end of the line** the match is on, on
    /// **either console**.
    ///
    /// Both, because the shipped image's console is the USB link, not UART0:
    /// a run with `--usb-host attached` prints nothing on UART0 at all, and a
    /// sentinel that only ever looked there could never stop it. A run whose
    /// host is absent delivers nothing on the USB link, so the two are never
    /// both non-empty by accident, and each carries its own anchor.
    ///
    /// Not at the match. A console drains one byte at a time in emulated time
    /// and this is checked between bytes, so a needle that is a *prefix* of
    /// its line would stop the run mid-line and leave the rest of it unsent —
    /// which is how M3 P7 first recorded `[stack] heartbeat: high-water` with
    /// neither of the two figures the payload exists to report. A transcript
    /// is lines; committing a truncated one would be worse than not stopping.
    ///
    /// If the newline never arrives the run goes on to its deadline, which is
    /// the safe direction: a run that ran too long says so in its own report,
    /// while a capture cut in half looks like data.
    ///
    /// The search is over **bytes**, not `str`. A console is a byte stream —
    /// the flash-backed image's `[BOOTCTL] unusable record (invalid) —
    /// booting normally` carries an em dash — and anchoring an index into a
    /// lossily-decoded `String` lands inside a multi-byte character and
    /// panics. (It did: the first `--exit-on` run of the flash-backed image
    /// in M4 stopped with "byte index 650 is not a char boundary".)
    fn exit_on_match(&self, needle: &str, from: &mut [usize; 2]) -> Option<Cycles> {
        // Borrow each log in place and scan bytes: this runs after every
        // slice, and cloning + re-decoding a console each time was
        // measurable. Console output is ASCII (see `ByteLog::text`), and
        // UTF-8 is self-synchronising, so a byte search finds exactly what
        // the lossy-text search found.
        let needle = needle.as_bytes();
        let matched = self
            .uart0_log
            .with_bytes(|text| Self::line_complete(text, needle, &mut from[0]))
            || self
                .usb_sj_log
                .with_bytes(|text| Self::line_complete(text, needle, &mut from[1]));
        matched.then(|| self.cycles())
    }

    /// Is `needle` in `text` at or after `from`, with a newline behind it?
    /// Moves the anchor as described on [`exit_on_match`](Self::exit_on_match).
    fn line_complete(text: &[u8], needle: &[u8], from: &mut usize) -> bool {
        if text.len() <= *from {
            return false;
        }
        let found = if needle.is_empty() {
            Some(0)
        } else {
            text[*from..]
                .windows(needle.len())
                .position(|w| w == needle)
        };
        match found.map(|i| *from + i) {
            Some(at) if text[at + needle.len()..].contains(&b'\n') => true,
            // Matched, but the line is still arriving: hold the anchor here so
            // the next byte re-checks this same match rather than the tail.
            Some(at) => {
                *from = at;
                false
            }
            // Keep the search anchored so a long run does not rescan the whole
            // console every slice; back off by the needle so a match split
            // across two slices is still found.
            None => {
                *from = text.len().saturating_sub(needle.len());
                false
            }
        }
    }

    // ---- snapshot -------------------------------------------------------

    /// Everything a run's future depends on. See [`Snapshot`].
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            harts: self.harts.clone(),
            regions: self.bus.save_regions(),
            periph: self.bus.save_peripherals(),
            scalars: self.bus.save_scalars(),
            sched: self.bus.sched.save(),
            matrix: self.bus.matrix().save_state(),
            rng: self.rng,
            hook_calls: self.hook_calls,
            uart0: self.uart0_log.bytes(),
            usb_sj: self.usb_sj_log.bytes(),
            usb_sj_tried: self.usb_sj_tried_log.bytes(),
            pins: self.pins.state.clone(),
        }
    }

    /// Put the machine back. Watchpoints are re-armed from the restored
    /// hart's trigger CSRs, because they live on the bus and the bus does not
    /// know they came from a hart.
    pub fn restore(&mut self, s: &Snapshot) {
        // The block cache is not architectural state and is absent from a
        // snapshot: `Clone for MachineHart` hands back an empty one, which is
        // the whole of "restore invalidates all". A translated core rides on
        // exactly the same rule, and for a sharper reason — it holds host
        // code compiled from guest bytes the restored regions are about to
        // replace. The bus's pending code writes go with it: they described
        // the machine that was.
        self.harts.clone_from(&s.harts);
        self.bus.restore_regions(&s.regions);
        let _ = self.bus.take_code_writes();
        self.bus.restore_peripherals(&s.periph);
        self.bus.restore_scalars(&s.scalars);
        self.bus.sched.restore(&s.sched);
        self.bus.matrix_mut().load_state(&s.matrix);
        self.rng = s.rng;
        self.hook_calls = s.hook_calls;
        self.uart0_log.replace(&s.uart0);
        self.usb_sj_log.replace(&s.usb_sj);
        self.usb_sj_tried_log.replace(&s.usb_sj_tried);
        // The decoders and their frames, including one caught mid-bit; the
        // routing itself came back with the bus's scalars.
        self.pins.state = s.pins.clone();
        self.pins.epoch = self.bus.pins.route_epoch();

        for slot in 0..TRIGGER_COUNT {
            let wp = self.harts[0].triggers().watchpoint(slot);
            self.bus.set_watchpoint(slot, wp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::RegFile;

    #[test]
    fn a_rom_only_machine_boots_at_the_reset_vector_with_mie_set() {
        let m = Esp32C6Builder::new().build().unwrap();
        assert_eq!(m.harts.len(), 1, "PD6: a slot list, one hart");
        assert_eq!(m.harts[0].pc(), memmap::ROM_MASK_BASE);
        assert_eq!(
            m.harts[0].csr().mstatus,
            lp_riscv_emu::mach::csr::MSTATUS_BOOT
        );
        assert!(
            m.harts[0].csr().mie_enabled(),
            "mstatus.MIE must be 1 at entry — nothing in the esp-hal stack sets it"
        );
        assert!(m.harts[0].allow_unaligned() && m.bus.allow_unaligned());
        assert_eq!(m.cycles(), 0);
        assert!(m.hooks().is_empty(), "the table ships empty");
    }

    #[test]
    fn the_time_grades_are_the_three_cycle_models_and_their_configuration_names() {
        assert_eq!(TimeGrade::T1.cycle_model(), CycleModel::InstructionCount);
        assert_eq!(TimeGrade::T2.cycle_model(), CycleModel::Esp32C6);
        assert_eq!(TimeGrade::T3.cycle_model(), CycleModel::Esp32C6Kernels);
        assert_eq!(TimeGrade::T1.configuration(), "lp-emu:esp32c6:t1");
        assert_eq!(TimeGrade::T2.configuration(), "lp-emu:esp32c6:t2");
        assert_eq!(TimeGrade::T3.configuration(), "lp-emu:esp32c6:t3");
        assert_eq!(TimeGrade::parse("t3"), Ok(TimeGrade::T3));
        assert!(TimeGrade::parse("t4").is_err());

        let m = Esp32C6Builder::new()
            .time_grade(TimeGrade::T2)
            .build()
            .unwrap();
        assert_eq!(m.harts[0].cycle_model(), CycleModel::Esp32C6);
    }

    /// The memory-cost hook is installed by the grade and by nothing else.
    /// `t1` and `t2` are `None` **by construction**, which is the whole of
    /// why their cycle counts cannot move (M1 P3 G3-3).
    #[test]
    fn only_t3_installs_a_memory_cost_model() {
        for (grade, expected) in [
            (TimeGrade::T1, false),
            (TimeGrade::T2, false),
            (TimeGrade::T3, true),
        ] {
            assert!(grade.memory_cost().is_some() == expected, "{grade:?}");
            let m = Esp32C6Builder::new().time_grade(grade).build().unwrap();
            assert_eq!(m.bus.has_memory_cost(), expected, "{grade:?} on the bus");
        }
    }

    #[test]
    fn micros_are_cycles_over_one_sixty() {
        let mut m = Esp32C6Builder::new().build().unwrap();
        m.harts[0].advance_to_cycle(160_000);
        assert_eq!(m.micros(), 1_000);
        assert_eq!(StopCondition::after_micros(5).stop_cycle, Some(800));
    }

    #[test]
    fn the_rom_is_in_the_map_and_its_symbols_are_reachable_by_name() {
        let mut m = Esp32C6Builder::new().build().unwrap();
        assert_eq!(m.rom_segments().len(), 4);
        // The linker script's `rtc_get_reset_reason` address is the ROM's
        // trampoline slot; the body is elsewhere. Both are real code.
        assert_ne!(m.peek_word(0x4000_0018).unwrap(), 0);
        assert_ne!(m.peek_word(0x4001_9680).unwrap(), 0);
        assert_eq!(
            m.symbolize(0x4000_0018).as_deref(),
            Some("__call_rtc_get_reset_reason")
        );
        assert_eq!(
            m.symbolize(0x4001_9682).as_deref(),
            Some("rtc_get_reset_reason+0x2")
        );
    }

    #[test]
    fn a_guest_store_into_the_mask_rom_is_refused() {
        let mut m = Esp32C6Builder::new().build().unwrap();
        assert!(m.bus.write_word(0x4001_9680, 0).is_err());
    }

    #[test]
    fn the_registration_order_is_checked_not_just_documented() {
        // In order: fine.
        let ok = Esp32C6Builder::bare()
            .peripheral(0x6000_8000, 0x100, Box::new(RegFile::new("TIMG0", 0x100)))
            .peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)))
            .build();
        assert!(ok.is_ok());

        // Reversed: refused, because event ids pack the index.
        let bad = Esp32C6Builder::bare()
            .peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)))
            .peripheral(0x6000_8000, 0x100, Box::new(RegFile::new("TIMG0", 0x100)))
            .build();
        assert!(matches!(bad, Err(BuildError::RegistrationOrder { .. })));

        let undeclared = Esp32C6Builder::bare()
            .peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("NOPE", 0x100)))
            .build();
        assert!(matches!(
            undeclared,
            Err(BuildError::UndeclaredPeripheral { .. })
        ));
    }

    #[test]
    fn the_declared_order_has_no_duplicates() {
        let mut seen = std::collections::BTreeSet::new();
        for name in PERIPHERAL_REGISTRATION_ORDER {
            assert!(seen.insert(*name), "`{name}` is listed twice");
        }
    }

    #[test]
    fn the_boot_set_is_registered_in_the_declared_order_and_a_bare_machine_has_none() {
        let m = Esp32C6Builder::new().build().unwrap();
        let names: Vec<&str> = (0..m.bus.peripheral_count())
            .map(|i| m.bus.peripheral(i).unwrap().name())
            .collect();
        // Every registered block is in the declared order, in that order.
        let mut cursor = 0;
        for name in &names {
            let at = PERIPHERAL_REGISTRATION_ORDER[cursor..]
                .iter()
                .position(|d| d == name)
                .unwrap_or_else(|| panic!("`{name}` out of order"));
            cursor += at + 1;
        }
        // And the declared order names nothing the boot set lacks.
        let missing: Vec<&&str> = PERIPHERAL_REGISTRATION_ORDER
            .iter()
            .filter(|d| !names.contains(d))
            .collect();
        assert!(
            missing.is_empty(),
            "declared but not registered: {missing:?}"
        );
        assert!(matches!(
            m.bus.matrix().as_any().downcast_ref::<Esp32C6IntMatrix>(),
            Some(_)
        ));

        let bare = Esp32C6Builder::bare().build().unwrap();
        assert_eq!(bare.bus.peripheral_count(), 0);
    }

    #[test]
    fn a_reset_request_from_a_peripheral_ends_the_run_with_exit_code_two() {
        assert_eq!(
            Outcome::Reset {
                cycle: 5,
                source: "LP_WDT stage 0 (ResetSystem)",
                strap: Strap::App,
            }
            .exit_code(),
            2
        );
        let mut m = Esp32C6Builder::new().build().unwrap();
        // A guest that does nothing: `j .` in HP SRAM, so time passes
        // without the ROM's reset path (which is M7's) being run.
        m.bus
            .load_image(memmap::HP_SRAM_BASE, &0x0000_006fu32.to_le_bytes())
            .unwrap();
        m.harts[0].set_pc(memmap::HP_SRAM_BASE);
        // Arm the RWDT from the host side the way the firmware does: unlock,
        // hold of one tick, wdt_en + stage 0 = ResetSystem, lock.
        let base = memmap::periph::LP_WDT;
        m.bus.write_word(base + 0x18, 0x50D8_3AA1).unwrap();
        m.bus.write_word(base + 0x04, 1).unwrap();
        m.bus
            .write_word(base + 0x00, (1u32 << 31 | 4 << 28) as i32)
            .unwrap();
        m.bus.write_word(base + 0x18, 0).unwrap();
        let out = m.run_until(&StopCondition::after_micros(10_000));
        assert!(
            matches!(
                out,
                Outcome::Reset {
                    source: "LP_WDT stage 0 (ResetSystem)",
                    strap: Strap::App,
                    ..
                }
            ),
            "{out:?}"
        );
    }

    #[test]
    fn the_seeded_rng_is_the_same_run_twice_and_a_different_one_at_a_new_seed() {
        let draw = |seed| {
            let mut m = Esp32C6Builder::new().seed(seed).build().unwrap();
            (0..4).map(|_| m.next_random()).collect::<Vec<_>>()
        };
        assert_eq!(draw(7), draw(7));
        assert_ne!(draw(7), draw(8));
    }

    /// A machine whose guest is `j .` in HP SRAM: time passes and nothing
    /// touches a register, so what the control channel does is the only
    /// thing in the run.
    fn idle_machine() -> Esp32C6Machine {
        let mut m = Esp32C6Builder::new().build().unwrap();
        m.bus
            .load_image(memmap::HP_SRAM_BASE, &0x0000_006fu32.to_le_bytes())
            .unwrap();
        m.harts[0].set_pc(memmap::HP_SRAM_BASE);
        m
    }

    #[test]
    fn a_control_command_is_applied_at_the_cycle_its_reply_names() {
        let mut m = idle_machine();
        assert_eq!(m.usb_host(), UsbHost::Absent);

        // Absent: no frames, nothing held, and `open` is refused because a
        // cable is not a port open.
        let ControlReply::State { host, .. } = m.apply_control(&ControlCommand::State, 0) else {
            panic!("state answers a state");
        };
        assert_eq!(
            host,
            HostReport {
                attached: false,
                draining: false,
                sof: false,
                in_pending: 0,
                out_queued: 0,
            }
        );
        assert!(matches!(
            m.apply_control(&ControlCommand::Open, 0),
            ControlReply::Err(_)
        ));

        // The cable, then the port.
        assert_eq!(
            m.apply_control(&ControlCommand::Attach, 320).to_string(),
            "ok attach cyc=320 us=2"
        );
        assert_eq!(
            m.apply_control(&ControlCommand::Open, 480).to_string(),
            "ok open cyc=480 us=3"
        );
        let ControlReply::State { host, .. } = m.apply_control(&ControlCommand::State, 640) else {
            panic!("state answers a state");
        };
        assert!(host.attached && host.draining && host.sof);

        // Host bytes with no socket behind them reach the OUT endpoint.
        assert_eq!(
            m.apply_control(&ControlCommand::UsbWrite(b"M!x\n".to_vec()), 800)
                .to_string(),
            "ok usb-write cyc=800 us=5"
        );
        let ControlReply::State { host, .. } = m.apply_control(&ControlCommand::State, 800) else {
            panic!("state answers a state");
        };
        assert_eq!(host.out_queued, 4, "staged for the guest to read");
        assert_eq!(m.control_lines(), 6);
    }

    #[test]
    fn a_command_that_cannot_be_applied_answers_err_and_changes_nothing() {
        let mut m = idle_machine();
        m.apply_control(&ControlCommand::Attach, 0);
        for (command, needle) in [
            (ControlCommand::Attach, "already attached"),
            (ControlCommand::Close, "not open"),
            (ControlCommand::Wait(5), "--usb-script"),
        ] {
            let reply = m.apply_control(&command, 160);
            let ControlReply::Err(reason) = &reply else {
                panic!(
                    "`{}` should have been refused, got {reply:?}",
                    command.verb()
                );
            };
            assert!(reason.contains(needle), "{reason:?}");
        }
        // Refused, and the host is where it was: attached, port closed.
        let ControlReply::State { host, .. } = m.apply_control(&ControlCommand::State, 160) else {
            panic!("state answers a state");
        };
        assert!(host.attached && !host.draining);
    }

    #[test]
    fn chip_rst_disable_refuses_the_reset_and_names_the_bit() {
        let mut m = idle_machine();
        m.apply_control(&ControlCommand::Attach, 0);
        // The guest sets `chip_rst.disable` (bit 2), as an image that does
        // not want a host resetting it would.
        m.bus
            .write_word(memmap::periph::USB_DEVICE + 0x4c, 0b100)
            .unwrap();

        for command in [ControlCommand::Reset, ControlCommand::DownloadMode] {
            let reply = m.apply_control(&command, 160);
            let ControlReply::Err(reason) = &reply else {
                panic!(
                    "`{}` should have been refused, got {reply:?}",
                    command.verb()
                );
            };
            assert!(reason.contains("chip_rst bit 2"), "{reason:?}");
        }
        assert!(
            m.bus.take_request().is_none(),
            "a suppressed dance asks the machine for nothing"
        );

        // Cleared again, the same command is performed.
        m.bus
            .write_word(memmap::periph::USB_DEVICE + 0x4c, 0)
            .unwrap();
        assert_eq!(
            m.apply_control(&ControlCommand::DownloadMode, 320)
                .to_string(),
            "ok download-mode cyc=320 us=2"
        );
        assert!(matches!(
            m.bus.take_request(),
            Some(lp_emu_esp_common::MachineRequest::Reset {
                source: "USB_DEVICE chip_rst (serial)",
                strap: Strap::Download,
                ..
            })
        ));
    }

    /// **A reset does not invent a byte client for a port nobody opened.**
    ///
    /// `--usb-host attached` — `emu serve`'s default — powers on with the
    /// port already open and no byte client at all. [`Esp32C6Machine::reboot`]
    /// re-derives the coupling's memory of the client from the port AND the
    /// socket; from the port alone it would come back believing in a client
    /// that was never there, and the very next host poll would see
    /// `connected=false` against that belief and issue the matching `close` —
    /// slamming shut the port the restore had just opened. That is a `reset`
    /// through `emu serve`'s door racing its own board's boot for the hello,
    /// which is what `reset_reboots_the_board_and_the_server_stays_up` in
    /// `lp-cli/tests/emu_serve_door.rs` caught intermittently.
    #[test]
    fn a_reboot_does_not_invent_a_byte_client_for_an_open_port() {
        let mut m = Esp32C6Builder::new()
            .usb_host(UsbHost::Attached { draining: true })
            .reboot_on_reset(true)
            .build()
            .unwrap();

        assert!(m.usb_sj_open(), "the power-on port is open");
        assert!(!m.usb_client_connected, "and no byte client opened it");

        assert!(m.reboot(Strap::App), "the machine reboots");

        assert!(m.usb_sj_open(), "the restore puts the open port back");
        assert!(
            !m.usb_client_connected,
            "…and the coupling still remembers no client, so the next poll \
             has no falling edge to close that port with"
        );
    }

    #[test]
    fn a_scripted_command_lands_at_its_own_cycle_and_bounds_the_idle_skip() {
        // The guest here is `wfi` in a loop, so without `next_host_service`
        // the machine would jump straight to the deadline and the script
        // would never be applied. `wfi; j .`
        let mut m = Esp32C6Builder::new()
            .usb_script(vec![
                (160_000, ControlCommand::Attach),
                (320_000, ControlCommand::Open),
            ])
            .build()
            .unwrap();
        m.bus
            .load_image(memmap::HP_SRAM_BASE, &0x1050_0073u32.to_le_bytes())
            .unwrap();
        m.bus
            .load_image(memmap::HP_SRAM_BASE + 4, &0x0000_006fu32.to_le_bytes())
            .unwrap();
        m.harts[0].set_pc(memmap::HP_SRAM_BASE);

        assert_eq!(m.scripted_commands_left(), 2);
        let out = m.run_until(&StopCondition::after_micros(5_000));
        assert!(matches!(out, Outcome::Deadline { .. }), "{out:?}");
        assert_eq!(m.scripted_commands_left(), 0, "both came due");
        assert_eq!(m.control_lines(), 2);
        assert_eq!(m.usb_host(), UsbHost::Absent, "the power-on state is kept");
        let ControlReply::State { host, .. } = m.apply_control(&ControlCommand::State, m.cycles())
        else {
            panic!("state answers a state");
        };
        assert!(
            host.attached && host.draining,
            "the script attached and opened while the guest idled"
        );
    }

    #[test]
    fn an_outcome_carries_the_exit_code_the_cli_contract_promises() {
        assert_eq!(Outcome::Deadline { cycle: 0 }.exit_code(), 0);
        assert_eq!(Outcome::ExitMatched { cycle: 0 }.exit_code(), 0);
        assert_eq!(
            Outcome::Fault {
                cycle: 0,
                pc: 0,
                fault: HartFault::TrapVectorFetch { vector: 0 }
            }
            .exit_code(),
            2
        );
        assert_eq!(
            Outcome::WallTimeout { cycle: 0 }.exit_code(),
            4,
            "the wall-clock net is its own code — it is not a fault"
        );
    }
}
