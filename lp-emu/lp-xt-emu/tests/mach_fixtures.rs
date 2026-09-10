//! The privileged hart's **evidence**: real bare-metal Xtensa images, built by
//! the esp toolchain in `lp-xt/fixtures/mach`, running **xtensa-lx-rt's own
//! vector table** on a RAM-only bus.
//!
//! `src/mach/tests.rs` is the hart's conformance *claim*, and it is written by
//! the same reasoning that wrote the hart — a wrong-but-plausible exception
//! model passes every test that reasoning writes. This file is the answer to
//! that: the window overflow/underflow handlers, the user-exception vector, the
//! level-3 and debug vectors here are the ones that ship in ESP32 firmware, and
//! the fixtures' own logic is compiled Rust, not hand-encoded instructions.
//!
//! # The bus is `Memory` and the interrupt line is a script
//!
//! No SoC, no peripherals, no memory map beyond one SRAM1 region. A fixture
//! that needs an interrupt either raises it itself (`wsr.intset`, which is what
//! a software interrupt *is*, and which pins the preemption point to the hart's
//! poll point (b)) or gets it from [`Script`] — a list of `(guest cycle, mask)`
//! pairs the runner applies at slice boundaries through
//! `XtHart::set_external_mask`. Guest cycles, never wall time (plan PD9).
//!
//! # Goldens
//!
//! Each fixture's run produces a **symbolic** transcript: architectural values
//! (`EXCCAUSE`, `LCOUNT`, a loaded word), the vector *offsets* entered, and,
//! where an address is the point, the *symbol* it falls in — never the address
//! itself. The linker's addresses are a function of the toolchain version; the
//! symbol is a function of the fixture. A golden full of raw PCs would have to
//! be re-blessed on every esp-toolchain bump, and a golden that gets re-blessed
//! routinely stops being evidence.
//!
//! **Never edit a golden by hand.** A mismatch is a regression or a deliberate
//! re-capture; a re-capture is `LP_XT_MACH_BLESS=1 cargo test -p lp-xt-emu
//! --test mach_fixtures` as its own commit, with its reason in the message.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use lp_emu_core::{Bus, MemoryAccessKind, MemoryError, Watchpoint};
use lp_xt_elf::XtensaElf;
use lp_xt_emu::mach::interrupt::{IntKind, IntLine};
use lp_xt_emu::mach::sr::PS_BOOT;
use lp_xt_emu::mach::trap::{NUM_INTERRUPTS, VECTOR_TABLE_SIZE, is_vector_entry};
use lp_xt_emu::mach::{CoreConfig, SliceEnd, XtHart};
use lp_xt_emu::memory::Memory;
use lp_xt_emu::trace::{TraceEvent, Tracer};

// ---------------------------------------------------------------------------
// The memory map — one SRAM1 region, matching lp-xt/fixtures/mach/memory.x
// ---------------------------------------------------------------------------

/// D-bus base of the single modeled region.
const SRAM1_BASE: u32 = 0x3FC8_8000;
/// 160 KiB: vectors + text + rodata + rwtext + data + stack.
const SRAM1_LEN: usize = 0x2_8000;
/// `_init_start` in `memory.x`. `Reset` writes it to VECBASE itself; the hart
/// is given it as `reset_vecbase` so a window exception raised *before* that
/// `wsr` still lands in the table.
const VECBASE: u32 = 0x4037_8000;

/// Cycles per slice. Small enough that a scripted interrupt lands at a
/// predictable point, large enough that a fixture is not a thousand slices.
const SLICE: u64 = 512;
/// Total cycle ceiling. A fixture that exceeds it has hung — an exception loop,
/// or a window handler that never returns — and the runner says so by name
/// rather than by the harness timing out.
const CYCLE_LIMIT: u64 = 8_000_000;

/// The `PRID` the fixtures see: the classic ESP32's PRO_CPU number. A chip
/// number, not a hart index (`CoreConfig::prid`).
const PRID_PRO_CPU: u32 = 0xCDCD;

// Vector offsets the assertions name, from `mach::trap`'s table.
const VECOFS_OF4: u32 = 0x000;
const VECOFS_LEVEL3: u32 = 0x1C0;
const VECOFS_DEBUG: u32 = 0x280;
const VECOFS_KERNEL: u32 = 0x300;
const VECOFS_USER: u32 = 0x340;
const VECOFS_DOUBLE: u32 = 0x3C0;

// The CPU interrupt lines, in the classic's shape (esp-hal's `CpuInterrupt`
// table). Core configuration, so it arrives through `CoreConfig` — the same
// table `src/mach/tests.rs` configures, so a fixture and a unit test mean the
// same thing by "line 7".
const IRQ_EXTERNAL_L1: u8 = 0;
const IRQ_SOFTWARE_L1: u8 = 7;
const IRQ_SOFTWARE_L3: u8 = 29;

fn core_config(reset_pc: u32) -> CoreConfig {
    let mut interrupts = [IntLine::UNUSED; NUM_INTERRUPTS];
    interrupts[usize::from(IRQ_EXTERNAL_L1)] = IntLine::new(1, IntKind::Level);
    interrupts[6] = IntLine::new(1, IntKind::Timer(0));
    interrupts[usize::from(IRQ_SOFTWARE_L1)] = IntLine::new(1, IntKind::Software);
    interrupts[15] = IntLine::new(3, IntKind::Timer(1));
    interrupts[19] = IntLine::new(2, IntKind::Level);
    interrupts[22] = IntLine::new(3, IntKind::Edge);
    interrupts[23] = IntLine::new(3, IntKind::Level);
    interrupts[usize::from(IRQ_SOFTWARE_L3)] = IntLine::new(3, IntKind::Software);
    interrupts[31] = IntLine::new(5, IntKind::Level);
    CoreConfig {
        reset_pc,
        reset_vecbase: VECBASE,
        prid: PRID_PRO_CPU,
        interrupts,
    }
}

// ---------------------------------------------------------------------------
// The bus: `Memory`, plus the two DBREAK slots it does not implement
// ---------------------------------------------------------------------------

