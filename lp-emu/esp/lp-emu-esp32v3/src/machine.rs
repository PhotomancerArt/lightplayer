//! The classic machine: two hart slots, a bus, a mask ROM and a run loop.
//!
//! `lp-emu-esp32c6`'s twin in shape, and the two places it differs are the
//! two things about this chip that matter:
//!
//! - the hart is [`lp_xt_emu::mach::XtHart`] rather than
//!   `lp_riscv_emu::mach::MachineHart`, and
//! - **`harts` has two slots, and slot 1 is stalled for the whole of M3.**
//!
//! # What "stalled" means here
//!
//! The classic is dual-core. Slot 1 is *constructed* — it holds architectural
//! state, it appears in a snapshot, `--probe` and the run report name it —
//! and it is **never given a slice**, so it consumes no guest time. That is
//! Q5's answer for this milestone, and it is a supported configuration of the
//! firmware rather than a hole: `start_app_core` times out and the image
//! takes its documented single-core fallback
//! (`lp-fw/fw-esp32v3/src/main.rs:838-845`, which prints `[INIT] APP core
//! unavailable; RMT ISR on PRO core (single-core semantics)`).
//!
//! On silicon a stalled core is held by two register pairs, and P4 wires
//! [`Machine::core_stalled`] to them: `RTC_CNTL.options0.sw_stall_appcpu_c0`
//! plus `RTC_CNTL.sw_cpu_stall.sw_stall_appcpu_c1` (both halves of the key)
//! and `DPORT.appcpu_ctrl_c.appcpu_runstall`. In P2 none of those registers
//! exists, so it is a plain machine field.
//!
//! # Peripherals: accept blocks, in the order the boot met them
//!
//! P2 registered **no** peripheral, so a `--strict-bus` run stopped at the
//! first MMIO access of each boot path. P3 ran that loop and registers, in
//! [`PERIPHERAL_REGISTRATION_ORDER`], exactly the blocks the strict runs
//! demanded — each an accept-and-remember [`lp_emu_esp_common::RegFile`]
//! seeded from the PAC ([`crate::periph::accept`]), each a probe that lets
//! the *next* stop become visible. The order is the ledger
//! (`docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`), and it is
//! a contract: the bus packs a peripheral's index into every scheduler event
//! id, so a block a later phase adds goes in its place in the list, never on
//! the end.
//!
//! # The bring-up loop
//!
//! 1. Run `--strict-bus --trace`.
//! 2. Read the **first** stop. [`Machine::first_strict_violation`] names the
//!    earliest, and **the earliest strict stop is the root** — an exception
//!    after it is downstream and tells you nothing.
//! 3. Model that one block, with the pin cited: a PAC reset value, a ROM
//!    disassembly, a linker-script constant. Never "what the boot needed".
//! 4. Run again.
//!
//! Without `--strict-bus` the run carries on with unmapped reads answering
//! zero, which is how far the machine gets before a block is modelled —
//! useful for scouting, never for a claim.

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use lp_emu_core::Bus;
use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::bus::StrictViolation;
use lp_emu_esp_common::{ElfImage, SocBus};
use lp_xt_emu::mach::interrupt::{IntKind, IntLine};
use lp_xt_emu::mach::sr::PS_BOOT;
use lp_xt_emu::mach::trap::NUM_INTERRUPTS;
use lp_xt_emu::mach::{CoreConfig, HartFault, SliceEnd, XtHart};

pub use crate::loader::PlacedAppSegment;
use crate::loader::{self, FlashChipSeed, LoadError};
use crate::memmap;
use crate::rom::{self, DataImage, HookResult, HookTable, PlacedSegment, RomError, SeededSection};
use crate::snapshot::Snapshot;

/// The longest slice the loop hands a hart. Small enough that a scheduled
/// event lands within a bounded number of instructions of its cycle, large
/// enough that the per-slice bookkeeping is amortised. The C6's number
/// (`lp-emu-esp32c6/src/machine.rs:102`), for the same reason.
pub const MAX_SLICE_CYCLES: u64 = 8_192;

/// Under `--strict-bus` the loop checks for a refusal between slices, so a
/// shorter slice reports the violating access closer to where it happened.
const STRICT_SLICE_CYCLES: u64 = 1_024;

/// `PRID` on the PRO core. **A chip number, not a hart index**: the two cores
/// answer `0xCDCD` and `0xABAB`, and esp-hal tells them apart by bit 13
/// (`esp-hal-1.1.1/src/system.rs:302-311` — `0xCDCD` has it clear, `0xABAB`
/// has it set).
pub const PRID_PRO: u32 = 0x0000_CDCD;

/// `PRID` on the APP core. See [`PRID_PRO`].
pub const PRID_APP: u32 = 0x0000_ABAB;

/// The number of cores the classic has. Both are constructed; M3 runs one.
pub const CORES: usize = 2;

