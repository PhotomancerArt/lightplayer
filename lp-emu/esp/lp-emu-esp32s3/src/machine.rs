//! The S3 machine: two hart slots, a bus, a mask ROM and a run loop.
//!
//! `lp-emu-esp32v3`'s twin in shape, file by file and on purpose (ruling
//! DD45 R1: **copy**, do not generalise — the extraction into
//! `lp-emu-esp-common` is M8's, after RV32's M7 P5). Where the two chips
//! genuinely differ the difference is written down where the number is,
//! never left for a reader to infer.
//!
//! # Two cores, one clock
//!
//! The S3 is dual-core. Both slots are constructed — each holds architectural
//! state, appears in a snapshot, and is named by `--probe` and the run report
//! — and [`Machine::run_until`] hands **each core that is not held** a window
//! of at most [`CORE_QUANTUM_DEFAULT`] cycles (`--core-quantum`) per
//! iteration, core 0 then core 1, on **one** guest clock: every running core
//! is given the same window, due scheduler events fire between windows and
//! never inside one, and the machine's clock is the furthest any hart has
//! got. A core that is held costs nothing and its counters do not move; a
//! core parked in `waiti` costs nothing either. When every running core is
//! parked, guest time jumps to the next thing that can wake any of them (the
//! deterministic idle skip): a scheduled event, or **either hart's own
//! `CCOMPARE` match** (ruling DD45 R9).
//!
//! The interleave is a pure function of the instruction streams and the
//! quantum. It is **not silicon's scheduling**, and the classic's statement
//! of that holds here unchanged: a store by core 0 is visible to core 1's
//! next load with no store buffer and no cache-coherence window in between,
//! so cross-core visibility is *stronger* than the part's, on purpose.
//!
//! # Slot 1 is held, and nothing in this milestone releases it
//!
//! Q4: two slots, slot 1 **permanently held**. That is not a simplification,
//! it is what this firmware does. `fw-esp32s3` calls no `start_app_core`, and
//! `ets_set_appcpu_boot_addr` is linker-provided but **unreferenced**
//! (`m6/notes.md` §2.4). The 12 `INTERRUPT_CORE1` accesses the census found
//! are esp-hal's per-core-*generic* code — `interrupt::mapped_to_raw` and
//! friends indexing a table by `raw_core()` — not a second core starting.
//!
//! [`Machine::core_stalled`] is the OR of three inputs:
//!
//! - the machine's own `stalled` flag, set for slot 1 at build and cleared by
//!   nothing in M6;
//! - **`SYSTEM.core_1_control_0`** ([`CoreOneControl`]), whose PAC reset value
//!   is `0x04` — `reseting = 1`, `clkgate_en = 0` — i.e. held from the first
//!   cycle by the chip's own reset state, which is why the field's default is
//!   not a choice this file made. P04's `SYSTEM` view
//!   ([`crate::periph::system`]) is the door a guest writes it through;
//! - RTC_CNTL's `sw_stall_appcpu_c0`/`sw_stall_appcpu_c1` pair, which stalls
//!   only when the two read `0x86` (`third_party/esp-hal/src/soc/esp32s3/
//!   cpu_control.rs:55-77`) — [`crate::periph::rtc_cntl::StallKey`], P04's
//!   third input, published by the RTC_CNTL view and read here.
//!
//! ⚠️ **This machine does not implement a core-1 release, and the classic's
//! must not be copied.** DD53's model — core 1 begins at `appcpu_ctrl_d`'s
//! address with no ROM code run — is the classic's *finding*, pinned on
//! classic silicon by a canary (`lp-emu-esp32v3/src/machine.rs`'s module
//! docs). The S3's registers are different ones and no equivalent
//! measurement exists. So the **hold** is modelled, faithfully and with the
//! register cited, and the release is left unimplemented and said to be. A
//! guest that wrote `start_core1`'s sequence would clear the hold's inputs
//! and this machine would notice — see [`Machine::core_stalled`] — and there
//! is deliberately nothing behind that to put a core anywhere, because
//! "where" is a measurement nobody has made.
//!
//! # Why the ROM comes before everything
//!
//! On this chip the mask ROM is **most of the dynamic instruction count**:
//! 4,769 `memcpy` call sites across 800 caller symbols, 191 `__divsf3`, 162
//! `memmove`, 150 `memset` (`m6/notes.md` §2.5). So the bring-up order is
//! hart → memory map → ROM → direct load → first MMIO stop, and
//! `tests/boot.rs` asserts a direct load of the shipped image really executes
//! ROM code rather than merely having it mapped. A machine that cannot
//! execute ROM `memcpy` never reaches a peripheral.
//!
//! # Peripherals: the blocks before the console, in the order the boot met them
//!
//! P03 registered **no** peripheral, so a `--strict-bus` run stopped at the
//! first MMIO access of the boot. P04 ran that loop and registers, in
//! [`PERIPHERAL_REGISTRATION_ORDER`], every block the boot touches before it
//! needs the console — each a view or an accept block from
//! [`crate::periph`], each a probe that let the *next* stop become visible.
//! The order is the ledger, and it is a contract: the bus packs a
//! peripheral's index into every scheduler event id, so a block a later
//! phase adds goes in its place in the list, never on the end.
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
//! After P04 the loop stands at the console (`USB_DEVICE`), which is P05's.
//!
//! # Candidates for M8's extraction, noted and not taken
//!
//! Three things in this file are now written three times across the C6, the
//! classic and the S3 and differ only in their constants: the quantum
//! interleave loop, the idle skip's wake set, and `Outcome`'s exit-code
//! table. DD45 R1 says copy; this note is so M8 does not have to find them.

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lp_emu_core::Bus;
use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::bus::StrictViolation;
use lp_emu_esp_common::host::ByteSink;
use lp_emu_esp_common::{ByteLog, ByteSource, ElfImage, MachineRequest, SocBus, Strap};

use crate::control::{ControlCommand, ControlReply, HostReport};
use crate::periph::HostStreams;
use crate::periph::usb_sj::{LIVE_POLL_CYCLES, UsbSerialJtag};

use lp_xt_emu::mach::interrupt::{IntKind, IntLine};
use lp_xt_emu::mach::sr::PS_BOOT;
use lp_xt_emu::mach::trap::NUM_INTERRUPTS;
use lp_xt_emu::mach::{CoreConfig, HartFault, SliceEnd, XtHart};

pub use crate::loader::PlacedAppSegment;
use crate::loader::{self, EfuseIdentity, LoadError, ResetCause};
use crate::memmap;
use crate::periph::rtc_cntl::StallKey;
use crate::rom::{self, DataImage, HookResult, HookTable, PlacedSegment, RomError, SeededSection};
use crate::snapshot::Snapshot;

/// The longest slice the loop hands a hart. Small enough that a scheduled
/// event lands within a bounded number of instructions of its cycle, large
/// enough that the per-slice bookkeeping is amortised. Both other machines'
/// number, for the same reason.
pub const MAX_SLICE_CYCLES: u64 = 8_192;

/// Under `--strict-bus` the loop checks for a refusal between slices, so a
/// shorter slice reports the violating access closer to where it happened.
const STRICT_SLICE_CYCLES: u64 = 1_024;

/// The default per-core window, in guest cycles (`--core-quantum`).
///
/// The classic's number and the classic's argument, which ports because the
/// clock does: a window this size puts ~30 of them inside the shortest thing
/// either core waits on. It is the *upper* bound on one hart's window — a
/// scheduled event or a strict slice still shortens it — and it is a
/// **parameter**, recorded in the run report, in [`Machine::core_report`] and
/// in the snapshot. Never a tuned constant, and never a claim about silicon's
/// scheduling.
pub const CORE_QUANTUM_DEFAULT: u64 = 256;

/// The number of cores the S3 has. Both are constructed; slot 1 is held.
pub const CORES: usize = 2;