/// `lp-xt-emu`'s own [`Memory`] with hardware watchpoints bolted on.
///
/// **Finding, reported with this phase:** `Memory`'s `Bus` impl does not
/// override [`Bus::set_watchpoint`], so it inherits the trait default — which
/// ignores it ("user-mode `Memory`, which has no privileged state to trap
/// from"). The hart mirrors its `DBREAK` slots onto the bus faithfully; the
/// bus then drops them, and **`DBREAK` cannot fire against a bare `Memory`**.
/// Fixture (f) would silently pass through its own guard.
///
/// So the watchpoint half lives here, in the test, rather than in
/// `src/memory.rs` — this phase does not own `lp-emu/lp-xt-emu/src/**` (that is
/// P5's, in parallel). The semantics are the RM's and match what
/// `src/mach/tests.rs`'s own bus double does: the error is returned **instead
/// of** performing the access, which is the whole point of a stack guard.
struct GuardedMemory {
    mem: Memory,
    watchpoints: [Option<Watchpoint>; 2],
}

impl GuardedMemory {
    fn new(mem: Memory) -> Self {
        Self {
            mem,
            watchpoints: [None; 2],
        }
    }

    /// The `[base, len)` a watchpoint covers. NAPOT encodes a naturally
    /// aligned `2^(k+1)` block as `k` low one-bits under a zero.
    fn region(wp: &Watchpoint) -> (u32, u32) {
        if !wp.napot {
            return (wp.address, 1);
        }
        let ones = wp.address.trailing_ones().min(30);
        let len = 1u32 << (ones + 1);
        (wp.address & !(len - 1), len)
    }

    fn check(&self, address: u32, size: u32, kind: MemoryAccessKind) -> Result<(), MemoryError> {
        for (slot, wp) in self.watchpoints.iter().enumerate() {
            let Some(wp) = wp else { continue };
            let wanted = match kind {
                MemoryAccessKind::Read => wp.on_load,
                MemoryAccessKind::Write => wp.on_store,
                MemoryAccessKind::InstructionFetch => wp.on_execute,
            };
            if !wanted {
                continue;
            }
            let (base, len) = Self::region(wp);
            if address < base.wrapping_add(len) && base < address.wrapping_add(size) {
                return Err(MemoryError::Watchpoint {
                    address,
                    kind,
                    slot: slot as u8,
                });
            }
        }
        Ok(())
    }
}

impl Bus for GuardedMemory {
    fn fetch_instruction(&mut self, address: u32) -> Result<u32, MemoryError> {
        self.mem.fetch_instruction(address)
    }

    /// Forwarded, **not** inherited: the trait default assembles the answer
    /// from `read_u8`, which is a data read and would run every fetch past the
    /// load watchpoints.
    fn fetch_bytes(&mut self, pc: u32, out: &mut [u8; 3]) -> Result<usize, MemoryError> {
        self.mem.fetch_bytes(pc, out)
    }

    fn read_word(&mut self, address: u32) -> Result<i32, MemoryError> {
        self.check(address, 4, MemoryAccessKind::Read)?;
        self.mem.read_word(address)
    }

    fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError> {
        self.check(address, 2, MemoryAccessKind::Read)?;
        self.mem.read_halfword(address)
    }

    fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError> {
        self.check(address, 1, MemoryAccessKind::Read)?;
        self.mem.read_byte(address)
    }

    fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError> {
        self.check(address, 1, MemoryAccessKind::Read)?;
        Bus::read_u8(&mut self.mem, address)
    }

    fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError> {
        self.check(address, 4, MemoryAccessKind::Write)?;
        self.mem.write_word(address, value)
    }

    fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError> {
        self.check(address, 2, MemoryAccessKind::Write)?;
        self.mem.write_halfword(address, value)
    }

    fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError> {
        self.check(address, 1, MemoryAccessKind::Write)?;
        self.mem.write_byte(address, value)
    }

    fn set_watchpoint(&mut self, slot: usize, wp: Option<Watchpoint>) {
        if let Some(entry) = self.watchpoints.get_mut(slot) {
            *entry = wp;
        }
    }
}

// ---------------------------------------------------------------------------
// The tracer: which vectors did this run enter, and how often?
// ---------------------------------------------------------------------------

/// Counts entries to each of the 16 vector table entries.
///
/// This is what turns "the handler ran" into "the handler was reached through
/// `_UserExceptionVector`". A fixture can report what its handler saw, but it
/// cannot report which vector delivered it there — the host can, because the
/// vector entry addresses are `VECBASE + n * 0x40` and nothing else in the
/// image executes at them.
#[derive(Default)]
struct VectorTracer {
    counts: BTreeMap<u32, usize>,
    /// Vector offsets in the order they were first entered.
    order: Vec<u32>,
    /// The last [`TAIL`] instructions, kept only under `LP_XT_MACH_TAIL`.
    ///
    /// A fixture that hangs hangs inside a handler, and the one question worth
    /// asking is "which instruction, with which operands". This is that
    /// question's answer, off by default because keeping it costs a `String`
    /// per instruction.
    tail: Option<std::collections::VecDeque<String>>,
    /// `LP_XT_MACH_TAIL=0x080`: start recording at the first entry to that
    /// vector offset and keep the **first** [`TAIL`] instructions after it,
    /// rather than the last ones. A hang's ring buffer shows the loop; its
    /// first turn shows the cause.
    arm_at: Option<u32>,
    armed: bool,
}

/// How many instructions `LP_XT_MACH_TAIL` keeps.
const TAIL: usize = 400;