/// The order the classic's peripheral blocks are registered in. See the
/// module docs for why this is a contract.
///
/// The list is the blocks the P3 strict runs reached, **in the order the
/// boot met them** — the direct load's stops first, then the ones only the
/// mask ROM's reset path touches. A block a later phase needs is added in
/// its place in this list, never appended for convenience.
/// [`crate::periph::boot_set`] registers exactly this order.
pub const PERIPHERAL_REGISTRATION_ORDER: &[&str] = &[
    // Direct load, stop 1: `esp32_init` clears the APP core's interrupt map.
    "DPORT",
    // Direct load, stop 2: the ROM's `rtc_get_reset_reason` reads
    // `reset_state` for `esp_hal::rtc_cntl::reset_reason`.
    "RTC_CNTL",
    // Direct load, stop 3: `Clocks::init` reads `APB_CTRL.sysclk_conf`.
    "APB_CTRL",
    // ROM-up, stop 1: `_ResetHandler_efuse_check_patch` reads its own
    // fuses seven instructions after the reset vector.
    "EFUSE",
];

/// How far back [`Machine::symbolize`] will look for a name when no symbol's
/// size covers the address. Four kilobytes: further than any routine in this
/// ROM, and short enough that a pc in a genuine hole is reported as a hole.
pub const NEAREST_SYMBOL_WINDOW: u32 = 0x1000;

/// The classic's 32 CPU interrupt lines: each one's fixed level and type.
///
/// Source: `third_party/esp-hal/src/interrupt/xtensa.rs:19-98`, whose
/// `CpuInterrupt` variant names spell out both fields
/// (`Interrupt6Timer0Priority1`, `Interrupt22EdgePriority3`, …). That file is
/// the table the shipped firmware itself allocates out of, which is what
/// makes it the right source rather than a datasheet transcription.
///
/// ⚠️ **Eight lines are [`IntLine::UNUSED`] here and are not known to be
/// unused on silicon**: 14, 16, 24, 25, 26, 28, 30 and 31. esp-hal's own
/// table carries a `// TODO: re-add higher level interrupts` at `:68` and
/// stops at 29, so those eight have no cited level or type in this repo. The
/// firmware cannot allocate a line esp-hal does not name, so M3 never needs
/// them; a phase that does must pin them from the TRM's CPU-interrupt table
/// and say so here, rather than filling them in by analogy.
pub const CORE_INTERRUPTS: [IntLine; NUM_INTERRUPTS] = {
    let mut lines = [IntLine::UNUSED; NUM_INTERRUPTS];
    lines[0] = IntLine::new(1, IntKind::Level);
    lines[1] = IntLine::new(1, IntKind::Level);
    lines[2] = IntLine::new(1, IntKind::Level);
    lines[3] = IntLine::new(1, IntKind::Level);
    lines[4] = IntLine::new(1, IntKind::Level);
    lines[5] = IntLine::new(1, IntKind::Level);
    lines[6] = IntLine::new(1, IntKind::Timer(0));
    lines[7] = IntLine::new(1, IntKind::Software);
    lines[8] = IntLine::new(1, IntKind::Level);
    lines[9] = IntLine::new(1, IntKind::Level);
    lines[10] = IntLine::new(1, IntKind::Edge);
    // `Interrupt11ProfilingPriority3`: the profiling timer, an edge line.
    lines[11] = IntLine::new(3, IntKind::Edge);
    lines[12] = IntLine::new(1, IntKind::Level);
    lines[13] = IntLine::new(1, IntKind::Level);
    lines[15] = IntLine::new(3, IntKind::Timer(1));
    lines[17] = IntLine::new(1, IntKind::Level);
    lines[18] = IntLine::new(1, IntKind::Level);
    lines[19] = IntLine::new(2, IntKind::Level);
    lines[20] = IntLine::new(2, IntKind::Level);
    lines[21] = IntLine::new(2, IntKind::Level);
    lines[22] = IntLine::new(3, IntKind::Edge);
    lines[23] = IntLine::new(3, IntKind::Level);
    lines[27] = IntLine::new(3, IntKind::Level);
    lines[29] = IntLine::new(3, IntKind::Software);
    lines
};

/// Which cycle model a run uses.
///
/// **t1 only in M3** (ruling DD24 / R9). There is no measured Xtensa class
/// table in this repo, and `../m1/p3-xt-hart.md` is explicit about why one
/// must not be invented: six months later an invented number is
/// indistinguishable from a measured one. The classic has a better
/// calibration source than the C6 ever had — `CCOUNT` is CPU cycles at
/// 240 MHz — which is an M5/M7 opportunity, not an M3 one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeGrade {
    /// `lp-emu:esp32v3:t1` — one cycle per instruction.
    #[default]
    T1,
}

impl TimeGrade {
    pub const fn cycle_model(self) -> lp_emu_core::CycleModel {
        match self {
            TimeGrade::T1 => lp_emu_core::CycleModel::InstructionCount,
        }
    }

    /// The validation system's configuration name (plan PD4).
    pub const fn configuration(self) -> &'static str {
        match self {
            TimeGrade::T1 => "lp-emu:esp32v3:t1",
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "t1" => Ok(TimeGrade::T1),
            "t2" | "t3" => Err(format!(
                "time grade `{text}` does not exist on this machine: there is no measured \
                 Xtensa per-instruction-class table in this repo, and inventing one would be \
                 indistinguishable from a measured one six months later. t1 (cycles = \
                 instructions) is the only grade M3 defines"
            )),
            other => Err(format!("unknown time grade `{other}` (expected t1)")),
        }
    }
}

