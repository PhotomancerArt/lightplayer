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
//! a software interrupt *is*) or gets it from [`Script`] below — a list of
//! `(guest cycle, mask)` pairs the runner applies at slice boundaries via
//! `XtHart::set_external_mask`. Guest cycles, never wall time (plan PD9).
//!
//! # Goldens
//!
//! Each fixture's run produces a **symbolic** transcript: architectural values
//! (`EXCCAUSE`, `LCOUNT`, a loaded word) and, where an address is the point,
//! the *symbol* the address falls in — never the address itself. The linker's
//! addresses are a function of the toolchain version; the symbol is a function
//! of the fixture. A golden full of raw PCs would have to be re-blessed on
//! every esp-toolchain bump, and a golden that gets re-blessed routinely stops
//! being evidence.
//!
//! **Never edit a golden by hand.** A mismatch is a regression or a deliberate
//! re-capture; a re-capture is `LP_XT_MACH_BLESS=1 cargo test -p lp-xt-emu
//! --test mach_fixtures` as its own commit, with its reason in the message.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use lp_xt_elf::XtensaElf;
use lp_xt_emu::mach::interrupt::{IntKind, IntLine};
use lp_xt_emu::mach::sr::PS_BOOT;
use lp_xt_emu::mach::trap::NUM_INTERRUPTS;
use lp_xt_emu::mach::{CoreConfig, SliceEnd, XtHart};
use lp_xt_emu::memory::Memory;

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
/// Total cycle ceiling. A fixture that exceeds it has hung — an exception loop
/// or a window handler that never returns — and the runner says so by name
/// rather than by the harness timing out.
const CYCLE_LIMIT: u64 = 8_000_000;

/// The `PRID` the fixtures see: the classic ESP32's PRO_CPU number. A chip
/// number, not a hart index (`CoreConfig::prid`).
const PRID_PRO_CPU: u32 = 0xCDCD;

// The CPU interrupt lines, in the classic's shape (esp-hal's `CpuInterrupt`
// table). Core configuration, so it arrives through `CoreConfig` — the same
// table `src/mach/tests.rs` configures, so a fixture and a unit test mean the
// same thing by "line 7".
const IRQ_SOFTWARE_L1: u8 = 7;
const IRQ_LEVEL_L1: u8 = 0;
const IRQ_SOFTWARE_L3: u8 = 29;
const IRQ_LEVEL_L3: u8 = 23;

fn core_config(reset_pc: u32) -> CoreConfig {
    let mut interrupts = [IntLine::UNUSED; NUM_INTERRUPTS];
    interrupts[usize::from(IRQ_LEVEL_L1)] = IntLine::new(1, IntKind::Level);
    interrupts[6] = IntLine::new(1, IntKind::Timer(0));
    interrupts[usize::from(IRQ_SOFTWARE_L1)] = IntLine::new(1, IntKind::Software);
    interrupts[15] = IntLine::new(3, IntKind::Timer(1));
    interrupts[19] = IntLine::new(2, IntKind::Level);
    interrupts[22] = IntLine::new(3, IntKind::Edge);
    interrupts[usize::from(IRQ_LEVEL_L3)] = IntLine::new(3, IntKind::Level);
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
// Finding the ELFs — and refusing to skip where it matters
// ---------------------------------------------------------------------------

/// The seven fixture images this phase owns. `lp-xt/fixtures/elf/` is
/// **gitignored**, so these are built, never committed — which is exactly how a
/// test suite comes to skip and report success. [`mach_fixtures_present`] and
/// the `xtensa-host` CI job's assert step are the two guards against that.
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
            .last()
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
    /// Address → symbol, for turning a captured PC into a name.
    symbols: SymbolMap,
    hart: XtHart<Memory>,
    mem: Memory,
    end: SliceEnd,
    cycles: u64,
}

/// Function symbols sorted by address, so a PC can be attributed to one.
struct SymbolMap {
    /// start address → (name, size-inferred end)
    by_addr: Vec<(u32, String)>,
}