/// The order the S3's peripheral blocks are registered in. See the module
/// docs for why this is a contract.
///
/// The list is the blocks the P04 strict runs reached, **in the order the
/// direct load met them**, with the cycle of each block's first access on
/// the shipped image beside it. A block a later phase needs is added in its
/// place in this list, never appended for convenience.
/// [`crate::periph::boot_set`] registers exactly this order.
pub const PERIPHERAL_REGISTRATION_ORDER: &[&str] = &[
    // Cycle 36: the mask ROM's `Cache_Occupy_ICache_MEMORY+0xc` reads
    // `cache_dataarray_connect_1` on `rom_config_instruction_cache_mode`'s
    // path — P03's first strict stop.
    "SENSITIVE",
    // Cycle 71: `Cache_Set_ICache_Mode` reads `icache_ctrl`; the
    // invalidate, freeze and preload polls follow inside the same routine.
    "EXTMEM",
    // Cycle 403: `esp32_init` → `setup_interrupts` clears the maps, and
    // esp-hal does the *other* core's first (`Cpu::other()`), so core 1's
    // half is reached four cycles before core 0's.
    "INTERRUPT_CORE1",
    // Cycle 407: core 0's half of the same block.
    "INTERRUPT_CORE0",
    // Cycle 205,054: the ROM's `rtc_get_reset_reason` reads `reset_state`
    // for `lp_recovery`'s reset-cause map.
    "RTC_CNTL",
    // Cycle 206,309: `system::disable_peripherals` read-modify-writes
    // `perip_clk_en1`.
    "SYSTEM",
    // Cycle 207,842: `pvt_supported` reads `BLK_VERSION`
    // (`rd_sys_part1_data4`).
    "EFUSE",
    // Cycle 207,952: `ensure_voltage_raised`'s `regi2c` writes through the
    // ROM's `rom_chip_i2c_writeReg`, which reads `ana_config2` first.
    "I2C_ANA_MST",
    // Cycle 288,925: `calibrate_rtc_slow_clock` reads `rtccalicfg`; the
    // 1024-cycle measurement that follows is the 7.5 ms gap before the next
    // block.
    "TIMG0",
    // Cycle 2,097,791: `esp_hal::init`'s inlined clock-gate writes, in the
    // order the census predicted — `APB_CTRL.clkgate_force_on`, then the
    // two SPI `clock_gate`s, then the four RF blocks' one register each.
    "APB_CTRL",
    "SPI0",
    "SPI1",
    "BB",
    "NRX",
    "FE",
    "FE2",
    // Cycle 2,098,065: `Wdt::<TIMG1>::new().disable()` — the second group
    // is met only for its watchdog.
    "TIMG1",
    // ⚠️ Not reached before the console. `time_init` on this chip is
    // `init_timestamp_scaler`, which computes from the clock tree and
    // touches no register; the first `Instant::now()` in the shipped image
    // comes after `esp_println`'s first write, which is P05's stop. It is
    // registered here — last, where the boot will meet it — because it is
    // the S3's clock and `tests/clock.rs` pins its derivation.
    "SYSTIMER",
    // Cycle 2,137,399: `esp_println::Printer::write_bytes+0x9` reads
    // `ep1_conf` at `0x6003_8004` — P04's last strict stop, and the console
    // (M6 P05).
    //
    // ⚠️ **Appended, not inserted**, and the two readings of this list part
    // company here for the first time. The boot meets the console BEFORE it
    // meets SYSTIMER: the note above registered SYSTIMER last on the
    // expectation that `Instant::now()` was the last thing the boot would
    // reach, and the console beats it by a few hundred cycles. Re-sorting
    // the pair to match the boot would move SYSTIMER's peripheral index, and
    // the bus packs that index into every scheduler event id — so the index
    // contract wins over the narrative, and this comment is the narrative.
    "USB_DEVICE",
];

/// How far back [`Machine::symbolize`] will look for a name when no symbol's
/// size covers the address. Four kilobytes: further than any routine in this
/// ROM, and short enough that a pc in a genuine hole is reported as a hole.
pub const NEAREST_SYMBOL_WINDOW: u32 = 0x1000;

/// The 32 CPU interrupt lines: each one's fixed level and type.
///
/// Source: `third_party/esp-hal/src/interrupt/xtensa.rs:19-98`, whose
/// `CpuInterrupt` variant names spell out both fields
/// (`Interrupt6Timer0Priority1`, `Interrupt22EdgePriority3`, …). That file is
/// **not per-chip** — it is the table every Espressif Xtensa part in esp-hal
/// allocates out of, the classic included — which is what makes it the right
/// source rather than a datasheet transcription, and why this constant is
/// identical to the classic's.
///
/// ⚠️ **Eight lines are [`IntLine::UNUSED`] here and are not known to be
/// unused on silicon**: 14, 16, 24, 25, 26, 28, 30 and 31. esp-hal's own
/// table carries a `// TODO: re-add higher level interrupts` and stops at 29,
/// so those eight have no cited level or type in this repository. The
/// firmware cannot allocate a line esp-hal does not name, so P03 never needs
/// them; a phase that does must pin them from the TRM and say so here rather
/// than filling them in by analogy.
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
/// **t1 only** (E1, notes Q2). There is no measured LX7 per-instruction-class
/// table in this repository, and M1 P3 is explicit about why one must not be
/// invented: six months later an invented number is indistinguishable from a
/// measured one. `CCOUNT` on this part is CPU cycles at 240 MHz, which is a
/// better future calibration source than the C6 ever had — an M8 opportunity,
/// not a P03 one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeGrade {
    /// `lp-emu:esp32s3:t1` — one cycle per instruction.
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
            TimeGrade::T1 => "lp-emu:esp32s3:t1",
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "t1" => Ok(TimeGrade::T1),
            "t2" | "t3" => Err(format!(
                "time grade `{text}` does not exist on this machine: there is no measured \
                 Xtensa per-instruction-class table in this repo, and inventing one would be \
                 indistinguishable from a measured one six months later. t1 (cycles = \
                 instructions) is the only grade M6 defines"
            )),
            other => Err(format!("unknown time grade `{other}` (expected t1)")),
        }
    }
}

/// The USB host's state at power-on, under the name the CLI spells it
/// (`--usb-host absent|attached|attached-idle`). The control channel and
/// `--usb-script` move it from there; see [`crate::periph::usb_sj`] for what
/// each state does to the registers.
pub use crate::periph::usb_sj::HostState as UsbHost;

/// Where the USB-Serial-JTAG bytes go. Used twice: for the `usb-sj` stream —
/// what a **host received** from the IN endpoint (`--usb-sj`) — and for the
/// observation stream — what the guest handed over that no host took
/// (`--usb-sj-tried`). Both are always also kept in memory
/// ([`Machine::usb_sj`], [`Machine::usb_sj_tried`]).
#[derive(Clone, Debug, Default)]
pub enum UsbSjSink {
    #[default]
    Memory,
    Stderr,
    File(PathBuf),
    /// **Listen** on this address for one client at a time: the client
    /// receives what a host received, and its bytes are the OUT endpoint's
    /// live source. `lp-cli … serial:tcp://<addr>` connects to it unchanged.
    ///
    /// Only the `usb-sj` stream may be a socket — the observation stream is
    /// an observation, not a link.
    Tcp(String),
}

/// Whether connecting to the USB byte socket opens the port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UsbSjDrain {
    /// A client connecting is an application opening the port;
    /// disconnecting is it closing. What `lp-cli`'s readiness engine
    /// expects, and what a Web Serial `open()`/`close()` means.
    #[default]
    Auto,
    /// The control channel owns `open` and `close`; the socket only carries
    /// bytes. For a run that wants a host attached and *not* draining while
    /// a client watches the byte stream.
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

/// A `ByteSink` that keeps every byte in a log as well as passing it on.
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

/// A `ByteSink` over the host's stderr, kept off stdout so it never mixes
/// with a run report.
struct StderrSink;

impl ByteSink for StderrSink {
    fn write(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(bytes);
        let _ = err.flush();
    }
}

/// A `ByteSink` over a file handle, buffered: a `write(2)` per byte is
/// measurable on a console that drains a byte at a time.
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
/// grammar — never an `M!` frame.
struct ControlChannel {
    host: lp_emu_esp_common::TcpHost,
    partial: Vec<u8>,
}

/// The longest control line the channel will assemble. A client that sends
/// more without a newline is answered once and resynchronised, rather than
/// growing a buffer for as long as it keeps typing.
const MAX_CONTROL_LINE: usize = 4 << 10;

/// Where the mask ROM comes from. There is no "no ROM" variant: plan PD7 says
/// the ROM is loaded in every configuration, and on this chip it is most of
/// the instruction count.
#[derive(Clone, Debug, Default)]
pub enum RomSource {
    /// The compiled-in `esp32s3_rev0_rom.elf`.
    #[default]
    Vendored,
    /// `--rom <path>`.
    Path(PathBuf),
}

/// The application image, if there is one.
///
/// ⚠️ [`AppSource::None`] is **not a ROM-up boot**. It builds a machine with
/// the map, the ROM and two harts and leaves core 0 at the ROM's reset
/// vector with no boot frame; the tests use it, and the CLI refuses it,
/// because starting the real ROM's reset path is **P06**'s and this phase has
/// walked none of it.
#[derive(Clone, Debug, Default)]
pub enum AppSource {
    #[default]
    None,
    /// `--elf <path>`.
    Path(PathBuf),
}