/// Where the mask ROM comes from. There is no "no ROM" variant: plan PD7
/// says the ROM is loaded in every configuration.
#[derive(Clone, Debug, Default)]
pub enum RomSource {
    /// The compiled-in `esp32_rev300_rom.elf`.
    #[default]
    Vendored,
    /// `--rom <path>`.
    Path(PathBuf),
}

/// The application image, if there is one.
#[derive(Clone, Debug, Default)]
pub enum AppSource {
    #[default]
    None,
    /// `--elf <path>`.
    Path(PathBuf),
}

/// How the machine starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BootMode {
    /// Place the app's `PT_LOAD`s and start at its entry with the boot state
    /// a bootloader would have left ([`BootFrame`]).
    #[default]
    Direct,
    /// Start at the mask ROM's reset vector and let the ROM do whatever it
    /// does. P7 makes this reach a real bootloader; in P2 it is how the ROM
    /// path itself is read.
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

    pub const fn as_str(self) -> &'static str {
        match self {
            BootMode::Direct => "direct",
            BootMode::RomUp => "rom-up",
        }
    }
}

/// The stack state a direct load seeds — **the seam M1 P4's finding created**.
///
/// # Why a direct load must seed a stack at all
///
/// [`PS_BOOT`] is `WOE | UM | CALLINC(2)` = `0x0006_0020`, and the `CALLINC(2)`
/// is not decoration: the IDF bootloader reaches the application's entry point
/// through an ordinary C call, which on the windowed ABI is `callx8`. So the
/// app's `Reset:` runs as frame **2**, and frame 0 — the bootloader's own —
/// stays live behind it.
///
/// A hart left with `a1 = 0` therefore has a live outermost frame whose stack
/// pointer is null. Nothing notices until the first register spill, and then
/// `_WindowOverflow8`'s `l32e a0, a1, -12` reads `0xFFFF_FFF4`, faults, and
/// the fault's own spill faults again: a double exception, forever. It shows
/// up nowhere near its cause.
///
/// So a direct load seeds two things:
///
/// - **`a1`**, the outermost frame's stack pointer, and
/// - **the four words of that frame's base save area** at `[a1-16, a1)`,
///   which is where `_WindowOverflow4/8/12` store `a0..a3` and where
///   `l32e a0, a1, -12` reads the *next* frame's stack pointer from.
///
/// # What P2 pins, and what it does not
///
/// [`BootFrame::rom_pro_stack`] uses the mask ROM's own PRO-core stack top,
/// `__stack` — resolved from the vendored ROM ELF's symbol table, where it is
/// `0x3FFE_3F20`, the same number `third_party/esp-hal/ld/esp32/memory.x:32`
/// derives `reserved_rom_stack_pro` from. That is a cited value, and it is
/// the right *shape*.
///
/// It is **not yet the bootloader's own SP**. The IDF second-stage bootloader
/// runs from `0x4007_8000` on a stack its own linker script places, and the
/// value it holds at the `callx8` into the app is P3's to pin — from the
/// bootloader disassembly or from a ROM-up run that gets that far. Until then
/// this is the machine's *seam*: a builder parameter with a cited default and
/// a test that proves a seeded hart survives its first exception, not a claim
/// about what silicon holds.
///
/// The base save area's default is `a0 = 0, a1 = sp, a2 = 0, a3 = 0`. The
/// saved `a1` points back at the stack top, so an overflow-8 spill through it
/// writes into the ROM stack region — mapped, and inside memory the ROM owns.
/// The saved `a0` is zero, so a *return* through the outermost frame lands at
/// address zero and stops loudly instead of running into whatever is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootFrame {
    /// `a1` at entry: the outermost live frame's stack pointer.
    pub sp: u32,
    /// The four words written at `[sp-16, sp)`: `a0, a1, a2, a3` in that
    /// order, which is the order `_WindowOverflow4` stores them in
    /// (`s32e a0, a5, -16` … `s32e a3, a5, -4`).
    pub save_area: [u32; 4],
}

impl BootFrame {
    /// The mask ROM's PRO-core stack top, from the ROM ELF's `__stack`.
    ///
    /// Falls back to the linker script's `reserved_rom_stack_pro` end
    /// (`0x3FFE_3F20`) if the symbol is missing, which for the vendored ROM
    /// it is not — the fallback exists so a `--rom` override without symbols
    /// still produces a machine rather than an error.
    pub fn rom_pro_stack(rom: &ElfImage) -> Self {
        let sp = rom
            .symbol("__stack")
            .map(|s| s.address)
            .unwrap_or(memmap::ROM_PRO_STACK_TOP);
        Self::at(sp)
    }

    /// A boot frame at an explicit stack pointer, with the default save area.
    pub const fn at(sp: u32) -> Self {
        Self {
            sp,
            save_area: [0, sp, 0, 0],
        }
    }
}

