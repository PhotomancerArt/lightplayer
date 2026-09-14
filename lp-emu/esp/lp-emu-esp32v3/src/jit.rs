//! The classic's translated core: `lp-xt-jit`'s module, wired to this
//! machine's hart and bus.
//!
//! The C6's `jit.rs` is the twin, and the shape is deliberately the same one:
//! a [`V3Ops`] serving the four imports out of raw hart and bus pointers that
//! are live for exactly one stay, an [`XtJitCore`] implementing
//! [`TranslatedCore`] with a list of refusals rather than a list of
//! assertions, and an [`install`] that walks, emits, compiles and instantiates
//! once per core.
//!
//! # Two harts, two instances, one module
//!
//! The classic has two cores and they run the same image. A module is
//! **compiled once and instantiated once per core** (XD10), because the
//! exchange area is per instance and a stay belongs to one hart. `install`
//! is therefore called per hart and the engine's compilation cost is paid
//! twice in this phase — the shared-compilation path is P07's, with the
//! publish-by-store event that makes more than one module per core possible.
//!
//! # What this phase installs
//!
//! `lp-xt-jit`'s escape-everything module: every guest instruction goes back
//! to [`XtHart::step_one`]. It is slower than the interpreter and it is not a
//! product path. What it proves is that a stay leaves the machine byte for
//! byte where the interpreter would have — `scripts/emu/v3-oracle.sh
//! --flags-a --jit --flags-b --interpreter`, every column `same`.
//!
//! # The refusals are the design
//!
//! Every condition below is a **refusal**, never an assertion: refusing hands
//! the slice back to the interpreter, which is always correct and only ever
//! slow. A translated core that cannot be exact about something does not try.

use std::collections::BTreeMap;

use lp_emu_core::Bus;
use lp_emu_esp_common::bus::{PERMISSION_PAGE_LEN, SocBus};
use lp_xt_emu::mach::translated::{RunOutcome, TranslatedCore};
use lp_xt_emu::mach::{SliceEnd, XtHart};
use lp_xt_jit::blocks::BlockSet;
use lp_xt_jit::lp_emu_jit::host::{
    self, EXCHANGE_LEN, ExchangeLayout, HostOps, MmioLoad, MmioStore, PERM_ENTRIES, PERM_NONE,
    PERM_SHIFT, Polled, STEP_CONTINUE, STEP_SLICE_ENDED, StepOne,
};
use lp_xt_jit::lp_emu_jit::host_wasmtime::WasmtimeCore;
use lp_xt_jit::lp_emu_jit::translate::Layout;

/// How many why-codes the report buckets exits into.
const WHY_CODES: usize = 16;

// ---- the host -------------------------------------------------------------

/// The four imports, served out of this machine's hart and bus.
pub struct V3Ops {
    hart: *mut XtHart<SocBus>,
    bus: *mut SocBus,
    /// The exchange area, inside the arena. Stable for the life of the
    /// machine, because the arena is.
    exchange: *mut u8,
    /// How long that area is — Xtensa's layout, not RV32's.
    exchange_len: usize,
    /// The slice end the escape hatch reported, if any.
    slice_end: Option<SliceEnd>,
    escape_hatch: u64,
}

// SAFETY: the pointers are only dereferenced from inside a
// `WasmtimeCore::enter` call on the thread that set them, and `XtJitCore::run`
// clears them before it returns. `Send` is wasmtime's requirement on a store's
// data, not a claim that this may be shared.
unsafe impl Send for V3Ops {}

impl V3Ops {
    /// # Safety
    ///
    /// Only valid between [`XtJitCore::run`] setting the pointers and clearing
    /// them, which is exactly the window translated code runs in.
    unsafe fn parts(&mut self) -> (&mut XtHart<SocBus>, &mut SocBus) {
        debug_assert!(!self.hart.is_null() && !self.bus.is_null());
        // SAFETY: the caller's obligation, above.
        unsafe { (&mut *self.hart, &mut *self.bus) }
    }
}

impl HostOps for V3Ops {
    fn layout(&self) -> ExchangeLayout {
        lp_xt_jit::LAYOUT
    }