impl SymbolMap {
    fn build(elf: &XtensaElf<'_>) -> Self {
        let mut map: BTreeMap<u32, String> = BTreeMap::new();
        for (name, addr) in elf.symbols() {
            // Local assembler labels and the linker's absolute markers are not
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
    // duplicate is deliberate, because only one of the two runs in CI's
    // Xtensa-less jobs.
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
            panic!("{name}: segment at {:#010x} is unmapped at {a:#010x}", seg.vaddr)
        });
        let tail = seg.memsz.saturating_sub(seg.data.len() as u32);
        mem.try_zero(seg.vaddr.wrapping_add(seg.data.len() as u32), tail)
            .unwrap_or_else(|a| panic!("{name}: bss tail unmapped at {a:#010x}"));
    }

    let mut hart = XtHart::new(0, core_config(elf.entry()));
    // What the ROM and the second-stage bootloader leave behind for a direct
    // load: WOE = 1, EXCM = 0, CALLINC = 2 (the bootloader reaches the entry
    // point through a `callx8`). `Reset`'s own `entry a1, 0x10` needs WOE.
    hart.set_ps_raw(PS_BOOT);
    // Bring-up honesty: an encoding this emulator does not implement stops the
    // run by name instead of vectoring into the guest's illegal-instruction
    // handler, where it would look like a fixture bug.
    hart.set_strict_unsupported(true);

    let mut end;
    loop {
        hart.set_external_mask(script.mask_at(hart.cycle_count()));
        end = hart.run_slice(&mut mem, SLICE);
        match end {
            SliceEnd::BudgetExhausted | SliceEnd::BusYield => {}
            SliceEnd::Wfi => {
                // Nothing on this bus generates work while the hart idles, so
                // the only thing that can wake it is the script. Step one
                // slice's worth of guest time and poll.
                let next = hart.cycle_count() + SLICE;
                hart.advance_to_cycle(next);
                hart.set_external_mask(script.mask_at(hart.cycle_count()));
                hart.poll_interrupts();
            }
            SliceEnd::Ebreak { .. } | SliceEnd::Fault(_) => break,
        }
        assert!(
            hart.cycle_count() < CYCLE_LIMIT,
            "{name}: ran past {CYCLE_LIMIT} cycles without reaching `break` — \
             the fixture is looping (an exception loop, or a window handler \
             that never returns). pc = {:#010x}",
            hart.pc()
        );
    }

    let symbols = SymbolMap::build(&elf);
    if let SliceEnd::Fault(f) = end {
        panic!("{name}: hart fault {f:?} at pc {:#010x}", hart.pc());
    }

    let base = elf
        .symbol("MACH_RESULT")
        .unwrap_or_else(|| panic!("{name}: no MACH_RESULT symbol"));
    let slots: Vec<u32> = (0..RESULT_SLOTS)
        .map(|i| {
            mem.read_u32(base + 4 * i as u32)
                .unwrap_or_else(|e| panic!("{name}: read MACH_RESULT[{i}]: {e:?}"))
        })
        .collect();

    assert_ne!(
        slots[0], MAGIC_PANIC,
        "{name}: the fixture panicked (slot 0 = MAGIC_PANIC). Its own assertions failed."
    );
    assert_eq!(
        slots[0], MAGIC_DONE,
        "{name}: the fixture did not reach `finish()` (slot 0 = {:#010x}). \
         It stopped at {:#010x} ({}), so every other slot is whatever it was \
         before the run stopped.",
        slots[0],
        hart.pc(),
        symbols.resolve(hart.pc())
    );

    let cycles = hart.cycle_count();
    Some(Run {
        name,
        slots,
        symbols,
        hart,
        mem,
        end,
        cycles,
    })
}