impl Tracer for VectorTracer {
    fn event(&mut self, event: TraceEvent<'_>) {
        if let TraceEvent::Inst { pc, inst, .. } = event {
            if is_vector_entry(VECBASE, pc) {
                let offset = pc - VECBASE;
                let entry = self.counts.entry(offset).or_default();
                if *entry == 0 {
                    self.order.push(offset);
                }
                *entry += 1;
                if self.arm_at == Some(offset) {
                    self.armed = true;
                }
            }
            if let Some(tail) = self.tail.as_mut() {
                match self.arm_at {
                    // First TAIL instructions after the armed vector.
                    Some(_) => {
                        if self.armed && tail.len() < TAIL {
                            tail.push_back(format!("{pc:#010x}  {inst:?}"));
                        }
                    }
                    // Last TAIL instructions of the run.
                    None => {
                        if tail.len() == TAIL {
                            tail.pop_front();
                        }
                        tail.push_back(format!("{pc:#010x}  {inst:?}"));
                    }
                }
            }
        }
    }
}

impl VectorTracer {
    fn count(&self, offset: u32) -> usize {
        self.counts.get(&offset).copied().unwrap_or(0)
    }

    fn entered(&self, offset: u32) -> bool {
        self.count(offset) > 0
    }

    /// The instruction tail, when `LP_XT_MACH_TAIL` asked for one.
    fn tail_dump(&self) -> String {
        match self.tail.as_ref() {
            None => "  (set LP_XT_MACH_TAIL=1 for the last instructions)".to_string(),
            Some(t) => format!("  last {} instructions:\n    {}", t.len(), {
                let v: Vec<&str> = t.iter().map(String::as_str).collect();
                v.join("\n    ")
            }),
        }
    }

    /// `0x000 x12, 0x340 x1` — first-entry order, so it reads as a story.
    fn summary(&self) -> String {
        if self.order.is_empty() {
            return "(none)".to_string();
        }
        self.order
            .iter()
            .map(|o| format!("{o:#05x} x{}", self.count(*o)))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// ---------------------------------------------------------------------------
// Finding the ELFs — and refusing to skip where it matters
// ---------------------------------------------------------------------------

/// The seven fixture images this phase owns. `lp-xt/fixtures/elf/` is
/// **gitignored**, so these are built, never committed — which is exactly how a
/// test suite comes to skip and report success. [`mach_fixtures_present`] and
/// the `Validate Xtensa (host)` CI job's assert step are the two guards.
const FIXTURES: &[&str] = &[
    "mach_backtrace",
    "mach_interrupts",
    "mach_loopnez",
    "mach_s32c1i",
    "mach_ctxswitch",
    "mach_dbreak",
    "mach_lserr",
];

fn elf_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../lp-xt/fixtures/elf/{name}.elf"))
}

/// Is a missing ELF a hard failure? CI sets `LP_XT_MACH_FIXTURES_REQUIRED=1`;
/// on a developer machine with no esp toolchain the fixtures legitimately do
/// not exist and the tests say so out loud instead.
fn fixtures_required() -> bool {
    std::env::var_os("LP_XT_MACH_FIXTURES_REQUIRED").is_some_and(|v| !v.is_empty())
}

/// Read a fixture's ELF, or `None` when it is absent and absence is allowed.
///
/// # Panics
/// When the ELF is absent and `LP_XT_MACH_FIXTURES_REQUIRED` is set.
fn read_elf(name: &str) -> Option<Vec<u8>> {
    let path = elf_path(name);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            assert!(
                !fixtures_required(),
                "LP_XT_MACH_FIXTURES_REQUIRED is set but {} is missing ({e}). \
                 Build it with lp-xt/fixtures/build.sh — a mach fixture that is \
                 not run proves nothing about the privileged hart.",
                path.display()
            );
            eprintln!(
                "SKIP {name}: {} not found — run lp-xt/fixtures/build.sh (esp toolchain) first",
                path.display()
            );
            None
        }
    }
}

// ---------------------------------------------------------------------------
// The scripted interrupt line
// ---------------------------------------------------------------------------

/// `(guest cycle, asserted-line mask)`, applied at the first slice boundary at
/// or after `cycle`. Deterministic in guest cycles: the same image, the same
/// script and the same slice size give the same run on every host.
#[derive(Clone, Default)]
struct Script(Vec<(u64, u32)>);

impl Script {
    fn empty() -> Self {
        Self(Vec::new())
    }

    fn at(mut self, cycle: u64, mask: u32) -> Self {
        self.0.push((cycle, mask));
        self
    }

    /// The mask in force at `cycle`: the last entry whose cycle has passed.
    fn mask_at(&self, cycle: u64) -> u32 {
        self.0
            .iter()
            .filter(|(c, _)| *c <= cycle)
            .next_back()
            .map_or(0, |(_, m)| *m)
    }
}

// ---------------------------------------------------------------------------
// A run
// ---------------------------------------------------------------------------

/// Result slot 0 after the fixture's `finish()` (`lp-xt/fixtures/mach/src/lib.rs`).
const MAGIC_DONE: u32 = 0x4D41_4348;
const MAGIC_PANIC: u32 = 0xDEAD_4348;
const RESULT_SLOTS: usize = 64;

struct Run {
    name: &'static str,
    /// `MACH_RESULT[0..RESULT_SLOTS]` read back out of the bus.
    slots: Vec<u32>,
    symbols: SymbolMap,
    vectors: VectorTracer,
    cycles: u64,
}

/// Function symbols sorted by address, so a PC can be attributed to one.
struct SymbolMap {
    by_addr: Vec<(u32, String)>,
}

impl SymbolMap {
    fn build(elf: &XtensaElf<'_>) -> Self {
        let mut map: BTreeMap<u32, String> = BTreeMap::new();
        for (name, addr) in elf.symbols() {
            // Assembler-local labels and lx-rt's literal-pool markers are not
            // functions and would swallow a PC that belongs to a real one.
            if name.starts_with(".L") || name.starts_with("sym_") {
                continue;
            }
            map.entry(addr).or_insert(name);
        }
        Self {
            by_addr: map.into_iter().collect(),
        }
    }

    /// The symbol whose address is the greatest one not above `pc`.
    fn resolve(&self, pc: u32) -> String {
        match self.by_addr.binary_search_by_key(&pc, |(a, _)| *a) {
            Ok(i) => self.by_addr[i].1.clone(),
            Err(0) => format!("<{pc:#010x}>"),
            Err(i) => self.by_addr[i - 1].1.clone(),
        }
    }
}

