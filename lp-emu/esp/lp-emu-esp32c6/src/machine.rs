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
//! The CPU runs at 160 MHz, so `micros = cycles / 160`. Two grades, both of
//! which are just the hart's cycle model:
//!
//! - `t1` (`lp-emu:esp32c6:t1`) — [`CycleModel::InstructionCount`], what the
//!   vendor emulator does.
//! - `t2` (`lp-emu:esp32c6:t2`) — [`CycleModel::Esp32C6`], the measured
//!   per-class model.
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

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use lp_emu_core::sched::Cycles;
use lp_emu_core::{Bus, CycleModel};
use lp_emu_esp_common::bus::StrictViolation;
use lp_emu_esp_common::periph::BoxedPeripheral;
use lp_emu_esp_common::{ByteLog, ByteSink, ByteSource, ElfImage, RamRegion, SocBus};
use lp_riscv_emu::mach::trigger::TRIGGER_COUNT;
use lp_riscv_emu::mach::{HartFault, MachineHart, SliceEnd};

use crate::intmatrix::Esp32C6IntMatrix;
use crate::loader::{self, EfuseIdentity, LoadError, PlacedAppSegment, ResetCause};
use crate::memmap;
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
const MAX_SLICE_CYCLES: u64 = 8_192;

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
}

impl TimeGrade {
    pub const fn cycle_model(self) -> CycleModel {
        match self {
            TimeGrade::T1 => CycleModel::InstructionCount,
            TimeGrade::T2 => CycleModel::Esp32C6,
        }
    }