/// `SYSTEM.core_1_control_0` (`0x600C_0000 + 0x00`) — the register that holds
/// core 1 on this part.
///
/// Fields from `esp32s3-0.35.2/src/system/core_1_control_0.rs`:
/// `control_core_1_runstall` bit 0 ("Set 1 to stall core1"),
/// `control_core_1_clkgate_en` bit 1 ("Set 1 to open core1 clock"),
/// `control_core_1_reseting` bit 2 ("Set 1 to let core1 reset").
///
/// **The reset value is `0x04`** — the PAC's own `Resettable::RESET_VALUE` —
/// so at power-on `reseting` is set and `clkgate_en` is clear, and core 1 is
/// held by the chip before any software has run. That is why
/// [`CoreOneControl::reset`] is not a choice this file made.
///
/// P04 gives this register an MMIO view and shares this handle with it.
/// [`Machine::core_stalled`] already reads it, so a guest that wrote
/// `start_core1`'s sequence would meet a modelled hold rather than an
/// unmapped stop — and a future S3 firmware with a second core is a P04
/// change, not a machine rewrite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreOneControl {
    pub runstall: bool,
    pub clkgate_en: bool,
    pub reseting: bool,
}

impl CoreOneControl {
    /// The PAC's reset value, `0x04`.
    pub const fn reset() -> Self {
        Self {
            runstall: false,
            clkgate_en: false,
            reseting: true,
        }
    }

    /// The register as a word, in the PAC's bit order.
    pub const fn bits(self) -> u32 {
        (self.runstall as u32) | ((self.clkgate_en as u32) << 1) | ((self.reseting as u32) << 2)
    }

    /// Read the three fields out of a word.
    pub const fn from_bits(v: u32) -> Self {
        Self {
            runstall: v & 1 != 0,
            clkgate_en: v & 2 != 0,
            reseting: v & 4 != 0,
        }
    }

    /// Does this register hold core 1?
    ///
    /// esp-hal's own `is_running` reads exactly two of the three —
    /// "CORE_1_RUNSTALL in bit 0 -> needs to be 0 to not stall, CORE_1_CLKGATE_EN
    /// in bit 1 -> needs to be 1 to even be enabled"
    /// (`third_party/esp-hal/src/soc/esp32s3/cpu_control.rs:41-51`) — and
    /// `reseting` is the third because `start_core1` ends by pulsing it set
    /// then clear (`:80-101`), i.e. a core with `reseting` set is in reset.
    pub const fn holds_core1(self) -> bool {
        self.runstall || self.reseting || !self.clkgate_en
    }
}

impl Default for CoreOneControl {
    fn default() -> Self {
        Self::reset()
    }
}

/// The handle P04's `SYSTEM` view and the machine share.
pub type CoreOneHandle = Arc<Mutex<CoreOneControl>>;

/// The stack state a direct load seeds — **the seam M1 P4's finding created**.
///
/// # Why a direct load must seed a stack at all
///
/// [`PS_BOOT`] is `WOE | UM | CALLINC(2)` = `0x0006_0020`, and the `CALLINC(2)`
/// is not decoration: a bootloader reaches the application's entry point
/// through an ordinary C call, which on the windowed ABI is `callx8`. So the
/// app's `Reset:` runs as frame **2**, and frame 0 — the bootloader's own —
/// stays live behind it.
///
/// A hart left with `a1 = 0` therefore has a live outermost frame whose stack
/// pointer is null. Nothing notices until the first register spill, and then
/// `_WindowOverflow8`'s `l32e a0, a1, -12` reads `0xFFFF_FFF4`, faults, and
/// the fault's own spill faults again: a double exception, forever, nowhere
/// near its cause.
///
/// So a direct load seeds two things: **`a1`**, and **the four words of that
/// frame's base save area** at `[a1-16, a1)`, which is where
/// `_WindowOverflow4/8/12` store `a0..a3` and where `l32e a0, a1, -12` reads
/// the *next* frame's stack pointer from.
///
/// # What P03 pins, and what it does not
///
/// [`BootFrame::rom_pro_stack`] uses the mask ROM's own PRO-core stack top,
/// `__stack` — resolved from the vendored ROM ELF's symbol table, where it is
/// `0x3FCE_B710` ([`memmap::ROM_PRO_STACK_TOP`]). That is a cited value and
/// it is the right *shape*.
///
/// ⚠️ **It is not the bootloader's own SP, and P06 owns that.** The ESP-IDF
/// second-stage bootloader runs on a stack its own linker script places, and
/// the value it holds at the `callx8` into the app is P06's to derive from
/// the ROM-up chain — from the bootloader disassembly or from a ROM-up run
/// that gets that far. Until then this is the machine's *seam*: a builder
/// parameter with a cited default and a test that proves a seeded hart
/// survives its first exception, not a claim about what silicon holds.
///
/// # `owb` is zero here, and the classic's 7 is not carried over
///
/// The classic seeds `PS.OWB = 7` because a cross-check **measured** it: its
/// ROM-up walk read `PS = 0x0006_0720` at the application's entry against the
/// direct load's `0x0006_0020`. No such walk exists on the S3 until P06, so
/// this field is zero — the architectural value for a frame no window
/// exception has touched — and a P06 cross-check is what would change it.
/// Carrying the classic's 7 across would be a measurement of one chip
/// reported as a fact about another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootFrame {
    /// `a1` at entry: the outermost live frame's stack pointer.
    pub sp: u32,
    /// The four words written at `[sp-16, sp)`: `a0, a1, a2, a3` in that
    /// order, which is the order `_WindowOverflow4` stores them in
    /// (`s32e a0, a5, -16` … `s32e a3, a5, -4`).
    pub save_area: [u32; 4],
    /// `PS.OWB` at the application's entry. Zero until P06 measures one.
    pub owb: u8,
}

impl BootFrame {
    /// The mask ROM's PRO-core stack top, from the ROM ELF's `__stack`.
    ///
    /// Falls back to [`memmap::ROM_PRO_STACK_TOP`] if the symbol is missing,
    /// which for the vendored ROM it is not — the fallback exists so a
    /// `--rom` override without symbols still produces a machine rather than
    /// an error.
    pub fn rom_pro_stack(rom: &ElfImage) -> Self {
        let sp = rom
            .symbol("__stack")
            .map(|s| s.address)
            .unwrap_or(memmap::ROM_PRO_STACK_TOP);
        Self::at(sp)
    }

    /// A boot frame at an explicit stack pointer, with the default save area.
    ///
    /// The saved `a1` points back at the stack top, so an overflow-8 spill
    /// through it writes into the ROM stack region — mapped, and inside
    /// memory the ROM owns. The saved `a0` is zero, so a *return* through the
    /// outermost frame lands at address zero and stops loudly instead of
    /// running into whatever is there.
    pub const fn at(sp: u32) -> Self {
        Self {
            sp,
            save_area: [0, sp, 0, 0],
            owb: 0,
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
    /// the symbol's first instruction with every register as the caller left
    /// it.
    Breakpoint { core: usize, cycle: Cycles, pc: u32 },
    /// A peripheral asked for a chip reset — the RWDT's stage 0 expired —
    /// and this machine has no boot chain to restart until P06, so the run
    /// ends here and says who asked. Exit code 2, the classic's code for the
    /// same outcome.
    Reset {
        cycle: Cycles,
        source: &'static str,
        strap: Strap,
    },
    /// `--exit-on <substr>` matched a **complete** line on the console.
    /// Exit code 0: the run did what it was asked for.
    ExitMatched { cycle: Cycles },
}

impl Outcome {
    /// The CLI's exit code for this outcome.
    ///
    /// **A cross-machine contract**, so a script that drives all three
    /// machines reads one table: 0 deadline or `--exit-on`, 2 fault or a
    /// reset the machine could not perform, 3 strict-bus refusal, 4 wall
    /// timeout, 5 `--break-at`, 6 cache-off fetch, 7 MMU divergence.
    ///
    /// Two of those cannot arise here, and saying so is cheaper than leaving
    /// a reader to wonder:
    ///
    /// - **6** is the cache-off fetch stop. This machine has no cache model —
    ///   `EXTMEM` is **P06**'s — so nothing can produce it yet, and P06 adds
    ///   the variant rather than reusing a code.
    /// - **7** is a flash-MMU divergence between two cores' tables, and it is
    ///   **not applicable on this machine at all**: one core runs, so there
    ///   is no second MMU table to diverge from. The code stays reserved
    ///   across the family and this machine never emits it — which is stated
    ///   here rather than left as a silent gap in the table.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Outcome::Deadline { .. } | Outcome::ExitMatched { .. } => 0,
            Outcome::Fault { .. } | Outcome::Reset { .. } => 2,
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
            | Outcome::Breakpoint { cycle, .. }
            | Outcome::ExitMatched { cycle }
            | Outcome::Reset { cycle, .. } => *cycle,
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
    /// strict refusal or the first fault.
    pub stop_cycle: Option<Cycles>,
    /// Host-side safety net.
    pub wall_timeout: Option<Duration>,
    /// `(cycle, symbol)` — print the word at `symbol` when guest time reaches
    /// `cycle`.
    pub probes: Vec<(Cycles, String)>,
    /// `--exit-on <substr>`: stop at the end of the **complete** line this
    /// appears on, on the console. Checked at slice boundaries against the
    /// `usb-sj` log — which on this chip is the console, because the link is.
    pub exit_on: Option<String>,
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

/// The S3 machine, built.
///
/// ⚠️ Not `derive(Debug)`: `usb_sj_source` is a `Box<dyn ByteSource>` and a
/// trait object has none. The manual impl below prints the fields a test
/// failure wants and prints the source as a flag, which is the same thing
/// [`TraceSink`] does one field along.
#[derive(Default)]
pub struct Esp32S3Builder {
    rom: RomSource,
    app: AppSource,
    time_grade: TimeGrade,
    strict: bool,
    strict_unsupported: Option<bool>,
    boot_frame: Option<BootFrame>,
    cpenable_reset: Option<u32>,
    core_quantum: Option<u64>,
    trace: Option<TraceSink>,
    trace_blocks: Vec<String>,
    seed: u64,
    reset_cause: ResetCause,
    efuse: EfuseIdentity,
    // ---- the link (M6 P05) ----
    usb_sj: UsbSjSink,
    usb_sj_tried: UsbSjSink,
    usb_sj_source: Option<Box<dyn ByteSource>>,
    usb_script_source: Option<lp_emu_esp_common::ScriptedSource>,
    usb_host: UsbHost,
    usb_sj_drain: UsbSjDrain,
    control: Option<String>,
    usb_script: Vec<(Cycles, ControlCommand)>,
    reboot_on_reset: bool,
}

impl fmt::Debug for Esp32S3Builder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Esp32S3Builder")
            .field("rom", &self.rom)
            .field("app", &self.app)
            .field("time_grade", &self.time_grade)
            .field("strict", &self.strict)
            .field("boot_frame", &self.boot_frame)
            .field("core_quantum", &self.core_quantum)
            .field("seed", &self.seed)
            .field("usb_sj", &self.usb_sj)
            .field("usb_sj_tried", &self.usb_sj_tried)
            .field("usb_host", &self.usb_host)
            .field("usb_sj_drain", &self.usb_sj_drain)
            .field("usb_sj_source", &self.usb_sj_source.is_some())
            .field("usb_script_source", &self.usb_script_source.is_some())
            .field("control", &self.control)
            .field("usb_script", &self.usb_script.len())
            .field("reboot_on_reset", &self.reboot_on_reset)
            .finish_non_exhaustive()
    }
}

/// A trace destination, kept out of [`Esp32S3Builder`]'s `Debug` by being its
/// own type: a `Box<dyn Write + Send>` has none, and a builder that could not
/// be printed in a test failure would be worse than one whose sink prints as
/// a flag.
pub struct TraceSink(pub Box<dyn std::io::Write + Send>);

impl fmt::Debug for TraceSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TraceSink(..)")
    }
}