/// Load `name`, run it to `break`, and return everything the assertions need.
///
/// Returns `None` only when the ELF is absent and that is allowed
/// ([`fixtures_required`]).
fn run(name: &'static str, script: &Script) -> Option<Run> {
    let bytes = read_elf(name)?;
    let elf = XtensaElf::parse(&bytes).unwrap_or_else(|e| panic!("{name}: parse ELF: {e}"));

    // `.data` must be empty: lx-rt links it `AT > RODATA`, and this loader
    // writes PT_LOAD to p_vaddr, so an initialized static would be zeroed by
    // `Reset`'s own copy loop. `lp-xt/fixtures/build.sh` asserts this too; the
    // duplicate is deliberate, because only one of the two runs in a CI job
    // without the esp toolchain.
    let (dstart, dend) = (elf.symbol("_data_start"), elf.symbol("_data_end"));
    assert_eq!(
        dstart, dend,
        "{name}: .data is non-empty ({dstart:?}..{dend:?}); see lp-xt/fixtures/mach/memory.x"
    );

    let mut mem = Memory::new();
    mem.add_sram1(SRAM1_BASE, SRAM1_LEN);
    for seg in elf
        .segments()
        .unwrap_or_else(|e| panic!("{name}: segments: {e}"))
    {
        mem.try_load_bytes(seg.vaddr, seg.data).unwrap_or_else(|a| {
            panic!(
                "{name}: segment at {:#010x} is unmapped at {a:#010x}",
                seg.vaddr
            )
        });
        let tail = seg.memsz.saturating_sub(seg.data.len() as u32);
        mem.try_zero(seg.vaddr.wrapping_add(seg.data.len() as u32), tail)
            .unwrap_or_else(|a| panic!("{name}: bss tail unmapped at {a:#010x}"));
    }
    let mut bus = GuardedMemory::new(mem);

    let mut hart = XtHart::new(0, core_config(elf.entry()));
    // What the ROM and the second-stage bootloader leave behind for a direct
    // load: WOE = 1, EXCM = 0, CALLINC = 2 (the bootloader reaches the entry
    // point through a `callx8`). `Reset`'s own `entry a1, 0x10` needs WOE.
    hart.set_ps_raw(PS_BOOT);
    // ...and the stack pointer it left in `a1`.
    //
    // `XtHart::new` leaves `WindowStart = 1`, so **frame 0 is resident**, and
    // `PS_BOOT`'s `CALLINC = 2` makes `Reset` frame 2 — leaving frame 0 live
    // with `a1 = 0`. The first `SPILL_REGISTERS` (every exception runs one)
    // then takes `_WindowOverflow8` for frame 0 and does `l32e a0, a1, -12`
    // against `0xFFFFFFF4`; a load/store error inside a window handler is a
    // double exception, and the run never comes back. Seeding `PS` without
    // seeding the boot stack pointer is only half of "as the bootloader left
    // it" — a finding for M3, which owns the real machine's direct-load seed.
    // The value and the save area under it are `lp-xt/fixtures/mach/memory.x`'s
    // `_boot_frame_sp`; `mach::__pre_init` fills the rest in.
    let boot_frame_sp = elf
        .symbol("_boot_frame_sp")
        .unwrap_or_else(|| panic!("{name}: no _boot_frame_sp symbol; see mach/memory.x"));
    hart.cpu_mut().set_a(1, boot_frame_sp);
    // Bring-up honesty: an encoding this emulator does not implement stops the
    // run by name instead of vectoring into the guest's illegal-instruction
    // handler, where it would look like a fixture bug.
    hart.set_strict_unsupported(true);

    let tail_env = std::env::var("LP_XT_MACH_TAIL")
        .ok()
        .filter(|v| !v.is_empty());
    let mut vectors = VectorTracer {
        arm_at: tail_env
            .as_deref()
            .and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()),
        tail: tail_env.is_some().then(std::collections::VecDeque::new),
        ..VectorTracer::default()
    };
    let end = loop {
        hart.set_external_mask(script.mask_at(hart.cycle_count()));
        let end = hart.run_slice_traced(&mut bus, SLICE, &mut vectors);
        match end {
            SliceEnd::BudgetExhausted | SliceEnd::BusYield => {}
            SliceEnd::Wfi => {
                // Nothing on this bus generates work while the hart idles, so
                // only the script can wake it. Step one slice of guest time
                // and poll — the deterministic idle skip.
                let next = hart.cycle_count() + SLICE;
                hart.advance_to_cycle(next);
                hart.set_external_mask(script.mask_at(hart.cycle_count()));
                hart.poll_interrupts();
            }
            SliceEnd::Ebreak { .. } | SliceEnd::Fault(_) => break end,
        }
        // A hang is almost always an exception loop, and the four registers
        // below say which one in one line. Without them the report is "it
        // stopped somewhere", which costs a bisect every time.
        assert!(
            hart.cycle_count() < CYCLE_LIMIT,
            "{name}: ran past {CYCLE_LIMIT} cycles without reaching `break` — \
             the fixture is looping (an exception loop, or a window handler \
             that never returns).\n  pc       = {:#010x}\n  \
             EXCCAUSE = {}\n  EXCVADDR = {:#010x}\n  EPC1     = {:#010x}\n  \
             DEPC     = {:#010x}\n  PS       = {:#010x}\n  \
             WindowBase = {}  WindowStart = {:#06x}\n  \
             vectors entered: {}\n{}",
            hart.pc(),
            hart.sr().exccause,
            hart.sr().excvaddr,
            hart.sr().epc[1],
            hart.sr().depc,
            hart.ps(),
            hart.cpu().window_base,
            hart.cpu().window_start,
            vectors.summary(),
            vectors.tail_dump()
        );
    };

    let symbols = SymbolMap::build(&elf);
    if let SliceEnd::Fault(f) = end {
        panic!(
            "{name}: hart fault {f:?} at pc {:#010x} ({}); vectors entered: {}",
            hart.pc(),
            symbols.resolve(hart.pc()),
            vectors.summary()
        );
    }

    let base = elf
        .symbol("MACH_RESULT")
        .unwrap_or_else(|| panic!("{name}: no MACH_RESULT symbol"));
    let slots: Vec<u32> = (0..RESULT_SLOTS)
        .map(|i| {
            bus.mem
                .read_u32(base + 4 * i as u32)
                .unwrap_or_else(|e| panic!("{name}: read MACH_RESULT[{i}]: {e:?}"))
        })
        .collect();

    assert_ne!(
        slots[0],
        MAGIC_PANIC,
        "{name}: the fixture PANICKED (slot 0 = MAGIC_PANIC) — its own \
         assertions failed. Vectors entered: {}",
        vectors.summary()
    );
    assert_eq!(
        slots[0],
        MAGIC_DONE,
        "{name}: the fixture did not reach `finish()` (slot 0 = {:#010x}). It \
         stopped at {:#010x} ({}), so every other slot is whatever it was \
         before the run stopped. Vectors entered: {}",
        slots[0],
        hart.pc(),
        symbols.resolve(hart.pc()),
        vectors.summary()
    );

    // A double exception on this bus means a window handler faulted, which is
    // never a legitimate outcome for these fixtures and which every downstream
    // assertion would otherwise be read against.
    assert_eq!(
        vectors.count(VECOFS_DOUBLE),
        0,
        "{name}: the double-exception vector was entered {} time(s). Vectors: {}",
        vectors.count(VECOFS_DOUBLE),
        vectors.summary()
    );

    Some(Run {
        name,
        slots,
        symbols,
        vectors,
        cycles: hart.cycle_count(),
    })
}