impl Run {
    fn slot(&self, i: usize) -> u32 {
        self.slots[i]
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

/// Collect every fixture's transcript once, so `mach_goldens` and the
/// per-fixture tests cannot disagree about what a run produced.
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

/// Header every transcript starts with: the values that are the same in every
/// run and whose drift would explain every downstream difference at once.
fn header(run: &Run) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "fixture: {}", run.name);
    let _ = writeln!(s, "vecbase: {:#010x}", run.hart.sr().vecbase);
    let _ = writeln!(
        s,
        "end: {}",
        match run.end {
            SliceEnd::Ebreak { .. } => "break",
            _ => "other",
        }
    );
    s
}

// ===========================================================================
// (a) the 25-deep backtrace — the milestone's headline
// ===========================================================================

/// Slots: 1 = frames reported, 2 = expected depth, 3.. = the PCs.
const BT_FRAME_BASE: usize = 3;

fn backtrace_run() -> Option<(Run, String)> {
    let run = run("mach_backtrace", &Script::empty())?;
    let reported = run.slot(1) as usize;
    let expected = run.slot(2) as usize;

    let names: Vec<String> = (0..reported)
        .map(|i| run.symbols.resolve(run.slot(BT_FRAME_BASE + i)))
        .collect();

    let mut s = header(&run);
    let _ = writeln!(s, "chain depth: {expected}");
    let _ = writeln!(s, "frames reported: {reported}");
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
    let depth = run.slot(2) as usize;
    assert_eq!(depth, 25, "the fixture's own chain depth");

    let reported = run.slot(1) as usize;
    let pcs: Vec<u32> = (0..reported).map(|i| run.slot(BT_FRAME_BASE + i)).collect();
    let names: Vec<String> = pcs.iter().map(|p| run.symbols.resolve(*p)).collect();

    // The historical wrong answer, on the record in the ADR: a 25-deep chain
    // reporting 19 IDENTICAL PCs. Name it, because a walk that produces it
    // otherwise reads as "19 frames, plausible".
    let distinct: std::collections::BTreeSet<u32> = pcs.iter().copied().collect();
    assert!(
        distinct.len() >= 25,
        "backtrace reported {reported} frames but only {} DISTINCT PCs. \
         The ADR's known wrong answer for this exact fixture was 19 identical \
         ones — a window spill that wrote every frame's save area to the same \
         place. Frames: {names:?}",
        distinct.len()
    );

    // The 25 fixture frames, innermost first, are `bt_f25 .. bt_f01`. They may
    // be preceded by the capture helper's own frame(s), depending on whether
    // the optimizer inlined `capture_frames` into it, so find the run rather
    // than assuming it starts at 0.
    let want: Vec<String> = (1..=25).rev().map(|n| format!("bt_f{n:02}")).collect();
    let start = names
        .windows(want.len())
        .position(|w| w == want.as_slice())
        .unwrap_or_else(|| {
            panic!(
                "the 25 fixture frames do not appear in call order.\n  \
                 wanted: {want:?}\n  got:    {names:?}"
            )
        });
    assert!(
        start <= 2,
        "the fixture's 25 frames start at index {start}; more than two frames \
         of capture scaffolding in front of them means the walk picked up \
         something it should not have. Frames: {names:?}"
    );
}

// ===========================================================================
// (b) level-1 and level-3 interrupts
// ===========================================================================

// Slots: 1 = level-1 handler ran, 2 = EXCCAUSE seen at level 1,
// 3 = level-1 vector entry PC, 4 = level-3 handler ran,
// 5 = EPC3 seen, 6 = EPS3 seen, 7 = level-3 vector entry PC,
// 8 = the resume marker the interrupted stream wrote after `rfi 3`,
// 9 = PS after the level-1 return, 10 = PS after the level-3 return,
// 11 = PS captured just before raising the level-3 interrupt.