    /// The escape hatch, and **the only import this phase's module calls**.
    ///
    /// No register file crosses. `step_one_wide` exists for exactly this case
    /// (M7 XD6): the AR file is the hart's, in the hart's own physical order,
    /// and a translated stay that cached none of it has nothing to marshal.
    /// The counters do cross, at every point the bus can observe them (JD17) —
    /// the interpreter is about to read `CCOUNT` off them.
    fn step_one_wide(&mut self, pc: u32, cycle: u64, instret: u64) -> StepOne {
        self.escape_hatch += 1;
        // SAFETY: called from inside `enter`.
        let (hart, bus) = unsafe { self.parts() };
        hart.set_pc(pc);
        hart.set_counters(cycle, instret);
        let ended = hart.step_one(bus);
        let out = StepOne {
            pc: hart.pc(),
            cycle: hart.cycle_count(),
            instret: hart.instruction_count(),
            status: if ended.is_some() {
                STEP_SLICE_ENDED
            } else {
                STEP_CONTINUE
            },
        };
        self.slice_end = ended;
        out
    }

    fn step_one(&mut self, _pc: u32, _cycle: u64, _instret: u64, _regs: &mut [i32; 32]) -> StepOne {
        unreachable!(
            "an Xtensa host escapes through `step_one_wide`: the AR file is 64 physical \
             registers and the hart owns it"
        )
    }

    /// An access on a page the permission table does not call plain RAM.
    ///
    /// Nothing this phase emits calls it — every instruction escapes, so every
    /// access is the interpreter's — and it is written anyway because it is
    /// the one of the three that can be written exactly with what `XtHart` and
    /// `SocBus` make public, and P06's emitted loads are its first caller.
    fn mmio_load(&mut self, pc: u32, cycle: u64, address: u32, kind: u32) -> MmioLoad {
        // SAFETY: called from inside `enter`.
        let (_, bus) = unsafe { self.parts() };
        // The exact `(pc, cycle)` the interpreter would have set before the
        // access, because peripheral models read both (JD17).
        bus.set_issuing(pc, cycle);
        let read = match kind {
            host::load_kind::W => bus.read_word(address).map(|v| v as u32),
            host::load_kind::H => bus.read_halfword(address).map(|v| i32::from(v) as u32),
            host::load_kind::HU => bus.read_halfword(address).map(|v| u32::from(v as u16)),
            host::load_kind::B => bus.read_byte(address).map(|v| i32::from(v) as u32),
            host::load_kind::BU => bus.read_u8(address).map(u32::from),
            other => unreachable!("{other} is not a load kind the protocol defines"),
        };
        match read {
            Ok(value) => MmioLoad {
                value,
                status: if bus.sideband_or_yield_pending() {
                    host::MMIO_PENDING
                } else {
                    host::MMIO_OK
                },
            },
            // The interpreter re-runs the instruction and traps at exactly the
            // pc it always trapped at.
            Err(_) => MmioLoad {
                value: 0,
                status: host::MMIO_REFUSED,
            },
        }
    }

    /// # Panics
    ///
    /// Always, and deliberately — see [`poll`](V3Ops::poll).
    fn mmio_store(
        &mut self,
        _pc: u32,
        _cycle: u64,
        _address: u32,
        _kind: u32,
        _value: u32,
        _post_pc: u32,
        _post_cycle: u64,
        _post_instret: u64,
    ) -> MmioStore {
        unreachable!("{POLL_GAP}")
    }

    /// # Panics
    ///
    /// Always, and deliberately.
    ///
    /// **Polling point (c) is not reachable from outside `lp-xt-emu`.** The
    /// hart runs it as `take_sideband` → `resample_external` → the code-dirty
    /// drain → `take_yield`, and the middle two are private
    /// (`XtHart::resample_external`, `XtHart::drain_code_dirty`). The plan's
    /// "one polling point, one copy" rule forbids writing a second copy here,
    /// and the right answer — one public method on `XtHart` that runs the
    /// hart's own three lines — is a change to `lp-xt-emu`, which is not this
    /// phase's to make.
    ///
    /// It costs this phase nothing: with every instruction escaping, poll
    /// point (c) runs inside `XtHart::step` where it always has. P06's first
    /// inline store is what needs it, and it is a stop for P06 rather than a
    /// gap this phase can paper over.
    fn poll(&mut self, _pc: u32, _cycle: u64, _instret: u64) -> Polled {
        unreachable!("{POLL_GAP}")
    }