impl Run {
    fn slot(&self, i: usize) -> u32 {
        self.slots[i]
    }

    /// Header every transcript starts with: the facts whose drift would
    /// explain every downstream difference at once.
    fn header(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "fixture: {}", self.name);
        let _ = writeln!(s, "vectors entered: {}", self.vectors.summary());
        s
    }
}

// ---------------------------------------------------------------------------
// Goldens
// ---------------------------------------------------------------------------

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/goldens/mach/{name}.txt"))
}

/// Compare a transcript against its committed golden, or re-bless it when
/// `LP_XT_MACH_BLESS=1`.
fn check_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("LP_XT_MACH_BLESS").is_some_and(|v| !v.is_empty()) {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        std::fs::write(&path, actual).expect("write golden");
        eprintln!("BLESSED {}", path.display());
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{name}: golden {} is missing ({e}). A fixture without a committed \
             trace is not a fixture; re-capture with LP_XT_MACH_BLESS=1 and \
             commit it with its reason.",
            path.display()
        )
    });
    assert_eq!(
        actual,
        expected,
        "\n{name}: trace differs from {}.\n--- actual ---\n{actual}\n--- expected ---\n{expected}\n\
         Never edit a golden toward the value you wanted: a mismatch is either a \
         regression in the hart or a deliberate re-capture.",
        path.display()
    );
}

/// Every fixture's transcript, built by the same function the per-fixture
/// assertions use, so `mach_goldens` and they cannot disagree about what a run
/// produced.
fn transcript_of(name: &'static str) -> Option<String> {
    Some(match name {
        "mach_backtrace" => backtrace_run()?.1,
        "mach_interrupts" => interrupts_run()?.1,
        "mach_loopnez" => loopnez_run()?.1,
        "mach_s32c1i" => s32c1i_run()?.1,
        "mach_ctxswitch" => ctxswitch_run()?.1,
        "mach_dbreak" => dbreak_run()?.1,
        "mach_lserr" => lserr_run()?.1,
        other => panic!("no transcript builder for fixture {other}"),
    })
}

// ===========================================================================
// (a) the 25-deep backtrace — the milestone's headline
// ===========================================================================

/// Slots: 1 = frames reported, 2 = the chain's depth, 3.. = the PCs.
const BT_FRAME_BASE: usize = 3;

fn backtrace_run() -> Option<(Run, String)> {
    let run = run("mach_backtrace", &Script::empty())?;
    let reported = run.slot(1) as usize;
    let names: Vec<String> = (0..reported)
        .map(|i| run.symbols.resolve(run.slot(BT_FRAME_BASE + i)))
        .collect();

    let mut s = run.header();
    let _ = writeln!(s, "chain depth: {}", run.slot(2));
    let _ = writeln!(s, "frames reported: {reported}");
    let distinct: BTreeSet<u32> = (0..reported).map(|i| run.slot(BT_FRAME_BASE + i)).collect();
    let _ = writeln!(s, "distinct PCs: {}", distinct.len());
    for (i, n) in names.iter().enumerate() {
        let _ = writeln!(s, "frame[{i:02}]: {n}");
    }
    Some((run, s))
}

#[test]
fn mach_backtrace_25_deep() {
    let Some((run, _)) = backtrace_run() else {
        return;
    };
    assert_eq!(run.slot(2), 25, "the fixture's own chain depth");

    let reported = run.slot(1) as usize;
    let pcs: Vec<u32> = (0..reported).map(|i| run.slot(BT_FRAME_BASE + i)).collect();
    let names: Vec<String> = pcs.iter().map(|p| run.symbols.resolve(*p)).collect();

    // The window exceptions are what makes this walk possible at all: without
    // an overflow, the frames' `a0`/`a1` are still in the physical register
    // file and the walk reads whatever was last below those stack pointers.
    assert!(
        run.vectors.entered(VECOFS_OF4),
        "no window-overflow vector was entered, so nothing was spilled and \
         whatever the walk reported it did not read from save areas. Vectors: {}",
        run.vectors.summary()
    );

    // The historical wrong answer, on the record in the ADR: a 25-deep chain
    // reporting 19 IDENTICAL PCs. Name it, because a walk that produces it
    // otherwise reads as "19 frames, plausible".
    let distinct: BTreeSet<u32> = pcs.iter().copied().collect();
    assert!(
        distinct.len() >= 25,
        "backtrace reported {reported} frames but only {} DISTINCT PCs. The \
         ADR's known wrong answer for this exact shape was 19 identical ones — \
         a window spill that wrote every frame's save area to the same place. \
         Frames: {names:?}",
        distinct.len()
    );

    // The 25 fixture frames, innermost first, are `bt_f25 .. bt_f01`. They may
    // be preceded by the capture helper's own frame, depending on whether the
    // optimizer folded `capture_frames` into it, so find the run rather than
    // assuming it starts at index 0.
    let want: Vec<String> = (1..=25).rev().map(|n| format!("bt_f{n:02}")).collect();
    let start = names
        .windows(want.len())
        .position(|w| w == want.as_slice())
        .unwrap_or_else(|| {
            panic!(
                "the 25 fixture frames do not appear in call order.\n  wanted: \
                 {want:?}\n  got:    {names:?}"
            )
        });
    assert!(
        start <= 2,
        "the fixture's 25 frames start at index {start}; more than two frames of \
         capture scaffolding in front of them means the walk picked up \
         something it should not have. Frames: {names:?}"
    );
}

