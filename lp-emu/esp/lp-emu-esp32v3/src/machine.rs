//! The classic machine: two hart slots, a bus, a mask ROM and a run loop.
//!
//! `lp-emu-esp32c6`'s twin in shape, and the two places it differs are the
//! two things about this chip that matter:
//!
//! - the hart is [`lp_xt_emu::mach::XtHart`] rather than
//!   `lp_riscv_emu::mach::MachineHart`, and
//! - **`harts` has two slots, and the run loop gives every core that is not
//!   held a window per iteration on one guest clock** — the deterministic
//!   quantum interleave, plan decision D3 (M4 P1).
//!
//! # Two cores, one clock
//!
//! The classic is dual-core. Both slots are constructed — each holds
//! architectural state, appears in a snapshot, and is named by `--probe` and
//! the run report — and [`Machine::run_until`] hands **each core that is not
//! held** a window of at most [`CORE_QUANTUM_DEFAULT`] cycles
//! (`--core-quantum`) per iteration, core 0 then core 1, on **one** guest
//! clock: every running core is given the same window, due scheduler events
//! fire between windows and never inside one, and the machine's clock is the
//! furthest any hart has got. A core that is held costs nothing and its
//! counters do not move; a core parked in `waiti` costs nothing either — it
//! is not given a window until an interrupt is *taken*, which is what
//! `waiti` means on silicon. When every running core is parked, guest time
//! jumps to the next thing that can wake any of them (the deterministic idle
//! skip): a scheduled event, the host's next service, a scripted byte, or
//! **either hart's own `CCOMPARE` match** (ruling R9).
//!
//! The interleave is a pure function of the instruction streams, the
//! scripted input and the quantum. It is **not silicon's scheduling**: on
//! silicon the two cores run at once, and here a store by core 0 is visible
//! to core 1's next load with no store buffer and no cache-coherence window
//! in between — cross-core visibility is *stronger* than the part's, on
//! purpose and stated (D3). A firmware race that needs a weak memory model to
//! reproduce will not reproduce here, and two quanta are two different
//! interleavings whose cycle counts may legitimately differ. What may not
//! differ between quanta is anything the guest can observe about itself:
//! the console bytes. A quantum that changes the console is a race the model
//! is hiding, and `tests/determinism.rs` asserts it does not.
//!
//! # What "held" means here
//!
//! On silicon core 1 is held by three inputs, and [`Machine::core_stalled`]
//! is the OR of them: the machine's own field (a core nothing has started),
//! RTC_CNTL's two-register stall key (`options0.sw_stall_appcpu_c0` plus
//! `sw_cpu_stall.sw_stall_appcpu_c1`, which stall only when the pair reads
//! `0x86` — `with_app_core_stalled` uses this around every flash write), and
//! DPORT's `appcpu_ctrl_*` (`appcpu_resetting`, `appcpu_clkgate_en`,
//! `appcpu_runstall`: [`crate::periph::dport::AppCoreControl::holds_core1`]).
//!
//! # How core 1 starts
//!
//! **At `appcpu_ctrl_d`'s address, with no ROM code executed.**
//! `esp_hal::system::CpuControl::start_app_core` writes the entry into
//! `appcpu_ctrl_d.appcpu_boot_addr`, sets `clkgate_en`, clears `runstall`
//! and pulses `appcpu_resetting`
//! (`third_party/esp-hal/src/soc/esp32/cpu_control.rs`, `start_core1`).
//! The DPORT view yields to the machine at the store that completes the
//! release, and [`Machine::service_app_core_start`] puts slot 1 straight at
//! `appcpu_boot_addr` in the state esp-hal's trampoline is entered in — see
//! [`Machine::service_app_core_start`] for the field-by-field list and
//! [`APP_CORE_RELEASE_PS`] for the `PS` word. No `_ResetVector`, no unpack
//! or bss tables, no ROM `main`.
//!
//! ## Why this is the model, and what is still owed
//!
//! **The bench ruled it.** This machine's first model (PR #692 as first
//! written) did the architectural thing: released core 1 at `_ResetVector`
//! and let the vendored ROM's reset path run. That path is real — its
//! unpack and bss tables' `flag = 1` entries re-copy `.data_xtos_pro`
//! (`0x3ffe0440..0x3ffe0858`) and zero `.bss_xtos_pro`
//! (`0x3ffe0860..0x3ffe1320`), and ROM `main` writes seven
//! `_xtos_set_exception_handler` pairs into the same span — and running it
//! killed the shipped image in `LpFs::read_file`, because `fw-esp32v3`
//! hands exactly that span to its allocator as heap region 0.
//!
//! Lab task L2 (PR #695) put the question to the DOM-Z-102 with a canary:
//! allocate 4,096 B of `0xA5` at `0x3ffe0440` through the product's own
//! `init_board`, call the product's `start_app_core_isr`, wait for the
//! bind, and scan. Two sittings, two flashes, identical:
//!
//! ```text
//! [APPCORE-CANARY] start_app_core_isr bound=true wait_us=210
//! [APPCORE-CANARY] scan changed=0 ranges=0 handler_words=0 data_xtos_pro=0 bss_xtos_pro=0 outside=0
//! [APPCORE-CANARY] verdict=B
//! ```
//!
//! **Not one byte.** Not the tables, not even ROM `main`'s handler pairs —
//! whose ROM-written values the same capture shows present *before* the
//! canary buried them, so the scan can see them. Nothing in the ROM's reset
//! path runs on core 1 when esp-hal starts it. The same harness on this
//! machine scored `verdict=A`, `changed=3800`; that was the emulator's
//! claim and the bench refused it
//! (`docs/defects/2026-09-11-app-core-release-modelled-as-a-rom-reset.md`).
//!
//! **What is owed: the hardware rationale.** *Why* the `appcpu_resetting`
//! pulse does not re-run the ROM is not established here. The plausible
//! reading — the APP core has sat parked since power-on in ROM `main`'s
//! `appcpu_boot_addr` poll, and the pulse either leaves that poll running
//! or lands the core on a fast path that takes `ctrl_d` directly — matches
//! the bench, but nothing in this repository proves it and it is **not**
//! claimed. What is claimed is the observable the bench pinned: after the
//! release, core 1 executes the firmware's entry and writes nothing into
//! `0x3ffe0440..0x3ffe1320`. A machine that reproduces the observable and
//! states the mechanism as unknown is honest; one that invents the
//! mechanism to justify the observable is not.
//!
//! **Power-on is modelled the same way.** On a ROM-up boot core 1 is held
//! from the first cycle by DPORT's reset values (`appcpu_resetting = 1`,
//! `clkgate_en = 0`), so nothing runs on it until the firmware's release —
//! and whether silicon's APP core ran its own ROM path at power-on, before
//! the firmware's heap existed, is unobservable to both the firmware and
//! the bench: the PRO core's own boot re-unpacks the same spans afterwards.
//! So this machine runs no ROM on core 1 at power-on either, and says so
//! rather than pretending the question was answered.
//!
//! The ROM's APP-core path, for the record, is this — read off the vendored
//! ELF with `xtensa-esp32-elf-objdump`. It is what the bench says does
//! *not* run, kept because a future reading of the open question starts
//! here:
//!
//! ```text
//! 40000456 <_ResetHandler+0x6>:  rsr.prid a2
//! 40000459:  l32r a3, (0xabab)  ; bne a2, a3, .noappfastboot   — the PRO core
//! 4000045f:  l32r a3, (0x3ff00038); l32i a3, a3, 0   — DPORT.appcpu_ctrl_d
//! 40000465:  bbci a3, 31, .noappfastboot  — bit 31 clear: no "app fast boot"
//! ...                                       (esp-hal's address has it clear)
//! 40000704 <_start>:   a1 = __stack (0x3ffe3f20), PS = 0x40020, call4 main
//! 400076d1 <main+0xd>: rsr.prid; bne → the PRO arm
//! 400076dd <main+0x19>: memw; l32i a2, (0x3ff00038); beqz a2, main+0x19
//!                       — the APP core's own wait loop on appcpu_boot_addr
//! 40007bcf <main+0x50b>: user_code_start = a2; eight
//!                        _xtos_set_exception_handler calls; callx8 a2
//! ```
//!
//! `_start`'s `PS = 0x40020` and the APP core's ROM stack top are where the
//! released core's seeded state comes from: the ROM is still the source for
//! *what state the firmware's entry is entered in*, even though its code no
//! longer runs. [`Machine::service_app_core_start`] cites each field.
//!
//! What is *not* reproduced is a timing claim: esp-hal's caller waits up to
//! 10 ms of **guest** time for the bind, and this machine gets core 1 there
//! within a window or two. That is the guest's own wait, never a gate.
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
use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::strip::ws281x::{Frame, Ws281xDecoder, unpermute};
use lp_emu_esp_common::{ByteLog, ByteSink, ByteSource, ElfImage, SocBus, Strap};
use lp_ws281x::{ChannelTiming, ColorOrder};

use crate::control::{Cable, CableReport, ControlCommand, ControlReply};
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

/// The default per-core window, in guest cycles (`--core-quantum`; ruling
/// R2 of M4). D3 says "low hundreds": a window this size puts ~30 windows
/// inside the shortest thing either core waits on (the io_task's 1 ms
/// pacer is 240,000 cycles; the pusher's refill deadline is 80 µs = 19,200)
/// and costs a few per cent in per-window bookkeeping. It is the *upper*
/// bound on one hart's window — a scheduled event, a probe, a host service
/// or a strict slice still shortens it. It is a **parameter**, recorded in
/// the run report, in `core_report()` and in the snapshot — never a tuned
/// constant, and never a claim about silicon's scheduling.
pub const CORE_QUANTUM_DEFAULT: u64 = 256;

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
    // Direct load, stop 4: `measure_rtc_clock` reads `TIMG0.rtccalicfg`.
    "TIMG0",
    // Direct load, stop 5: the ROM's `rom_chip_i2c_writeReg` programs the
    // BBPLL through the analog I2C master — on the AHB bus.
    "I2C_ANA_MST",
    // Direct load, stop 6: `esp_hal::init` disables TIMG1's watchdog
    // (`wdtwprotect` first, then `wdtconfig0`).
    "TIMG1",
    // Direct load, stop 7: `Uart::new(…).with_rx(GPIO3)` routes U0RXD
    // through the GPIO matrix.
    "GPIO",
    // Direct load, stop 8: `Uart::new` reads `conf0` to pick the UART's
    // clock source.
    "UART0",
    // Direct load, stop 9: `Uart::new(…).with_tx(GPIO1)` configures the
    // U0TXD pad.
    "IO_MUX",
    // Direct load, stop 10: esp-storage's flash read reaches SPI1 — the
    // last P3 stop on this path; the run then spins on `cmd.usr` (P7).
    "SPI1",
    // Direct load, stop 11: the ROM's idle wait polls SPI0's state machine
    // as well as SPI1's.
    "SPI0",
    // ROM-up, stop 1: `_ResetHandler_efuse_check_patch` reads its own
    // fuses seven instructions after the reset vector.
    "EFUSE",
    // ROM-up, cycle 7,430: `gpio_pad_unhold` reads `RTC_IO.dig_pad_hold`.
    "RTC_IO",
    // ROM-up, in `main`: `uartAttach` (`0x4000_9013`) touches `UART1 +0x10`
    // as well as UART0's, at cycle 30,992 — before `mmu_init` below. The
    // application never opens it, so this is the first place in the boot that
    // meets it. P6.
    "UART1",
    // ROM-up, after the eFuse gate: `mmu_init` clears both flash MMU tables
    // and `cache_flash_mmu_set` fills them (P4). The direct load never
    // reaches them, so this is where the boot meets them — last.
    "FLASH_MMU",
    // ROM-up, last of all: the ESP-IDF second-stage bootloader hashes the
    // application image before it will run it, through the mask ROM's
    // `ets_sha_*` family. Nothing earlier on either path touches it — the
    // direct load has no bootloader, and the mask ROM's own reset path does
    // not hash anything.
    // ROM-up, cycle 9,284,455: the IDF bootloader's RNG early entropy
    // source reads `SENS.sar_read_ctrl2`.
    "SENS",
    // ROM-up, 112 cycles later: the same step reads the SAR through I2S0.
    "I2S0",
    // ROM-up, cycle 22,247,148: `bootloader_fill_random()` reads
    // `WDEV_RND_REG` on the AHB bus (ruling R4).
    "RNG",
    "SHA",
    // Both paths, last of all — the block between `flash filesystem mounted`
    // and the idle heartbeat: `init_board`'s `Channel::new` reads
    // `RMT.ch0conf1` at cycle 5,640,047 (direct) / 65,360,003 (ROM-up). P8's
    // accept block; M4's waveform.
    "RMT",
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
    /// `PS.OWB` at the application's entry — see [`BOOTLOADER_OWB`].
    pub owb: u8,
}

/// `PS.OWB` the ESP-IDF second-stage bootloader leaves at the application's
/// entry: **7**.
///
/// **Measured, not reasoned.** P7 wrote the direct-vs-ROM-up cross-check and
/// it skipped, because the ROM-up walk stopped one instruction short of the
/// application; M1 P6 landed `rer`/`wer`, the walk reached the entry, and
/// `rom_up_and_direct_load_agree_on_what_the_app_sees` compared `PS` for the
/// first time. ROM-up read `0x0006_0720`, the direct load `0x0006_0020`: one
/// field, `OWB` (bits 11:8), 7 against 0.
///
/// OWB is the Old Window Base a window exception saves, and nothing in the
/// application's `Reset` reads it — the field is scratch until the next
/// window exception overwrites it, and the boot is unchanged either way. It
/// is seeded anyway, for the reason the whole direct path exists: the claim
/// is *"what the classic ROM and the IDF bootloader leave behind"*, and a
/// field this repository can measure and chooses not to reproduce makes that
/// claim smaller for nothing. It is also the cheapest kind of latent bug —
/// a field nobody reads until somebody does.
///
/// It is **this bootloader's** number, not the architecture's: it is where
/// `v5.1-beta1-378-gea5e0ff298`'s own call depth happened to leave the
/// window when it jumped. A different bootloader would leave a different one,
/// and the cross-check is what would say so.
pub const BOOTLOADER_OWB: u8 = 7;

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

    /// The mask ROM's **APP**-core stack top, from the ROM ELF's
    /// `__stack_app` — `0x3FFE_7E30`, which is also
    /// `reserved_rom_stack_app`'s end (`third_party/esp-hal/ld/esp32/
    /// memory.x:35`) and `dram2_seg`'s origin. Three sources, one number,
    /// the same shape as [`BootFrame::rom_pro_stack`].
    ///
    /// This is what core 1's `a1` is seeded with at the release
    /// ([`Machine::service_app_core_start`]). The `owb` field is *not* the
    /// bootloader's: the released core is the outermost frame and no window
    /// exception has run on it, so `PS.OWB` is zero — see
    /// [`APP_CORE_RELEASE_PS`], which is the word actually written.
    pub fn rom_app_stack(rom: &ElfImage) -> Self {
        let sp = rom
            .symbol("__stack_app")
            .map(|s| s.address)
            .unwrap_or(memmap::ROM_APP_STACK_TOP);
        Self {
            owb: 0,
            ..Self::at(sp)
        }
    }

    /// A boot frame at an explicit stack pointer, with the default save area.
    pub const fn at(sp: u32) -> Self {
        Self {
            sp,
            save_area: [0, sp, 0, 0],
            owb: BOOTLOADER_OWB,
        }
    }
}