fn interrupts_run() -> Option<(Run, String)> {
    let run = run("mach_interrupts", &Script::empty())?;
    let mut s = header(&run);
    let _ = writeln!(s, "l1 handler ran: {}", run.slot(1));
    let _ = writeln!(s, "l1 exccause: {}", run.slot(2));
    let _ = writeln!(s, "l1 vector: {}", run.symbols.resolve(run.slot(3)));
    let _ = writeln!(s, "l1 vector offset: {:#05x}", run.slot(3) - VECBASE);
    let _ = writeln!(s, "l3 handler ran: {}", run.slot(4));
    let _ = writeln!(s, "l3 vector: {}", run.symbols.resolve(run.slot(7)));
    let _ = writeln!(s, "l3 vector offset: {:#05x}", run.slot(7) - VECBASE);
    let _ = writeln!(s, "l3 epc3 in text: {}", run.symbols.resolve(run.slot(5)));
    let _ = writeln!(s, "l3 eps3: {:#010x}", run.slot(6));
    let _ = writeln!(s, "ps before raising l3: {:#010x}", run.slot(11));
    let _ = writeln!(s, "resume marker after rfi 3: {:#x}", run.slot(8));
    let _ = writeln!(s, "ps after l1 return: {:#010x}", run.slot(9));
    let _ = writeln!(s, "ps after l3 return: {:#010x}", run.slot(10));
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
        "a level-1 interrupt arrives through the user/kernel exception vector \
         with EXCCAUSE = 4 (Level1InterruptCause), not through a vector of its own"
    );
    assert_eq!(
        run.slot(3) - VECBASE,
        0x340,
        "entered _UserExceptionVector at VECBASE + 0x340"
    );
    // `rfe` clears PS.EXCM; the fixture reads PS back in `main` afterwards.
    assert_eq!(
        run.slot(9) & 0x10,
        0,
        "PS.EXCM is clear after the level-1 return (PS = {:#010x})",
        run.slot(9)
    );
    assert_eq!(
        run.slot(9) & 0xF,
        0,
        "PS.INTLEVEL is back to 0 after the level-1 return (PS = {:#010x})",
        run.slot(9)
    );
}

#[test]
fn mach_interrupt_level3_rfi() {
    let Some((run, _)) = interrupts_run() else {
        return;
    };
    assert_eq!(run.slot(4), 1, "the level-3 handler ran exactly once");
    assert_eq!(
        run.slot(7) - VECBASE,
        0x1C0,
        "entered _Level3InterruptVector at VECBASE + 0x1C0"
    );
    // EPC3 names the instruction the interrupt preempted: it must fall inside
    // the fixture's own raising function, not in a handler or a vector.
    assert_eq!(
        run.symbols.resolve(run.slot(5)),
        "raise_level3",
        "EPC3 ({:#010x}) points into the interrupted function",
        run.slot(5)
    );
    // EPS3 is the PS in force when the interrupt was taken — the fixture
    // captured that PS itself just before, so this is a direct comparison
    // rather than a guess at what the hart should have saved.
    assert_eq!(
        run.slot(6),
        run.slot(11),
        "EPS3 ({:#010x}) is the PS the interrupted code was running under ({:#010x})",
        run.slot(6),
        run.slot(11)
    );
    assert_eq!(
        run.slot(8),
        0xA5A5_1234,
        "the interrupted instruction stream resumed after `rfi 3` and wrote its marker"
    );
    assert_eq!(
        run.slot(10),
        run.slot(11),
        "`rfi 3` restored PS exactly ({:#010x} vs {:#010x})",
        run.slot(10),
        run.slot(11)
    );
}

// ===========================================================================
// (c) loopnez
// ===========================================================================

fn loopnez_run() -> Option<(Run, String)> {
    let run = run("mach_loopnez", &Script::empty())?;
    let mut s = header(&run);
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
        "inside the first iteration of an 8-count loop, LCOUNT still reads 7 \
         (it is decremented on the loop-back, not on entry)"
    );
}

// ===========================================================================
// (d) s32c1i
// ===========================================================================