// ===========================================================================
// (b) level-1 and level-3 interrupts
// ===========================================================================

fn interrupts_run() -> Option<(Run, String)> {
    let run = run("mach_interrupts", &Script::empty())?;
    let mut s = run.header();
    let _ = writeln!(s, "l1 handler runs: {}", run.slot(1));
    let _ = writeln!(s, "l1 exccause: {}", run.slot(2));
    let _ = writeln!(s, "l1 ps before: {:#010x}", run.slot(3));
    let _ = writeln!(s, "l1 ps after: {:#010x}", run.slot(4));
    let _ = writeln!(s, "l3 handler runs: {}", run.slot(5));
    let _ = writeln!(s, "l3 epc3 in: {}", run.symbols.resolve(run.slot(6)));
    let _ = writeln!(s, "l3 eps3: {:#010x}", run.slot(7));
    let _ = writeln!(s, "l3 ps before: {:#010x}", run.slot(8));
    let _ = writeln!(s, "l3 ps after: {:#010x}", run.slot(9));
    let _ = writeln!(s, "resume marker after rfi 3: {:#x}", run.slot(10));
    Some((run, s))
}

#[test]
fn mach_interrupt_level1_cause4() {
    let Some((run, _)) = interrupts_run() else {
        return;
    };
    assert_eq!(run.slot(1), 1, "the level-1 handler ran exactly once");
    assert_eq!(
        run.slot(2),
        4,
        "a level-1 interrupt arrives through the general exception vector with \
         EXCCAUSE = 4 (Level1InterruptCause), not through a vector of its own"
    );
    assert!(
        run.vectors.entered(VECOFS_USER),
        "entered _UserExceptionVector (VECBASE + 0x340). PS.UM is 1, as the \
         bootloader leaves it, so the USER vector is the right one. Vectors: {}",
        run.vectors.summary()
    );
    assert_eq!(
        run.vectors.count(VECOFS_KERNEL),
        0,
        "the kernel vector was entered; with PS.UM = 1 it must not be"
    );
    // `rfe` clears PS.EXCM and restores PS.INTLEVEL.
    assert_eq!(
        run.slot(4),
        run.slot(3),
        "PS after the level-1 return ({:#010x}) is the PS the interrupted code \
         was running under ({:#010x}) — `rfe` restored it, EXCM cleared",
        run.slot(4),
        run.slot(3)
    );
    assert_eq!(run.slot(3) & 0x10, 0, "PS.EXCM was clear to begin with");
}

#[test]
fn mach_interrupt_level3_rfi() {
    let Some((run, _)) = interrupts_run() else {
        return;
    };
    assert_eq!(run.slot(5), 1, "the level-3 handler ran exactly once");
    assert_eq!(
        run.vectors.count(VECOFS_LEVEL3),
        1,
        "entered _Level3InterruptVector (VECBASE + 0x1C0) exactly once. \
         Vectors: {}",
        run.vectors.summary()
    );
    // EPC3 names the instruction the interrupt preempted: it must fall inside
    // the fixture's own raising function, not in a handler or a vector.
    assert_eq!(
        run.symbols.resolve(run.slot(6)),
        "raise_level3",
        "EPC3 ({:#010x}) points into the interrupted function",
        run.slot(6)
    );
    // EPS3 is the PS in force when the interrupt was taken — and the fixture
    // captured that PS itself one instruction earlier, so this is a direct
    // comparison rather than a guess at what the hart should have banked.
    assert_eq!(
        run.slot(7),
        run.slot(8),
        "EPS3 ({:#010x}) is the PS the interrupted code was running under ({:#010x})",
        run.slot(7),
        run.slot(8)
    );
    assert_eq!(
        run.slot(10),
        0xA5A5_1234,
        "the interrupted instruction stream resumed after `rfi 3` and wrote its \
         marker — the `mov` is in the same asm block as the `wsr.intset`"
    );
    assert_eq!(
        run.slot(9),
        run.slot(8),
        "`rfi 3` restored PS exactly ({:#010x} vs {:#010x})",
        run.slot(9),
        run.slot(8)
    );
}

// ===========================================================================
// (c) loopnez
// ===========================================================================

fn loopnez_run() -> Option<(Run, String)> {
    let run = run("mach_loopnez", &Script::empty())?;
    let mut s = run.header();
    for (i, (label, start)) in [("as=1", 0u32), ("as=2", 1), ("as=8", 7)]
        .into_iter()
        .enumerate()
    {
        let _ = writeln!(
            s,
            "{label}: lcount_start={start} iterations={} lcount_after={}",
            run.slot(1 + 2 * i),
            run.slot(2 + 2 * i)
        );
    }
    let _ = writeln!(s, "lcount inside first iteration (as=8): {}", run.slot(7));
    Some((run, s))
}