    /// The validation system's configuration name (plan PD4).
    pub const fn configuration(self) -> &'static str {
        match self {
            TimeGrade::T1 => "lp-emu:esp32c6:t1",
            TimeGrade::T2 => "lp-emu:esp32c6:t2",
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "t1" => Ok(TimeGrade::T1),
            "t2" => Ok(TimeGrade::T2),
            other => Err(format!("unknown time grade `{other}` (expected t1 or t2)")),
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
    /// ROM-up boot will start from, and what the ROM tests use.
    None,
    Path(PathBuf),
    Bytes(Vec<u8>),
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

/// Where USB-Serial-JTAG's IN-endpoint bytes are observed: what the guest
/// tried to print with no host attached (esp-println's `[INIT] …` lines).
/// Always also kept in memory ([`Esp32C6Machine::usb_sj`]).
#[derive(Clone, Debug, Default)]
pub enum UsbSjSink {
    #[default]
    Memory,
    Stderr,
    File(PathBuf),
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
    /// expired with a reset action. The chip would reboot; the emulator
    /// reports it (M7 owns the boot chain). Exit code 2, like a fault —
    /// on silicon this is `rst:0x10 (RTCWDT_RTC_RST)` in the boot log.
    Reset { cycle: Cycles, source: &'static str },
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
    efuse: EfuseIdentity,
    time_grade: TimeGrade,
    strict: bool,
    trace: Option<Box<dyn std::io::Write + Send>>,
    trace_blocks: Vec<String>,
    uart0: Uart0Sink,
    /// Scripted host input for UART0 (`--uart0-script`). Ignored when the
    /// sink is `Tcp`, whose client is the source.
    uart0_source: Option<Box<dyn ByteSource>>,
    usb_sj: UsbSjSink,
    seed: u64,
    /// Register the boot set before `peripherals`.
    boot_set: bool,
    peripherals: Vec<(u32, u32, BoxedPeripheral)>,
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
            efuse: EfuseIdentity::default(),
            time_grade: TimeGrade::default(),
            strict: false,
            trace: None,
            trace_blocks: Vec::new(),
            uart0: Uart0Sink::default(),
            uart0_source: None,
            usb_sj: UsbSjSink::default(),
            seed: 0,
            boot_set: true,
            peripherals: Vec::new(),
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

    pub fn usb_sj(mut self, sink: UsbSjSink) -> Self {
        self.usb_sj = sink;
        self
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
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
            efuse,
            time_grade,
            strict,
            trace,
            trace_blocks,
            uart0,
            uart0_source,
            usb_sj,
            seed,
            boot_set,
            peripherals,
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
                    Box::new(FileSink(file)),
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

        let usb_sj_log = ByteLog::new();
        let usb_inner: Box<dyn ByteSink> = match &usb_sj {
            UsbSjSink::Memory => Box::new(lp_emu_esp_common::host::NullSink),
            UsbSjSink::Stderr => Box::new(StderrSink),
            UsbSjSink::File(path) => {
                let file = std::fs::File::create(path)
                    .map_err(|e| BuildError::Io(format!("creating {}: {e}", path.display())))?;
                Box::new(FileSink(file))
            }
        };
        let usb_sj_id = bus.host.add(
            "usb-sj",
            Box::new(TeeSink {
                log: usb_sj_log.clone(),
                inner: usb_inner,
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
        };

        // Peripherals, in the declared order: the boot set first, then
        // whatever the caller added. The check is what stops a re-sort from
        // re-pointing scheduled events.
        let mut all = if boot_set {
            crate::periph::boot_set(efuse, seed, streams)
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
        let mut app_segments = Vec::new();
        let mut entry = rom_image.entry;
        if let Some(app) = &app_image {
            app_segments = loader::load_app(&mut bus, app)?;
            loader::clear_dram2(&mut bus)?;
            entry = app.entry;
        }

        bus.set_strict(strict);

        // Guest time is zero and the schedule is empty: the peripherals that
        // need a first event (a UART polling its host source) take it now.
        bus.set_time(0);
        bus.start_peripherals();

        let mut hart = MachineHart::new(0);
        loader::reset_hart(&mut hart, &mut bus, entry);
        hart.set_cycle_model(time_grade.cycle_model());

        Ok(Esp32C6Machine {
            harts: vec![hart],
            bus,
            time_grade,
            hooks: HookTable::new(),
            rom: rom_image,
            app: app_image,
            efuse,
            reset_cause: ResetCause::PowerOn,
            seed,
            rng: seed,
            rom_segments,
            rom_data,
            app_segments,
            uart0_log,
            usb_sj_log,
            uart0_tcp,
            hook_calls: 0,
            idle_skips: 0,
            stop_at: None,
        })
    }
}

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

/// A `ByteSink` over a file handle.
struct FileSink(std::fs::File);

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
    hooks: HookTable,
    rom: ElfImage,
    app: Option<ElfImage>,
    efuse: EfuseIdentity,
    reset_cause: ResetCause,
    seed: u64,
    rng: u64,
    rom_segments: Vec<PlacedSegment>,
    rom_data: Vec<rom::SeededSection>,
    app_segments: Vec<PlacedAppSegment>,
    uart0_log: ByteLog,
    usb_sj_log: ByteLog,
    /// The UART0 listener, when the sink is `Tcp`; held so it lives as long
    /// as the machine and so a runner can ask whether a client ever came.
    uart0_tcp: Option<lp_emu_esp_common::TcpHost>,
    hook_calls: u64,
    idle_skips: u64,
    /// Set by a hook that answered [`HookResult::Stop`]; the run loop ends
    /// with [`Outcome::Breakpoint`] at that pc.
    stop_at: Option<u32>,
}

impl Esp32C6Machine {
    // ---- what it is made of -------------------------------------------

    pub fn time_grade(&self) -> TimeGrade {
        self.time_grade
    }

    pub fn efuse(&self) -> EfuseIdentity {
        self.efuse
    }

    pub fn reset_cause(&self) -> ResetCause {
        self.reset_cause
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

    /// Everything the guest handed to USB-Serial-JTAG's IN endpoint with no
    /// host attached: an observation of what it *tried* to print, never
    /// guest output that reached anyone.
    pub fn usb_sj(&self) -> &ByteLog {
        &self.usb_sj_log
    }

    /// The UART0 TCP listener, when `Uart0Sink::Tcp` was chosen.
    pub fn uart0_tcp(&self) -> Option<&lp_emu_esp_common::TcpHost> {
        self.uart0_tcp.as_ref()
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
        let mut matched = 0usize;

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
            if self.bus.strict() {
                deadline = deadline.min(now + STRICT_SLICE_CYCLES);
            }
            let budget = deadline.saturating_sub(now).max(1);

            self.bus.set_time(now);
            let end = self.harts[0].run_slice(&mut self.bus, budget);

            match end {
                SliceEnd::BudgetExhausted => {}
                SliceEnd::Wfi => {
                    // The deterministic idle skip: nothing can happen before
                    // the next scheduled event, so move guest time there.
                    let wake = self
                        .bus
                        .sched
                        .next_deadline()
                        .or_else(|| self.bus.host.next_ready())
                        .unwrap_or(stop_cycle)
                        .min(stop_cycle);
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
            let external = self.bus.pending_cpu_interrupt();
            self.harts[0].set_external(external);
            self.harts[0].poll_interrupts();
            // A peripheral's event may also have written a register the
            // hart's side-band would have reported; consume it so the next
            // slice's entry poll is not answering a stale flag.
            let _ = self.bus.take_sideband();

            if let Some(violation) = self.bus.first_strict_violation() {
                return Outcome::StrictBus { violation };
            }
            if let Some(pc) = self.stop_at.take() {
                return Outcome::Breakpoint {
                    cycle: self.cycles(),
                    pc,
                };
            }
            if let Some(lp_emu_esp_common::MachineRequest::Reset { source, at }) =
                self.bus.take_request()
            {
                return Outcome::Reset { cycle: at, source };
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

    fn exit_on_match(&self, needle: &str, from: &mut usize) -> Option<Cycles> {
        let text = self.uart0_log.text();
        if text.len() <= *from {
            return None;
        }
        let found = text[*from..].find(needle).map(|i| *from + i);
        // Keep the search anchored so a long run does not rescan the whole
        // console every slice; back off by the needle so a match split
        // across two slices is still found.
        *from = text.len().saturating_sub(needle.len());
        found.map(|_| self.cycles())
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
        }
    }

    /// Put the machine back. Watchpoints are re-armed from the restored
    /// hart's trigger CSRs, because they live on the bus and the bus does not
    /// know they came from a hart.
    pub fn restore(&mut self, s: &Snapshot) {
        self.harts.clone_from(&s.harts);
        self.bus.restore_regions(&s.regions);
        self.bus.restore_peripherals(&s.periph);
        self.bus.restore_scalars(&s.scalars);
        self.bus.sched.restore(&s.sched);
        self.bus.matrix_mut().load_state(&s.matrix);
        self.rng = s.rng;
        self.hook_calls = s.hook_calls;
        self.uart0_log.replace(&s.uart0);
        self.usb_sj_log.replace(&s.usb_sj);

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
    fn the_time_grades_are_the_two_cycle_models_and_their_configuration_names() {
        assert_eq!(TimeGrade::T1.cycle_model(), CycleModel::InstructionCount);
        assert_eq!(TimeGrade::T2.cycle_model(), CycleModel::Esp32C6);
        assert_eq!(TimeGrade::T1.configuration(), "lp-emu:esp32c6:t1");
        assert_eq!(TimeGrade::T2.configuration(), "lp-emu:esp32c6:t2");
        assert!(TimeGrade::parse("t3").is_err());

        let m = Esp32C6Builder::new()
            .time_grade(TimeGrade::T2)
            .build()
            .unwrap();
        assert_eq!(m.harts[0].cycle_model(), CycleModel::Esp32C6);
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
                source: "LP_WDT stage 0 (ResetSystem)"
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