/// Why a run stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The emulated deadline was reached with no fault.
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
    /// A `--break-at` symbol was reached; the guest is stopped at its first
    /// instruction with every register as the caller left it.
    Breakpoint { cycle: Cycles, pc: u32 },
}

impl Outcome {
    /// The CLI's exit code for this outcome. The C6's contract, so a script
    /// that drives both machines reads one table.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Outcome::Deadline { .. } => 0,
            Outcome::Fault { .. } => 2,
            Outcome::StrictBus { .. } => 3,
            Outcome::WallTimeout { .. } => 4,
            Outcome::Breakpoint { .. } => 5,
        }
    }

    pub const fn cycle(&self) -> Cycles {
        match self {
            Outcome::Deadline { cycle }
            | Outcome::Fault { cycle, .. }
            | Outcome::WallTimeout { cycle }
            | Outcome::Breakpoint { cycle, .. } => *cycle,
            Outcome::StrictBus { violation } => violation.cycle,
        }
    }
}

/// When to stop. Everything but `wall_timeout` is emulated time (PD9: no host
/// gate runs on emulated microseconds, and `wall_timeout` is the separate
/// wall-clock end).
#[derive(Clone, Debug, Default)]
pub struct StopCondition {
    /// Absolute guest cycle to stop at. `None` means "until something else
    /// stops it", which on a machine with no peripherals means the first
    /// fault.
    pub stop_cycle: Option<Cycles>,
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
    App(String),
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
            BuildError::App(m) => write!(f, "application image: {m}"),
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

/// Hold [`PERIPHERAL_REGISTRATION_ORDER`]: every block in the set is in the
/// list, and the set is in the list's order.
fn check_registration_order(
    set: &[(u32, u32, lp_emu_esp_common::periph::BoxedPeripheral)],
) -> Result<(), BuildError> {
    let mut last: Option<(usize, &str)> = None;
    for (_, _, periph) in set {
        let name = periph.name();
        let Some(pos) = PERIPHERAL_REGISTRATION_ORDER
            .iter()
            .position(|n| *n == name)
        else {
            return Err(BuildError::UndeclaredPeripheral {
                name: name.to_string(),
            });
        };
        if let Some((prev, prev_name)) = last
            && pos < prev
        {
            return Err(BuildError::RegistrationOrder {
                name: name.to_string(),
                after: prev_name.to_string(),
            });
        }
        last = Some((pos, name));
    }
    Ok(())
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

/// Builds a [`Machine`].
pub struct Esp32V3Builder {
    rom: RomSource,
    app: AppSource,
    boot_mode: BootMode,
    time_grade: TimeGrade,
    strict: bool,
    strict_unsupported: bool,
    boot_frame: Option<BootFrame>,
    flash_size: u32,
    reset_cause: loader::ResetCause,
    boot_set: bool,
    trace: Option<Box<dyn std::io::Write + Send>>,
    trace_blocks: Vec<String>,
    seed: u64,
}

impl Default for Esp32V3Builder {
    fn default() -> Self {
        Self {
            rom: RomSource::default(),
            app: AppSource::default(),
            boot_mode: BootMode::default(),
            time_grade: TimeGrade::default(),
            strict: false,
            strict_unsupported: true,
            boot_frame: None,
            flash_size: loader::DEFAULT_FLASH_SIZE,
            reset_cause: loader::ResetCause::default(),
            boot_set: true,
            trace: None,
            trace_blocks: Vec::new(),
            seed: 0,
        }
    }
}

impl fmt::Debug for Esp32V3Builder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Written out rather than derived: the trace sink is a
        // `Box<dyn Write + Send>` and has no `Debug`, and a builder that
        // could not be printed in a test failure would be worse than one
        // whose sink prints as a flag.
        f.debug_struct("Esp32V3Builder")
            .field("rom", &self.rom)
            .field("app", &self.app)
            .field("boot_mode", &self.boot_mode)
            .field("time_grade", &self.time_grade)
            .field("strict", &self.strict)
            .field("strict_unsupported", &self.strict_unsupported)
            .field("boot_frame", &self.boot_frame)
            .field("flash_size", &self.flash_size)
            .field("reset_cause", &self.reset_cause)
            .field("boot_set", &self.boot_set)
            .field("trace", &self.trace.is_some())
            .field("trace_blocks", &self.trace_blocks)
            .field("seed", &self.seed)
            .finish()
    }
}