impl Esp32S3Builder {
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

    pub fn time_grade(mut self, grade: TimeGrade) -> Self {
        self.time_grade = grade;
        self
    }

    /// Every access to an address nothing claims becomes a stop instead of a
    /// silent zero. **The bring-up switch**: this phase's deliverable is the
    /// first stop a strict run produces.
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Stop on an instruction word this hart does not implement, rather than
    /// giving the guest an illegal-instruction exception it would handle
    /// silently. On by default, as on the classic (DD16).
    pub fn strict_unsupported(mut self, on: bool) -> Self {
        self.strict_unsupported = Some(on);
        self
    }

    /// Override the boot frame a direct load seeds. See [`BootFrame`] for
    /// what the default claims and what P06 owes.
    pub fn boot_frame(mut self, frame: BootFrame) -> Self {
        self.boot_frame = Some(frame);
        self
    }

    /// What `CPENABLE` holds when a core comes out of reset.
    ///
    /// ⚠️ **A parameter, and the default is the ISA's generic reset, not the
    /// classic's `0xff`** (assumption A4). The classic's value is a
    /// *measurement* on classic silicon; the S3 firmware arms `CPENABLE`
    /// itself with a read-modify-write precisely because the provenance of an
    /// already-armed value is unpinned — its own module says so
    /// (`lp-fw/fw-esp32s3/src/board/esp32s3/fpu.rs:18-24`: "That is a
    /// measured fact about *this boot chain*, not a guarantee from the
    /// architecture, and its provenance is unpinned"). **P09's silicon
    /// capture is what changes this default**, and until then a machine that
    /// assumed `0xff` would be reporting one chip's measurement as another's.
    pub fn cpenable_reset(mut self, value: u32) -> Self {
        self.cpenable_reset = Some(value);
        self
    }

    /// The upper bound on one core's window, in cycles.
    pub fn core_quantum(mut self, cycles: u64) -> Self {
        self.core_quantum = Some(cycles);
        self
    }