#[test]
fn mach_loopnez_runs_every_iteration() {
    let Some((run, _)) = loopnez_run() else {
        return;
    };
    for (i, start) in [0u32, 1, 7].into_iter().enumerate() {
        let iterations = run.slot(1 + 2 * i);
        let after = run.slot(2 + 2 * i);
        assert_eq!(
            iterations,
            start + 1,
            "a loopnez with LCOUNT = {start} runs its body LCOUNT + 1 times; \
             this hart ran it {iterations} times. One iteration for every count \
             is the silent-single-iteration failure this fixture exists to catch."
        );
        assert_eq!(after, 0, "LCOUNT lands at 0 after the loop");
    }
    assert_eq!(
        run.slot(7),
        7,
        "inside the first iteration of an 8-count loop LCOUNT still reads 7 — it \
         is decremented on the loop-back, not on entry"
    );
}

// ===========================================================================
// (d) s32c1i
// ===========================================================================

fn s32c1i_run() -> Option<(Run, String)> {
    let run = run("mach_s32c1i", &Script::empty())?;
    let mut s = run.header();
    let _ = writeln!(s, "initial: {:#010x}", run.slot(5));
    let _ = writeln!(s, "new: {:#010x}", run.slot(6));
    let _ = writeln!(s, "wrong expectation: {:#010x}", run.slot(7));
    let _ = writeln!(
        s,
        "success: mem={:#010x} reg={:#010x}",
        run.slot(1),
        run.slot(2)
    );
    let _ = writeln!(
        s,
        "failure: mem={:#010x} reg={:#010x}",
        run.slot(3),
        run.slot(4)
    );
    Some((run, s))
}

#[test]
fn mach_s32c1i_success_and_failure() {
    let Some((run, _)) = s32c1i_run() else {
        return;
    };
    let (initial, new) = (run.slot(5), run.slot(6));
    assert_eq!(run.slot(1), new, "on success the store happened");
    assert_eq!(
        run.slot(2),
        initial,
        "on success the destination register holds the OLD value"
    );
    assert_eq!(run.slot(3), new, "on failure nothing was stored");
    assert_eq!(
        run.slot(4),
        new,
        "on failure the destination register holds the CURRENT value"
    );
}

// ===========================================================================
// (e) two-task context switch
// ===========================================================================

/// The asynchronous half of fixture (e)'s preemption: line 0 (level 1,
/// `IntKind::Level`) raised at chosen guest cycles, on top of the tasks' own
/// deterministic `wsr.intset`. A level line follows the mask, so each entry is
/// paired with the boundary that lowers it again.
fn ctxswitch_script() -> Script {
    let mut s = Script::empty();
    let mut cycle = 4_000;
    for _ in 0..12 {
        s = s.at(cycle, 1 << IRQ_EXTERNAL_L1).at(cycle + SLICE, 0);
        cycle += 4 * SLICE;
    }
    s
}

fn ctxswitch_run() -> Option<(Run, String)> {
    let run = run("mach_ctxswitch", &ctxswitch_script())?;
    let mut s = run.header();
    let _ = writeln!(s, "task A rounds: {}", run.slot(1));
    let _ = writeln!(s, "task B rounds: {}", run.slot(2));
    let _ = writeln!(s, "switches: {}", run.slot(3));
    let _ = writeln!(s, "cross-task loop-state sightings: {}", run.slot(4));
    let _ = writeln!(s, "task A lcount fold: {}", run.slot(5));
    let _ = writeln!(s, "task B lcount fold: {}", run.slot(6));
    let _ = writeln!(s, "task A loop bounds stable: {}", run.slot(7));
    let _ = writeln!(s, "task B loop bounds stable: {}", run.slot(8));
    Some((run, s))
}

#[test]
fn mach_esp_rtos_context_switch() {
    let Some((run, _)) = ctxswitch_run() else {
        return;
    };
    assert!(run.slot(3) >= 2, "at least two switches were taken");
    assert!(run.slot(1) > 0, "task A made progress");
    assert!(run.slot(2) > 0, "task B made progress");
    assert!(
        run.vectors.entered(VECOFS_USER),
        "the switches went through the level-1 path, i.e. the general \
         exception vector. Vectors: {}",
        run.vectors.summary()
    );
    assert_eq!(
        run.slot(4),
        0,
        "a task saw the OTHER task's loop state {} time(s). LBEG/LEND/LCOUNT \
         are per-task state, saved and restored by `save_context`/\
         `restore_context` on every preemption; a hart that leaks them across a \
         switch corrupts a zero-overhead loop in whichever task it resumes.",
        run.slot(4)
    );
    // 64 iterations folding LCOUNT 63..0, and 48 folding 47..0.
    assert_eq!(run.slot(5), 64 * 63 / 2, "task A's LCOUNT fold");
    assert_eq!(run.slot(6), 48 * 47 / 2, "task B's LCOUNT fold");
    assert_eq!(
        run.slot(7),
        1,
        "task A's LBEG/LEND survived its preemptions"
    );
    assert_eq!(
        run.slot(8),
        1,
        "task B's LBEG/LEND survived its preemptions"
    );
}

// ===========================================================================
// (f) DBREAK stack guard
// ===========================================================================

fn dbreak_run() -> Option<(Run, String)> {
    let run = run("mach_dbreak", &Script::empty())?;
    let mut s = run.header();
    let _ = writeln!(s, "handler runs: {}", run.slot(4));
    let _ = writeln!(s, "debugcause: {:#010x}", run.slot(2));
    let _ = writeln!(s, "dbreak slot: {}", run.slot(7));
    let _ = writeln!(s, "guarded word before store: {:#010x}", run.slot(3));
    let _ = writeln!(s, "guarded word seen by handler: {:#010x}", run.slot(1));
    let _ = writeln!(s, "store wanted to write: {:#010x}", run.slot(6));
    let _ = writeln!(s, "guarded word at end: {:#010x}", run.slot(5));
    Some((run, s))
}

