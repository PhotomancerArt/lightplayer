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
use lp_emu_esp_common::{ByteLog, ByteSink, ElfImage, RamRegion, SocBus};
use lp_riscv_emu::mach::trigger::TRIGGER_COUNT;
use lp_riscv_emu::mach::{HartFault, MachineHart, SliceEnd};

use crate::intmatrix::Esp32C6IntMatrix;
use crate::loader::{self, EfuseIdentity, LoadError, PlacedAppSegment, ResetCause};
use crate::memmap;
use crate::rom::{self, HookResult, HookTable, PlacedSegment, RomError};
use crate::snapshot::Snapshot;

/// The largest slice the machine ever asks for, so `--exit-on` and
/// `--wall-timeout` are checked at a bounded guest interval (≈6.5 ms at
/// 160 MHz) without the hot loop leaving the hart.
const MAX_SLICE_CYCLES: u64 = 1_000_000;

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
/// convenience.
pub const PERIPHERAL_REGISTRATION_ORDER: &[&str] = &[
    // Reached inside `esp_hal::init`, in `init` order (discovery §4).
    "LP_APM",
    "LP_AON",
    "PMU",
    "LP_CLKRST",
    "LP_WDT",
    "I2C_ANA_MST",
    "PCR",
    "TIMG0",
    "TIMG1",
    "EFUSE",
    "APB_SARADC",
    "SYSTIMER",
    "ASSIST_DEBUG",
    // The interrupt path (P5).
    "INTERRUPT_CORE0",
    "PLIC_MX",
    "INTPRI",
    // Consoles (P6) and the rest of the product path (M4, M5).
    "UART0",
    "UART1",
    "USB_DEVICE",
    "IO_MUX",
    "GPIO",
    "SPI1",
    "RMT",
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

/// Where a run's UART0 bytes go. P6 attaches the peripheral that produces
/// them; P4 builds the stream so the plumbing and the flag are settled.
#[derive(Clone, Debug, Default)]
pub enum Uart0Sink {
    /// Collected in memory only (and matched against `--exit-on`).
    #[default]
    Memory,
    Stdout,
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
    /// The wall-clock safety net fired. The only non-deterministic outcome,
    /// and it can only end a run.
    WallTimeout { cycle: Cycles },
}

impl Outcome {
    /// The CLI's exit code for this outcome.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Outcome::ExitMatched { .. } | Outcome::Deadline { .. } => 0,
            Outcome::Fault { .. } => 2,
            Outcome::StrictBus { .. } => 3,
            Outcome::WallTimeout { .. } => 4,
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
    RegistrationOrder { name: String, after: String },
    /// A peripheral name that is not in the declared order at all.
    UndeclaredPeripheral { name: String },
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
pub struct Esp32C6Builder {
    rom: RomSource,
    app: AppSource,
    efuse: EfuseIdentity,
    time_grade: TimeGrade,
    strict: bool,
    trace: Option<Box<dyn std::io::Write + Send>>,
    trace_blocks: Vec<String>,
    uart0: Uart0Sink,
    seed: u64,
    peripherals: Vec<(u32, u32, BoxedPeripheral)>,
}

impl Default for Esp32C6Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Esp32C6Builder {
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
            seed: 0,
            peripherals: Vec::new(),
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
            seed,
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

        // UART0's host stream exists from P4 so the flag, the `--exit-on`
        // match and the snapshot all have one thing to point at; P6 attaches
        // the peripheral that writes to it.
        let uart0_log = ByteLog::new();
        let inner: Box<dyn ByteSink> = match &uart0 {
            Uart0Sink::Memory => Box::new(lp_emu_esp_common::host::NullSink),
            Uart0Sink::Stdout => Box::new(lp_emu_esp_common::host::StdoutSink),
            Uart0Sink::File(path) => {
                let file = std::fs::File::create(path)
                    .map_err(|e| BuildError::Io(format!("creating {}: {e}", path.display())))?;
                Box::new(FileSink(file))
            }
        };
        bus.host.add(
            "uart0",
            Box::new(TeeSink {
                log: uart0_log.clone(),
                inner,
            }),
            Box::new(lp_emu_esp_common::host::NullSource),
        );

        // Peripherals, in the declared order. P4 registers none; the check
        // is here so P5 cannot quietly re-sort them.
        check_registration_order(&peripherals)?;
        for (base, len, periph) in peripherals {
            bus.add_peripheral(base, len, periph);
        }

        // The ROM first, then the app on top of it: the ROM's `.bss` reaches
        // across what the app calls RAM, and the real bootloader overwrites
        // it the same way.
        let rom_segments = rom::load(&mut bus, &rom_image)?;
        let mut app_segments = Vec::new();
        let mut entry = rom_image.entry;
        if let Some(app) = &app_image {
            app_segments = loader::load_app(&mut bus, app)?;
            loader::clear_dram2(&mut bus)?;
            entry = app.entry;
        }

        bus.set_strict(strict);

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
            app_segments,
            uart0_log,
            hook_calls: 0,
        })
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
    app_segments: Vec<PlacedAppSegment>,
    uart0_log: ByteLog,
    hook_calls: u64,
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

    pub fn app_segments(&self) -> &[PlacedAppSegment] {
        &self.app_segments
    }

    pub fn hooks(&self) -> &HookTable {
        &self.hooks
    }

    pub fn hooks_mut(&mut self) -> &mut HookTable {
        &mut self.hooks
    }

    /// How many times a ROM hook has stood in for a routine.
    pub fn hook_calls(&self) -> u64 {
        self.hook_calls
    }

    /// Everything UART0 has produced. Empty until P6 models the peripheral.
    pub fn uart0(&self) -> &ByteLog {
        &self.uart0_log
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
        self.app
            .as_ref()
            .and_then(from)
            .or_else(|| from(&self.rom))
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
    pub fn peek_symbol(&mut self, name: &str) -> Option<(u32, u32)> {
        let address = self
            .app
            .as_ref()
            .and_then(|a| a.symbol(name))
            .or_else(|| self.rom.symbol(name))?
            .address;
        self.peek_word(address).map(|v| (address, v))
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
            let line = format!(
                "cyc={} pc={pc:#010x} HOOK {}",
                self.cycles(),
                hook.symbol
            );
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
        let ok = Esp32C6Builder::new()
            .peripheral(0x6000_8000, 0x100, Box::new(RegFile::new("TIMG0", 0x100)))
            .peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)))
            .build();
        assert!(ok.is_ok());

        // Reversed: refused, because event ids pack the index.
        let bad = Esp32C6Builder::new()
            .peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)))
            .peripheral(0x6000_8000, 0x100, Box::new(RegFile::new("TIMG0", 0x100)))
            .build();
        assert!(matches!(
            bad,
            Err(BuildError::RegistrationOrder { .. })
        ));

        let undeclared = Esp32C6Builder::new()
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