// Slots: 1 = memory after the successful CAS, 2 = the register the successful
// CAS left, 3 = memory after the failing CAS, 4 = the register the failing CAS
// left, 5 = the initial value, 6 = the new value, 7 = the wrong expectation.

fn s32c1i_run() -> Option<(Run, String)> {
    let run = run("mach_s32c1i", &Script::empty())?;
    let mut s = header(&run);
    let _ = writeln!(s, "initial: {:#010x}", run.slot(5));
    let _ = writeln!(s, "new: {:#010x}", run.slot(6));
    let _ = writeln!(s, "wrong expectation: {:#010x}", run.slot(7));
    let _ = writeln!(s, "success: mem={:#010x} reg={:#010x}", run.slot(1), run.slot(2));
    let _ = writeln!(s, "failure: mem={:#010x} reg={:#010x}", run.slot(3), run.slot(4));
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
// (e) two-task context switch through the level-1 interrupt
// ===========================================================================

// Slots: 1 = task A progress, 2 = task B progress, 3 = switches taken,
// 4 = task A's LCOUNT seen after a switch, 5 = task B's LCOUNT seen after a
// switch, 6 = task A's LBEG/LEND agreement flag, 7 = task B's ditto,
// 8 = the number of times a task observed the OTHER task's loop state.

/// The preemption schedule. Every entry raises the level-1 software line; the
/// handler lowers it by writing INTCLEAR, so a `Level` line would re-fire —
/// line 7 is `IntKind::Software` and behaves as the classic's does.
fn ctxswitch_script() -> Script {
    let mut s = Script::empty();
    let mut cycle = 4_000;
    for _ in 0..12 {
        s = s.at(cycle, 1 << IRQ_SOFTWARE_L1).at(cycle + SLICE, 0);
        cycle += 4 * SLICE;
    }
    s
}

fn ctxswitch_run() -> Option<(Run, String)> {
    let run = run("mach_ctxswitch", &ctxswitch_script())?;
    let mut s = header(&run);
    let _ = writeln!(s, "task A progress: {}", run.slot(1));
    let _ = writeln!(s, "task B progress: {}", run.slot(2));
    let _ = writeln!(s, "switches: {}", run.slot(3));
    let _ = writeln!(s, "task A lcount after switch: {}", run.slot(4));
    let _ = writeln!(s, "task B lcount after switch: {}", run.slot(5));
    let _ = writeln!(s, "task A loop bounds intact: {}", run.slot(6));
    let _ = writeln!(s, "task B loop bounds intact: {}", run.slot(7));
    let _ = writeln!(s, "cross-task loop-state sightings: {}", run.slot(8));
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
    assert_eq!(
        run.slot(8),
        0,
        "a task saw the OTHER task's loop state {} times. LBEG/LEND/LCOUNT are \
         per-task state saved and restored by the context switch; a hart that \
         leaks them across a preemption corrupts a zero-overhead loop in \
         whichever task is resumed.",
        run.slot(8)
    );
    assert_eq!(run.slot(6), 1, "task A's LBEG/LEND survived its preemptions");
    assert_eq!(run.slot(7), 1, "task B's LBEG/LEND survived its preemptions");
}

// ===========================================================================
// (f) DBREAK stack guard
// ===========================================================================

// Slots: 1 = the guarded word as the handler saw it, 2 = DEBUGCAUSE,
// 3 = the debug vector entry PC, 4 = handler ran, 5 = the guarded word after
// the run, 6 = the value the store meant to write, 7 = the word's value before
// the store, 8 = DEBUGCAUSE's DBREAK slot number.

fn dbreak_run() -> Option<(Run, String)> {
    let run = run("mach_dbreak", &Script::empty())?;
    let mut s = header(&run);
    let _ = writeln!(s, "handler ran: {}", run.slot(4));
    let _ = writeln!(s, "debug vector: {}", run.symbols.resolve(run.slot(3)));
    let _ = writeln!(s, "debug vector offset: {:#05x}", run.slot(3) - VECBASE);
    let _ = writeln!(s, "debugcause: {:#010x}", run.slot(2));
    let _ = writeln!(s, "dbreak slot: {}", run.slot(8));
    let _ = writeln!(s, "guarded word before store: {:#010x}", run.slot(7));
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
        run.slot(3) - VECBASE,
        0x280,
        "a DBREAK match vectors to the debug exception vector, VECBASE + 0x280"
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
    assert_eq!(run.slot(8), 0, "the match is attributed to DBREAK slot 0");
    assert_eq!(
        run.slot(1),
        run.slot(7),
        "THE ACCESS DID NOT HAPPEN: the guarded word still held its pre-store \
         value ({:#010x}) when the handler read it back. A watchpoint that \
         traps AFTER the store is a watchpoint that cannot guard a stack.",
        run.slot(7)
    );
    assert_ne!(
        run.slot(6),
        run.slot(7),
        "the fixture's store would have changed the word (otherwise the \
         assertion above proves nothing)"
    );
}

// ===========================================================================
// (g) load/store error
// ===========================================================================

// Slots: 1 = EXCCAUSE, 2 = EXCVADDR, 3 = the vector entry PC, 4 = EPC1,
// 5 = the address the fixture faulted on, 6 = handler ran.

fn lserr_run() -> Option<(Run, String)> {
    let run = run("mach_lserr", &Script::empty())?;
    let mut s = header(&run);
    let _ = writeln!(s, "handler ran: {}", run.slot(6));
    let _ = writeln!(s, "exccause: {}", run.slot(1));
    let _ = writeln!(s, "excvaddr == faulting address: {}", run.slot(2) == run.slot(5));
    let _ = writeln!(s, "faulting address: {:#010x}", run.slot(5));
    let _ = writeln!(s, "vector: {}", run.symbols.resolve(run.slot(3)));
    let _ = writeln!(s, "vector offset: {:#05x}", run.slot(3) - VECBASE);
    let _ = writeln!(s, "epc1 in: {}", run.symbols.resolve(run.slot(4)));
    Some((run, s))
}

#[test]
fn mach_load_store_error_excvaddr() {
    let Some((run, _)) = lserr_run() else {
        return;
    };
    assert_eq!(run.slot(6), 1, "the exception handler ran exactly once");
    assert_eq!(run.slot(1), 3, "EXCCAUSE = 3 (LoadStoreError)");
    assert_eq!(
        run.slot(2),
        run.slot(5),
        "EXCVADDR ({:#010x}) is the faulting address ({:#010x})",
        run.slot(2),
        run.slot(5)
    );
    assert_eq!(
        run.slot(3) - VECBASE,
        0x340,
        "entered _UserExceptionVector at VECBASE + 0x340"
    );
    assert_eq!(
        run.symbols.resolve(run.slot(4)),
        "fault_load",
        "EPC1 ({:#010x}) names the faulting instruction",
        run.slot(4)
    );
}

// ===========================================================================
// Goldens and the anti-skip guard
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

/// Kept honest against the runner's own list: a fixture added to `FIXTURES`
/// with no golden, or a golden with no fixture, is a silence.
#[test]
fn every_fixture_has_a_golden_and_every_golden_a_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/mach");
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
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

/// Cheap sanity on the runner itself: a fixture that finished must have spent
/// real cycles, and the memory it wrote must still read back.
#[test]
fn mach_runs_are_bounded_and_deterministic() {
    let Some(a) = run("mach_loopnez", &Script::empty()) else {
        return;
    };
    let b = run("mach_loopnez", &Script::empty()).expect("second run");
    assert_eq!(a.cycles, b.cycles, "two runs of one fixture cost the same");
    assert_eq!(a.slots, b.slots, "two runs of one fixture agree");
    assert!(a.cycles > 0 && a.cycles < CYCLE_LIMIT);
    // The bus outlives the run; reading it back is what every assertion above
    // depends on.
    assert!(a.mem.read_u32(SRAM1_BASE).is_ok());
}