impl Esp32V3Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rom(mut self, rom: RomSource) -> Self {
        self.rom = rom;
        self
    }

    pub fn app(mut self, app: AppSource) -> Self {
        self.app = app;
        self
    }

    pub fn boot_mode(mut self, mode: BootMode) -> Self {
        self.boot_mode = mode;
        self
    }

    pub fn time_grade(mut self, grade: TimeGrade) -> Self {
        self.time_grade = grade;
        self
    }

    /// `--strict-bus`: every access to an address nothing claims becomes a
    /// stop instead of a silent zero.
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// The unsupported-opcode stop (DD16). On by default: a bring-up wants a
    /// named stop with the word, not a guest quietly reaching its own
    /// illegal-instruction handler.
    pub fn strict_unsupported(mut self, on: bool) -> Self {
        self.strict_unsupported = on;
        self
    }

    /// Override the direct load's boot frame. See [`BootFrame`]; the default
    /// is [`BootFrame::idf_bootloader`].
    pub fn boot_frame(mut self, frame: BootFrame) -> Self {
        self.boot_frame = Some(frame);
        self
    }

    /// The flash chip's size, written into the ROM's chip description by a
    /// direct load ([`loader::seed_rom_flash_chip`]). Default
    /// [`loader::DEFAULT_FLASH_SIZE`], the desk board's 4 MiB.
    pub fn flash_size(mut self, bytes: u32) -> Self {
        self.flash_size = bytes;
        self
    }

    /// What the machine asserts the reset cause was (loader item 7).
    pub fn reset_cause(mut self, cause: loader::ResetCause) -> Self {
        self.reset_cause = cause;
        self
    }

    /// A machine with **no** peripherals — the memory map, the ROM and the
    /// harts alone. What P2 built; kept so a test can still read the first
    /// strict stop of each boot path against an empty MMIO window.
    pub fn bare(mut self) -> Self {
        self.boot_set = false;
        self
    }

    pub fn trace(mut self, sink: Box<dyn std::io::Write + Send>, blocks: Vec<String>) -> Self {
        self.trace = Some(sink);
        self.trace_blocks = blocks;
        self
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    pub fn build(self) -> Result<Machine, BuildError> {
        let rom_image = match &self.rom {
            RomSource::Vendored => rom::vendored()?,
            RomSource::Path(p) => rom::parse_file(p)?,
        };

        let mut bus = crate::bus_setup::build();
        bus.set_strict(self.strict);
        if let Some(sink) = self.trace {
            bus.trace =
                lp_emu_esp_common::Trace::to_sink(sink).with_block_filter(self.trace_blocks);
        }

        // The peripherals, in the declared order, before any memory is
        // placed: a block's index is fixed at registration and the schedule
        // is empty until `start_peripherals`.
        let mut peripheral_map = Vec::new();
        if self.boot_set {
            let set = crate::periph::boot_set(self.reset_cause);
            check_registration_order(&set)?;
            for (base, len, periph) in set {
                peripheral_map.push((periph.name(), base, len));
                bus.add_peripheral(base, len, periph);
            }
        }

        // The ROM first, always (PD7), and its non-alloc data with it.
        let rom_segments = rom::load(&mut bus, &rom_image)?;
        let rom_data = rom::seed_data(&mut bus, &rom_image)?;
        let rom_data_image = rom::seed_data_image(&rom_data);

        let app_image = match &self.app {
            AppSource::None => None,
            AppSource::Path(p) => {
                let bytes = std::fs::read(p)
                    .map_err(|e| BuildError::Io(format!("reading {}: {e}", p.display())))?;
                Some(ElfImage::parse(&bytes).map_err(|e| BuildError::App(e.to_string()))?)
            }
        };

        // Two harts, always. The classic is dual-core and a machine that
        // pretended otherwise would have nowhere for P4's DPORT view to
        // write, and nothing for a snapshot to carry.
        let config = |prid| CoreConfig {
            reset_pc: memmap::ROM_MASK_BASE + RESET_VECTOR_OFS,
            reset_vecbase: memmap::ROM_MASK_BASE,
            prid,
            interrupts: CORE_INTERRUPTS,
        };
        let mut harts: Vec<XtHart<SocBus>> = vec![
            XtHart::new(0, config(PRID_PRO)),
            XtHart::new(1, config(PRID_APP)),
        ];
        for hart in &mut harts {
            hart.set_cycle_model(self.time_grade.cycle_model());
            hart.set_strict_unsupported(self.strict_unsupported);
        }

        let mut machine = Machine {
            bus,
            harts,
            // P4 wires this to DPORT.appcpu_ctrl_c.appcpu_runstall +
            // RTC_CNTL.options0/sw_cpu_stall. M3 never runs core 1 (Q5).
            stalled: [false, true],
            rom: rom_image,
            app: app_image,
            rom_segments,
            rom_data,
            rom_data_image,
            app_segments: Vec::new(),
            flash_seed: None,
            peripheral_map,
            hooks: HookTable::new(),
            boot_mode: self.boot_mode,
            time_grade: self.time_grade,
            boot_frame: None,
            stop_at: None,
            hook_calls: 0,
            idle_skips: 0,
            seed: self.seed,
            rng: self.seed,
        };

        if self.boot_mode == BootMode::Direct {
            let frame = self.boot_frame.unwrap_or_else(BootFrame::idf_bootloader);
            machine.direct_load(frame, self.flash_size)?;
        }

        // Guest time is zero and everything is placed: the one moment a
        // peripheral may schedule something before the guest touches it.
        machine.bus.start_peripherals();

        Ok(machine)
    }
}

/// `_ResetVector`'s offset from the mask ROM's base. The ROM ELF's `e_entry`
/// is `0x4000_0400`, which is also `XCHAL_RESET_VECTOR_VADDR` in
/// `xtensa-lx-rt`'s `config/esp32.rs` — two independent sources for one
/// number (`m3/notes.md` §2), which is why `tests/boot.rs` asserts it.
pub const RESET_VECTOR_OFS: u32 = 0x400;