    pub fn trace(mut self, sink: Box<dyn std::io::Write + Send>, blocks: Vec<String>) -> Self {
        self.trace = Some(TraceSink(sink));
        self.trace_blocks = blocks;
        self
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// What `RTC_CNTL.reset_state` reports (loader item 6). One variant
    /// today; see [`ResetCause`].
    pub fn reset_cause(mut self, cause: ResetCause) -> Self {
        self.reset_cause = cause;
        self
    }

    /// The part this run claims to be: the MAC and the wafer version the
    /// eFuse block answers (loader item 7; `--efuse-mac`, `--efuse-rev`).
    pub fn efuse(mut self, identity: EfuseIdentity) -> Self {
        self.efuse = identity;
        self
    }

    // ---- the link (M6 P05) ------------------------------------------------

    /// Where the `usb-sj` stream goes: what a **host received** from the IN
    /// endpoint. A `Tcp` sink also **is** the OUT endpoint's live source.
    pub fn usb_sj(mut self, sink: UsbSjSink) -> Self {
        self.usb_sj = sink;
        self
    }

    /// Where the observation stream goes: bytes the guest handed over that
    /// no host took. Never a socket — it is an observation, not a link.
    pub fn usb_sj_tried(mut self, sink: UsbSjSink) -> Self {
        self.usb_sj_tried = sink;
        self
    }

    /// Host → device bytes, for a test that wants neither a socket nor a
    /// script file.
    pub fn usb_sj_source(mut self, source: Box<dyn ByteSource>) -> Self {
        self.usb_sj_source = Some(source);
        self
    }

    /// The byte half of a `--usb-script`. Preferred over
    /// [`usb_sj_source`](Self::usb_sj_source) for a script, because the
    /// machine wires it to watch the delivered log — which is what makes an
    /// `after "<line>"` step see exactly what a host on this link saw.
    pub fn usb_script_source(mut self, script: lp_emu_esp_common::ScriptedSource) -> Self {
        self.usb_script_source = Some(script);
        self
    }

    /// The host's state at power-on. `Absent` is the honest default for a
    /// machine with nothing plugged into it; the control channel and
    /// `--usb-script` move it from there.
    pub fn usb_host(mut self, host: UsbHost) -> Self {
        self.usb_host = host;
        self
    }

    /// Whether a byte-socket client counts as an application opening the
    /// port.
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

    /// **Perform** a reset request instead of reporting it: the machine goes
    /// back to the state it was built in and runs again, as a chip does.
    ///
    /// ⚠️ There is still no ROM-up boot on this machine (P06), so a reboot
    /// is a **direct load** replayed: the app's segments are placed again and
    /// core 0 starts at its entry with the same boot frame. That is what a
    /// reboot of a direct-loaded machine can honestly be, and the run report
    /// says how many happened.
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
        if let Some(TraceSink(sink)) = self.trace {
            bus.trace =
                lp_emu_esp_common::Trace::to_sink(sink).with_block_filter(self.trace_blocks);
        }

        // The chip's interrupt matrix, before any peripheral: the two
        // INTERRUPT_CORE views downcast to it on their first store, and
        // `esp32_init`'s `setup_interrupts` performs that store a few
        // hundred cycles in. **The mask form, and nothing else** — see
        // `crate::intmatrix`'s module docs for X43.
        bus.set_matrix(Box::new(crate::intmatrix::Esp32S3IntMatrix::new()));

        // RTC_CNTL publishes its half of the CPU stall key through this
        // handle and the machine reads it; SYSTEM's `core_1_control_0` is
        // the other chip-side input and rides in `core_1_control` below.
        let stall_key = StallKey::new();
        let core_1_control: CoreOneHandle = Arc::new(Mutex::new(CoreOneControl::reset()));

        // The link's two byte streams. `usb-sj` is what a host received —
        // and, from its source, what it sent; `usb-sj-tried` is the
        // observation stream, bytes the guest handed over that no host took.
        let usb_sink = |sink: &UsbSjSink| -> Result<Box<dyn ByteSink>, BuildError> {
            Ok(match sink {
                UsbSjSink::Memory => Box::new(lp_emu_esp_common::host::NullSink),
                UsbSjSink::Stderr => Box::new(StderrSink),
                UsbSjSink::File(path) => {
                    let file = std::fs::File::create(path)
                        .map_err(|e| BuildError::Io(format!("creating {}: {e}", path.display())))?;
                    Box::new(FileSink(std::io::BufWriter::new(file)))
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
        // A USB script watches the same log the sink tees into, so an
        // `after "<line>"` step sees exactly what a host on this link saw.
        let usb_sj_source = match self.usb_script_source {
            Some(script) => {
                Some(Box::new(script.watching(usb_sj_log.clone())) as Box<dyn ByteSource>)
            }
            None => self.usb_sj_source,
        };
        let (usb_inner, usb_source): (Box<dyn ByteSink>, Box<dyn ByteSource>) = match &self.usb_sj {
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
                inner: usb_sink(&self.usb_sj_tried)?,
            }),
            Box::new(lp_emu_esp_common::host::NullSource),
        );
        let streams = HostStreams {
            usb_sj: Some(usb_sj_id),
            usb_sj_tried: Some(usb_sj_tried_id),
        };

        // The peripherals, in the declared order, before any memory is
        // placed: a block's index is fixed at registration and the schedule
        // is empty until `start_peripherals`.
        let set = crate::periph::boot_set(
            self.reset_cause,
            self.efuse,
            stall_key.clone(),
            Arc::clone(&core_1_control),
            streams,
            self.usb_host,
        );
        check_registration_order(&set)?;
        for (base, len, periph) in set {
            bus.add_peripheral(base, len, periph);
        }

        // The control channel's listener, and the block it drives. The index
        // is looked up once: it is the peripheral's identity for the whole
        // run (see PERIPHERAL_REGISTRATION_ORDER).
        let control = match self.control {
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

        // The ROM first, always (PD7), and its non-alloc data with it.
        let rom_segments = rom::load(&mut bus, &rom_image)?;
        let rom_data = rom::seed_data(&mut bus, &rom_image)?;

        let app_image = match &self.app {
            AppSource::None => None,
            AppSource::Path(p) => {
                let bytes = std::fs::read(p)
                    .map_err(|e| BuildError::Io(format!("reading {}: {e}", p.display())))?;
                Some(ElfImage::parse(&bytes).map_err(|e| BuildError::App(e.to_string()))?)
            }
        };

        let strict_unsupported = self.strict_unsupported.unwrap_or(true);
        let cpenable_reset = self.cpenable_reset.unwrap_or(CPENABLE_RESET_DEFAULT);

        // Two harts, always. The S3 is dual-core and a machine that pretended
        // otherwise would have nowhere for P04's SYSTEM view to write, and
        // nothing for a snapshot to carry.
        let harts: Vec<XtHart<SocBus>> = (0..CORES)
            .map(|core| fresh_hart(core, self.time_grade, strict_unsupported, cpenable_reset))
            .collect();

        let mut machine = Machine {
            bus,
            harts,
            // Slot 1 is held by the machine as well as by the chip's own
            // reset state, and nothing in M6 P03 clears either.
            stalled: [false, true],
            core_1_control,
            stall_key,
            reset_cause: self.reset_cause,
            efuse: self.efuse,
            core_quantum: self.core_quantum.unwrap_or(CORE_QUANTUM_DEFAULT),
            wfi_ends: [0; CORES],
            strict_unsupported,
            cpenable_reset,
            rom: rom_image,
            app: app_image,
            rom_segments,
            rom_data,
            app_segments: Vec::new(),
            hooks: HookTable::new(),
            time_grade: self.time_grade,
            boot_frame: None,
            stop_at: None,
            hook_calls: 0,
            idle_skips: 0,
            seed: self.seed,
            rng: self.seed,
            usb_sj_log,
            usb_sj_tried_log,
            usb_host: self.usb_host,
            usb_sj_tcp,
            usb_sj_drain: self.usb_sj_drain,
            usb_client_connected: false,
            usb_index,
            control,
            script: self.usb_script.into(),
            control_lines: 0,
            next_host_poll: 0,
            reboot_on_reset: self.reboot_on_reset,
            reboots: 0,
            strap: Strap::App,
            power_on: None,
        };

        if machine.app.is_some() {
            let frame = self
                .boot_frame
                .unwrap_or_else(|| BootFrame::rom_pro_stack(&machine.rom));
            machine.direct_load(frame)?;
        }

        // Guest time is zero and everything is placed: the one moment a
        // peripheral may schedule something before the guest touches it.
        machine.bus.start_peripherals();

        // The state a reboot goes back to, taken only when one was asked
        // for: a snapshot costs a copy of every region.
        if self.reboot_on_reset {
            machine.power_on = Some(machine.snapshot());
        }

        Ok(machine)
    }
}

/// `_ResetVector`'s offset from the mask ROM's base. The vendored ROM ELF's
/// `e_entry` is `0x4000_0400`.
pub const RESET_VECTOR_OFS: u32 = 0x400;

/// The architectural configuration of core `core`: the reset vector, the
/// reset `VECBASE`, the chip's `PRID` for that core and the fixed interrupt
/// table — the same for both cores except the `PRID`.
pub fn core_config(core: usize) -> CoreConfig {
    CoreConfig {
        reset_pc: memmap::ROM_MASK_BASE + RESET_VECTOR_OFS,
        reset_vecbase: memmap::ROM_MASK_BASE,
        prid: if core == 0 {
            memmap::PRID_CORE0
        } else {
            memmap::PRID_CORE1
        },
        interrupts: CORE_INTERRUPTS,
    }
}

/// What `CPENABLE` holds when a core comes out of reset on this part, as far
/// as this repository can say: **the ISA's generic reset, zero**.
///
/// ⚠️ **Not `0xff`.** See [`Esp32S3Builder::cpenable_reset`] for the whole
/// argument; the short version is that `0xff` is the *classic's* silicon
/// measurement, the S3 board's own `0xff` reading is explicitly recorded as
/// "a measured fact about this boot chain, not a guarantee from the
/// architecture" by the firmware that reads it, and this machine's job is to
/// hold the difference rather than paper over it. The shipped image arms bit
/// 0 itself before its first FP instruction, so a direct load is unaffected;
/// what *would* be affected is esp-hal's `float-save-restore` interrupt entry
/// on a core taking its first doorbell, which is P04's ground and is the
/// place P09's capture will matter.
pub const CPENABLE_RESET_DEFAULT: u32 = 0;

/// A hart at the architectural reset state of this part, with the machine's
/// cycle model and unsupported-opcode policy applied.
fn fresh_hart(
    core: usize,
    grade: TimeGrade,
    strict_unsupported: bool,
    cpenable_reset: u32,
) -> XtHart<SocBus> {
    let mut hart = XtHart::new(core as u32, core_config(core));
    hart.cpu_mut().cpenable = cpenable_reset;
    hart.set_cycle_model(grade.cycle_model());
    hart.set_strict_unsupported(strict_unsupported);
    hart
}

/// The ESP32-S3 machine.
pub struct Machine {
    bus: SocBus,
    /// **Two** slots: the S3 is dual-core. Slot 1 is held — see the module
    /// docs.
    pub harts: Vec<XtHart<SocBus>>,
    stalled: [bool; CORES],
    /// `SYSTEM.core_1_control_0`, shared with the `SYSTEM` view.
    core_1_control: CoreOneHandle,
    /// RTC_CNTL's two-register stall key, published by its view.
    stall_key: StallKey,
    /// The builder's two loader inputs, kept for the run report.
    reset_cause: ResetCause,
    efuse: EfuseIdentity,
    /// The upper bound on one core's window, in cycles (`--core-quantum`).
    core_quantum: u64,
    /// How many windows each core has ended in `waiti`.
    wfi_ends: [u64; CORES],
    /// The builder's policies, kept so a rebuilt hart is built the same way.
    strict_unsupported: bool,
    cpenable_reset: u32,
    rom: ElfImage,
    app: Option<ElfImage>,
    rom_segments: Vec<PlacedSegment>,
    rom_data: Vec<SeededSection>,
    app_segments: Vec<PlacedAppSegment>,
    hooks: HookTable,
    time_grade: TimeGrade,
    boot_frame: Option<BootFrame>,
    stop_at: Option<(usize, u32)>,
    hook_calls: u64,
    idle_skips: u64,
    seed: u64,
    rng: u64,
    /// **Everything the console said** — which on this chip is the `usb-sj`
    /// stream, because the console *is* the link: `esp-println` with the
    /// `jtag-serial` feature writes `ep1` directly and there is no
    /// `spike_uart0_link` build of this firmware. P03 declared the log and
    /// `--console` with no producer; M6 P05 is the producer.
    ///
    /// What is in here is what a **host received**, never what the guest
    /// tried: bytes committed into an endpoint nobody drained are on
    /// [`Machine::usb_sj_tried`] instead, and telling those two apart is the
    /// whole point of the host model.
    usb_sj_log: ByteLog,
    /// The observation stream: bytes the guest handed over that no host took.
    usb_sj_tried_log: ByteLog,
    /// The host's state at power-on, kept for the run report; the live state
    /// is the block's.
    usb_host: UsbHost,
    /// The byte socket's listener, when `--usb-sj tcp:` was chosen.
    usb_sj_tcp: Option<lp_emu_esp_common::TcpHost>,
    usb_sj_drain: UsbSjDrain,
    /// Whether a byte client was connected at the last poll — the edge the
    /// open/close coupling fires on.
    usb_client_connected: bool,
    /// `USB_DEVICE`'s peripheral index, the control channel's target.
    usb_index: Option<usize>,
    control: Option<ControlChannel>,
    /// Scripted control commands still to apply, in file order.
    script: std::collections::VecDeque<(Cycles, ControlCommand)>,
    /// How many control commands have been applied (scripted and socket).
    control_lines: u64,
    /// The next cycle the socket side is polled at.
    next_host_poll: Cycles,
    reboot_on_reset: bool,
    reboots: u64,
    /// The strapping the last reboot used.
    strap: Strap,
    /// The state a reboot goes back to. `None` unless
    /// [`Esp32S3Builder::reboot_on_reset`] asked for one.
    power_on: Option<Snapshot>,
}

impl Machine {
    // ---- construction ----------------------------------------------------

    /// Place the application's `PT_LOAD`s and seed the boot state a
    /// bootloader would have left: [`crate::loader`] is the documentation.
    fn direct_load(&mut self, frame: BootFrame) -> Result<(), BuildError> {
        let Some(app) = self.app.as_ref() else {
            return Err(BuildError::App("a direct load needs an --elf".into()));
        };
        // Cloned because the loader borrows the bus mutably and the app image
        // is borrowed from `self`. Placed once, at build.
        let app = app.clone();
        self.app_segments = loader::load_app(&mut self.bus, &app)?;
        self.seed_boot_state(app.entry, frame)?;
        Ok(())
    }

    /// Seed core 0's entry, `PS` and boot frame.
    ///
    /// Separated from [`direct_load`](Self::direct_load) so a test can seed a
    /// hart at a synthetic entry point without an application image — which
    /// is what `tests/boot.rs` uses to prove a seeded hart survives its first
    /// window exception.
    pub fn seed_boot_state(&mut self, entry: u32, frame: BootFrame) -> Result<(), BuildError> {
        self.harts[0].set_pc(entry);
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

    // ---- what is in here -------------------------------------------------

    pub fn time_grade(&self) -> TimeGrade {
        self.time_grade
    }

    pub fn boot_frame(&self) -> Option<BootFrame> {
        self.boot_frame
    }

    pub fn cpenable_reset(&self) -> u32 {
        self.cpenable_reset
    }

    /// Whether a word this hart cannot decode stops the run
    /// ([`Esp32S3Builder::strict_unsupported`]).
    ///
    /// Reported on every run, and it matters more on this chip than on the
    /// other two: `tests/isa_gaps.rs` found `salt`/`saltu` with no arm in
    /// `lp-xt-inst` (all six sites in this image are literal-pool phantoms,
    /// so nothing executes one), and this flag is what turns a future one
    /// into a named stop instead of a silent illegal-instruction handler.
    pub fn strict_unsupported(&self) -> bool {
        self.strict_unsupported
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
        DataImage {
            sections: self.rom_data.len(),
            bytes: self.rom_data.iter().map(|s| s.len).sum(),
        }
    }

    pub fn app_segments(&self) -> &[PlacedAppSegment] {
        &self.app_segments
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

    /// Everything the console said. On this chip the console **is** the
    /// link, so this is the `usb-sj` stream — see the field's docs.
    pub fn console(&self) -> &ByteLog {
        &self.usb_sj_log
    }

    /// What a host received from the IN endpoint.
    pub fn usb_sj(&self) -> Vec<u8> {
        self.usb_sj_log.bytes()
    }

    /// Bytes the guest handed to the IN endpoint that no host ever took.
    pub fn usb_sj_tried(&self) -> Vec<u8> {
        self.usb_sj_tried_log.bytes()
    }

    /// The host's state at power-on (the live one is the block's).
    pub fn usb_host(&self) -> UsbHost {
        self.usb_host
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

    /// How many reset requests this run performed
    /// ([`Esp32S3Builder::reboot_on_reset`]).
    pub fn reboots(&self) -> u64 {
        self.reboots
    }

    /// The strapping the chip was started with, or the one the last reboot
    /// used.
    pub fn strap(&self) -> Strap {
        self.strap
    }

    /// The USB block's live host state, `None` on a machine with no block.
    pub fn usb_host_now(&mut self) -> Option<UsbHost> {
        let index = self.usb_index?;
        self.bus
            .with_peripheral::<UsbSerialJtag, _>(index, |u, _| u.host())
    }

    /// Drive the link from the host's side, outside a run — what a test uses
    /// instead of a socket or a script.
    pub fn apply_control_now(&mut self, command: &ControlCommand) -> ControlReply {
        let at = self.cycles();
        self.apply_control(command, at)
    }

    /// `SYSTEM.core_1_control_0` as the guest left it, and the handle P04's
    /// view will share.
    pub fn core_1_control(&self) -> CoreOneControl {
        *self.core_1_control.lock().expect("core_1_control poisoned")
    }

    /// The handle itself, shared with the `SYSTEM` view.
    pub fn core_1_control_handle(&self) -> CoreOneHandle {
        Arc::clone(&self.core_1_control)
    }

    /// RTC_CNTL's stall key, as its view last published it.
    pub fn stall_key(&self) -> &StallKey {
        &self.stall_key
    }

    /// What `RTC_CNTL.reset_state` was seeded with.
    pub fn reset_cause(&self) -> ResetCause {
        self.reset_cause
    }

    /// The identity the eFuse block answers.
    pub fn efuse(&self) -> EfuseIdentity {
        self.efuse
    }

    /// Is `core` held?
    ///
    /// The OR of the three inputs named in the module docs, in the order
    /// they are consulted: the machine's own flag, `SYSTEM.core_1_control_0`,
    /// and RTC_CNTL's `0x86` stall key.
    pub fn core_stalled(&self, core: usize) -> bool {
        if self.stalled.get(core).copied().unwrap_or(true) {
            return true;
        }
        if core == 1 && self.core_1_control().holds_core1() {
            return true;
        }
        self.stall_key.stalled(core)
    }

    /// Which inputs hold `core` right now, by name, in the order
    /// [`core_stalled`](Self::core_stalled) consults them. Empty for a
    /// running core.
    pub fn stall_inputs(&self, core: usize) -> Vec<&'static str> {
        let mut held = Vec::new();
        if self.stalled.get(core).copied().unwrap_or(true) {
            held.push("machine (M6 releases no core; there is no S3 measurement of where a released core starts)");
        }
        if core == 1 {
            let c = self.core_1_control();
            if c.reseting {
                held.push("SYSTEM.core_1_control_0.reseting");
            }
            if c.runstall {
                held.push("SYSTEM.core_1_control_0.runstall");
            }
            if !c.clkgate_en {
                held.push("SYSTEM.core_1_control_0.!clkgate_en");
            }
        }
        if self.stall_key.stalled(core) {
            held.push("RTC_CNTL sw_stall_*_c1:c0 == 0x86");
        }
        held
    }

    /// One line per core plus the quantum, for `--probe` and the run report.
    /// Slot 1 is never silently absent, and a held core says *which* input is
    /// holding it.
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

    pub fn core_quantum(&self) -> u64 {
        self.core_quantum
    }

    /// How many windows `core` has ended in `waiti`.
    pub fn wfi_ends(&self, core: usize) -> u64 {
        self.wfi_ends.get(core).copied().unwrap_or(0)
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

    pub fn hook_calls(&self) -> u64 {
        self.hook_calls
    }

    pub fn idle_skips(&self) -> u64 {
        self.idle_skips
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The machine's seeded PRNG (SplitMix64). The only source of "random" in
    /// the machine, so a run with the same seed is the same run.
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
    /// While slot 1 has never been released this is exactly
    /// `harts[0].cycle_count()`, which is asserted in debug builds so a
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

    /// [`Machine::clock`], under the name the other two machines' callers use.
    pub fn cycles(&self) -> Cycles {
        self.clock()
    }

    /// Emulated microseconds: `cycles / 240` ([`memmap::CPU_HZ`]).
    pub fn micros(&self) -> u64 {
        self.cycles() / memmap::CYCLES_PER_US
    }

    /// Instructions retired by **both** cores. The per-core counts are
    /// [`Machine::core_instructions`].
    pub fn instructions(&self) -> u64 {
        self.harts.iter().map(XtHart::instruction_count).sum()
    }

    // ---- symbols and memory ----------------------------------------------

    /// What is at `address`: the app's symbol if the app claims it, else the
    /// ROM's. Both are asked because on this chip a fault inside `memcpy` is
    /// in the ROM and a fault inside `esp_hal::init` is in the app, and the
    /// ROM is where most of the instructions are.
    ///
    /// A nearest-match is prefixed with `~`, and the tilde is load-bearing:
    /// the S3 ROM's linker scripts `PROVIDE` several names at one address
    /// (`m6/notes.md` §2.5 — `0x4000_1C68` reads as
    /// `r_llc_rem_phy_upd_proc_continue_hook` when the real target is
    /// `esp_rom_md5_update`), so the nearest preceding label is often *not*
    /// the enclosing function. The tilde says "nearest label", not "this
    /// function".
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
    /// with `name` after a `::`. An ambiguous short name is refused rather
    /// than guessed.
    pub fn resolve_symbol(&self, name: &str) -> Option<u32> {
        if let Some(s) = self
            .app
            .as_ref()
            .and_then(|a| a.symbol(name))
            .or_else(|| self.rom.symbol(name))
        {
            return Some(s.address);
        }
        let suffix = format!("::{name}");
        let mut hit = None;
        for image in self.app.iter().chain(std::iter::once(&self.rom)) {
            for s in image.symbols() {
                if s.name.ends_with(&suffix) {
                    if hit.is_some_and(|a| a != s.address) {
                        log::warn!(
                            "machine: `{name}` is ambiguous — more than one symbol ends with \
                             `{suffix}`; name it in full"
                        );
                        return None;
                    }
                    hit = Some(s.address);
                }
            }
            if hit.is_some() {
                break;
            }
        }
        hit
    }

    /// Read a word of guest memory from the host side.
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

    /// The word at a symbol, for `--probe <cycle>:<symbol>`.
    pub fn peek_symbol(&mut self, name: &str) -> Option<(u32, u32)> {
        let address = self.resolve_symbol(name)?;
        self.peek_word(address).map(|v| (address, v))
    }

    /// Stop the run the first time `address` is reached.
    pub fn break_at_address(&mut self, address: u32) -> Result<u32, RomError> {
        self.hooks
            .install_at(&mut self.bus, address, "break-at", |_| HookResult::Stop)
    }

    /// Stop the run the first time `symbol` is reached.
    pub fn break_at(&mut self, symbol: &str) -> Result<u32, RomError> {
        let address = self
            .resolve_symbol(symbol)
            .ok_or_else(|| RomError::NoSuchSymbol(symbol.to_string()))?;
        // The name is leaked so the table can hold a `&'static str`: a
        // `--break-at` is set up once per run and there are at most a handful.
        let leaked: &'static str = Box::leak(symbol.to_string().into_boxed_str());
        self.hooks
            .install_at(&mut self.bus, address, leaked, |_| HookResult::Stop)
    }

    // ---- the run loop ----------------------------------------------------

    /// Run until `stop` says otherwise. See the module docs for the loop and
    /// the bring-up rule.
    pub fn run_until(&mut self, stop: &StopCondition) -> Outcome {
        let started = Instant::now();
        let mut stop_cycle = stop.stop_cycle.unwrap_or(u64::MAX);
        let mut probes = stop.probes.clone();
        probes.sort_by(|a, b| a.0.cmp(&b.0));
        let mut next_probe = 0usize;
        // `--exit-on`'s scan anchor, so a long run does not rescan the whole
        // console at every slice boundary.
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
            // The host's side bounds the slice too, or a guest sitting in
            // `waiti` would jump clean over a whole script.
            if let Some(at) = self.next_host_service(now) {
                deadline = deadline.min(at.max(now + 1));
            }
            if self.bus.strict() {
                deadline = deadline.min(now + STRICT_SLICE_CYCLES);
            }
            // The window. The quantum is the *upper* bound on one core's
            // window; every bound above still applies.
            let window = deadline.saturating_sub(now).max(1).min(self.core_quantum);

            // One window per core that is neither held nor parked, core 0
            // then core 1, each opening at `now`. A held core's counter does
            // not move.
            for core in 0..CORES {
                if self.core_stalled(core) || self.harts[core].is_waiti() {
                    continue;
                }
                // A core that fell behind the clock is brought up to it
                // before it runs again, so its window is the same span of
                // guest time as the other core's.
                self.harts[core].advance_to_cycle(now);

                self.bus.set_time(now);
                self.bus.set_hart(core);
                let end = self.harts[core].run_slice(&mut self.bus, window);

                match end {
                    SliceEnd::BudgetExhausted | SliceEnd::BusYield => {}
                    SliceEnd::Wfi => {
                        self.wfi_ends[core] += 1;
                        // ⚠️ **Resample the matrix before leaving the core
                        // parked.** The hart polls at the `waiti` itself, but
                        // against the external mask the machine last handed
                        // it — at the *previous* boundary. A line raised
                        // during this window is invisible to that poll, and
                        // parking on it would leave the guest owed an
                        // interrupt forever. The classic found this as a
                        // wedge that cost a milestone its heartbeat (its
                        // module docs carry the cycle numbers); the S3 has no
                        // interrupt source until P04 and the resample is here
                        // from the first commit so it cannot be forgotten
                        // when one arrives.
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
                        // A strict refusal that turned into a double fault is
                        // still a strict refusal, and naming the access is
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
            // The host's side, at a slice boundary and never inside one: a
            // scripted command due by now, then — on the poll cadence — the
            // byte socket's client edge and the control channel's lines.
            self.service_host(at);
            self.feed_cores(at);

            // The deterministic idle skip: only when **every** running core is
            // parked in `waiti` and none un-parked on the feed above. Nothing
            // can then happen before the earliest of: the next scheduled
            // event, and **each running hart's own `CCOMPARE` match** (DD45
            // R9 — the S3's tick is SYSTIMER, so this changes nothing
            // observable today; it is the difference between a loop that is
            // correct and one that happens to work).
            //
            // A machine on which *no* core is running — both held — moves its
            // harts' counters the same way rather than spinning: on silicon a
            // stalled core's `CCOUNT` keeps counting, and only the deadline
            // can end that state here.
            if self.no_running_core_is_awake() {
                let event = self.bus.sched.next_deadline();
                let bytes = self.bus.host.next_ready();
                // …and anything the host's side is due to do, for the same
                // reason the slice is bounded by it.
                let host = self.next_host_service(at);
                let timers = (0..CORES)
                    .filter(|c| !self.core_stalled(*c))
                    .filter_map(|c| self.harts[c].next_timer_cycle());
                let wake = [event, bytes, host]
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
                let at = self.clock();
                self.bus.run_due_events(at);
                self.feed_cores(at);
            }

            // ⚠️ The code-write drain is per hart. A block cache on core 1
            // that never heard about core 0's write would execute stale code
            // — which on this chip is exactly what the product path does on
            // purpose: a shader written through the D-bus view and executed
            // through the I-bus alias. `canonical()` makes both halves one
            // address before the span is recorded (DD81), so the range below
            // is the canonical one and the invalidation covers both doors.
            if self.bus.code_writes_pending() {
                for (lo, hi) in self.bus.take_code_writes() {
                    for hart in &mut self.harts {
                        hart.invalidate_block_range(lo, hi);
                    }
                }
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
            // A peripheral asked for a reset — the RWDT's stage 0 expired, or
            // the link's control channel asked for one. Reported by default;
            // `--reboot-on-reset` performs it instead.
            if let Some(MachineRequest::Reset { source, at, strap }) = self.bus.take_request() {
                // ⚠️ `stop_cycle` is an **absolute** guest cycle and a reboot
                // moves what zero means. Read what is LEFT of the budget while
                // the old origin still stands and rebase it onto the new one,
                // or the loop `continue`s against a bound the board's whole
                // prior lifetime away and replays it inside one slice — which
                // is `docs/defects/2026-09-11-a-reset-replays-the-boards-lifetime.md`,
                // measured on the C6 at exactly `old + budget`.
                let remaining = stop
                    .stop_cycle
                    .map(|_| stop_cycle.saturating_sub(self.cycles()));
                if self.reboot_on_reset && self.reboot(strap) {
                    log::info!("machine: {source} at cycle {at} — rebooting into strap {strap}");
                    if let Some(remaining) = remaining {
                        stop_cycle = self.cycles().saturating_add(remaining);
                    }
                    matched = 0;
                    continue;
                }
                log::info!("machine: {source} at cycle {at}; the reset is reported, not performed");
                return Outcome::Reset {
                    cycle: at,
                    source,
                    strap,
                };
            }
            if let Some(needle) = &stop.exit_on
                && self
                    .usb_sj_log
                    .with_bytes(|text| Self::line_complete(text, needle.as_bytes(), &mut matched))
            {
                return Outcome::ExitMatched {
                    cycle: self.cycles(),
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

    // ---- between windows -------------------------------------------------

    /// Bring every running core up to `at` and hand each the matrix as **its
    /// own** input (`set_hart` first — the feed is per hart, PD6), then let it
    /// take what it can. A core parked in `waiti` un-parks here and nowhere
    /// else.
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

    fn no_running_core_is_awake(&self) -> bool {
        (0..CORES).all(|c| self.core_stalled(c) || self.harts[c].is_waiti())
    }

    /// A `break` the hart could not answer: ask the hook table.
    fn serve_breakpoint(&mut self, core: usize, pc: u32) -> bool {
        let Some(hook) = self.hooks.get(pc) else {
            return false;
        };
        self.hook_calls += 1;
        match (hook.call)(self) {
            HookResult::Ret => {
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

    // ---- the host's side (M6 P05) ----------------------------------------

    /// The next cycle at which [`service_host`](Self::service_host) has
    /// something to do, or `None` when nothing outside can reach this run.
    ///
    /// A scripted command's own cycle, or the socket poll cadence. It bounds
    /// the slice and the idle skip, which is what stops a guest sitting in
    /// `waiti` from jumping over the whole script.
    fn next_host_service(&self, _now: Cycles) -> Option<Cycles> {
        let scripted = self.script.front().map(|(at, _)| *at);
        let polled =
            (self.control.is_some() || self.usb_sj_tcp.is_some()).then_some(self.next_host_poll);
        [scripted, polled].into_iter().flatten().min()
    }

    /// Apply everything the host has asked for by cycle `now`.
    ///
    /// Order is deliberate: the script first (its times are the contract),
    /// then the byte socket's coupling edge, then the control channel's
    /// lines. Called only at a slice boundary, so a command never lands
    /// between two instructions of one slice, and the reply names the cycle
    /// it was drained at.
    fn service_host(&mut self, now: Cycles) {
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
    ///
    /// ⚠️ **`reset` and `download-mode` are unconditional on this chip.**
    /// The C6 refuses them when the guest has set `chip_rst` bit 2; the S3
    /// has no `chip_rst`, so there is no such reply here and the difference
    /// is the README's.
    fn apply_control(&mut self, command: &ControlCommand, now: Cycles) -> ControlReply {
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
                // The lines reach no guest-visible register on this chip —
                // see the module docs — but the RTS falling edge still
                // decodes a dance into a reset or a download-mode request,
                // which is what esptool's sequences are made of.
                ControlCommand::Signals { dtr, rts } => {
                    u.set_signals(*dtr, *rts, cx);
                    Ok(None)
                }
                ControlCommand::Reset | ControlCommand::DownloadMode => {
                    let download = matches!(command, ControlCommand::DownloadMode);
                    let performed = if download {
                        u.download_mode(cx)
                    } else {
                        u.reset(cx)
                    };
                    // `chip_reset_disabled` is always false on this part, so
                    // this can only be `true` — and asserting it here is
                    // cheaper than a reader wondering whether the C6's
                    // refusal path was dropped or forgotten.
                    debug_assert!(performed, "the S3 has no chip_rst to refuse with");
                    Ok(None)
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

    /// Reboot into `strap`, as a chip does when something asserts its reset.
    ///
    /// ⚠️ **This machine has no ROM-up boot** (P06), so a reboot is the
    /// power-on state put back: the machine returns to the snapshot taken
    /// after the direct load, with the clock, the peripherals, the schedule
    /// and both harts where they were at cycle zero. `strap` is recorded and
    /// reported; **nothing reads it**, because there is no boot chain to
    /// choose with it yet — saying so is cheaper than a reader assuming a
    /// download console appears.
    ///
    /// The consoles keep their bytes: a restore would put back the empty
    /// logs the machine was built with, and a boot log that lost everything
    /// before the reset would be a worse record than one with both boots in
    /// it.
    pub fn reboot(&mut self, strap: Strap) -> bool {
        let Some(power_on) = self.power_on.clone() else {
            return false;
        };
        let (usb_sj, tried) = (self.usb_sj_log.bytes(), self.usb_sj_tried_log.bytes());
        self.restore(&power_on);
        self.usb_sj_log.replace(&usb_sj);
        self.usb_sj_tried_log.replace(&tried);

        let usb_host = self.usb_host;
        self.strap = strap;
        // ⚠️ The host-poll deadline is an ABSOLUTE guest cycle and it lives
        // outside the peripheral set, so a reboot that puts the clock back to
        // zero would leave it in the guest's future for as long as the board
        // had been up — the C6 measured a board 120 s old going silent for
        // 3.5 minutes after a flasher's reset. A reset is the moment a host
        // needs the chip most, so the deadline goes back with the clock.
        self.next_host_poll = 0;
        // Same class, one layer up: the port's open/closed state is the
        // block's and the byte client's connectedness is not, and the
        // coupling only fires on an EDGE. Re-derive it from BOTH sides, so
        // the next poll sees an edge exactly when one is needed and none
        // when it is not.
        let client = self
            .usb_sj_tcp
            .as_ref()
            .is_some_and(|tcp| tcp.client_connected());
        self.usb_client_connected = client && usb_host.draining();
        self.reboots += 1;
        true
    }

    /// Is `needle` in `text` at or after `from`, with a newline behind it?
    ///
    /// The search is over **bytes**, not `str`: a console is a byte stream
    /// and anchoring an index into a lossily-decoded `String` lands inside a
    /// multi-byte character and panics. (It did, on the C6, the first time
    /// `--exit-on` met an em dash.) Console output is ASCII and UTF-8 is
    /// self-synchronising, so a byte search finds exactly what a text search
    /// would.
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
            // Matched, but the line is still arriving: hold the anchor here
            // so the next byte re-checks this same match rather than the tail.
            Some(at) => {
                *from = at;
                false
            }
            // Keep the search anchored so a long run does not rescan the
            // whole console every slice; back off by the needle so a match
            // split across two slices is still found.
            None => {
                *from = text.len().saturating_sub(needle.len());
                false
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
            core_1_control: self.core_1_control(),
            regions: self.bus.save_regions(),
            periph: self.bus.save_peripherals(),
            matrix: self.bus.matrix().save_state(),
            scalars: self.bus.save_scalars(),
            sched: self.bus.sched.save(),
            rng: self.rng,
            hook_calls: self.hook_calls,
            idle_skips: self.idle_skips,
            wfi_ends: self.wfi_ends,
            core_quantum: self.core_quantum,
            console: self.usb_sj_log.bytes(),
        }
    }

    /// Put the machine back exactly where [`snapshot`](Self::snapshot) took
    /// it.
    pub fn restore(&mut self, snap: &Snapshot) {
        self.harts = snap.harts.clone();
        self.stalled = snap.stalled;
        *self.core_1_control.lock().expect("core_1_control poisoned") = snap.core_1_control;
        self.bus.restore_regions(&snap.regions);
        self.bus.restore_peripherals(&snap.periph);
        self.bus.matrix_mut().load_state(&snap.matrix);
        self.bus.restore_scalars(&snap.scalars);
        self.bus.sched.restore(&snap.sched);
        self.rearm_watchpoints();
        self.rng = snap.rng;
        self.hook_calls = snap.hook_calls;
        self.idle_skips = snap.idle_skips;
        self.wfi_ends = snap.wfi_ends;
        self.usb_sj_log.replace(&snap.console);
        // The quantum is something a run's future depends on, so a restored
        // run takes the snapshot's — and says so if that differs from what
        // this machine was built with.
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
    /// A DBREAK watchpoint lives on the **bus** and is written only by the
    /// hart's `wsr` to `DBREAKA`/`DBREAKC`. A restore replaces the hart
    /// wholesale, so the bus would keep whatever the *previous* hart had
    /// armed while the restored hart's `DBREAK` pair says something else —
    /// the bug the classic's P6 found by being the first code that rebooted.
    ///
    /// ⚠️ **This re-arms from the power-on state, not from the restored
    /// hart.** Every slot is disarmed through the ordinary
    /// [`Bus::set_watchpoint`] path, so the armed bitmask is recomputed with
    /// it. That is exactly right for every snapshot this machine takes;
    /// reading the pair back off the hart would need a public accessor for
    /// its `BreakUnit`, which `lp-xt-emu` does not publish — **M1 owns that
    /// crate and M6 reads it**, so the accessor is an M1 item and this is the
    /// honest half of the fix, not the whole one.
    fn rearm_watchpoints(&mut self) {
        for slot in 0..crate::bus_setup::WATCHPOINT_SLOTS {
            Bus::set_watchpoint(&mut self.bus, slot, None);
        }
    }
}

impl fmt::Debug for Machine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Machine")
            .field("time_grade", &self.time_grade)
            .field("cores", &CORES)
            .field("stalled", &self.stalled)
            .field("core_1_control", &self.core_1_control())
            .field("core_quantum", &self.core_quantum)
            .field("cycle", &self.cycles())
            .field("pc", &format_args!("{:#010x}", self.harts[0].pc()))
            .field("pc1", &format_args!("{:#010x}", self.harts[1].pc()))
            .finish()
    }
}