/// Why a run stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The emulated deadline was reached with no fault.
    Deadline { cycle: Cycles },
    /// A hart cannot continue. `core` names which.
    Fault {
        core: usize,
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
    /// A `--break-at` symbol was reached on `core`; that core is stopped at
    /// the symbol's first instruction with every register as the caller
    /// left it.
    Breakpoint { core: usize, cycle: Cycles, pc: u32 },
    /// **Ruling R4.** A fill met a flash-MMU entry on which the two cores'
    /// tables disagree, under `--app-mmu-divergence stop` (the default). See
    /// [`crate::cache::MmuDivergencePolicy`] and
    /// [`Machine::mmu_divergence_message`].
    MmuDivergence {
        cycle: Cycles,
        divergence: crate::cache::MmuDivergence,
    },
    /// **D4.** A core reached through a flash window with its own read cache
    /// disabled. See [`crate::cache`] for the design and
    /// [`Machine::cache_off_message`] for what the message claims and what it
    /// does not.
    CacheOffFetch {
        cycle: Cycles,
        pc: u32,
        symbol: Option<String>,
        access: crate::cache::CacheOffAccess,
    },
    /// The auto-reset circuit released EN and the run was **not** asked to
    /// perform reboots ([`Esp32V3Builder::reboot_on_reset`]). The strap is
    /// what IO0 was holding at the release.
    Reset { cycle: Cycles, strap: Strap },
    /// `--exit-on`'s line appeared on UART0 and the run stopped there.
    ExitMatched { cycle: Cycles },
}