    fn exchange(&mut self) -> &mut [u8] {
        // SAFETY: `exchange` points at `self.exchange_len` bytes inside the
        // bus's arena, which is allocated once and never moves, and which no
        // region covers — see `areas`.
        unsafe { core::slice::from_raw_parts_mut(self.exchange, self.exchange_len) }
    }
}

const POLL_GAP: &str = "this phase emits no inline store, so nothing fuses a polling point; \
                        poll point (c) needs a public `XtHart` method running the hart's own \
                        three lines before P06 can emit one";

// ---- placing the tables in the arena ---------------------------------------

/// Where the translator's tables sit inside the guest arena.
#[derive(Clone, Copy, Debug)]
struct Areas {
    perm_at: u32,
    exchange_at: u32,
    pages: u64,
    gap_len: u32,
    gap_left: u32,
}

/// Place the permission table and the exchange area in the arena's largest
/// gap, and say how much of the arena a wasm memory can cover.
///
/// No indirect-target tables: this phase emits no indirect resolution, so the
/// page map and its slot arrays are not built and not paid for. At Xtensa's
/// byte granularity they are **twice** the RV32 size per covered page
/// (`SLOT_SHIFT = 0`), which is a number P05 will have to place rather than
/// one this phase should reserve blind.
///
/// The exchange area is per **instance**, and the two harts get two of them
/// side by side in the gap — a stay belongs to one hart and its counters are
/// that hart's.
fn areas(bus: &SocBus, instances: u32) -> Result<Areas, String> {
    let arena_len = bus.guest_arena().len();
    let pages = (arena_len / 65536) as u64;
    if pages == 0 {
        return Err("the guest arena is smaller than one wasm page".into());
    }
    let exchange_len = lp_xt_jit::LAYOUT.len();
    let need = u64::from(PERM_ENTRIES) + u64::from(exchange_len) * u64::from(instances);
    let (gap_base, gap_len) = bus
        .largest_arena_gap()
        .ok_or_else(|| "the guest arena has no gap for the translator's tables".to_string())?;
    if u64::from(gap_len) < need {
        return Err(format!(
            "the arena's largest gap is {gap_len} bytes and this core's tables need {need}"
        ));
    }
    let perm_at = gap_base - bus.guest_arena_base();
    let exchange_at = perm_at + PERM_ENTRIES;
    if u64::from(exchange_at) + u64::from(exchange_len) * u64::from(instances) > pages * 65536 {
        return Err("the translator's tables fall outside the wasm memory".into());
    }
    Ok(Areas {
        perm_at,
        exchange_at,
        pages,
        gap_len,
        gap_left: gap_len - (need as u32),
    })
}

/// Write the permission table into the arena.
///
/// The bus's table is one byte per 16 KiB page **of the arena**; emitted code
/// indexes one byte per 16 KiB page of the whole 32-bit guest space, so it
/// needs no bounds compare. This is where one becomes the other, and it is
/// also where the pages a wasm memory cannot reach are taken back to
/// [`PERM_NONE`].
///
/// Nothing this phase emits reads it. It is written anyway because the
/// [`Layout`] promises it and a half-built table found by P06's first emitted
/// load would be a bug with no symptom until then.
fn write_permission_table(bus: &mut SocBus, at: Areas) {
    let table = bus.permission_table();
    let base = bus.guest_arena_base();
    let reachable = at.pages * 65536;
    let arena = bus.guest_arena_mut();
    arena[at.perm_at as usize..][..PERM_ENTRIES as usize].fill(PERM_NONE);
    for (page, &perm) in table.iter().enumerate() {
        if perm == PERM_NONE {
            continue;
        }
        let offset = (page as u64) * u64::from(PERMISSION_PAGE_LEN);
        if offset + u64::from(PERMISSION_PAGE_LEN) > reachable {
            continue;
        }
        let entry = ((u64::from(base) + offset) >> PERM_SHIFT) as usize;
        arena[at.perm_at as usize + entry] = perm;
    }
}