#[test]
fn mach_dbreak_stack_guard() {
    let Some((run, _)) = dbreak_run() else {
        return;
    };
    assert_eq!(run.slot(4), 1, "the debug handler ran exactly once");
    assert_eq!(
        run.vectors.count(VECOFS_DEBUG),
        1,
        "a DBREAK match vectors to the debug exception vector (VECBASE + 0x280) \
         exactly once. Vectors: {}",
        run.vectors.summary()
    );
    // DEBUGCAUSE bit 2 = DBREAK (RM §4.7.6.2, Table 4-123); bits 11:8 = slot.
    assert_eq!(
        run.slot(2) & 0b100,
        0b100,
        "DEBUGCAUSE ({:#010x}) names DBREAK as the cause",
        run.slot(2)
    );
    assert_eq!(
        run.slot(2) & !0b0000_1111_0000_0100,
        0,
        "DEBUGCAUSE ({:#010x}) names nothing else — not ICOUNT, not IBREAK, not `break`",
        run.slot(2)
    );
    assert_eq!(run.slot(7), 0, "the match is attributed to DBREAK slot 0");
    assert_eq!(
        run.slot(1),
        run.slot(3),
        "THE ACCESS DID NOT HAPPEN: the guarded word still held its pre-store \
         value ({:#010x}) when the handler read it back. A watchpoint that traps \
         AFTER the store is a watchpoint that cannot guard a stack.",
        run.slot(3)
    );
    assert_ne!(
        run.slot(6),
        run.slot(3),
        "the fixture's store would have changed the word — otherwise the \
         assertion above proves nothing"
    );
    assert_eq!(
        run.slot(5),
        run.slot(6),
        "after the handler disarmed the slot, `rfi 6` re-executed the SAME \
         store and it completed"
    );
}

// ===========================================================================
// (g) load/store error
// ===========================================================================

fn lserr_run() -> Option<(Run, String)> {
    let run = run("mach_lserr", &Script::empty())?;
    let mut s = run.header();
    let _ = writeln!(s, "handler runs: {}", run.slot(5));
    let _ = writeln!(s, "exccause: {}", run.slot(1));
    let _ = writeln!(
        s,
        "excvaddr == faulting address: {}",
        run.slot(2) == run.slot(4)
    );
    let _ = writeln!(s, "faulting address: {:#010x}", run.slot(4));
    let _ = writeln!(s, "epc1 in: {}", run.symbols.resolve(run.slot(3)));
    Some((run, s))
}

#[test]
fn mach_load_store_error_excvaddr() {
    let Some((run, _)) = lserr_run() else {
        return;
    };
    assert_eq!(run.slot(5), 1, "the exception handler ran exactly once");
    assert_eq!(run.slot(1), 3, "EXCCAUSE = 3 (LoadStoreError)");
    assert_eq!(
        run.slot(2),
        run.slot(4),
        "EXCVADDR ({:#010x}) is the faulting address ({:#010x})",
        run.slot(2),
        run.slot(4)
    );
    assert!(
        run.vectors.entered(VECOFS_USER),
        "entered _UserExceptionVector (VECBASE + 0x340). Vectors: {}",
        run.vectors.summary()
    );
    assert_eq!(
        run.vectors.count(VECOFS_KERNEL),
        0,
        "the kernel vector was entered; with PS.UM = 1 it must not be"
    );
    assert_eq!(
        run.symbols.resolve(run.slot(3)),
        "fault_load",
        "EPC1 ({:#010x}) names the faulting instruction",
        run.slot(3)
    );
}

// ===========================================================================
// Goldens and the anti-skip guards
// ===========================================================================

#[test]
fn mach_goldens() {
    for name in FIXTURES {
        let Some(actual) = transcript_of(name) else {
            continue;
        };
        check_golden(name, &actual);
    }
}

/// The guard the phase exists behind: `lp-xt/fixtures/elf/` is gitignored, so a
/// host test whose ELF is absent skips and reports success. In CI
/// (`LP_XT_MACH_FIXTURES_REQUIRED=1`) that must be a failure instead.
#[test]
fn mach_fixtures_present() {
    let missing: Vec<&str> = FIXTURES
        .iter()
        .copied()
        .filter(|n| !elf_path(n).is_file())
        .collect();
    if !fixtures_required() {
        if !missing.is_empty() {
            eprintln!(
                "SKIP mach_fixtures_present: {} of {} fixture ELFs are absent \
                 ({missing:?}); build them with lp-xt/fixtures/build.sh. CI sets \
                 LP_XT_MACH_FIXTURES_REQUIRED=1 and this is a failure there.",
                missing.len(),
                FIXTURES.len()
            );
        }
        return;
    }
    assert!(
        missing.is_empty(),
        "LP_XT_MACH_FIXTURES_REQUIRED is set and these fixture ELFs are missing: \
         {missing:?}. Every mach test would have SKIPPED and reported success — \
         which is the exact failure this phase exists to prevent."
    );
}

/// A fixture in [`FIXTURES`] with no golden, or a golden with no fixture, is a
/// silence. Keep the two lists welded together.
#[test]
fn every_fixture_has_a_golden_and_every_golden_a_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/mach");
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".txt"))
        .map(|n| n.trim_end_matches(".txt").to_string())
        .collect();
    on_disk.sort();
    let mut expected: Vec<String> = FIXTURES.iter().map(|s| (*s).to_string()).collect();
    expected.sort();
    assert_eq!(
        on_disk,
        expected,
        "the goldens in {} and the FIXTURES list have drifted",
        dir.display()
    );
}

/// The runner's own contract: a run costs the same cycles every time and
/// produces the same slots, and the vector table is where the fixtures think
/// it is.
#[test]
fn mach_runs_are_bounded_and_deterministic() {
    let Some(a) = run("mach_loopnez", &Script::empty()) else {
        return;
    };
    let b = run("mach_loopnez", &Script::empty()).expect("second run");
    assert_eq!(a.cycles, b.cycles, "two runs of one fixture cost the same");
    assert_eq!(a.slots, b.slots, "two runs of one fixture agree");
    assert!(a.cycles > 0 && a.cycles < CYCLE_LIMIT);
    assert_eq!(
        VECBASE % VECTOR_TABLE_SIZE,
        0,
        "VECBASE must be 1 KiB aligned; its low bits are not writable"
    );
}