impl Outcome {
    /// The CLI's exit code for this outcome. The C6's contract, so a script
    /// that drives both machines reads one table.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Outcome::ExitMatched { .. } | Outcome::Deadline { .. } => 0,
            Outcome::Fault { .. } | Outcome::Reset { .. } => 2,
            Outcome::StrictBus { .. } => 3,
            Outcome::WallTimeout { .. } => 4,
            Outcome::Breakpoint { .. } => 5,
            // Six is new with P4 and extends the C6's table rather than
            // reusing one of its codes: a script that drives both machines
            // reads 0/2/3/4/5 the same way on either, and 6 is a stop only
            // this chip can produce.
            Outcome::CacheOffFetch { .. } => 6,
            // Seven is new with M4 P1 (ruling R4), for the same reason six
            // was: a stop only this chip can produce.
            Outcome::MmuDivergence { .. } => 7,
        }
    }

    pub const fn cycle(&self) -> Cycles {
        match self {
            Outcome::Deadline { cycle }
            | Outcome::Fault { cycle, .. }
            | Outcome::WallTimeout { cycle }
            | Outcome::Breakpoint { cycle, .. }
            | Outcome::CacheOffFetch { cycle, .. }
            | Outcome::MmuDivergence { cycle, .. } => *cycle,
            Outcome::Reset { cycle, .. } | Outcome::ExitMatched { cycle } => *cycle,
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
    /// Stop when this line appears in UART0's bytes (`--exit-on`). Matched
    /// against the bytes that have **left the shifter**, once the line they
    /// are on is complete.
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

/// Where a run's UART0 bytes go — and, for `Tcp`, where its RX bytes come
/// from. Whatever the choice, the bytes are also kept in memory for
/// `--exit-on` and [`Machine::uart0`].
#[derive(Clone, Debug, Default)]
pub enum Uart0Sink {
    /// Collected in memory only.
    #[default]
    Memory,
    Stdout,
    File(PathBuf),
    /// **Listen** on this address (`127.0.0.1:5555`) for one client at a
    /// time; the client's bytes are UART0's RX. A run with a live socket is
    /// not deterministic — `--uart0-script` is the deterministic path.
    ///
    /// ⚠️ On this chip a client connecting is **not** a port open and moves
    /// no chip state: the port is a bridge chip on the board (`crate::control`).
    Tcp(String),
}

/// Where decoded WS281x frames go (`--dump-frames`).
///
/// One JSON line per frame, as it is decoded — a stream, not a report, so a
/// run that is killed still leaves the frames it had already seen.
#[derive(Clone, Debug, Default)]
pub enum FrameSink {
    /// Kept in memory only, for [`Machine::frames`].
    #[default]
    Memory,
    Stdout,
    File(PathBuf),
}

/// Where the raw pin log goes (`--pin-log`): one line per edge.
///
/// Never the default: a 300-LED frame is 14,400 edges and a second of the
/// desk board's five wires is over two million.
#[derive(Clone, Debug, Default)]
pub enum PinLogSink {
    #[default]
    Off,
    File(PathBuf),
}

/// Lines the pin log writes before it stops, with a closing note.
pub const PIN_LOG_LINE_CAP: u64 = 2_000_000;

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
    pub decoders: std::collections::BTreeMap<u8, Ws281xDecoder>,
    /// Per pad, the frames it has completed.
    pub frames: std::collections::BTreeMap<u8, Vec<Frame>>,
    /// Per pad, what it is routed to right now.
    pub routed: std::collections::BTreeMap<u8, RouteSource>,
    /// Per pad, edges seen.
    pub edges: std::collections::BTreeMap<u8, u64>,
    /// Pads whose frame list reached [`FRAMES_PER_PAD_CAP`].
    pub capped: std::collections::BTreeSet<u8>,
}

/// The pads, online: one decoder per routed pad, fed from the fabric every
/// window, plus the two host sinks.
///
/// ⚠️ **Two cores, one fabric.** The edges are drained once per *window*, not
/// once per core, and the cycle the decoder reads is the edge's own `at` —
/// stamped by whoever drove the signal, not by the drain. So the interleave
/// cannot reorder a waveform, and a frame decoded here is the same frame at
/// any `--core-quantum`. That is the first thing a reader of a two-core
/// machine will want to know about a decoded frame, so it is said here and
/// in the crate README's "The pad".
///
/// `cpu_hz` is a **parameter** of [`Ws281xDecoder`] and the classic's is
/// [`memmap::CPU_HZ`] = 240 MHz against the C6's 160: one WS2812 bit is a
/// different number of cycles here and every threshold follows from it.
struct PinObserver {
    strip: StripConfig,
    cpu_hz: u64,
    state: PinState,
    /// The last [`lp_emu_esp_common::pins::Fabric::route_epoch`] seen, so a
    /// window that changed no routing walks no pads.
    epoch: u64,
    dump: Option<Box<dyn std::io::Write + Send>>,
    pin_log: Option<Box<dyn std::io::Write + Send>>,
    pin_log_lines: u64,
    pin_log_capped: bool,
}

/// One decoded frame as a JSON line — the `--dump-frames` record.
///
/// `wire` is what the wire carried and `rgb` is that unpermuted with the
/// configured order: the frame the *driver* was handed. Both are in the
/// record on purpose, so a wrong order assumption is a visible difference
/// between two fields rather than a silent one inside `rgb`.
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

/// Lowercase hex, no separators — the shape `[ORACLE] rgb=` and the
/// firmware's own `[OUT] dump` line already use.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
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

/// A `ByteSink` over a file handle. Buffered: UART0 drains one byte at a
/// time in emulated time, and a `write(2)` per byte is measurable.
struct FileSink(std::io::BufWriter<std::fs::File>);

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

/// The control channel's listener plus the tail of a line that arrived in
/// pieces. One reply per command, `\n`-terminated, in [`crate::control`]'s
/// grammar.
struct ControlChannel {
    host: lp_emu_esp_common::TcpHost,
    partial: Vec<u8>,
}

/// The longest control line the channel will assemble. A client that sends
/// more without a newline is answered once and resynchronised.
const MAX_CONTROL_LINE: usize = 4 << 10;

/// How often the host's side is looked at, in guest cycles: 1 ms, the C6's
/// cadence and the UART's own live-source poll.
const HOST_POLL_CYCLES: Cycles = crate::periph::uart::LIVE_POLL_CYCLES;

/// Builds a [`Machine`].
pub struct Esp32V3Builder {
    rom: RomSource,
    app: AppSource,
    boot_mode: BootMode,
    time_grade: TimeGrade,
    strict: bool,
    strict_unsupported: bool,
    boot_frame: Option<BootFrame>,
    flash_backing: crate::flash::FlashBacking,
    flash_len: u32,
    reset_cause: loader::ResetCause,
    efuse: loader::EfuseIdentity,
    cache_off: crate::cache::CacheOffPolicy,
    app_mmu_divergence: crate::cache::MmuDivergencePolicy,
    core_quantum: u64,
    boot_set: bool,
    /// Keep the RMT's pulse and word logs ([`crate::periph::rmt::Rmt`]).
    rmt_logs: bool,
    /// Where decoded frames go (`--dump-frames`).
    dump_frames: FrameSink,
    /// Where the per-edge pin log goes (`--pin-log`).
    pin_log: PinLogSink,
    /// How every routed pad is decoded (`--strip-timing` / `--strip-order`).
    strip: StripConfig,
    trace: Option<Box<dyn std::io::Write + Send>>,
    trace_blocks: Vec<String>,
    seed: u64,
    uart0: Uart0Sink,
    uart0_source: Option<Box<dyn ByteSource>>,
    uart0_script: Option<lp_emu_esp_common::ScriptedSource>,
    uart0_baud: Option<u64>,
    control: Option<String>,
    control_script: Vec<(Cycles, ControlCommand)>,
    reboot_on_reset: bool,
    strap_word: u32,
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
            flash_backing: crate::flash::FlashBacking::default(),
            flash_len: crate::flash::DEFAULT_FLASH_LEN,
            reset_cause: loader::ResetCause::default(),
            efuse: loader::EfuseIdentity::default(),
            cache_off: crate::cache::CacheOffPolicy::default(),
            app_mmu_divergence: crate::cache::MmuDivergencePolicy::default(),
            core_quantum: CORE_QUANTUM_DEFAULT,
            boot_set: true,
            rmt_logs: false,
            dump_frames: FrameSink::default(),
            pin_log: PinLogSink::default(),
            strip: StripConfig::default(),
            trace: None,
            trace_blocks: Vec::new(),
            seed: 0,
            uart0: Uart0Sink::default(),
            uart0_source: None,
            uart0_script: None,
            uart0_baud: None,
            control: None,
            control_script: Vec::new(),
            reboot_on_reset: false,
            strap_word: crate::periph::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT,
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
            .field("flash_backing", &self.flash_backing)
            .field("flash_len", &self.flash_len)
            .field("reset_cause", &self.reset_cause)
            .field("efuse", &self.efuse)
            .field("cache_off", &self.cache_off)
            .field("app_mmu_divergence", &self.app_mmu_divergence)
            .field("core_quantum", &self.core_quantum)
            .field("boot_set", &self.boot_set)
            .field("trace", &self.trace.is_some())
            .field("trace_blocks", &self.trace_blocks)
            .field("seed", &self.seed)
            .field("uart0", &self.uart0)
            .field("uart0_source", &self.uart0_source.is_some())
            .field("uart0_script", &self.uart0_script.is_some())
            .field("uart0_baud", &self.uart0_baud)
            .field("control", &self.control)
            .field("control_script", &self.control_script.len())
            .field("reboot_on_reset", &self.reboot_on_reset)
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

    /// Where the flash chip's bytes come from and whether they go back:
    /// `--flash <path>` is read-write, `--flash-copy <path>` reads once and
    /// never writes, and the default is a blank chip that lives and dies
    /// with the process ([`crate::flash`]).
    pub fn flash(mut self, backing: crate::flash::FlashBacking) -> Self {
        self.flash_backing = backing;
        self
    }

    /// The flash chip's size. This is the number the JEDEC capacity byte and
    /// the ROM's `chip_size` word are both derived from
    /// ([`loader::seed_rom_flash_chip`]); default
    /// [`crate::flash::DEFAULT_FLASH_LEN`], the desk board's 4 MiB.
    pub fn flash_len(mut self, bytes: u32) -> Self {
        self.flash_len = bytes;
        self
    }

    /// What the machine asserts the reset cause was (loader item 7).
    pub fn reset_cause(mut self, cause: loader::ResetCause) -> Self {
        self.reset_cause = cause;
        self
    }

    /// The part this run claims to be: the MAC and the chip revision the
    /// eFuse block answers with, and — through `APB_CTRL.date` bit 31 — the
    /// top bit of that revision. Default: the desk board, `30:76:f5:ec:f6:34`
    /// and v3.1 (`../bench.md`).
    pub fn efuse(mut self, identity: loader::EfuseIdentity) -> Self {
        self.efuse = identity;
        self
    }

    /// `--cache-off-fetch` (D4). The default is
    /// [`crate::cache::CacheOffPolicy::Stop`], and that is what every gate
    /// run uses.
    pub fn cache_off_fetch(mut self, policy: crate::cache::CacheOffPolicy) -> Self {
        self.cache_off = policy;
        self
    }

    /// `--app-mmu-divergence` (ruling R4). The default is
    /// [`crate::cache::MmuDivergencePolicy::Stop`], and that is what every
    /// gate run uses.
    pub fn app_mmu_divergence(mut self, policy: crate::cache::MmuDivergencePolicy) -> Self {
        self.app_mmu_divergence = policy;
        self
    }

    /// `--core-quantum <cycles>`: the upper bound on one core's window
    /// (D3). Default [`CORE_QUANTUM_DEFAULT`]. Zero is refused by the CLI;
    /// here it is clamped to one, because a zero-cycle window would run
    /// nothing forever.
    pub fn core_quantum(mut self, cycles: u64) -> Self {
        self.core_quantum = cycles.max(1);
        self
    }

    /// Keep the RMT's per-channel pulse and word logs
    /// ([`crate::periph::rmt::Rmt::set_keep_logs`]).
    ///
    /// **Off by default**: a 300-LED frame is 7,201 words and 14,402 pulses,
    /// which a run that only wants a boot has no use for. The logs are the
    /// word-level oracle a decoder test compares against, so the gates that
    /// need them ask for them.
    pub fn rmt_logs(mut self, keep: bool) -> Self {
        self.rmt_logs = keep;
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

    /// Where UART0's bytes go (`--uart0`).
    pub fn uart0(mut self, sink: Uart0Sink) -> Self {
        self.uart0 = sink;
        self
    }

    /// Where decoded WS281x frames go (`--dump-frames`). The default keeps
    /// them in memory only, for [`Machine::frames`].
    pub fn dump_frames(mut self, sink: FrameSink) -> Self {
        self.dump_frames = sink;
        self
    }

    /// Where the per-edge pin log goes (`--pin-log`). Off by default: this is
    /// the rawest of the three readings and the largest by far.
    pub fn pin_log(mut self, sink: PinLogSink) -> Self {
        self.pin_log = sink;
        self
    }

    /// How every routed pad is decoded (`--strip-timing` / `--strip-order`).
    /// The default is the driver's own: WS2812 timing, GRB on the wire.
    pub fn strip(mut self, order: ColorOrder, timing: ChannelTiming) -> Self {
        self.strip = StripConfig { timing, order };
        self
    }

    /// Deterministic host input for UART0, as anything that is not a socket.
    pub fn uart0_source(mut self, source: Box<dyn ByteSource>) -> Self {
        self.uart0_source = Some(source);
        self
    }

    /// Deterministic host input written as a script
    /// ([`lp_emu_esp_common::ScriptedSource`]). Preferred over
    /// [`uart0_source`](Self::uart0_source), because the builder hands it the
    /// UART0 log — without which an `after "<line>"` step has nothing to
    /// watch and never fires.
    pub fn uart0_script(mut self, script: lp_emu_esp_common::ScriptedSource) -> Self {
        self.uart0_script = Some(script);
        self
    }

    /// What `GPIO.strap` reads: the strapping pins as the pads were latched
    /// at reset (`--strap`).
    ///
    /// **An input to the run**, like the reset cause and the chip revision,
    /// and the default is the desk board's own — `0x13`, which is what the
    /// mask ROM prints raw as the `boot:0x%x` half of its banner
    /// (`../bench.md`: `boot:0x13 (SPI_FAST_FLASH_BOOT)`; the derivation is
    /// on [`crate::periph::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT`]). Zero is
    /// not "no straps": it is the SDIO boot mode, and the mask ROM takes it
    /// seriously enough to walk into `slc_init_attach`.
    ///
    /// ⚠️ **There is no download-mode default here, and that is deliberate.**
    /// [`Strap::Download`] on the cable's side says IO0 was low when EN was
    /// released; what word `GPIO.strap` then reads is a second fact, and this
    /// repository has not measured it (`m3/notes.md` R8: L0 exercised the EN
    /// half of the auto-reset circuit and not the IO0 half). A guessed word
    /// would send the ROM down a path nobody checked. Pass the measured one
    /// when L1 brings it back.
    pub fn strap(mut self, word: u32) -> Self {
        self.strap_word = word;
        self
    }

    /// The rate the host on the other end of the cable sends at
    /// (`--uart0-baud`, default
    /// [`crate::periph::uart::DEFAULT_HOST_BAUD`]). It changes what the
    /// auto-baud counters report and nothing else; a scripted byte still
    /// lands when the script says it does, and it never overrides what the
    /// guest programs into `clkdiv`.
    pub fn uart0_baud(mut self, baud: u64) -> Self {
        self.uart0_baud = Some(baud);
        self
    }

    /// Listen for a control-channel client on this address (`--control`).
    pub fn control(mut self, addr: impl Into<String>) -> Self {
        self.control = Some(addr.into());
        self
    }

    /// The deterministic twin of a control client (`--control-script`).
    pub fn control_script(mut self, script: Vec<(Cycles, ControlCommand)>) -> Self {
        self.control_script = script;
        self
    }

    /// Whether the auto-reset circuit releasing EN actually reboots the
    /// machine, or ends the run with [`Outcome::Reset`].
    ///
    /// Off for a plain `run`, as on the C6: a reboot costs a whole copy of
    /// guest memory taken at build time, and a run that will never reset
    /// should not pay for it.
    pub fn reboot_on_reset(mut self, reboot: bool) -> Self {
        self.reboot_on_reset = reboot;
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
        // RTC_CNTL publishes its half of the CPU stall key through this
        // handle and the machine reads it; P5 owns the two RTC_CNTL halves
        // and P4 adds DPORT's `appcpu_ctrl_*` alongside. A machine built
        // with `bare()` has no RTC_CNTL and therefore an all-zero key,
        // which reads as "not stalled" — the right answer for a block that
        // is not there.
        let stall_key = crate::periph::rtc_cntl::StallKey::new();

        // The chip's interrupt matrix, before any peripheral: the DPORT view
        // downcasts to it on its first store, and `esp32_init` performs that
        // store twenty-nine instructions in.
        bus.set_matrix(Box::new(crate::intmatrix::Esp32V3IntMatrix::new()));

        let cache = crate::cache::ClassicCache::handle();
        {
            let mut c = cache.lock().expect("cache poisoned");
            c.set_policy(self.cache_off);
            c.set_divergence_policy(self.app_mmu_divergence);
        }
        let appcpu = crate::periph::dport::AppCoreControl::handle();
        // The host's side of the wire, before any peripheral: UART0 holds a
        // `StreamId` and nothing else. Its bytes are always tee'd into memory
        // for `--exit-on`, the snapshot and `Machine::uart0`, whatever else
        // they go to.
        let uart0_log = ByteLog::new();
        let mut uart0_tcp: Option<lp_emu_esp_common::TcpHost> = None;
        // A script watches the same log the sink tees into, so an
        // `after "<line>"` step sees exactly what the device sent.
        let uart0_source = match self.uart0_script {
            Some(script) => {
                Some(Box::new(script.watching(uart0_log.clone())) as Box<dyn ByteSource>)
            }
            None => self.uart0_source,
        };
        let (inner, source): (Box<dyn ByteSink>, Box<dyn ByteSource>) = match &self.uart0 {
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
                    Box::new(FileSink(std::io::BufWriter::new(file))),
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
        let uart0_stream = bus.host.add(
            "uart0",
            Box::new(TeeSink {
                log: uart0_log.clone(),
                inner,
            }),
            source,
        );

        // The two pad sinks. Opened here, with the run's other host handles,
        // so a path that cannot be written fails the build rather than the
        // first frame — a run that decoded for ten seconds into a file it
        // could not create is worse than one that never started.
        let open_write = |path: &PathBuf| -> Result<Box<dyn std::io::Write + Send>, BuildError> {
            let file = std::fs::File::create(path)
                .map_err(|e| BuildError::Io(format!("creating {}: {e}", path.display())))?;
            Ok(Box::new(std::io::BufWriter::new(file)))
        };
        let dump_sink: Option<Box<dyn std::io::Write + Send>> = match &self.dump_frames {
            FrameSink::Memory => None,
            FrameSink::Stdout => Some(Box::new(std::io::stdout())),
            FrameSink::File(path) => Some(open_write(path)?),
        };
        let pin_log_sink: Option<Box<dyn std::io::Write + Send>> = match &self.pin_log {
            PinLogSink::Off => None,
            PinLogSink::File(path) => Some(open_write(path)?),
        };

        let control = match &self.control {
            Some(addr) => {
                let host = lp_emu_esp_common::TcpHost::listen(addr)
                    .map_err(|e| BuildError::Io(format!("listening on {addr}: {e}")))?;
                eprintln!("control listening on {}", host.local_addr());
                Some(ControlChannel {
                    host,
                    partial: Vec::new(),
                })
            }
            None => None,
        };

        // The flash chip is shared state, not a peripheral: SPI1 executes
        // commands against it and the cache fill reads through it.
        let flash: crate::flash::FlashHandle = std::sync::Arc::new(std::sync::Mutex::new(
            crate::flash::FlashImage::open(self.flash_backing, self.flash_len)
                .map_err(|e| BuildError::Io(format!("opening the flash image: {e}")))?,
        ));

        let mut peripheral_map = Vec::new();
        let mut alias_map = Vec::new();
        if self.boot_set {
            let set = crate::periph::boot_set(
                self.reset_cause,
                self.efuse,
                stall_key.clone(),
                cache.clone(),
                appcpu.clone(),
                Some(uart0_stream),
                flash.clone(),
                self.seed,
                self.strap_word,
            );
            check_registration_order(&set)?;
            for (base, len, periph) in set {
                let name = periph.name();
                peripheral_map.push((name, base, len));
                let index = bus.add_peripheral(base, len, periph);
                // The classic's second peripheral window (P3 §3.2): every
                // block from `0x3FF4_0000` up answers at its AHB address too,
                // and it is the **same state** — one registration, a second
                // decode (DD38). Registered here rather than in `boot_set`
                // because an alias needs the index the bus just handed back.
                if let Some(ahb) = memmap::dport_to_ahb(base) {
                    bus.add_peripheral_alias(ahb, len, index);
                    alias_map.push((name, ahb, len));
                }
            }
        }

        // The ROM first, always (PD7), and its non-alloc data with it.
        let rom_segments = rom::load(&mut bus, &rom_image)?;
        let rom_data = rom::seed_data(&mut bus, &rom_image)?;
        // …and the ROM's own copy of those bytes at the source addresses its
        // unpack table names, so that the reset vector's `unpcopy` — which a
        // ROM-up boot really runs — copies them rather than the zeros the
        // ELF leaves at their source addresses (`rom`'s module docs).
        let rom_data_image = rom::seed_data_image(&mut bus, &rom_image, &rom_data)?;

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
        let harts: Vec<XtHart<SocBus>> = (0..CORES)
            .map(|core| fresh_hart(core, self.time_grade, self.strict_unsupported))
            .collect();

        let mut machine = Machine {
            bus,
            harts,
            cache,
            appcpu,
            flash,
            cache_fills: 0,
            staging: loader::FlashStaging::default(),
            cache_watch: None,
            // Core 1 is held by the machine until the guest releases it
            // through DPORT (`service_app_core_start`). `core_stalled` ORs
            // this field with `stall_key` (RTC_CNTL's two halves, P5) and
            // with DPORT's `appcpu_ctrl_*` (P4).
            stalled: [false, true],
            core_quantum: self.core_quantum,
            wfi_ends: [0; CORES],
            #[cfg(feature = "bench")]
            bench_windows: [0; CORES],
            strict_unsupported: self.strict_unsupported,
            pending_breaks: Vec::new(),
            stall_key,
            rom: rom_image,
            app: app_image,
            rom_segments,
            rom_data,
            rom_data_image,
            app_segments: Vec::new(),
            flash_seed: None,
            peripheral_map,
            alias_map,
            hooks: HookTable::new(),
            app_core_frame: None,
            boot_mode: self.boot_mode,
            time_grade: self.time_grade,
            boot_frame: None,
            stop_at: None,
            hook_calls: 0,
            idle_skips: 0,
            seed: self.seed,
            rng: self.seed,
            uart0_log,
            uart0_tcp,
            control,
            control_script: self.control_script.into(),
            cable: Cable::default(),
            attached: false,
            port_open: false,
            reboot_on_reset: self.reboot_on_reset,
            reboots: 0,
            power_on: None,
            next_host_poll: 0,
            control_lines: 0,
            pending_reset: None,
            gpio_index: None,
            rmt_index: None,
            pins: PinObserver {
                strip: self.strip,
                cpu_hz: memmap::CPU_HZ,
                state: PinState::default(),
                epoch: 0,
                dump: dump_sink,
                pin_log: pin_log_sink,
                pin_log_lines: 0,
                pin_log_capped: false,
            },
        };

        machine.gpio_index = machine.bus.peripheral_index("GPIO");
        machine.rmt_index = machine.bus.peripheral_index("RMT");
        if self.rmt_logs {
            match machine.rmt_index {
                Some(i) => {
                    machine
                        .bus
                        .with_peripheral::<crate::periph::rmt::Rmt, _>(i, |r, _| {
                            r.set_keep_logs(true)
                        });
                }
                None => {
                    return Err(BuildError::App(
                        "rmt_logs was asked for and this machine has no RMT block".into(),
                    ));
                }
            }
        }

        if self.boot_mode == BootMode::Direct {
            let frame = self.boot_frame.unwrap_or_else(BootFrame::idf_bootloader);
            machine.direct_load(frame)?;
        }

        // Before the power-on snapshot, so a reboot restores the stated rate
        // rather than the default: `--uart0-baud` describes the host at the
        // other end of the cable, and that host does not change when the chip
        // resets.
        if let Some(baud) = self.uart0_baud
            && let Some(i) = machine.bus.peripheral_index("UART0")
        {
            machine
                .bus
                .with_peripheral::<crate::periph::uart::Uart, _>(i, |u, _| u.set_host_baud(baud));
        }

        // Guest time is zero and everything is placed: the one moment a
        // peripheral may schedule something before the guest touches it.
        machine.bus.start_peripherals();

        // The state a reboot goes back to, taken before a single instruction
        // runs. Only when a run asked to perform resets: it is a whole copy
        // of guest memory, and a run that will never reboot should not pay
        // for it.
        if self.reboot_on_reset {
            machine.power_on = Some(machine.snapshot());
        }

        Ok(machine)
    }
}

/// `_ResetVector`'s offset from the mask ROM's base. The ROM ELF's `e_entry`
/// is `0x4000_0400`, which is also `XCHAL_RESET_VECTOR_VADDR` in
/// `xtensa-lx-rt`'s `config/esp32.rs` — two independent sources for one
/// number (`m3/notes.md` §2), which is why `tests/boot.rs` asserts it.
pub const RESET_VECTOR_OFS: u32 = 0x400;

/// The architectural configuration of core `core`: the reset vector, the
/// reset `VECBASE`, the chip's `PRID` for that core and the fixed
/// interrupt table — the same for both cores except the `PRID`.
pub fn core_config(core: usize) -> CoreConfig {
    CoreConfig {
        reset_pc: memmap::ROM_MASK_BASE + RESET_VECTOR_OFS,
        reset_vecbase: memmap::ROM_MASK_BASE,
        prid: if core == 0 { PRID_PRO } else { PRID_APP },
        interrupts: CORE_INTERRUPTS,
    }
}

/// What `CPENABLE` holds when a core comes out of reset on this part:
/// **every coprocessor enabled**. Measured, not assumed, and found by M4 P1
/// the hard way.
///
/// The evidence: the firmware's own conformance harness read `cpenable
/// before=0x000000ff` at its first instruction on the desk board (classic
/// rev v3.1, 2026-08-06; `lp-fw/fw-esp32v3/src/board/esp32v3/fpu.rs`), and
/// **nothing between reset and that instruction writes the register** — the
/// vendored rev300 mask ROM contains no `wsr.cpenable`/`xsr.cpenable` at all
/// (`xtensa-esp32-elf-objdump -d` of `esp32_rev300_rom.elf`, zero hits), the
/// ESP-IDF second-stage bootloader in the merged image has no such encoding
/// (`t0 e0 13` / `t0 e0 61` scanned across `0x1000..0x8000`), and esp-hal
/// 1.1.1 and xtensa-lx-rt 0.22 have none either (the firmware's module says
/// so). So `0xff` is the reset state, on both cores.
///
/// `XtHart::new` leaves the ISA's generic `0`, and until M4 that was
/// harmless: the PRO core arms bit 0 itself before its first FP instruction.
/// **It is not harmless on core 1.** esp-hal is built with
/// `float-save-restore`, so its level-1 interrupt entry (`save_context`)
/// executes `rur.fcr` / `ssi f0..f15` unconditionally; with `CPENABLE = 0`
/// the first doorbell core 1 takes raises `EXCCAUSE = 32` inside the
/// handler, which is a double exception, and the core is dead
/// (`tests/dual_core.rs` pins the doorbell arriving; this constant is why it
/// is *taken*). The loader's `CPENABLE = 0` note was written against the
/// PRO core alone and is amended.
pub const CPENABLE_RESET: u32 = 0xff;

/// `PS` core 1 is released with: `WOE | UM | CALLINC(2)` = `0x0006_0020` —
/// [`PS_BOOT`], the same word a direct load seeds on core 0.
///
/// # Where the word comes from
///
/// Two halves, both cited. `WOE | UM` with `INTLEVEL = 0` and `EXCM` clear
/// is **the ROM's own post-`_start` word**, read off the vendored rev300 ELF
/// at `0x40000704` (`movi a2, 0x40020; wsr.ps a2`) — the module docs carry
/// the disassembly. `CALLINC = 2` is what reaching the firmware's entry
/// costs: esp-hal's `start_core1_init::<F>` is an ordinary windowed-ABI Rust
/// function whose prologue is `entry a1, N`, and on silicon ROM `main`
/// reaches it with `callx8`, which sets `PS.CALLINC = 2` and makes that
/// `entry` rotate the window by two register groups. It is the identical
/// argument [`PS_BOOT`] records for the IDF bootloader's `callx8` into the
/// application, on the identical kind of entry point.
///
/// # ⚠️ `CALLINC = 0` was tried, and the machine refused it
///
/// The director's ruling DD53 sketched `PS = 0x40020` — the `_start` word
/// alone, `CALLINC = 0` — reasoning that a machine which runs no ROM on core
/// 1 performs no call, so the released core should be the outermost frame.
/// That derivation misses what `entry` does. With `CALLINC = 0` it does not
/// rotate: it *consumes* the seeded frame, moving `a1` down by the frame
/// size in the same window, so the outermost live frame is now
/// `sp - N` and the [`BootFrame`] save area seeded at `[sp-16, sp)` sits
/// above it, belonging to nobody. The first window overflow deep enough to
/// wrap then runs `_WindowOverflow8`'s `l32e a0, a1, -12` over uninitialised
/// memory, reads zero, and stores through it:
///
/// ```text
/// StrictViolation { cycle: 3261708, pc: 1074266255, address: 4294967264,
///                   width: Word, access: Write, in_mmio_window: false, grade: None }
/// ```
///
/// — the stop `the_direct_load_mounts_the_flash_filesystem` reports on the
/// shipped image with this constant set to `0x0004_0020` and nothing else
/// changed; `pc = 0x400d_570f`, `address = 0xffff_ffe0`. It reproduced
/// byte-identically on two separate runs, and it is the exact failure
/// [`BootFrame`]'s docs describe for a hart left with `a1 = 0`: a spill
/// through a frame whose saved stack pointer is zero. With `CALLINC = 2` the
/// `entry` rotates past the seeded frame, that frame stays live with its save
/// area intact, and the overflow spills into the ROM's APP stack. The
/// ruling's *intent* — the ROM's state, nothing invented — is kept; its
/// arithmetic is corrected, and this is the correction.
pub const APP_CORE_RELEASE_PS: u32 = PS_BOOT;

/// A hart at the architectural reset state of **this part**, with the
/// machine's cycle model and unsupported-opcode policy applied. What `build`
/// makes both slots from, and what a core reset puts back.
fn fresh_hart(core: usize, grade: TimeGrade, strict_unsupported: bool) -> XtHart<SocBus> {
    let mut hart = XtHart::new(core as u32, core_config(core));
    hart.cpu_mut().cpenable = CPENABLE_RESET;
    hart.set_cycle_model(grade.cycle_model());
    hart.set_strict_unsupported(strict_unsupported);
    hart
}

/// The classic ESP32 machine.
pub struct Machine {
    bus: SocBus,
    /// RTC_CNTL's half of the CPU stall key
    /// ([`crate::periph::rtc_cntl::StallKey`]): both `options0` fields and
    /// both `sw_cpu_stall` fields, computed by the block that owns them.
    /// [`Machine::core_stalled`] reads it; P4 adds DPORT's
    /// `appcpu_runstall` as the third input to the same question.
    /// Breakpoints waiting for the bytes they name to appear in memory.
    /// See [`Machine::break_at_address_when`].
    pending_breaks: Vec<(u32, [u8; 3], &'static str)>,
    stall_key: crate::periph::rtc_cntl::StallKey,
    /// **Two** slots: the classic is dual-core. Slot 1 is held until the
    /// guest releases it through DPORT — see the module docs.
    pub harts: Vec<XtHart<SocBus>>,
    stalled: [bool; CORES],
    /// The upper bound on one core's window, in cycles (D3, `--core-quantum`).
    core_quantum: u64,
    /// How many windows each core has ended in `waiti` — the observable a
    /// test uses to say "the pusher parked on core 1".
    wfi_ends: [u64; CORES],
    /// How many windows each core was *given* (`--features bench` only): the
    /// denominator of "instructions per window", and with it the interleave's
    /// switch rate. Not a default-build field — the speed probe's question,
    /// not the machine's.
    #[cfg(feature = "bench")]
    bench_windows: [u64; CORES],
    /// The builder's unsupported-opcode policy, kept so a core reset
    /// (`service_app_core_start`) builds the new hart the same way.
    strict_unsupported: bool,
    /// The flash MMU tables and the cache-enable state, shared with the
    /// DPORT and FLASH_MMU views.
    cache: crate::cache::CacheHandle,
    /// `appcpu_ctrl_*`, shared with the DPORT view.
    appcpu: crate::periph::dport::AppCoreHandle,
    /// The flash chip, shared with SPI1 and read by the cache fill.
    flash: crate::flash::FlashHandle,
    /// How many window pages the fill has copied out of the chip.
    cache_fills: u64,
    /// What a direct load staged in the chip and mapped ([`loader`]).
    staging: loader::FlashStaging,
    /// Which core D4's watch is installed on the bus for right now, if any
    /// (ruling R3: the watch is armed per *running* core).
    cache_watch: Option<usize>,
    rom: ElfImage,
    app: Option<ElfImage>,
    rom_segments: Vec<PlacedSegment>,
    rom_data: Vec<SeededSection>,
    rom_data_image: DataImage,
    app_segments: Vec<PlacedAppSegment>,
    flash_seed: Option<FlashChipSeed>,
    peripheral_map: Vec<(&'static str, u32, u32)>,
    alias_map: Vec<(&'static str, u32, u32)>,
    hooks: HookTable,
    /// The stack state core 1 was released with, once it has been released
    /// ([`Machine::service_app_core_start`]). `None` while it is still held.
    app_core_frame: Option<BootFrame>,
    boot_mode: BootMode,
    time_grade: TimeGrade,
    boot_frame: Option<BootFrame>,
    /// `(core, pc)` of a `--break-at` hook that asked the run to stop.
    stop_at: Option<(usize, u32)>,
    hook_calls: u64,
    idle_skips: u64,
    seed: u64,
    rng: u64,
    /// Everything UART0 has put on the wire, in the guest's order.
    uart0_log: ByteLog,
    uart0_tcp: Option<lp_emu_esp_common::TcpHost>,
    control: Option<ControlChannel>,
    control_script: std::collections::VecDeque<(Cycles, ControlCommand)>,
    /// The two modem lines, as the host last drove them
    /// ([`crate::control`]). **Not chip state**: it is the cable's.
    cable: Cable,
    attached: bool,
    port_open: bool,
    reboot_on_reset: bool,
    reboots: u64,
    /// The state a reboot goes back to; `None` unless the run asked for one.
    power_on: Option<Snapshot>,
    /// The next cycle the host's side is looked at. **An absolute guest
    /// cycle that lives outside the snapshot** — see [`Machine::reboot`].
    next_host_poll: Cycles,
    control_lines: u64,
    /// Set when the auto-reset circuit released EN; drained at the next
    /// slice boundary by [`Machine::run_until`].
    pending_reset: Option<Strap>,
    /// `GPIO`'s peripheral index: the block the drained edges are handed to
    /// so its `status` latch sees what was on the wire. `None` on a machine
    /// built with [`Esp32V3Builder::bare`].
    gpio_index: Option<usize>,
    /// `RMT`'s peripheral index, for the observation accessors.
    rmt_index: Option<usize>,
    /// The pads, online: one [`Ws281xDecoder`] per routed pad, the frames
    /// they completed, and the two host sinks.
    ///
    /// **M4 P2** put the drain here, because the fabric had gained a driver
    /// and nobody was taking its edges off it; **P3** hangs the decoders, the
    /// `ws281x-frame` dump and the pin log off that one drain rather than
    /// adding a second.
    pins: PinObserver,
}

impl Machine {
    // ---- construction ---------------------------------------------------

    /// Place the application's `PT_LOAD`s and seed the boot state a
    /// bootloader would have left: [`crate::loader`] is the documentation.
    ///
    /// Segments, the ROM's flash chip description, then the hart — entry,
    /// [`PS_BOOT`] and the [`BootFrame`]. What a direct load does *not*
    /// reproduce is the eleven-item list in the loader's module docs.
    fn direct_load(&mut self, frame: BootFrame) -> Result<(), BuildError> {
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
        // Item 2, since P7: what the second-stage bootloader would have done
        // — the flash-resident half of the image into the chip, and the MMU
        // programmed for it.
        self.staging = loader::stage_image_in_flash(&self.flash, &self.cache, &app);
        let chip_len = self.staging.chip_len;
        self.flash_seed = Some(loader::seed_rom_flash_chip(
            &mut self.bus,
            &self.rom,
            chip_len,
        )?);
        // Item 11: the bootloader hands the app a core whose read cache is
        // on, and a direct load runs no bootloader.
        loader::seed_cache_enabled(&self.cache);
        // Everything staged was already placed into the window by
        // `load_app`; filling now proves the table and the placement agree,
        // and is what serves the window from here on.
        self.cache_fills = self.refill_now();
        // The staging is what a flasher left behind, not a guest write: the
        // window has just been filled from it, so nothing is stale.
        self.flash
            .lock()
            .expect("flash poisoned")
            .take_written_blocks();
        self.seed_boot_state(app.entry, frame)?;
        Ok(())
    }

    /// Fill every page the table marked dirty, with the accesses marked as
    /// the emulator's so D4's watch does not arm on them ([`crate::cache`]).
    fn refill_now(&mut self) -> u64 {
        self.set_host_access(true);
        let filled = crate::cache::fill(&mut self.bus, &self.flash, &self.cache) as u64;
        self.set_host_access(false);
        filled
    }

    /// Refill any window page whose mapping or backing bytes moved.
    ///
    /// Called once per slice. Almost always a flash-block check and a `bool`
    /// and nothing else: only an MMU entry write, a page-mode change or a
    /// flash write under a mapped page puts anything in the list.
    fn refill_cache(&mut self) {
        let written = self
            .flash
            .lock()
            .expect("flash poisoned")
            .take_written_blocks();
        if !written.is_empty() {
            let mut c = self.cache.lock().expect("cache poisoned");
            let page_mode = c.page_mode(0);
            let page_len = crate::cache::FlashMmu::page_len(page_mode);
            let mut moved: Vec<u32> = Vec::new();
            for block in written {
                // A 64 KiB flash block can hold several pages at a finer
                // page mode; mark each.
                let base = block * crate::flash::BLOCK_LEN;
                let mut at = base;
                while at < base + crate::flash::BLOCK_LEN {
                    moved.extend(c.mmu.invalidate_page_at(at, page_mode));
                    at += page_len;
                }
            }
            // The bytes under those window pages moved, so what they hold is
            // no longer what the fill last put there.
            for index in moved {
                c.forget_fill(index);
            }
        }
        if !self.cache.lock().expect("cache poisoned").mmu.has_dirty() {
            return;
        }
        self.cache_fills += self.refill_now();
    }

    /// The flash chip, for a test and for the run report.
    pub fn flash(&self) -> &crate::flash::FlashHandle {
        &self.flash
    }

    /// Write the flash image back if its backing says to.
    pub fn flush_flash(&mut self) -> std::io::Result<bool> {
        self.flash.lock().expect("flash poisoned").flush()
    }

    /// What a direct load staged in the chip and mapped; empty on a rom-up
    /// machine, where the bootloader does it.
    pub fn flash_staging(&self) -> &loader::FlashStaging {
        &self.staging
    }

    /// How many window pages the fill has copied out of the chip.
    pub fn cache_fills(&self) -> u64 {
        self.cache_fills
    }

    /// Put hart 0 into the state a bootloader's `callx8` into `entry` leaves.
    ///
    /// Separated from [`direct_load`](Self::direct_load) so a test can seed a
    /// hart at a synthetic entry point without an application image — which
    /// is what `tests/boot.rs` uses to prove a seeded hart survives its first
    /// exception.
    pub fn seed_boot_state(&mut self, entry: u32, frame: BootFrame) -> Result<(), BuildError> {
        self.harts[0].set_pc(entry);
        // `PS_BOOT | OWB` — the window field the bootloader leaves, measured
        // by the cross-check rather than assumed (`BOOTLOADER_OWB`).
        self.harts[0].set_ps_raw(
            PS_BOOT
                | ((u32::from(frame.owb) << lp_xt_emu::mach::sr::PS_OWB_SHIFT)
                    & lp_xt_emu::mach::sr::PS_OWB_MASK),
        );
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

    /// The stack state core 1 will be (or was) released with: the ROM's
    /// APP-core stack top and its save area
    /// ([`BootFrame::rom_app_stack`]).
    pub fn app_core_boot_frame(&self) -> BootFrame {
        BootFrame::rom_app_stack(&self.rom)
    }

    /// The frame core 1 *was* released with, or `None` while it is held.
    pub fn app_core_frame(&self) -> Option<BootFrame> {
        self.app_core_frame
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

    /// Is core `n` held?
    ///
    /// **Three inputs, three owners, one OR.** Composed here rather than in
    /// any one block, which is the seam P4 and P5 were dispatched against —
    /// neither phase reads the other's file.
    ///
    /// - The machine's own `stalled`: slot 1 is held for the whole of M3
    ///   (Q5), whatever the guest writes.
    /// - **RTC_CNTL's two-register stall key** — `options0.sw_stall_*_c0`
    ///   and `sw_cpu_stall.sw_stall_*_c1`, which stall a core only when the
    ///   pair reads `0x86` (`esp-hal-1.1.1/src/soc/esp32/cpu_control.rs:57-81`).
    ///   P5's registers, published through
    ///   [`crate::periph::rtc_cntl::StallKey`].
    /// - **DPORT's `appcpu_ctrl_*`** — `appcpu_ctrl_c.appcpu_runstall` and
    ///   `appcpu_ctrl_a.appcpu_resetting`, P4's, through
    ///   [`crate::periph::dport::AppCoreControl`]. They name the APP core and
    ///   have no PRO twin, so they answer for core 1 only.
    pub fn core_stalled(&self, core: usize) -> bool {
        if self.stalled.get(core).copied().unwrap_or(true) {
            return true;
        }
        if self.stall_key.stalled(core) {
            return true;
        }
        core == 1 && self.appcpu.lock().expect("appcpu poisoned").holds_core1()
    }

    /// RTC_CNTL's half of the stall key, for a test or a report that wants
    /// to see which input is holding a core.
    pub fn stall_key(&self) -> &crate::periph::rtc_cntl::StallKey {
        &self.stall_key
    }

    /// DPORT's `appcpu_ctrl_*` as the guest left them, for a test and for the
    /// run report.
    pub fn app_core_control(&self) -> crate::periph::dport::AppCoreControl {
        *self.appcpu.lock().expect("appcpu poisoned")
    }

    /// The cache state and the flash MMU tables, for a test and for P7.
    pub fn cache(&self) -> &crate::cache::CacheHandle {
        &self.cache
    }

    /// The PRO core's flash MMU table, the 256 entries that decide what the
    /// IROM and DROM windows contain.
    ///
    /// G2's app-entry cross-check compares this between the two boot paths:
    /// the loader programs it by arithmetic and the IDF bootloader's
    /// `cache_flash_mmu_set` fills it from the image header it parsed, and a
    /// difference here is a difference in every byte of `.text` the
    /// application reads afterwards.
    pub fn flash_mmu_entries(&self) -> Vec<u32> {
        let mmu = self.cache.lock().expect("cache poisoned");
        (0..crate::cache::FLASH_MMU_ENTRIES as usize)
            .map(|i| mmu.mmu.entry(0, i))
            .collect()
    }

    /// Which of the three stall inputs hold `core` right now, by name, in
    /// the order `core_stalled` consults them. Empty for a running core.
    pub fn stall_inputs(&self, core: usize) -> Vec<&'static str> {
        let mut held = Vec::new();
        if self.stalled.get(core).copied().unwrap_or(true) {
            held.push("machine");
        }
        if self.stall_key.stalled(core) {
            held.push("rtc_cntl key 0x86");
        }
        if core == 1 {
            let a = self.appcpu.lock().expect("appcpu poisoned");
            if a.resetting {
                held.push("dport appcpu_resetting");
            }
            if a.runstall {
                held.push("dport appcpu_runstall");
            }
            if !a.clkgate_en {
                held.push("dport !appcpu_clkgate_en");
            }
        }
        held
    }

    /// One line per core plus the quantum, for `--probe` and the run report.
    /// Slot 1 is never silently absent, and a held core says *which* input
    /// is holding it.
    pub fn core_report(&self) -> Vec<String> {
        let mut lines: Vec<String> = (0..CORES)
            .map(|c| {
                let hart = &self.harts[c];
                let pc = hart.pc();
                let sym = self
                    .symbolize(pc)
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default();
                if self.core_stalled(c) {
                    format!(
                        "core {c}: held by [{}], pc={pc:#010x}{sym} cycle={} instr={}",
                        self.stall_inputs(c).join(", "),
                        hart.cycle_count(),
                        hart.instruction_count()
                    )
                } else {
                    format!(
                        "core {c}: running, pc={pc:#010x}{sym} cycle={} instr={}{}",
                        hart.cycle_count(),
                        hart.instruction_count(),
                        if hart.is_waiti() {
                            "  parked(waiti)"
                        } else {
                            ""
                        }
                    )
                }
            })
            .collect();
        lines.push(format!("quantum: {} cycles/window", self.core_quantum));
        lines
    }

    /// The upper bound on one core's window, in cycles (`--core-quantum`).
    pub fn core_quantum(&self) -> u64 {
        self.core_quantum
    }

    /// How many windows `core` has ended in `waiti`.
    pub fn wfi_ends(&self, core: usize) -> u64 {
        self.wfi_ends.get(core).copied().unwrap_or(0)
    }

    /// How many windows `core` was given (`--features bench`). One
    /// `run_slice` call each, so `core_instructions(core) /
    /// bench_windows(core)` is what a window really bought and the sum over
    /// cores is the interleave's switch count.
    #[cfg(feature = "bench")]
    pub fn bench_windows(&self, core: usize) -> u64 {
        self.bench_windows.get(core).copied().unwrap_or(0)
    }

    /// Is `core` parked in `waiti` — not held, and waiting for an interrupt
    /// to be taken? Such a core is given no window and costs nothing.
    pub fn parked(&self, core: usize) -> bool {
        !self.core_stalled(core) && self.harts.get(core).is_some_and(XtHart::is_waiti)
    }

    /// Instructions retired by `core` alone.
    pub fn core_instructions(&self, core: usize) -> u64 {
        self.harts.get(core).map_or(0, XtHart::instruction_count)
    }

    pub fn first_strict_violation(&self) -> Option<StrictViolation> {
        self.bus.first_strict_violation()
    }

    /// The registered blocks, in registration order: `(name, base, len)`.
    pub fn peripheral_map(&self) -> &[(&'static str, u32, u32)] {
        &self.peripheral_map
    }

    /// The **second** address each block answers at, in the same order:
    /// `(name, ahb base, len)`. The classic's AHB mirror (DD38); empty on a
    /// machine built with [`Esp32V3Builder::bare`].
    pub fn peripheral_alias_map(&self) -> &[(&'static str, u32, u32)] {
        &self.alias_map
    }

    // ---- the pads and the RMT (M4 P2) ------------------------------------

    /// The RMT block, read-only, through the bus's peripheral downcast
    /// (`Peripheral::as_any`). `None` on a machine built with
    /// [`Esp32V3Builder::bare`], which registers no RMT.
    fn rmt(&self) -> Option<&crate::periph::rmt::Rmt> {
        let index = self.rmt_index?;
        self.bus.peripheral(index)?.as_any()?.downcast_ref()
    }

    /// Every pulse RMT channel `ch` has put on `RMT_SIG_0 + ch`, in order.
    /// Empty unless the machine was built with
    /// [`Esp32V3Builder::rmt_logs`].
    pub fn rmt_pulses(&self, ch: usize) -> &[crate::periph::rmt::Pulse] {
        self.rmt().map(|r| r.pulses(ch)).unwrap_or(&[])
    }

    /// Every word RMT channel `ch` fetched, with the cycle it was fetched
    /// at — end markers included, so a frame reads `data … latch STOP`.
    pub fn rmt_words(&self, ch: usize) -> &[(Cycles, u32)] {
        self.rmt().map(|r| r.words(ch)).unwrap_or(&[])
    }

    /// `tx_end`s raised on RMT channel `ch`. Counted whether or not the logs
    /// are kept.
    pub fn rmt_frames_ended(&self, ch: usize) -> usize {
        self.rmt().map(|r| r.frames_ended(ch)).unwrap_or(0)
    }

    /// What RMT channel `ch`'s refills have cost, in words. **Reported,
    /// never gated** (D13/PD9) — see
    /// [`crate::periph::rmt::RefillStats`].
    pub fn rmt_refill_stats(&self, ch: usize) -> crate::periph::rmt::RefillStats {
        self.rmt().map(|r| r.refill_stats(ch)).unwrap_or_default()
    }

    /// Which pads a **peripheral signal** — not `GPIO_OUT` — is routed to
    /// and output-enabled on, as `GPIO` resolves it.
    ///
    /// Empty through M3, because nothing drove a signal; non-empty from M4
    /// P2 once a wire has been claimed, and then it is the list of pads a
    /// strip decoder should be watching.
    pub fn peripheral_driven_pads(&mut self) -> Vec<(PadId, SignalId)> {
        let Some(index) = self.gpio_index else {
            return Vec::new();
        };
        self.bus
            .with_peripheral::<crate::periph::gpio::Gpio, _>(index, |g, cx| {
                g.peripheral_driven_pads(cx)
            })
            .unwrap_or_default()
    }

    /// Edges seen on pad `pad` since the run started.
    pub fn pin_edges(&self, pad: u8) -> u64 {
        self.pins.state.edges.get(&pad).copied().unwrap_or(0)
    }

    /// Every pad the guest has routed, ascending, with what it is routed to.
    ///
    /// A pad appears the moment `func_out_sel_cfg` names something, whether
    /// or not anything has driven it yet — so an empty `frames()` on a pad
    /// that *is* here separates "the matrix never routed it" from "the
    /// engine never pumped it", which are different failures.
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

    /// What is decoded, for a snapshot and for a test that wants to compare
    /// two runs.
    pub fn pin_state(&self) -> &PinState {
        &self.pins.state
    }

    /// How each pad is being read: the wire timing and the byte order the
    /// record's `rgb` field is unpermuted with.
    pub fn strip(&self) -> StripConfig {
        self.pins.strip
    }

    /// One line per routed pad: `pin gpio18: 22 frames, 22 complete, 0
    /// errors, 256 leds`. What the CLI prints at exit.
    pub fn pin_report(&self) -> Vec<String> {
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
                    "pin gpio{pad}: {} frames, {complete} complete, {errors} errors, {leds} leds, \
                     {} edges",
                    frames.len(),
                    self.pin_edges(*pad),
                )
            })
            .collect()
    }

    /// End of the run: a frame still open on a pad is reported incomplete.
    ///
    /// Not part of [`run_until`](Self::run_until), because a run can be
    /// resumed and closing a frame that is still being transmitted would
    /// invent one — a flushed frame carries `reset_cycles: None` and so is
    /// never `is_complete()`. The CLI calls it before its summary; so does a
    /// test that wants the last frame.
    pub fn flush_frames(&mut self) {
        let at = self.clock();
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

    pub fn hook_calls(&self) -> u64 {
        self.hook_calls
    }

    pub fn idle_skips(&self) -> u64 {
        self.idle_skips
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Everything UART0 has put on the wire — bytes that have left the
    /// shifter, in the guest's order. Bytes still in the TX FIFO are not
    /// here yet.
    pub fn uart0(&self) -> &ByteLog {
        &self.uart0_log
    }

    /// The UART0 TCP listener, when `Uart0Sink::Tcp` was chosen.
    pub fn uart0_tcp(&self) -> Option<&lp_emu_esp_common::TcpHost> {
        self.uart0_tcp.as_ref()
    }

    /// The two modem lines and what the auto-reset circuit makes of them.
    pub fn cable(&self) -> Cable {
        self.cable
    }

    /// How many times this run has actually rebooted the chip
    /// ([`Esp32V3Builder::reboot_on_reset`]).
    pub fn reboots(&self) -> u64 {
        self.reboots
    }

    /// How many control lines this run has applied.
    pub fn control_lines(&self) -> u64 {
        self.control_lines
    }

    /// The cable's side, as the `state` verb reports it.
    pub fn cable_report(&self) -> CableReport {
        CableReport {
            attached: self.attached,
            port_open: self.port_open,
            cable: self.cable,
            reboots: self.reboots,
        }
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

    /// The machine's one guest clock: the furthest any hart has got.
    ///
    /// Every running hart is given the same window, so they advance
    /// together; a hart that ended a window early (a `waiti`, a bus yield,
    /// a breakpoint) is brought up to this clock before its next window
    /// opens, and a held core's counter does not move at all. While core 1
    /// has never been released this is exactly `harts[0].cycle_count()`,
    /// which is the M3 behaviour and is asserted in debug builds so a
    /// single-core run is provably the run it always was.
    pub fn clock(&self) -> Cycles {
        let max = self
            .harts
            .iter()
            .map(XtHart::cycle_count)
            .max()
            .unwrap_or(0);
        debug_assert!(
            !self.stalled[1] || max == self.harts[0].cycle_count(),
            "a never-released core 1 must not own the clock"
        );
        max
    }

    /// [`Machine::clock`], under the name every caller has used since M3.
    pub fn cycles(&self) -> Cycles {
        self.clock()
    }

    /// Emulated microseconds: `cycles / 240` ([`memmap::CPU_HZ`]).
    pub fn micros(&self) -> u64 {
        self.cycles() / memmap::CYCLES_PER_US
    }

    /// Instructions retired by **both** cores (ruling R10). The per-core
    /// counts are [`Machine::core_instructions`] and the run report prints
    /// them beside this sum.
    pub fn instructions(&self) -> u64 {
        self.harts.iter().map(XtHart::instruction_count).sum()
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
        self.set_host_access(true);
        let value = self.bus.read_word(address).ok().map(|v| v as u32);
        self.set_host_access(false);
        self.bus.set_pc(saved);
        value
    }

    /// Tell D4's watch that the accesses that follow are the emulator's, not
    /// the guest's. See [`crate::cache`].
    fn set_host_access(&mut self, on: bool) {
        self.cache
            .lock()
            .expect("cache poisoned")
            .set_host_access(on);
    }

    /// Write a word of guest memory from the host side — the same decode
    /// [`peek_word`](Self::peek_word) reads through.
    pub fn poke_word(&mut self, address: u32, value: u32) -> bool {
        let saved = self.bus.pc();
        self.bus.set_pc(0);
        self.set_host_access(true);
        let ok = self.bus.write_word(address, value as i32).is_ok();
        self.set_host_access(false);
        self.bus.set_pc(saved);
        ok
    }

    /// The word at a symbol, for `--probe <symbol>:<cycle>`.
    pub fn peek_symbol(&mut self, name: &str) -> Option<(u32, u32)> {
        let address = self.resolve_symbol(name)?;
        self.peek_word(address).map(|v| (address, v))
    }

    /// Stop the run the first time `address` is reached.
    ///
    /// The address form of [`break_at`](Self::break_at), for a caller that
    /// already has one — the application's `e_entry`, say, whose symbol name
    /// (`_start`) the **mask ROM** also uses, so resolving it by name finds
    /// the wrong one on a machine that has both loaded.
    pub fn break_at_address(&mut self, address: u32) -> Result<u32, RomError> {
        self.hooks
            .install_at(&mut self.bus, address, "break-at", |_| HookResult::Stop)
    }

    /// Stop the run the first time `address` is reached, but **only once the
    /// three bytes there are `expect`**.
    ///
    /// ⚠️ A hook is a `break` written into the guest's instruction stream,
    /// which is exactly right for a mask ROM — nothing rewrites
    /// `0x4000_xxxx`. It is wrong for an address in IRAM on the ROM-up path,
    /// and wrong in two different ways at once:
    ///
    /// - the second-stage bootloader loads its own segment over
    ///   `0x4008_0404..`, and then the application's IRAM segment over
    ///   `0x4008_0000..`, so a `break` planted before the run is **gone**
    ///   twice over before the pc reaches it; and
    /// - the bootloader's own code occupies the same addresses on the way
    ///   past, at **different instruction boundaries**, so a patch that
    ///   simply re-armed itself would plant three bytes across the middle of
    ///   one of the bootloader's instructions and the run would die on an
    ///   undecodable word a few bytes later — which is exactly what P8 saw
    ///   before writing this.
    ///
    /// So the breakpoint waits. Every slice, while any is pending, the
    /// machine reads the three bytes at the address and arms the hook the
    /// moment they are the caller's — for the app-entry cross-check, the
    /// application ELF's own first instruction. Before that the address
    /// belongs to somebody else and nothing is patched.
    pub fn break_at_address_when(&mut self, address: u32, expect: [u8; 3]) {
        self.pending_breaks.push((address, expect, "break-at"));
    }

    /// Arm every pending breakpoint whose bytes have appeared.
    fn arm_pending_breaks(&mut self) {
        let mut still = Vec::new();
        for (address, expect, symbol) in std::mem::take(&mut self.pending_breaks) {
            match crate::rom::read_three(&mut self.bus, address) {
                Ok(bytes) if bytes == expect => {
                    if let Err(e) = self
                        .hooks
                        .install_at(&mut self.bus, address, symbol, |_| HookResult::Stop)
                    {
                        log::warn!("machine: arming `{symbol}` at {address:#010x}: {e}");
                    } else {
                        log::debug!(
                            "machine: armed `{symbol}` at {address:#010x} at cycle {}",
                            self.cycles()
                        );
                    }
                }
                _ => still.push((address, expect, symbol)),
            }
        }
        self.pending_breaks = still;
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
        // Before the first instruction as well as after every slice: a
        // direct load starts *at* the application's entry, so a breakpoint
        // armed only at a slice boundary would arm one slice too late and
        // the run would sail past the address it names.
        if !self.pending_breaks.is_empty() {
            self.arm_pending_breaks();
        }
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

            // The host's side, at a slice boundary and never inside one: the
            // cable script's own cycles, then a client's lines. A command
            // therefore never lands between two instructions of one slice,
            // and the reply names the cycle it was drained at.
            self.service_host(now);
            // EN went high again: the chip starts, and it starts *now* —
            // before the guest is given another slice of a chip that was
            // being held in reset.
            if let Some(strap) = self.pending_reset.take() {
                if self.reboot_on_reset && self.reboot(strap) {
                    log::info!(
                        "machine: the auto-reset circuit released EN at cycle {now} — rebooting \
                         into strap {strap}"
                    );
                    matched = 0;
                    continue;
                }
                return Outcome::Reset { cycle: now, strap };
            }

            let mut deadline = stop_cycle.min(now.saturating_add(MAX_SLICE_CYCLES));
            if let Some(event) = self.bus.sched.next_deadline() {
                deadline = deadline.min(event.max(now + 1));
            }
            if let Some((at, _)) = probes.get(next_probe) {
                deadline = deadline.min((*at).max(now + 1));
            }
            if let Some(at) = self.next_host_service() {
                deadline = deadline.min(at.max(now + 1));
            }
            if self.bus.strict() {
                deadline = deadline.min(now + STRICT_SLICE_CYCLES);
            }
            // D3: the window. The quantum is the *upper* bound on one core's
            // window; every bound above still applies, so a scheduled event,
            // a probe, a host service or a strict slice still shortens it.
            let window = deadline.saturating_sub(now).max(1).min(self.core_quantum);

            // One window per core that is neither held nor parked, core 0
            // then core 1, each opening at `now`. A held core's counter does
            // not move; a parked core's is brought up to the clock by the
            // feed below and it costs nothing else. See the module docs.
            for core in 0..CORES {
                if self.core_stalled(core) || self.harts[core].is_waiti() {
                    continue;
                }
                // A core that fell behind the clock — it ended its last
                // window early, or was held while the other ran — is brought
                // up to it before it runs again, so its window is the same
                // span of guest time as the other core's.
                self.harts[core].advance_to_cycle(now);

                // D4, per running core (ruling R3). Arming and disarming
                // happen here, between windows, and the DPORT view asks for
                // a boundary the moment either cache-enable bit moves — so
                // the window between "the cache went off" and "the check is
                // on" is zero instructions.
                self.sync_cache_watch(core);
                let budget = if self.cache_watch.is_some() {
                    // `MemoryCost` sees the address and not the pc, so while
                    // the watch is armed the hart runs one instruction at a
                    // time and the pc below is exactly the one that made the
                    // access. The armed window is a flash write's worth of
                    // instructions, on the core whose cache is off.
                    1
                } else {
                    window
                };
                let issuing_pc = self.harts[core].pc();

                #[cfg(feature = "bench")]
                {
                    self.bench_windows[core] += 1;
                }
                self.bus.set_time(now);
                self.bus.set_hart(core);
                let end = self.harts[core].run_slice(&mut self.bus, budget);

                if let Some(outcome) = self.take_cache_off_fetch(core, issuing_pc) {
                    return outcome;
                }
                // The DPORT view yields at the store that completes the
                // release, so this runs at the boundary of the instruction
                // that started core 1 — before core 1's own window opens.
                self.service_app_core_start();

                match end {
                    SliceEnd::BudgetExhausted | SliceEnd::BusYield => {}
                    SliceEnd::Wfi => {
                        self.wfi_ends[core] += 1;
                        // ⚠️ **Resample the matrix before leaving the core
                        // parked.**
                        //
                        // The hart polls at the `waiti` itself (`Priv::Waiti`:
                        // "an already-pending interrupt un-parks immediately"),
                        // but it polls against the external mask the machine
                        // last handed it — at the *previous* boundary. A line
                        // the bus raised during this window is therefore
                        // invisible to that poll, and parking on it would
                        // leave the guest owed an interrupt.
                        //
                        // P8 found it as a wedge and it cost the milestone
                        // its heartbeat: a level-2 interrupt (swi2, the
                        // io_task executor) and a level-1 one (TIMG0 timer1,
                        // the 1 ms pacer) came due in the same slice at cycle
                        // 7,732,422. The hart took the level-2 one, its
                        // handler ran, `rfi 2` returned to the idle loop, and
                        // the guest reached `waiti` **inside the same slice**
                        // with TIMG0's line still asserted and unserviced.
                        // Nothing was scheduled behind it — the pacer re-arms
                        // from its own handler, which had not run — so the
                        // idle skip jumped to the run's deadline with one
                        // interrupt pending and 7,072,512 instructions
                        // retired, forever.
                        //
                        // So: fire what is due, resample [`CpuIntMatrix`] for
                        // THIS hart, and give it the poll it could not make
                        // for itself. If that un-parks it, it is not parked
                        // at all and runs its next window. With two cores
                        // this happens per core, and the skip below is taken
                        // only when every running core stays parked.
                        let at = self.harts[core].cycle_count();
                        self.bus.run_due_events(at);
                        self.bus.set_hart(core);
                        let external = self.bus.pending_cpu_interrupt_mask();
                        self.harts[core].set_external_mask(external);
                        self.harts[core].poll_interrupts();
                    }
                    SliceEnd::Ebreak { pc } => {
                        if !self.serve_breakpoint(core, pc) {
                            self.harts[core].deliver_breakpoint(pc);
                        }
                    }
                    SliceEnd::Fault(fault) => {
                        // A strict refusal that turned into a double fault
                        // is still a strict refusal, and naming the access is
                        // more useful than naming the vector.
                        if let Some(violation) = self.bus.first_strict_violation() {
                            return Outcome::StrictBus { violation };
                        }
                        return Outcome::Fault {
                            core,
                            cycle: self.harts[core].cycle_count(),
                            pc: self.harts[core].pc(),
                            fault,
                        };
                    }
                }
            }

            // Between windows, once: the clock is the furthest hart, what is
            // due fires, and every running core is brought up to the clock
            // and given the matrix feed it could not sample for itself.
            let at = self.clock();
            self.bus.run_due_events(at);
            // The pads: whatever the window put on the wire, in cycle order,
            // and before the feed — a pad whose edge latches a GPIO
            // interrupt must reach the matrix in this same iteration.
            self.drain_pins();
            self.feed_cores(at);

            // The deterministic idle skip: only when **every** running core
            // is parked in `waiti` and none of them un-parked on the feed
            // above. Nothing can then happen before the earliest of: the
            // next scheduled event, the host's next service, the byte
            // source's next delivery (a `--uart0-script` step whose cycle is
            // already known — an idle guest used to jump clean over it, and
            // the walk's second request landed at the run's deadline), and
            // **each running hart's own `CCOMPARE` match** (ruling R9: the
            // classic's tick is TIMG0 today, so this changes nothing
            // observable now; it is the difference between a loop that is
            // correct and one that happens to work).
            //
            // A machine on which *no* core is running — both held — moves
            // its harts' counters the same way rather than spinning: on
            // silicon a RunStall'd core's `CCOUNT` keeps counting, and only
            // the host (a cable reset) or the deadline can end that state.
            if self.no_running_core_is_awake() {
                let event = self.bus.sched.next_deadline();
                let host = self.next_host_service();
                let bytes = self.bus.host.next_ready();
                let timers = (0..CORES)
                    .filter(|c| !self.core_stalled(*c))
                    .filter_map(|c| self.harts[c].next_timer_cycle());
                let wake = [event, host, bytes]
                    .into_iter()
                    .flatten()
                    .chain(timers)
                    .min()
                    .unwrap_or(stop_cycle)
                    .max(at + 1)
                    .min(stop_cycle);
                let all_held = (0..CORES).all(|c| self.core_stalled(c));
                for core in 0..CORES {
                    if all_held || !self.core_stalled(core) {
                        self.harts[core].advance_to_cycle(wake);
                    }
                }
                self.idle_skips += 1;
                // And the feed again at the wake: a timer the skip latched
                // or an event due there is taken now, before the next
                // iteration decides anything.
                let at = self.clock();
                self.bus.run_due_events(at);
                self.feed_cores(at);
            }

            // The window is a fill, so a table write or a flash write under
            // a mapped page has to reach the RAM behind it before the guest
            // runs again. Before the code-write drain below, because a fill
            // into an executable region *is* a code write.
            self.refill_cache();
            if let Some(outcome) = self.take_mmu_divergence() {
                return outcome;
            }
            // ⚠️ The code-write drain is per hart. A block cache on core 1
            // that never heard about core 0's write would execute stale
            // code — exactly what the classic's `codemem_esp32` install path
            // does on purpose (JIT code written through a D-bus alias and
            // executed from SRAM0).
            if self.bus.code_writes_pending() {
                for (lo, hi) in self.bus.take_code_writes() {
                    for hart in &mut self.harts {
                        hart.invalidate_block_range(lo, hi);
                    }
                }
            }

            if !self.pending_breaks.is_empty() {
                self.arm_pending_breaks();
            }

            if let Some(violation) = self.bus.first_strict_violation() {
                return Outcome::StrictBus { violation };
            }
            if let Some((core, pc)) = self.stop_at.take() {
                return Outcome::Breakpoint {
                    core,
                    cycle: self.cycles(),
                    pc,
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

    // ---- between windows -------------------------------------------------

    /// Bring every running core up to `at` and hand each the matrix as
    /// **its own** input (`set_hart` first — the feed is per hart, PD6),
    /// then let it take what it can. A core parked in `waiti` un-parks here
    /// and nowhere else.
    fn feed_cores(&mut self, at: Cycles) {
        for core in 0..CORES {
            if self.core_stalled(core) {
                continue;
            }
            self.harts[core].advance_to_cycle(at);
            self.bus.set_hart(core);
            let external = self.bus.pending_cpu_interrupt_mask();
            self.harts[core].set_external_mask(external);
            self.harts[core].poll_interrupts();
        }
    }

    /// Take the window's edges off the fabric and hand them to the blocks
    /// that watch pads.
    ///
    /// **M4 P2's, and load-bearing from here on.** Until the RMT view
    /// existed nothing drove a signal, so the fabric's edge buffer was
    /// always empty and the machine never had to empty it. Now a running
    /// channel puts two edges on the wire per word — 14,402 for a 300-LED
    /// frame — and a machine that never drained them would grow a buffer for
    /// the length of the run and hand `Gpio` nothing.
    ///
    /// Per edge rather than a before/after diff, so a pad that rose and fell
    /// inside one window latches both directions and neither is lost. **M4
    /// P3** hangs the strip decoders and the `.pins.jsonl` log off this same
    /// stream, which is what guarantees a decoder, a pin log and the GPIO
    /// input latch can never disagree about what was on the wire.
    ///
    /// ⚠️ **Two cores, one fabric.** This runs once per *window*, after both
    /// cores have had theirs and before the matrix feed — and the cycle each
    /// decoder reads is the edge's own `at`, stamped by whoever drove the
    /// signal. The RMT emits both halves of a word at the fetch, so an edge
    /// can be stamped slightly ahead of the boundary it is drained at; that
    /// is a timestamp the decoder reads, never a reordering, and it is why
    /// the decoded frames do not move with `--core-quantum`.
    fn drain_pins(&mut self) {
        // A pad becomes observed the moment the guest routes it, which is a
        // change to `Fabric::route_epoch` — so a window that routed nothing
        // walks no pads at all.
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
                // `or_insert_with`: a re-mux between the pusher's waves
                // rewrites `func_out_sel_cfg` for a pad that is already
                // being decoded, and throwing that decoder away would lose
                // the bits of the frame in flight.
                self.pins
                    .state
                    .decoders
                    .entry(pad.0)
                    .or_insert_with(|| Ws281xDecoder::new(pad, strip.timing(), cpu_hz));
                self.note_pin_route(pad.0, source);
            }
        }
        let edges = self.bus.pins.take_edges();
        if edges.is_empty() {
            return;
        }
        if let Some(index) = self.gpio_index {
            self.bus
                .with_peripheral::<crate::periph::gpio::Gpio, _>(index, |g, cx| {
                    g.observe_edges(&edges, cx)
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

    /// One `# route` note per pad in the pin log when the routing changes, so
    /// a reader of the edge stream can see which signal was driving it.
    fn note_pin_route(&mut self, pad: u8, source: RouteSource) {
        let Some(w) = self.pins.pin_log.as_mut() else {
            return;
        };
        let what = match source {
            RouteSource::Signal(sig, invert) => {
                let name = crate::regs::output_signals::output_signal_name(sig.0)
                    .map_or_else(|| format!("sig{}", sig.0), str::to_string);
                format!("{name} (out_sel={} inv={})", sig.0, u8::from(invert))
            }
            RouteSource::GpioOut => "GPIO_OUT".to_string(),
        };
        let _ = writeln!(w, "# route gpio{pad} <- {what}");
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
        // `<us> <pad> <level> cyc=<cycle>`: the microseconds are for a human
        // and the **cycle** is the number anything may compute with. Emulated
        // microseconds never gate anything (PD9).
        let us = edge.at as f64 / memmap::CYCLES_PER_US as f64;
        let _ = writeln!(
            w,
            "{us:.3} {} {} cyc={}",
            edge.pad,
            u8::from(edge.level),
            edge.at
        );
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

    /// Is there no core that could retire an instruction right now — every
    /// core either held or parked in `waiti`?
    fn no_running_core_is_awake(&self) -> bool {
        (0..CORES).all(|c| self.core_stalled(c) || self.harts[c].is_waiti())
    }

    // ---- D4, the cache-off fetch stop ------------------------------------

    /// Install or remove the watch for the core about to run, following
    /// [`crate::cache::ClassicCache::watch_wanted`] (ruling R3: per running
    /// core, because `CacheOffWatch` is built with a core and the bus's
    /// `MemoryCost` sees the address, not the hart).
    ///
    /// A run whose guest never disables the cache never installs it and pays
    /// one `Option` test per access, which is what the bus already cost.
    fn sync_cache_watch(&mut self, core: usize) {
        let wanted = self
            .cache
            .lock()
            .expect("cache poisoned")
            .watch_wanted(core);
        let armed_for = wanted.then_some(core);
        if armed_for == self.cache_watch {
            return;
        }
        match armed_for {
            Some(core) => {
                self.bus
                    .set_memory_cost(Some(Box::new(crate::cache::CacheOffWatch::new(
                        self.cache.clone(),
                        core,
                    ))))
            }
            None => self.bus.set_memory_cost(None),
        }
        self.cache_watch = armed_for;
    }

    /// The offence the watch recorded during the window just run on `core`,
    /// as an outcome. `pc` is the instruction that made the access — the
    /// window was one instruction long, which is what makes that exact
    /// rather than approximate.
    fn take_cache_off_fetch(&mut self, core: usize, pc: u32) -> Option<Outcome> {
        let access = self.cache.lock().expect("cache poisoned").take_offence()?;
        Some(Outcome::CacheOffFetch {
            cycle: self.harts[core].cycle_count(),
            pc,
            symbol: self.symbolize(pc),
            access,
        })
    }

    // ---- R4, the flash-MMU divergence stop --------------------------------

    /// The disagreement the last fill met, as an outcome.
    fn take_mmu_divergence(&mut self) -> Option<Outcome> {
        let divergence = self
            .cache
            .lock()
            .expect("cache poisoned")
            .take_divergence()?;
        Some(Outcome::MmuDivergence {
            cycle: self.clock(),
            divergence,
        })
    }

    /// What R4's stop says, and what it does not claim. Written for a reader
    /// who has just been stopped by it.
    pub fn mmu_divergence_message(&self, cycle: Cycles, d: &crate::cache::MmuDivergence) -> String {
        let page = match d.vaddr {
            Some(v) => format!("virtual page {v:#010x}"),
            None => "an index with no reachable virtual address on this chip".to_string(),
        };
        format!(
            "FLASH-MMU DIVERGENCE  entry={}  cycle={cycle}\n  \
             {page} (page mode {}): the APP core's table maps flash page {:#x}, the PRO \
             core's maps {:#x}\n  \
             core 1 is {}\n\n  \
             This machine has ONE flash window behind BOTH cores' tables and serves the PRO\n  \
             core's; through this entry core 1 would read bytes its own table does not name.\n  \
             The IDF bootloader and the direct loader program both tables from one image, so\n  \
             a disagreement is a finding, not a configuration.\n  \
             `--app-mmu-divergence permit` continues, serving the PRO core's view (and claims\n  \
             nothing about what core 1 would have read).",
            d.index,
            d.page_mode,
            d.app,
            d.pro,
            if self.core_stalled(1) {
                format!("held by [{}]", self.stall_inputs(1).join(", "))
            } else {
                "running".to_string()
            },
        )
    }

    /// What D4's stop says, and what it does not claim. Written for a reader
    /// who has just been stopped by it.
    pub fn cache_off_message(
        &self,
        cycle: Cycles,
        pc: u32,
        access: &crate::cache::CacheOffAccess,
    ) -> String {
        let symbol = |at: u32| match self.symbolize(at) {
            Some(name) => format!(" ({name})"),
            None => String::new(),
        };
        let side = if access.core == 0 { "pro" } else { "app" };
        let ctrl = if access.core == 0 {
            crate::periph::dport::PRO_CACHE_CTRL
        } else {
            crate::periph::dport::APP_CACHE_CTRL
        };
        let why = match access.disabled_by {
            Some(by) => format!(
                "at cycle {} by a write to\n  DPORT+{ctrl:#05x} \
                 ({side}_cache_ctrl.{side}_cache_enable <- 0) from pc={by:#010x}{}",
                access.disabled_at,
                symbol(by),
            ),
            // Nothing ever turned it on. On a direct load that would be a bug
            // in `loader::seed_cache_enabled`; on a ROM-up boot it means the
            // guest reached the window before `Cache_Read_Enable`.
            None => format!(
                "by reset and never turned on: {side}_cache_ctrl's PAC reset is\n  \
                 {:#010x}, with {side}_cache_enable (bit 3) clear, and on silicon it is \
                 the\n  bootloader's `Cache_Read_Enable` that sets it",
                crate::cache::CACHE_CTRL_RESET
            ),
        };
        let what = if access.fetch { "fetch" } else { "read" };
        format!(
            "CACHE-OFF FETCH  core={}  cycle={cycle}\n  \
             pc={pc:#010x}{}  {what} from {} {:#010x}\n  \
             this core's cache was disabled {why}\n\n  \
             On silicon this core stalls until the cache returns; nothing observes it\n  \
             except a crash or a watchdog. This emulator refuses instead.\n  \
             `--cache-off-fetch permit` continues (and claims nothing about the stall).",
            access.core,
            symbol(pc),
            access.window,
            access.addr,
        )
    }

    /// Start core 1 the way the bench says the part does, once, when DPORT
    /// has released it: `appcpu_resetting` cleared with `appcpu_clkgate_en`
    /// set and `appcpu_runstall` clear
    /// ([`crate::periph::dport::AppCoreControl`]).
    ///
    /// # What is modelled
    ///
    /// **Core 1 begins executing at `appcpu_ctrl_d.appcpu_boot_addr`, and no
    /// ROM code runs on it** — no `_ResetVector`, no reset handler, no unpack
    /// or bss table, no ROM `main`, no `_xtos_set_exception_handler`. Slot 1
    /// becomes a fresh hart, so every register it does not name below is at
    /// this part's reset value, and its counters start at the machine's
    /// clock. Then exactly five things are seeded, each cited:
    ///
    /// | field | value | where it comes from |
    /// |---|---|---|
    /// | `pc` | `appcpu_boot_addr` | esp-hal's `start_core1` wrote it ([`crate::periph::dport`]) |
    /// | `PS` | [`APP_CORE_RELEASE_PS`] = `0x0006_0020` | the ROM's own `_start` word (`0x40000704`) plus the `CALLINC(2)` of the call that reaches the entry |
    /// | `a1` | `__stack_app` = `0x3ffe7e30` | the ROM ELF's symbol; `reserved_rom_stack_app`'s end in `third_party/esp-hal/ld/esp32/memory.x:35` |
    /// | `[a1-16, a1)` | a [`BootFrame`] save area | the window-overflow guard the direct load's seam documents |
    /// | `CPENABLE` | [`CPENABLE_RESET`] = `0xff` | measured on the desk board by M4 P1 |
    ///
    /// `VECBASE` is the reset `0x4000_0000` that [`fresh_hart`] gives it, and
    /// `PRID` is [`PRID_APP`] — both from [`core_config`], neither seeded
    /// here. The firmware's entry (esp-hal's `start_core1_init`) sets its own
    /// `VECBASE`, stack pointer and interrupt mask within its first dozen
    /// instructions, so none of this survives long; what it must do is get
    /// that function's `entry a1, N` prologue onto a real stack, which is why
    /// `a1` and its save area are here at all.
    ///
    /// # Why, and what is owed
    ///
    /// The module docs carry the bench evidence (lab task L2, PR #695:
    /// `changed=0` over `0x3ffe0440..0x3ffe1440`, two sittings) and the open
    /// hardware question — *why* the `appcpu_resetting` pulse does not put
    /// the APP core back through the ROM is **not** established here and is
    /// not claimed. This models the observable, and the observable is the
    /// thing the bench pinned.
    ///
    /// The same path serves a power-on release: on a ROM-up boot DPORT's
    /// reset values hold core 1 from the first cycle and nothing runs on it
    /// until the firmware's release, which arrives here. Whether silicon's
    /// APP core ran its own ROM path at power-on is unobservable from either
    /// side — the PRO core's boot re-unpacks the same spans afterwards — so
    /// this machine does not run one and does not pretend to know.
    ///
    /// RTC_CNTL's half of the key may still hold the core (esp-hal unstalls
    /// it before the DPORT sequence, so on this firmware it does not); that
    /// is [`Machine::core_stalled`]'s question, asked before every window,
    /// and the release here does not pre-empt it.
    fn service_app_core_start(&mut self) {
        let (at, entry) = {
            let mut a = self.appcpu.lock().expect("appcpu poisoned");
            let Some(start) = a.start_attempt else {
                return;
            };
            if a.reported {
                return;
            }
            a.reported = true;
            start
        };
        let clock = self.clock();
        if entry == 0 {
            // esp-hal always writes `appcpu_ctrl_d` before the release, and
            // the ROM's own APP arm would have spun here. With no ROM on this
            // core there is nothing to spin in, so the honest answer is to
            // leave it held and say why rather than fetch from address zero.
            log::warn!(
                "core 1: DPORT released it at cycle {at} with appcpu_boot_addr = 0 — nothing \
                 to start, the core stays held (esp-hal writes the boot address first)"
            );
            return;
        }
        self.stalled[1] = false;
        let frame = self.app_core_boot_frame();
        let mut hart = fresh_hart(1, self.time_grade, self.strict_unsupported);
        hart.set_counters(clock, 0);
        hart.set_pc(entry);
        hart.set_ps_raw(APP_CORE_RELEASE_PS);
        hart.cpu_mut().set_a(1, frame.sp);
        self.harts[1] = hart;
        for (i, word) in frame.save_area.iter().enumerate() {
            let at = frame.sp.wrapping_sub(16).wrapping_add(4 * i as u32);
            if let Err(e) = self.bus.load_image(at, &word.to_le_bytes()) {
                log::warn!("core 1: seeding the APP boot frame at {at:#010x}: {e}");
            }
        }
        self.app_core_frame = Some(frame);
        log::info!(
            "core 1: released by DPORT at cycle {at} — started at appcpu_boot_addr at clock \
             {clock}, no ROM code run; appcpu_boot_addr = {entry:#010x} ({}){}",
            self.symbolize(entry).unwrap_or_else(|| "?".into()),
            if self.stall_key.stalled(1) {
                "; RTC_CNTL's key still holds it"
            } else {
                ""
            }
        );
    }

    // ---- the host's side: the cable, and the control channel -------------

    /// The next cycle at which [`service_host`](Self::service_host) has
    /// something to do, or `None` when nothing outside can reach this run.
    ///
    /// A scripted command's own cycle, or the socket poll cadence. It bounds
    /// the slice and the idle skip, which is what stops a guest sitting in
    /// `waiti` from jumping over the whole script.
    fn next_host_service(&self) -> Option<Cycles> {
        let scripted = self.control_script.front().map(|(at, _)| *at);
        let polled = self.control.is_some().then_some(self.next_host_poll);
        [scripted, polled].into_iter().flatten().min()
    }

    /// Apply everything the host has asked for by cycle `now`.
    ///
    /// The script first — its times are the contract — then the control
    /// channel's lines. There is **no coupling rule** on this chip: a client
    /// on the UART0 byte socket is a program that opened a bridge chip's tty,
    /// and the SoC cannot see it (`crate::control`).
    fn service_host(&mut self, now: Cycles) {
        while self
            .control_script
            .front()
            .is_some_and(|(at, _)| *at <= now)
        {
            let (_, command) = self.control_script.pop_front().expect("checked");
            let reply = self.apply_control(&command, now);
            if let ControlReply::Err(reason) = &reply {
                log::warn!("--control-script: {reason}");
            }
        }

        if self.control.is_none() || now < self.next_host_poll {
            return;
        }
        self.next_host_poll = now.saturating_add(HOST_POLL_CYCLES);
        self.service_control(now);
    }

    /// Read whole lines off the control socket and answer each with exactly
    /// one reply line.
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

    /// Apply one control command at guest cycle `now`.
    ///
    /// Every precondition is checked here rather than swallowed: a command
    /// that could not be applied answers `err <reason>` and changes nothing,
    /// so a script that has drifted out of step is visible rather than
    /// quietly ineffective.
    pub fn apply_control(&mut self, command: &ControlCommand, now: Cycles) -> ControlReply {
        self.control_lines += 1;
        match command {
            ControlCommand::State => {
                return ControlReply::State {
                    cycle: now,
                    report: self.cable_report(),
                };
            }
            ControlCommand::Wait(_) => {
                return ControlReply::Err(
                    "`wait` is a --control-script command; a client on this socket waits by \
                     waiting"
                        .to_string(),
                );
            }
            ControlCommand::Attach => {
                if self.attached {
                    return ControlReply::Err("attach: a cable is already attached".to_string());
                }
                self.attached = true;
                self.note_cable(now, "attach: the cable is in — no chip state moved");
                return ControlReply::Ok {
                    verb: "attach",
                    cycle: now,
                };
            }
            ControlCommand::Detach => {
                if !self.attached {
                    return ControlReply::Err("detach: no cable is attached".to_string());
                }
                self.attached = false;
                self.port_open = false;
                // The lines go slack, which on this circuit is both
                // deasserted, which is "run".
                self.drive_lines(Cable::default(), now);
                self.note_cable(now, "detach: the cable is out and both lines went slack");
                return ControlReply::Ok {
                    verb: "detach",
                    cycle: now,
                };
            }
            ControlCommand::Open => {
                if !self.attached {
                    return ControlReply::Err(
                        "open: no cable is attached (attach first — a cable is not a port open)"
                            .to_string(),
                    );
                }
                if self.port_open {
                    return ControlReply::Err("open: the port is already open".to_string());
                }
                self.port_open = true;
                self.note_cable(
                    now,
                    "open: an application has the tty — the SoC cannot see it",
                );
                return ControlReply::Ok {
                    verb: "open",
                    cycle: now,
                };
            }
            ControlCommand::Close => {
                if !self.port_open {
                    return ControlReply::Err("close: the port is not open".to_string());
                }
                self.port_open = false;
                self.note_cable(now, "close: the tty is closed — no chip state moved");
                return ControlReply::Ok {
                    verb: "close",
                    cycle: now,
                };
            }
            _ => {}
        }

        // What is left drives the lines. Every one of them is the same
        // circuit; the verbs differ only in which sequence they play.
        let sequence: Vec<Cable> = match command {
            ControlCommand::Signals { dtr, rts } => vec![Cable {
                dtr: dtr.unwrap_or(self.cable.dtr),
                rts: rts.unwrap_or(self.cable.rts),
            }],
            // `esptool_reset`, `spikes/serial-lab/index.html:341-357`.
            ControlCommand::Reset => vec![
                Cable {
                    dtr: false,
                    rts: true,
                },
                Cable {
                    dtr: false,
                    rts: false,
                },
            ],
            // `bootloader_entry`, same file. The third step releases IO0
            // after the chip has already started, which is why the strap is
            // latched at the second.
            ControlCommand::DownloadMode => vec![
                Cable {
                    dtr: false,
                    rts: true,
                },
                Cable {
                    dtr: true,
                    rts: false,
                },
                Cable {
                    dtr: false,
                    rts: false,
                },
            ],
            other => {
                return ControlReply::Err(format!(
                    "unreachable: `{}` is handled above",
                    other.verb()
                ));
            }
        };
        for step in sequence {
            self.drive_lines(step, now);
        }
        ControlReply::Ok {
            verb: command.verb(),
            cycle: now,
        }
    }

    /// Drive the two modem lines, and act on the **EN edge**.
    ///
    /// EN low holds the chip in reset; EN going high is the chip starting,
    /// latching IO0's level at that instant as the strap. Writing it as edges
    /// rather than as verbs is what makes `signals dtr=0 rts=1` then
    /// `signals dtr=0 rts=0` exactly one reboot — the circuit, not a special
    /// case for a verb — and what makes the download dance's third step
    /// harmless.
    fn drive_lines(&mut self, next: Cable, now: Cycles) {
        let was = self.cable;
        self.cable = next;
        if was == next {
            return;
        }
        if self.bus.trace.is_enabled() {
            let line = format!(
                "cyc={now} CABLE dtr={} rts={} -> en={} io0={}",
                u8::from(next.dtr),
                u8::from(next.rts),
                u8::from(next.en()),
                u8::from(next.io0()),
            );
            self.bus.trace.note(&line);
        }
        // The rising edge of EN. `pending_reset` is drained at the next slice
        // boundary, so a three-step dance that ends with EN high resets once.
        if !was.en() && next.en() {
            self.pending_reset = Some(next.strap());
        }
    }

    fn note_cable(&mut self, now: Cycles, what: &str) {
        if self.bus.trace.is_enabled() {
            let line = format!("cyc={now} CABLE {what}");
            self.bus.trace.note(&line);
        }
    }

    /// Reboot into `strap`, as the chip does when the auto-reset circuit
    /// releases EN.
    ///
    /// The machine goes back to the state it was built in. **The reset cause
    /// does not change**, and that is not an omission: an EN-pin reset on
    /// this part is a *chip* reset — it resets the RTC sub-system too — and
    /// the classic has no code for one. esp-hal's own `SocResetReason` table
    /// for esp32 (`third_party/esp-hal/src/rtc_cntl/rtc/esp32.rs:15-50`) runs
    /// `ChipPowerOn = 0x01`, `CoreSw = 0x03`, … `SysRtcWdt = 0x10` and has no
    /// external-reset variant at all, so `RTC_CNTL.reset_state` reads
    /// `POWERON_RESET` after a cable reset exactly as it does after a power-on
    /// — which is what the restored power-on state already says. **This is
    /// the one place the classic differs from the C6's `USB_UART_HPSYS`**, and
    /// it differs by having no code rather than by having a different one.
    ///
    /// The console keeps its bytes: a restore would put back the empty log the
    /// machine was built with, and a boot log that lost everything before the
    /// reset would be a worse record than one with both boots in it.
    pub fn reboot(&mut self, strap: Strap) -> bool {
        let Some(power_on) = self.power_on.clone() else {
            return false;
        };
        let uart0 = self.uart0_log.bytes();
        self.restore(&power_on);
        self.uart0_log.replace(&uart0);
        self.reboots += 1;
        log::info!(
            "machine: reboot {} — back to cycle {}, pc {:#010x}, strap {strap}",
            self.reboots,
            self.cycles(),
            self.harts[0].pc()
        );

        // The host-poll deadline is an ABSOLUTE guest cycle and it lives
        // outside the snapshot, so a restore that puts the clock back to zero
        // would leave it in the guest's future — for as long as the board had
        // been up. Until the guest re-earned those cycles the machine would
        // read no control line and write no reply. MEASURED on the C6 (plan
        // two M5, `emu serve`): a board 33 s of guest time old went silent for
        // ~6 s of wall after a reset dance; one 120 s old went silent for ~3.5
        // minutes, and the replies then arrived in a burst stamped 1 ms after
        // the reset. A reset is the moment a flasher needs the chip MOST, so
        // the deadline goes back with the clock.
        self.next_host_poll = 0;

        // The C6's second reboot rule is "re-derive the client/port coupling
        // from both sides". On this chip there is nothing to re-derive **and
        // that is the finding, not an omission**: no register reports the
        // cable, so the two sides cannot disagree. What the rule becomes here
        // is that the host's own state — the two lines, `attached`,
        // `port_open` — is deliberately *not* touched by `restore`, because a
        // reset does not unplug a cable. That is what lets the download
        // dance's third step act on the lines the second one left.
        if strap == Strap::Download {
            log::warn!(
                "machine: rebooted with IO0 low (the download strap). The classic's ROM \
                 download console is P7's; this machine boots the same way either way and the \
                 strap is recorded, not acted on."
            );
        }
        true
    }

    /// Is `needle` on a **complete** line of UART0's output at or after the
    /// anchor? Moves the anchor forward so a long run does not rescan.
    ///
    /// The search is over **bytes**, not `str`: a console is a byte stream and
    /// anchoring an index into a lossily-decoded `String` lands inside a
    /// multi-byte character and panics.
    fn exit_on_match(&self, needle: &str, from: &mut usize) -> Option<Cycles> {
        let needle = needle.as_bytes();
        self.uart0_log
            .with_bytes(|text| Self::line_complete(text, needle, from))
            .then(|| self.cycles())
    }

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
            None => {
                *from = text.len().saturating_sub(needle.len());
                false
            }
        }
    }

    /// Give the hook table first refusal on a `break` at `pc` on `core`.
    /// Returns `false` when nothing claims it, in which case the guest gets
    /// the architectural breakpoint.
    fn serve_breakpoint(&mut self, core: usize, pc: u32) -> bool {
        let Some(hook) = self.hooks.get(pc) else {
            // The guest executed a `break` nothing claims. It gets the
            // architectural debug exception, which on this chip's mask ROM
            // is three instructions ending in `simcall` — so what a reader
            // sees is an unsupported opcode inside
            // `_DebugExceptionVector`, a long way from the pc that asked for
            // it. Name the real one here, once per address.
            log::warn!(
                "break at {pc:#010x}{} with no hook: the guest takes the \
                 architectural debug exception",
                match self.symbolize(pc) {
                    Some(name) => format!(" ({name})"),
                    None => String::new(),
                }
            );
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
                let ra = self.harts[core].cpu().a(0);
                self.harts[core].set_pc(ra);
                true
            }
            HookResult::Breakpoint => false,
            HookResult::Stop => {
                self.stop_at = Some((core, pc));
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
            matrix: self.bus.matrix().save_state(),
            sched: self.bus.sched.save(),
            rng: self.rng,
            hook_calls: self.hook_calls,
            idle_skips: self.idle_skips,
            wfi_ends: self.wfi_ends,
            core_quantum: self.core_quantum,
            pins: self.pins.state.clone(),
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
        self.bus.matrix_mut().load_state(&snap.matrix);
        self.bus.sched.restore(&snap.sched);
        self.rearm_watchpoints();
        // The decoders and their frames ride; the sinks do not, and the
        // `epoch` is reset so the first drain after a restore re-reads the
        // fabric's routing rather than trusting a number from another run.
        self.pins.state = snap.pins.clone();
        self.pins.epoch = 0;
        self.rng = snap.rng;
        self.hook_calls = snap.hook_calls;
        self.idle_skips = snap.idle_skips;
        self.wfi_ends = snap.wfi_ends;
        // The released-frame record follows DPORT's own `reported` flag,
        // which `restore_peripherals` has just put back: a snapshot taken
        // before the release carries no frame, one taken after carries the
        // frame the release used. A reboot restores the power-on snapshot,
        // where `reported` is false, so the frame goes with it.
        self.app_core_frame = self
            .appcpu
            .lock()
            .expect("appcpu poisoned")
            .reported
            .then(|| BootFrame::rom_app_stack(&self.rom));
        // The quantum is something a run's future depends on (D3), so a
        // restored run takes the snapshot's — and says so if that differs
        // from what this machine was built with.
        if self.core_quantum != snap.core_quantum {
            log::info!(
                "machine: restore adopts the snapshot's core quantum ({} cycles/window; this \
                 machine was built with {})",
                snap.core_quantum,
                self.core_quantum
            );
            self.core_quantum = snap.core_quantum;
        }
    }

    /// Re-arm the bus's hardware watchpoints after a restore.
    ///
    /// **A bug P6 found by being the first code that reboots.** A DBREAK
    /// watchpoint lives on the **bus** and is written only by the hart's
    /// `wsr` to `DBREAKA`/`DBREAKC` (`lp-xt-emu/src/mach/exec.rs:651-660`).
    /// A restore replaces the hart wholesale, so the bus keeps whatever the
    /// *previous* hart had armed while the restored hart's `DBREAK` pair
    /// reads zero — and the two disagree until something writes `DBREAK`
    /// again, which on a fresh boot nothing does for milliseconds.
    ///
    /// The shipped image arms one: esp-hal's stack guard is a four-byte
    /// store watchpoint (`lp-xt-emu/src/mach/tests.rs:1448-1457`). Before
    /// this call, a cable reset of a running app took a **debug exception**
    /// fourteen instructions into the second boot, vectored through the mask
    /// ROM's table and died on an unsupported instruction at `0x4000_0705`.
    /// `src/snapshot.rs` already promised that "a restore re-arms them from
    /// the restored hart"; nothing did.
    ///
    /// ⚠️ **This re-arms them from the power-on state, not from the restored
    /// hart.** Every slot is disarmed through the ordinary
    /// [`Bus::set_watchpoint`] path, so the armed bitmask and the
    /// single-store fast path are recomputed with it. That is exactly right
    /// for a reboot — the power-on
    /// snapshot has `DBREAK` clear — and for every snapshot this machine
    /// takes. Reading the pair back off the hart would need a public
    /// accessor for its `BreakUnit`, which `lp-xt-emu` does not publish;
    /// **M1 owns that crate and M3 reads it**, so the accessor is an M1
    /// item and this is the honest half of the fix, not the whole one.
    fn rearm_watchpoints(&mut self) {
        for slot in 0..crate::bus_setup::WATCHPOINT_SLOTS {
            Bus::set_watchpoint(&mut self.bus, slot, None);
        }
    }
}

impl fmt::Debug for Machine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Machine")
            .field("boot_mode", &self.boot_mode)
            .field("time_grade", &self.time_grade)
            .field("cores", &CORES)
            .field("stalled", &self.stalled)
            .field("core_quantum", &self.core_quantum)
            .field("cycle", &self.cycles())
            .field("pc", &format_args!("{:#010x}", self.harts[0].pc()))
            .field("pc1", &format_args!("{:#010x}", self.harts[1].pc()))
            .finish()
    }
}