// ---- the build report ------------------------------------------------------

/// What one translation event cost and produced.
#[derive(Clone, Debug)]
pub struct BuildReport {
    pub seeds: usize,
    pub blocks: usize,
    pub insts: usize,
    pub escaped_insts: usize,
    pub module_bytes: usize,
    pub functions: usize,
    pub fn_blocks: usize,
    pub max_body_bytes: usize,
    pub discover_us: u128,
    pub emit_us: u128,
    pub compile_us: u128,
    pub instantiate_us: u128,
    pub gap_len: u32,
    pub gap_left: u32,
}

impl BuildReport {
    /// The boot-cost line: discover / emit / compile / instantiate, and what
    /// the module is.
    ///
    /// A **product number**, not a diagnostic: on the browser side it is time
    /// between the tab opening and the first frame, and JD20 asks every
    /// translation event to say what it cost rather than leaving the first
    /// press to absorb it.
    #[must_use]
    pub fn boot_line(&self, core: usize, event: &str) -> String {
        format!(
            "core{core} {event}: {} block(s) from {} seed(s), {} instruction(s) ({} escaped, 0 \
             emitted natively); {} function(s) at {} blocks each, largest body {} B, module {} B; \
             discover {:.1} ms, emit {:.1} ms, compile {:.1} ms, instantiate {:.1} ms; arena gap \
             {} B, {} B left",
            self.blocks,
            self.seeds,
            self.insts,
            self.escaped_insts,
            self.functions,
            self.fn_blocks,
            self.max_body_bytes,
            self.module_bytes,
            self.discover_us as f64 / 1000.0,
            self.emit_us as f64 / 1000.0,
            self.compile_us as f64 / 1000.0,
            self.instantiate_us as f64 / 1000.0,
            self.gap_len,
            self.gap_left,
        )
    }
}

/// What a run of a translated core did.
#[derive(Clone, Debug, Default)]
struct Stats {
    entries: u64,
    retired: u64,
    escape_hatch: u64,
    refused_no_entry: u64,
    refused_pending: u64,
    refused_watch: u64,
    refused_impure: u64,
    refused_timer: u64,
    refused_stale: u64,
    exit_slice_ended: u64,
    exit_no_progress: u64,
    /// Invalidations answered by re-reading the guest bytes and finding them
    /// unchanged — the common case, and the reason a loop-register write is
    /// not fatal.
    verified: u64,
    verify_failed: u64,
    why: [u64; WHY_CODES],
}

// ---- the core --------------------------------------------------------------

/// One core's installed module, and the entry index into it.
pub struct XtJitCore {
    core: WasmtimeCore<V3Ops>,
    /// Guest pc to block index. The hart's own byte-indexed entry table says
    /// *whether* a pc is an entry; this says which block it is.
    index: BTreeMap<u32, u32>,
    which: usize,
    /// The cost model the budget checks were emitted against.
    model: lp_emu_core::CycleModel,
    /// What installed this core: `boot`, or `app-core release` for the core
    /// the app core gets when DPORT hands it a fresh hart. Carried so the
    /// report names the event rather than assuming every core came from the
    /// boot one.
    event: String,
    report: BuildReport,
    stats: Stats,
    /// Set when the guest changed code this module was translated from, or
    /// when the cost model moved under it. A stale module is never entered
    /// again: retranslation is P07's.
    stale: bool,
    /// An invalidation arrived and the bytes have not been re-checked yet.
    ///
    /// Lazy on purpose, exactly as the C6's is: an invalidation is raised at
    /// the store, and the answer is only needed at the next **entry**. A run
    /// that never re-enters this core never pays for the check.
    verify_pending: bool,
    /// The guest bytes each block was translated from.
    ///
    /// This is what makes an invalidation answerable rather than fatal. Most
    /// of them are not about this module's code at all: `invalidate(None)` is
    /// raised by an `isync`, by a host-side write **and** by a `wsr`/`xsr` to
    /// `LBEG`/`LEND`/`LCOUNT`, and the last is by far the most common —
    /// `restore_context` writes all three on every context switch, which on
    /// `boot-idle` is hundreds of events in a hundred milliseconds. A core
    /// that went permanently stale on the first one would be installed and
    /// then never entered again, and a coverage line reading three
    /// instructions would be reporting a bug as a measurement.
    ///
    /// The loop registers cannot change what this module is: no block folds
    /// them in, and a loop back-edge is caught by the straight-on check like
    /// any other divergence. Guest **bytes** are the only thing that can, and
    /// those are what this compares.
    code: Vec<(u32, Box<[u8]>)>,
}