/// The classic ESP32 machine.
pub struct Machine {
    bus: SocBus,
    /// **Two** slots: the classic is dual-core. Slot 1 is stalled for the
    /// whole of M3 — see the module docs.
    pub harts: Vec<XtHart<SocBus>>,
    stalled: [bool; CORES],
    rom: ElfImage,
    app: Option<ElfImage>,
    rom_segments: Vec<PlacedSegment>,
    rom_data: Vec<SeededSection>,
    rom_data_image: DataImage,
    app_segments: Vec<PlacedAppSegment>,
    flash_seed: Option<FlashChipSeed>,
    peripheral_map: Vec<(&'static str, u32, u32)>,
    hooks: HookTable,
    boot_mode: BootMode,
    time_grade: TimeGrade,
    boot_frame: Option<BootFrame>,
    stop_at: Option<u32>,
    hook_calls: u64,
    idle_skips: u64,
    seed: u64,
    rng: u64,
}

impl Machine {
    // ---- construction ---------------------------------------------------

    /// Place the application's `PT_LOAD`s and seed the boot state a
    /// bootloader would have left: [`crate::loader`] is the documentation.
    ///
    /// Segments, the ROM's flash chip description, then the hart — entry,
    /// [`PS_BOOT`] and the [`BootFrame`]. What a direct load does *not*
    /// reproduce is the eleven-item list in the loader's module docs.
    fn direct_load(&mut self, frame: BootFrame, flash_size: u32) -> Result<(), BuildError> {
        let Some(app) = self.app.as_ref() else {
            return Err(BuildError::App(
                "--boot-mode direct needs an --elf to load".into(),
            ));
        };
        // Cloned because the loader borrows the bus mutably and the app
        // image is borrowed from `self`. Segment data on this image is
        // ~2 MiB; it is placed once, at build.
        let app = app.clone();
        self.app_segments = loader::load_app(&mut self.bus, &app)?;
        self.flash_seed = Some(loader::seed_rom_flash_chip(
            &mut self.bus,
            &self.rom,
            flash_size,
        )?);
        self.seed_boot_state(app.entry, frame)?;
        Ok(())
    }

    /// Put hart 0 into the state a bootloader's `callx8` into `entry` leaves.
    ///
    /// Separated from [`direct_load`](Self::direct_load) so a test can seed a
    /// hart at a synthetic entry point without an application image — which
    /// is what `tests/boot.rs` uses to prove a seeded hart survives its first
    /// exception.
    pub fn seed_boot_state(&mut self, entry: u32, frame: BootFrame) -> Result<(), BuildError> {
        self.harts[0].set_pc(entry);
        self.harts[0].set_ps_raw(PS_BOOT);
        self.harts[0].cpu_mut().set_a(1, frame.sp);
        for (i, word) in frame.save_area.iter().enumerate() {
            let at = frame.sp.wrapping_sub(16).wrapping_add(4 * i as u32);
            self.bus.load_image(at, &word.to_le_bytes()).map_err(|e| {
                BuildError::Io(format!("seeding the boot frame at {at:#010x}: {e}"))
            })?;
        }
        self.boot_frame = Some(frame);
        Ok(())
    }

    // ---- what is in here ------------------------------------------------

    pub fn boot_mode(&self) -> BootMode {
        self.boot_mode
    }

    pub fn time_grade(&self) -> TimeGrade {
        self.time_grade
    }