impl XtJitCore {
    /// Are the guest bytes still the ones this module was translated from?
    ///
    /// A module that answers no is stale for the rest of the run: an
    /// incremental retranslation over the changed spans is P07's
    /// publish-by-store event, and until then the interpreter carries what
    /// this core gives up, which is slow and exact.
    fn verify(&mut self, bus: &SocBus) {
        self.verify_pending = false;
        let base = bus.guest_arena_base();
        let arena = bus.guest_arena();
        for (pc, bytes) in &self.code {
            let at = (u64::from(*pc) - u64::from(base)) as usize;
            let now = arena.get(at..at + bytes.len());
            if now != Some(&bytes[..]) {
                self.stats.verify_failed += 1;
                self.stale = true;
                return;
            }
        }
        self.stats.verified += 1;
    }

    /// The pcs the hart's entry table should be built from.
    #[must_use]
    pub fn entries(&self) -> Vec<u32> {
        self.index.keys().copied().collect()
    }

    /// The boot-cost line for this core's one translation event.
    #[must_use]
    pub fn boot_line(&self) -> String {
        self.report.boot_line(self.which, &self.event)
    }
}

impl TranslatedCore<SocBus> for XtJitCore {
    fn run(&mut self, hart: &mut XtHart<SocBus>, bus: &mut SocBus, end: u64) -> RunOutcome {
        if self.stale {
            self.stats.refused_stale += 1;
            return RunOutcome::Refused;
        }
        if self.verify_pending {
            self.verify(bus);
            if self.stale {
                self.stats.refused_stale += 1;
                return RunOutcome::Refused;
            }
        }
        // The block set's budget checks have the cost model folded in as
        // constants, so a model that changed since emission is not something
        // the byte check above would catch.
        if hart.cycle_model() != self.model {
            log::warn!("jit: the cost model changed under a translated core; interpreting");
            self.stale = true;
            self.stats.refused_stale += 1;
            return RunOutcome::Refused;
        }
        let pc = hart.pc();
        let Some(&entry) = self.index.get(&pc) else {
            self.stats.refused_no_entry += 1;
            return RunOutcome::Refused;
        };

        // The refusals, in the order the C6's spike learned them.
        //
        // A pending side-band or yield must be observed at exactly the store
        // that raised it, and translated code cannot re-raise one it did not
        // make.
        if bus.sideband_or_yield_pending() {
            self.stats.refused_pending += 1;
            return RunOutcome::Refused;
        }
        // Translated code does accesses the bus never sees, so a load
        // watchpoint could not fire. (Nothing this phase emits does — every
        // access is the interpreter's — and the refusal stands anyway: it is
        // the rule P06 needs to already be here.)
        if bus.load_watchpoints_armed() {
            self.stats.refused_watch += 1;
            return RunOutcome::Refused;
        }
        if matches!(bus.store_watch(), lp_emu_core::StoreWatch::Many) {
            self.stats.refused_watch += 1;
            return RunOutcome::Refused;
        }
        let watch = match bus.store_watch() {
            lp_emu_core::StoreWatch::One { lo, hi } => (lo, hi),
            _ => (0, 0),
        };
        // An inline access charges nothing and fires no execute watchpoint, so
        // a bus that would have done either is one this core cannot be exact
        // on.
        if !bus.fetch_is_pure() {
            self.stats.refused_impure += 1;
            return RunOutcome::Refused;
        }
        // A `CCOMPARE` match inside the stay: the hart ticks its timers after
        // a stay, not during one, so a stay that would have run past a match
        // would deliver the interrupt late. Refusing hands those cycles to the
        // interpreter, which checks per instruction.
        if hart.next_timer_cycle().is_some_and(|at| at < end) {
            self.stats.refused_timer += 1;
            return RunOutcome::Refused;
        }

        self.stats.entries += 1;
        let (cycle, instret) = (hart.cycle_count(), hart.instruction_count());
        {
            let ops = self.core.ops_mut();
            ops.slice_end = None;
            ops.hart = hart;
            ops.bus = bus;
        }
        let entered = self.core.enter(entry, cycle, instret, end, watch);
        let ops = self.core.ops_mut();
        ops.hart = core::ptr::null_mut();
        ops.bus = core::ptr::null_mut();
        let escaped = core::mem::take(&mut ops.escape_hatch);
        self.stats.escape_hatch += escaped;
        let slice_end = ops.slice_end.take();

        let exit_pc = match entered {
            Ok(exit) => exit.pc,
            // The emitted module cannot trap on any path this translator
            // emits, so a trap is a translator bug. Saying so and refusing is
            // the only answer that keeps the transcript right; the run
            // continues interpreted.
            Err(e) => {
                log::error!("jit: translated code trapped at {pc:#010x}: {e}");
                self.stale = true;
                return RunOutcome::Refused;
            }
        };

        // The flags are read at **this layout's** offset. `WasmtimeCore::enter`
        // reads `exit.flags` at RV32's fixed `EXCHANGE_FLAGS`, which past a
        // 64-word register file is inside the AR file rather than the flags
        // word. Nothing else in either host is layout-shaped.
        let x = self.core.ops_mut().exchange();
        let read_i64 = |x: &[u8], at: u64| {
            u64::from_le_bytes(x[at as usize..][..8].try_into().expect("eight bytes"))
        };
        let read_i32 = |x: &[u8], at: u64| {
            i32::from_le_bytes(x[at as usize..][..4].try_into().expect("four bytes"))
        };
        let cycle_count = read_i64(x, lp_xt_jit::LAYOUT.cycle());
        let instruction_count = read_i64(x, lp_xt_jit::LAYOUT.instret());
        let flags = read_i32(x, lp_xt_jit::LAYOUT.flags());
        let exit_why = read_i32(x, lp_xt_jit::LAYOUT.exit_why());

        // The module leaves the hart's own pc and counters where the exchange
        // area says, so the outcome and the hart agree.
        hart.set_pc(exit_pc);
        hart.set_counters(cycle_count, instruction_count);
        self.stats.retired += instruction_count.saturating_sub(instret);
        self.stats.why[(exit_why as usize).min(WHY_CODES - 1)] += 1;
        if instruction_count == instret {
            self.stats.exit_no_progress += 1;
        }

        if flags & host::FLAG_SLICE_ENDED != 0 {
            self.stats.exit_slice_ended += 1;
            let end = slice_end.unwrap_or_else(|| {
                unreachable!("the module reported a slice end and the host recorded none")
            });
            return RunOutcome::Ended {
                pc: hart.pc(),
                cycle_count: hart.cycle_count(),
                instruction_count: hart.instruction_count(),
                end,
            };
        }
        RunOutcome::Ran {
            pc: hart.pc(),
            cycle_count: hart.cycle_count(),
            instruction_count: hart.instruction_count(),
            // **False, always, in this phase.** `after_store` asks the hart to
            // run polling point (c) after the stay; with every instruction
            // escaping, `XtHart::step` has already run it after each Store-,
            // Atomic- or System-class instruction, at exactly the instruction
            // that raised it. Saying `true` here would run the hart's three
            // lines a second time, at the wrong pc. P06's first inline store
            // is what sets it, through the flag the protocol already has.
            after_store: false,
        }
    }

    fn invalidate(&mut self, _range: Option<(u32, u32)>) {
        // Noted, not acted on: the answer is only needed at the next entry,
        // and most invalidations are not about this module's bytes at all
        // (see `XtJitCore::code`). The range is ignored because the check is
        // the whole module either way — with no retranslation to do (P07),
        // knowing *which* blocks moved buys nothing.
        self.verify_pending = true;
    }

    fn report(&self) -> String {
        let s = &self.stats;
        let entered = s.retired;
        let refused = s.refused_no_entry
            + s.refused_pending
            + s.refused_watch
            + s.refused_impure
            + s.refused_timer
            + s.refused_stale;
        format!(
            "core{}: {} entries, {} instruction(s) inside translated code ({} escaped to the \
             interpreter, 0 retired natively); {} refusal(s) (no-entry {}, pending {}, watch {}, \
             impure {}, timer {}, stale {}); {} invalidation(s) answered by re-reading the \
             bytes, {} that found them changed; {} slice end(s), {} no-progress exit(s); {}",
            self.which,
            s.entries,
            entered,
            s.escape_hatch,
            refused,
            s.refused_no_entry,
            s.refused_pending,
            s.refused_watch,
            s.refused_impure,
            s.refused_timer,
            s.refused_stale,
            s.verified,
            s.verify_failed,
            s.exit_slice_ended,
            s.exit_no_progress,
            self.boot_line(),
        )
    }
}