    pub fn boot_frame(&self) -> Option<BootFrame> {
        self.boot_frame
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

    pub fn rom_data(&self) -> &[SeededSection] {
        &self.rom_data
    }

    pub fn rom_data_image(&self) -> DataImage {
        self.rom_data_image
    }

    pub fn app_segments(&self) -> &[PlacedAppSegment] {
        &self.app_segments
    }

    /// What a direct load wrote into the ROM's flash chip description;
    /// `None` on a rom-up machine, where the bootloader does it.
    pub fn flash_seed(&self) -> Option<FlashChipSeed> {
        self.flash_seed
    }

    pub fn hooks(&self) -> &HookTable {
        &self.hooks
    }

    pub fn bus(&self) -> &SocBus {
        &self.bus
    }

    pub fn bus_mut(&mut self) -> &mut SocBus {
        &mut self.bus
    }

    /// Is core `n` stalled? Slot 1 is, for the whole of M3 (Q5).
    pub fn core_stalled(&self, core: usize) -> bool {
        self.stalled.get(core).copied().unwrap_or(true)
    }

    /// One line per core, for `--probe` and the run report. Slot 1 is never
    /// silently absent.
    pub fn core_report(&self) -> Vec<String> {
        (0..CORES)
            .map(|c| {
                if self.core_stalled(c) {
                    format!("core {c}: stalled (M3 is single-core; Q5)")
                } else {
                    format!(
                        "core {c}: running, pc={:#010x} cycle={}",
                        self.harts[c].pc(),
                        self.harts[c].cycle_count()
                    )
                }
            })
            .collect()
    }

    pub fn first_strict_violation(&self) -> Option<StrictViolation> {
        self.bus.first_strict_violation()
    }

    /// The registered blocks, in registration order: `(name, base, len)`.
    pub fn peripheral_map(&self) -> &[(&'static str, u32, u32)] {
        &self.peripheral_map
    }

    pub fn hook_calls(&self) -> u64 {
        self.hook_calls
    }

    pub fn idle_skips(&self) -> u64 {
        self.idle_skips
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The machine's seeded PRNG (SplitMix64). The only source of "random"
    /// in the machine, so a run with the same seed is the same run.
    pub fn next_random(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    // ---- time ------------------------------------------------------------

    pub fn cycles(&self) -> Cycles {
        self.harts[0].cycle_count()
    }

    /// Emulated microseconds: `cycles / 240` ([`memmap::CPU_HZ`]).
    pub fn micros(&self) -> u64 {
        self.cycles() / memmap::CYCLES_PER_US
    }

    pub fn instructions(&self) -> u64 {
        self.harts[0].instruction_count()
    }

    // ---- symbols and memory ---------------------------------------------

    /// What is at `address`: the app's symbol if the app claims it, else the
    /// ROM's. Both are asked because a fault inside `memcpy` is in the ROM
    /// and a fault inside `esp_hal::init` is in the app.
    ///
    /// `ElfImage::symbol_at` answers only when a symbol's `[address, address
    /// + size)` covers the query, and it stops at the *last* symbol starting
    /// at or before it. On the classic ROM that is not enough: its table is
    /// full of zero-sized labels sitting inside real functions, so a pc in
    /// the middle of `gpio_register_set` gets no name at all. So there is a
    /// second pass — **the nearest preceding symbol within
    /// [`NEAREST_SYMBOL_WINDOW`]** — bounded so a fault in a hole is reported
    /// as a hole rather than attributed to a function a hundred kilobytes
    /// back.
    ///
    /// A nearest-match is prefixed with `~`, and the tilde is load-bearing.
    /// The classic ROM's table is mostly zero-sized `NOTYPE` labels, so the
    /// nearest preceding one is often *not* the enclosing function: the first
    /// strict stop of a rom-up boot is at `0x4000_FDD8`, which this reports as
    /// `~_rtc_trigger_sw_system_reset+0x11` while the routine it is really
    /// inside is `_ResetHandler_efuse_check_patch` at `0x4000_FDA0`. Both
    /// labels are zero-sized; nothing in the ELF says which one owns the
    /// bytes. The tilde says "nearest label", not "this function".
    pub fn symbolize(&self, address: u32) -> Option<String> {
        let exact = |image: &ElfImage| {
            image.symbol_at(address).map(|s| {
                if s.address == address {
                    s.name.clone()
                } else {
                    format!("{}+0x{:x}", s.name, address - s.address)
                }
            })
        };
        if let Some(name) = self
            .app
            .as_ref()
            .and_then(exact)
            .or_else(|| exact(&self.rom))
        {
            return Some(name);
        }
        let nearest = |image: &ElfImage| {
            let symbols = image.symbols();
            let i = symbols
                .partition_point(|s| s.address <= address)
                .checked_sub(1)?;
            let s = &symbols[i];
            let back = address - s.address;
            (back <= NEAREST_SYMBOL_WINDOW).then(|| format!("~{}+0x{back:x}", s.name))
        };
        self.app
            .as_ref()
            .and_then(nearest)
            .or_else(|| nearest(&self.rom))
    }

    /// The exact ELF name first, then a **unique** symbol whose name ends
    /// with `name` after a `::` — so a bare `TIMED_OUT` works when only one
    /// exists. An ambiguous short name is refused rather than guessed.
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
        let mut matches = Vec::new();
        for image in images {
            for s in image.symbols() {
                if s.name.ends_with(name) && s.name[..s.name.len() - name.len()].ends_with("::") {
                    matches.push((s.address, s.name.clone()));
                }
            }
        }
        match matches.as_slice() {
            [] => None,
            [(address, _)] => Some(*address),
            many => {
                log::warn!(
                    "symbol `{name}`: {} matches, refusing to guess: {}",
                    many.len(),
                    many.iter()
                        .map(|(a, n)| format!("{n} @ {a:#010x}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                None
            }
        }
    }

    /// Read a word of guest memory from the host side (a `--probe`, a test),
    /// through the bus's own decode.
    pub fn peek_word(&mut self, address: u32) -> Option<u32> {
        let saved = self.bus.pc();
        self.bus.set_pc(0);
        let value = self.bus.read_word(address).ok().map(|v| v as u32);
        self.bus.set_pc(saved);
        value
    }

    /// Write a word of guest memory from the host side — the same decode
    /// [`peek_word`](Self::peek_word) reads through.
    pub fn poke_word(&mut self, address: u32, value: u32) -> bool {
        let saved = self.bus.pc();
        self.bus.set_pc(0);
        let ok = self.bus.write_word(address, value as i32).is_ok();
        self.bus.set_pc(saved);
        ok
    }

    /// The word at a symbol, for `--probe <symbol>:<cycle>`.
    pub fn peek_symbol(&mut self, name: &str) -> Option<(u32, u32)> {
        let address = self.resolve_symbol(name)?;
        self.peek_word(address).map(|v| (address, v))
    }

    /// Stop the run the first time `symbol` is reached. Installs a hook that
    /// returns [`HookResult::Stop`].
    pub fn break_at(&mut self, symbol: &str) -> Result<u32, RomError> {
        let address = self
            .resolve_symbol(symbol)
            .ok_or_else(|| RomError::NoSuchSymbol(symbol.to_string()))?;
        // The name is leaked so the table can hold a `&'static str`: a
        // `--break-at` is set up once per run and there are at most a
        // handful.
        let leaked: &'static str = Box::leak(symbol.to_string().into_boxed_str());
        self.hooks
            .install_at(&mut self.bus, address, leaked, |_| HookResult::Stop)
    }

    // ---- the run loop ----------------------------------------------------

    /// Run until `stop` says otherwise. See the module docs for the loop and
    /// the bring-up rule.
    pub fn run_until(&mut self, stop: &StopCondition) -> Outcome {
        let started = Instant::now();
        let stop_cycle = stop.stop_cycle.unwrap_or(u64::MAX);
        let mut probes = stop.probes.clone();
        probes.sort_by(|a, b| a.0.cmp(&b.0));
        let mut next_probe = 0usize;

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

            // M3's invariant, asserted every slice rather than assumed: core
            // 1 is stalled and is never given time. P4 turns the field into
            // a DPORT view; until then a slice for slot 1 would be a bug
            // nobody wrote a test for.
            assert!(
                self.stalled[1],
                "core 1 is not stalled: M3 is single-core (Q5) and has no scheduler for two"
            );

            self.bus.set_time(now);
            self.bus.set_hart(0);
            let end = self.harts[0].run_slice(&mut self.bus, budget);

            match end {
                SliceEnd::BudgetExhausted | SliceEnd::BusYield => {}
                SliceEnd::Wfi => {
                    // The deterministic idle skip: nothing can happen before
                    // the next scheduled event, so move guest time there.
                    let wake = self
                        .bus
                        .sched
                        .next_deadline()
                        .unwrap_or(stop_cycle)
                        .max(self.cycles() + 1)
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

            let at = self.cycles();
            self.bus.run_due_events(at);
            let external = self.bus.pending_cpu_interrupt_mask();
            self.harts[0].set_external_mask(external);
            self.harts[0].poll_interrupts();
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
            if let Some(limit) = stop.wall_timeout
                && started.elapsed() >= limit
            {
                return Outcome::WallTimeout {
                    cycle: self.cycles(),
                };
            }
        }
    }

    /// Give the hook table first refusal on a `break` at `pc`. Returns
    /// `false` when nothing claims it, in which case the guest gets the
    /// architectural breakpoint.
    fn serve_breakpoint(&mut self, pc: u32) -> bool {
        let Some(hook) = self.hooks.get(pc) else {
            return false;
        };
        self.hook_calls += 1;
        if self.bus.trace.is_enabled() {
            let line = format!("HOOK {} at {pc:#010x}", hook.symbol);
            self.bus.trace.note(&line);
        }
        match (hook.call)(self) {
            HookResult::Ret => {
                // The **CALL0** return: `pc = a0`. A hook standing in for a
                // *windowed* routine would have to perform `retw`'s window
                // rotation itself; none exists, and the table ships empty
                // (see `rom`'s module docs). Recorded here so the first hook
                // that needs `retw` finds the note instead of the bug.
                let ra = self.harts[0].cpu().a(0);
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
            Some((address, value)) => {
                log::info!("probe cycle={at} {name} @ {address:#010x} = {value:#010x} ({value})")
            }
            None => log::warn!("probe cycle={at} {name}: no such symbol"),
        }
    }

    // ---- snapshot --------------------------------------------------------

    /// Everything a run's future depends on. See [`crate::snapshot`].
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            harts: self.harts.clone(),
            stalled: self.stalled,
            regions: self.bus.save_regions(),
            periph: self.bus.save_peripherals(),
            scalars: self.bus.save_scalars(),
            sched: self.bus.sched.save(),
            rng: self.rng,
            hook_calls: self.hook_calls,
            idle_skips: self.idle_skips,
        }
    }

    /// Put the machine back exactly where [`snapshot`](Self::snapshot) took
    /// it.
    pub fn restore(&mut self, snap: &Snapshot) {
        self.harts = snap.harts.clone();
        self.stalled = snap.stalled;
        self.bus.restore_regions(&snap.regions);
        self.bus.restore_peripherals(&snap.periph);
        self.bus.restore_scalars(&snap.scalars);
        self.bus.sched.restore(&snap.sched);
        self.rng = snap.rng;
        self.hook_calls = snap.hook_calls;
        self.idle_skips = snap.idle_skips;
    }
}

impl fmt::Debug for Machine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Machine")
            .field("boot_mode", &self.boot_mode)
            .field("time_grade", &self.time_grade)
            .field("cores", &CORES)
            .field("stalled", &self.stalled)
            .field("cycle", &self.cycles())
            .field("pc", &format_args!("{:#010x}", self.harts[0].pc()))
            .finish()
    }
}