// ---- installing ------------------------------------------------------------

/// Translate the blocks reachable from `seeds` and install a core on `hart`.
///
/// `seeds` is a **supplied** list of block starts — `--jit-seeds` — because
/// the real sweep is P05's. Nothing it produces can be wrong, only short: a pc
/// no seed reaches is a pc the interpreter runs.
///
/// # Errors
///
/// A block set that does not fit the arena's shape, a module over the
/// per-function byte budget (use fewer `--jit-fn-blocks`), or a module the
/// engine will not compile.
pub fn install(
    hart: &mut XtHart<SocBus>,
    bus: &mut SocBus,
    which: usize,
    seeds: &[u32],
    max_blocks: usize,
    fn_blocks: usize,
    event: &str,
) -> Result<XtJitCore, String> {
    let at = areas(bus, 2)?;
    write_permission_table(bus, at);

    // A **pure** read of the guest's own bytes: this runs before the guest
    // reaches any of these addresses, so a fetch that charged a cycle or fired
    // a watchpoint would be a change the guest can see. Straight out of the
    // arena for exactly that reason.
    let started = std::time::Instant::now();
    let arena_base = bus.guest_arena_base();
    let found = {
        let arena = bus.guest_arena();
        let mut fetch = |pc: u32| {
            pc.checked_sub(arena_base)
                .and_then(|o| arena.get(o as usize))
                .copied()
        };
        let mut starts: Vec<u32> = seeds.to_vec();
        starts.sort_unstable();
        starts.dedup();
        starts.truncate(max_blocks);
        lp_xt_jit::discover::build(&starts, &mut fetch)
    };
    let discover_us = started.elapsed().as_micros();
    if found.set.is_empty() {
        return Err(format!(
            "none of the {} seed(s) named an address with a decodable instruction",
            seeds.len()
        ));
    }

    let arena_len = bus.guest_arena().len();
    let arena_ptr = bus.guest_arena_mut().as_mut_ptr();
    // Natively the module's memory *is* the arena — `host_wasmtime` hands the
    // engine an alias of it — so the arena starts at offset zero. In the
    // browser (P08) the module imports the emulator's whole linear memory and
    // every folded base shifts by the arena's address inside it.
    #[cfg(target_family = "wasm")]
    let (mem_base, memory_pages) = (arena_ptr as u32, core::arch::wasm32::memory_size(0) as u64);
    #[cfg(not(target_family = "wasm"))]
    let (mem_base, memory_pages) = (0u32, at.pages);

    // One exchange area per instance, side by side in the gap.
    let exchange_at = at.exchange_at + lp_xt_jit::LAYOUT.len() * which as u32;
    let layout = Layout {
        memory_pages,
        guest_base: arena_base,
        arena_offset: mem_base,
        perm_offset: mem_base + at.perm_at,
        exchange_offset: mem_base + exchange_at,
        // This phase emits no indirect resolution and no published reads.
        indirect: None,
        fast_reads: None,
    };

    let model = hart.cycle_model();
    let started = std::time::Instant::now();
    let emitted = lp_xt_jit::translate::emit_module(&found.set, model, layout, fn_blocks);
    let emit_us = started.elapsed().as_micros();
    // wasm's implementation limit applies per **function**, not per module, and
    // the split is what keeps every function under it. Checked here rather than
    // left to the engine, because every engine reports it differently, none say
    // what to do about it, and the answer — a smaller `fn_blocks` — is one the
    // caller can act on.
    if emitted.max_body_bytes > lp_xt_jit::lp_emu_jit::dispatch::BODY_BUDGET {
        return Err(format!(
            "at {fn_blocks} blocks per function the largest body is {} bytes, over the {}-byte \
             budget; use fewer (--jit-fn-blocks)",
            emitted.max_body_bytes,
            lp_xt_jit::lp_emu_jit::dispatch::BODY_BUDGET,
        ));
    }

    // The guest bytes behind each block, so an invalidation can be answered by
    // asking whether they changed rather than by giving up. See
    // `XtJitCore::code`.
    let code: Vec<(u32, Box<[u8]>)> = {
        let arena = bus.guest_arena();
        found
            .set
            .blocks
            .iter()
            .map(|b| {
                let at = (u64::from(b.pc) - u64::from(arena_base)) as usize;
                let len = b.end_pc().wrapping_sub(b.pc) as usize;
                let bytes = arena
                    .get(at..at + len)
                    .map_or_else(|| Vec::new().into_boxed_slice(), |b| b.to_vec().into());
                (b.pc, bytes)
            })
            .collect()
    };

    let arena_guard = bus.guest_arena_guard();
    let exchange = bus.guest_arena_mut()[exchange_at as usize..].as_mut_ptr();
    let ops = V3Ops {
        hart: core::ptr::null_mut(),
        bus: core::ptr::null_mut(),
        exchange,
        exchange_len: lp_xt_jit::LAYOUT.len() as usize,
        slice_end: None,
        escape_hatch: 0,
    };
    // SAFETY: `arena_ptr` is the bus's own arena, allocated once during
    // construction from the chip's declared memory map and documented as not
    // moving for the life of the machine; the memory wasmtime is given covers
    // only whole wasm pages of it. Nothing holds a Rust reference into the
    // arena while translated code runs: `XtJitCore::run` reaches the bus
    // through the raw pointer it just set and does not touch it otherwise.
    // `arena_guard` is the arena's own report: when it is `Some`, the bytes
    // past `arena_len` really are unmapped out to the end of the reservation
    // and its guard.
    let core = unsafe { WasmtimeCore::new(&emitted.wasm, ops, arena_ptr, arena_len, arena_guard) }
        .map_err(|e| format!("the translated module did not build: {e:?}"))?;

    let report = BuildReport {
        seeds: seeds.len(),
        blocks: found.set.blocks.len(),
        insts: found.stats.insts,
        escaped_insts: emitted.escaped_insts,
        module_bytes: emitted.wasm.len(),
        functions: emitted.functions,
        fn_blocks: fn_blocks.min(found.set.blocks.len()),
        max_body_bytes: emitted.max_body_bytes,
        discover_us,
        emit_us,
        compile_us: core.compile_us(),
        instantiate_us: core.instantiate_us(),
        gap_len: at.gap_len,
        gap_left: at.gap_left,
    };
    Ok(XtJitCore {
        core,
        index: index_of(&found.set),
        which,
        model,
        event: event.to_string(),
        report,
        stats: Stats::default(),
        stale: false,
        verify_pending: false,
        code,
    })
}

fn index_of(set: &BlockSet) -> BTreeMap<u32, u32> {
    set.blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.pc, i as u32))
        .collect()
}

/// The exchange area the RV32 protocol sizes, for the assertion below.
const _: () = assert!(
    lp_xt_jit::LAYOUT.len() > EXCHANGE_LEN,
    "the Xtensa exchange area is longer than RV32's; a host that sized it by the RV32 constant \
     would hand translated code a short slice"
);
