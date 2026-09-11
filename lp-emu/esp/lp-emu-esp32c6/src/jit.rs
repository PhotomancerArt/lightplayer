//! The glue between `lp-emu-jit` and this machine.
//!
//! `lp-emu-jit` owns the decoder, the translator, the emitted module and the
//! wasmtime host, and by JD2 it may not name a hart or a bus. `lp-riscv-emu`
//! owns the hart and the `TranslatedCore` seam, and by design it contains no
//! translator. Something has to see both, and this is it: the crate that
//! already owns the machine, inside the MIT fence, adding no AGPL edge.
//!
//! Two halves:
//!
//! - [`JitCore`] implements `lp_riscv_emu::mach::translated::TranslatedCore`.
//!   It holds the refusal rules, maps an entry pc to a block index, hands the
//!   hart's registers to the module and takes them back, and turns an exit
//!   into a `RunOutcome`.
//! - [`C6Ops`] implements `lp_emu_jit::host::HostOps`. It is what translated
//!   code calls back into: the two MMIO imports and the escape hatch.
//!
//! # Where the tables live, and why that is not a hack
//!
//! A translated module addresses guest RAM, its permission table and its host
//! exchange area as offsets in **one** linear memory, because that is what the
//! browser gives it for free: there the emulator is itself a wasm module and
//! all three are ordinary allocations inside its own memory. Natively there is
//! no such memory, so one has to be made to alias the bus's arena — and the
//! other two have to be inside it.
//!
//! They go in the arena's largest **gap**: the C6's regions leave ~7.7 MiB
//! between the mask ROM's data and HP SRAM, which the arena spans (it is flat
//! across region boundaries, JD4) and the bus never serves. A guest access
//! there has no region, so the permission table gives it zero, so translated
//! code sends it out to the bus, which faults it exactly as it always has.
//! Nothing is copied and nothing about the guest's view of memory changes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lp_emu_core::{Bus, CycleModel};
use lp_emu_esp_common::bus::{PERM_NONE, PERM_READ, PERM_READ_WRITE, PERMISSION_PAGE_LEN, SocBus};
use lp_emu_jit::blocks::BlockSet;
use lp_emu_jit::discover::{DiscoverStats, discover};
use lp_emu_jit::dispatch::{BODY_BUDGET, emit_module, target_table_bytes, write_target_tables};
use lp_emu_jit::host::{
    self, EXCHANGE_LEN, FLAG_AFTER_STORE, FLAG_SLICE_ENDED, HostOps, MMIO_LEAVE_AFTER, MMIO_OK,
    MMIO_PENDING, MMIO_REFUSED, MmioLoad, MmioStore, PERM_ENTRIES, PERM_SHIFT, STEP_CONTINUE,
    STEP_SLICE_ENDED, StepOne, load_kind, store_kind,
};
use lp_emu_jit::translate::{Emit, Emitted, Layout};

// Which host runs the emitted module, chosen by target and by nothing else.
//
// The two have the same surface on purpose — `new`, `enter`, `compile_us`,
// `instantiate_us`, `module_bytes`, `ops_mut` — so everything below this line
// is one code path. `WasmtimeCore` exists so identity can be proven on the
// desk (JD18); `BrowserCore` is the product host (JD11–JD13), and on a wasm
// target it is not a choice: it is the only thing that can run a module.
#[cfg(target_family = "wasm")]
use lp_emu_jit::host_browser::BrowserCore as HostCore;
#[cfg(not(target_family = "wasm"))]
use lp_emu_jit::host_wasmtime::WasmtimeCore as HostCore;
use lp_riscv_emu::mach::translated::{RunOutcome, TranslatedCore};
use lp_riscv_emu::mach::{MachineHart, SliceEnd};

use crate::jit_record::{CallRec, EntryRec, Recorder};

/// Read the guest word at `pc` out of the arena, without touching the bus.
///
/// The sweep runs before the guest reaches any of these addresses, so it must
/// charge nothing and trap nothing — which rules out `Bus::fetch_instruction`.
/// A word that would run off the end of its region is served as far as the
/// region goes: two bytes are enough for a compressed encoding, and a
/// four-byte one that does not fit ends the block, which is the right answer.
fn arena_word(arena: &[u8], base: u32, spans: &[(u32, u32, bool)], pc: u32) -> Option<u32> {
    let region = spans
        .iter()
        .find(|&&(b, len, _)| pc >= b && u64::from(pc) < u64::from(b) + u64::from(len))?;
    let room = (u64::from(region.0) + u64::from(region.1) - u64::from(pc)).min(4) as usize;
    let at = usize::try_from(u64::from(pc) - u64::from(base)).ok()?;
    let bytes = arena.get(at..at + room)?;
    match room {
        4 => Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        2 | 3 => Some(u32::from(u16::from_le_bytes([bytes[0], bytes[1]]))),
        _ => None,
    }
}

/// The smallest block budget worth retrying at. Below this the set is too
/// small to be evidence of anything, and a failure is a real failure.
const MIN_BLOCKS: usize = 64;

/// Every guest pc the whole-image walk finds an instruction at, unbounded by
/// any host's ceiling.
///
/// This is what separates the two questions a coverage number confuses:
/// **did discovery find the code** (this), and **would a host compile what it
/// found** (the installed set). P4 owns the first and P5 owns the second, and
/// a run has to be able to say which one a shortfall belongs to.
///
/// Pure — it reads the arena and touches no peripheral — so it is safe to run
/// at the end of a run, which is also when it is most honest: the image then
/// includes whatever the guest wrote and published.
#[must_use]
pub fn discovered_instruction_pcs(bus: &SocBus, seeds: &[u32]) -> BTreeSet<u32> {
    let spans = bus.region_spans();
    let base = bus.guest_arena_base();
    let arena = bus.guest_arena();
    let found = discover(seeds, usize::MAX, &mut |pc| {
        arena_word(arena, base, &spans, pc)
    });
    found
        .set
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter().map(|&(pc, _)| pc))
        .collect()
}

/// Discover the image from `seeds`, translate it, and install it on `hart`.
///
/// This is **both** of JD5's translation events: the machine calls it once
/// before the hart runs, and again at each `fence.i`. There is no third
/// caller, no counter and no threshold — the census the spike used to pick
/// hot regions is a diagnostic now (`--blockprof`) and is not on this path.
///
/// # Errors
///
/// Nothing reachable to translate, an arena with no room for the tables, or a
/// module the host will not build even at [`MIN_BLOCKS`].
///
/// # A host's ceiling is not the design's
///
/// `max_blocks` is what the caller *asked* for; what gets translated is the
/// largest set at or under it that the host will actually build. Halving and
/// retrying is not a workaround, it is the honest shape: **every engine refuses
/// a large enough function, and they refuse at wildly different sizes.** The
/// spike measured cranelift refusing somewhere between 1,161 and 5,161 blocks
/// while JSC compiled 8,161 without complaint, so a fixed budget would either
/// waste most of a browser's capacity or fail outright on the desk. It is also
/// why the same budget can build under `--jit` and be refused under
/// `--jit-escape-all`: an escaped instruction emits a register flush, a call
/// and a reload where a real one emits a few opcodes, so the same block count
/// is several times the wasm.
///
/// Each halving is logged with the reason, because a silent shrink would hide
/// a real translator bug behind a smaller module. Translated code is not
/// architectural state, so what was translated cannot change a transcript —
/// only how much of the run was fast.
///
/// **The discovered figure in the report is the whole image**, whatever the
/// budget ends up installing. That gap is the number P5's module splitting is
/// sized against, so burying it under the budget would hide the finding.
pub fn install(
    hart: &mut MachineHart<SocBus>,
    bus: &mut SocBus,
    seeds: &[u32],
    max_blocks: usize,
    fn_blocks: usize,
    model: CycleModel,
    policy: Emit,
    record: Option<&RecordRequest>,
) -> Result<BuildReport, String> {
    let walk = |bus: &SocBus, budget: usize| {
        let spans = bus.region_spans();
        let base = bus.guest_arena_base();
        let arena = bus.guest_arena();
        let started = std::time::Instant::now();
        let found = discover(seeds, budget, &mut |pc| arena_word(arena, base, &spans, pc));
        (found, started.elapsed().as_micros())
    };

    // The whole image first, and **unbounded**, whatever budget the caller
    // named. `max_blocks` is a bound on what a host will compile, not on what
    // the program can run, and reporting the walk through the budget would
    // hide the very number P5 is sized against.
    let (whole, discover_us) = walk(bus, usize::MAX);
    if whole.set.is_empty() {
        return Err(format!(
            "nothing translatable is reachable from {:#010x}",
            seeds.first().copied().unwrap_or(0)
        ));
    }
    let discovery = Discovery {
        stats: whole.stats,
        discover_us,
    };

    let budget = whole.set.blocks.len().min(max_blocks);
    let found = if budget < whole.set.blocks.len() {
        walk(bus, budget).0
    } else {
        whole
    };

    // What halves now is the **per-function** budget, not the block set. P4
    // halved the set, because one function was all there was and a refusal
    // left no other lever; that cost coverage, which is the whole thing P5
    // exists to stop paying. A module that will not build is now a module
    // whose functions are too big, and the fix does not drop a block.
    let mut per_fn = fn_blocks.max(MIN_BLOCKS);
    loop {
        match JitCore::build(bus, &found.set, model, policy, discovery, per_fn, record) {
            Ok(core) => {
                let report = core.build_report();
                if let Some(r) = record {
                    emit_sizes(bus, &found.set, model, policy, r);
                }
                let entries = found.set.entries().to_vec();
                hart.set_translated_core(Box::new(core), &entries);
                return Ok(report);
            }
            Err(e) if per_fn > MIN_BLOCKS => {
                per_fn = (per_fn / 2).max(MIN_BLOCKS);
                log::warn!("jit: {e}; retrying with {per_fn} blocks per function");
            }
            Err(e) => return Err(e),
        }
    }
}

/// Emit `set` at each of `record.sizes` and write the modules out beside the
/// recording, so JD26's sizing table is five modules of one block set rather
/// than five walks.
///
/// Best effort and loud about it: a size that will not emit is a finding for
/// the table, not a reason to abandon a run that has a working core.
fn emit_sizes(
    bus: &mut SocBus,
    set: &BlockSet,
    model: CycleModel,
    policy: Emit,
    record: &RecordRequest,
) {
    if record.sizes.is_empty() {
        return;
    }
    let Ok(at) = areas(bus, set) else { return };
    // Base zero, whatever host this build has: these bytes are for
    // `jit-image-bench.mjs`, which builds its own memory with the arena at
    // offset zero and replays a recording against it. See `JitCore::build` for
    // the base a module that will actually be *entered* in a browser gets.
    let layout = Layout {
        memory_pages: at.pages,
        guest_base: bus.guest_arena_base(),
        arena_offset: 0,
        perm_offset: at.perm_at,
        exchange_offset: at.exchange_at,
        indirect: Some(at.indirect_at),
    };
    if let Err(e) = std::fs::create_dir_all(&record.dir) {
        log::error!("jit: {}: {e}", record.dir.display());
        return;
    }
    for &size in &record.sizes {
        let started = std::time::Instant::now();
        let emitted = emit_module(set, model, layout, policy, size);
        let emit_us = started.elapsed().as_micros();
        let path = record.dir.join(format!("size-{size}.wasm"));
        match std::fs::write(&path, &emitted.wasm) {
            Ok(()) => eprintln!(
                "jit: size {size}: {} blocks in {} fn, {} B, largest body {} B ({} the \
                 {}-byte budget), emitted in {:.1} ms -> {}",
                set.blocks.len(),
                emitted.functions,
                emitted.wasm.len(),
                emitted.max_body_bytes,
                if emitted.max_body_bytes <= BODY_BUDGET {
                    "under"
                } else {
                    "OVER"
                },
                BODY_BUDGET,
                emit_us as f64 / 1000.0,
                path.display(),
            ),
            Err(e) => log::error!("jit: {}: {e}", path.display()),
        }
    }
}

/// Discover the image, emit the module for it, and write it out — without
/// asking any engine to compile it.
///
/// The JD26 sizing sweep's other half. A global block index does not depend on
/// how the module is split, so the *recording* is taken once, under cranelift,
/// and every other size is emitted by this and replayed against that same
/// recording in `bun` and in `node`. Cranelift is then off the sizing loop
/// entirely, which matters: it takes two minutes over the whole image and the
/// engines take seconds.
///
/// # Errors
///
/// Nothing translatable, an arena with no room for the tables, or a path the
/// filesystem refuses.
pub fn emit_only(
    bus: &mut SocBus,
    seeds: &[u32],
    fn_blocks: usize,
    model: CycleModel,
    policy: Emit,
    path: &std::path::Path,
) -> Result<String, String> {
    let spans = bus.region_spans();
    let base = bus.guest_arena_base();
    let started = std::time::Instant::now();
    let found = {
        let arena = bus.guest_arena();
        discover(seeds, usize::MAX, &mut |pc| {
            arena_word(arena, base, &spans, pc)
        })
    };
    let discover_us = started.elapsed().as_micros();
    if found.set.is_empty() {
        return Err("nothing translatable is reachable".to_string());
    }
    let at = areas(bus, &found.set)?;
    write_permission_table(bus, at);
    write_target_tables(bus.guest_arena_mut(), 0, at.indirect_at, &found.set);
    let layout = Layout {
        memory_pages: at.pages,
        guest_base: base,
        arena_offset: 0,
        perm_offset: at.perm_at,
        exchange_offset: at.exchange_at,
        indirect: Some(at.indirect_at),
    };
    let started = std::time::Instant::now();
    let emitted = emit_module(&found.set, model, layout, policy, fn_blocks);
    let emit_us = started.elapsed().as_micros();
    std::fs::write(path, &emitted.wasm).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!(
        "emit-only: discovered {} blocks / {} instr in {:.1} ms; emitted {} B in {:.1} ms in \
         {} fn x {} blocks (largest {} B, targets {} B, budget {}) -> {}",
        found.stats.blocks,
        found.stats.insts,
        discover_us as f64 / 1000.0,
        emitted.wasm.len(),
        emit_us as f64 / 1000.0,
        emitted.functions,
        fn_blocks.min(found.set.blocks.len()),
        emitted.max_body_bytes,
        at.indirect_len,
        BODY_BUDGET,
        path.display(),
    ))
}

/// The two permission encodings are separate constants in separate crates
/// (JD2 keeps `lp-emu-jit` off `lp-emu-esp-common`), so they are asserted
/// equal here — the one place that sees both.
const _: () = {
    assert!(PERM_NONE == host::PERM_NONE);
    assert!(PERM_READ == host::PERM_READ);
    assert!(PERM_READ_WRITE == host::PERM_READ_WRITE);
    assert!(PERMISSION_PAGE_LEN == 1 << PERM_SHIFT);
};

/// How often the core was asked and what it did.
#[derive(Clone, Copy, Debug, Default)]
pub struct JitStats {
    pub entries: u64,
    /// Refused: a side-band or a yield was already pending.
    pub refused_pending: u64,
    /// Refused: a load watchpoint was armed, or more than one store
    /// watchpoint was.
    pub refused_watch: u64,
    /// Refused: the pc is not an entry this core holds — or was one, and the
    /// guest bytes behind it changed.
    pub refused_no_entry: u64,
    /// Guest code changed under the core and the affected blocks were dropped
    /// (`fence.i`, a flash-cache refill, a ROM-hook patch).
    pub invalidations: u64,
    /// Blocks dropped by those invalidations.
    pub dropped_blocks: u64,
    /// Refused: the bus charges for accesses, or an execute watchpoint is
    /// armed, so an inline access would not be exact.
    pub refused_impure: u64,
    /// Refused: guest code this module was translated from has changed, so
    /// the whole module is stale. See [`JitCore::verify`].
    pub refused_stale: u64,
    /// Guest instructions handed back to the interpreter from inside
    /// translated code (JD10). Allowed to be non-zero; not allowed to be
    /// unmeasured.
    pub escape_hatch: u64,
    /// Guest instructions retired inside translated code, escapes included.
    pub retired: u64,
    /// Times control left one sub-dispatcher for another (JD8). The number
    /// the split's cost is read off; a high one against `retired` says the
    /// per-function budget is too small for this image's call graph.
    pub cross: u64,
    /// Indirect jumps the target table could not resolve, so the stay left.
    /// Every one of these is a host round trip P5 wanted to delete.
    pub indirect_miss: u64,

    // --- where the stays end, which is where the coverage goes -------------
    //
    // Installed coverage is not the same question as found coverage, and P5
    // is where the difference stops being academic: with the whole image
    // installed, every instruction the interpreter still retires is one that
    // ran *between* stays. These say why the stays ended and how much the
    // interpreter did before the next one began, so a shortfall names its own
    // cause instead of being attributed by argument.
    /// Exits whose pc is not a block this module holds. The stay could not be
    /// resumed and the interpreter had to find its own way back.
    pub exit_unknown_pc: u64,
    /// Exits at a pc this module does hold — a budget, an after-store poll,
    /// or a block whose own end left the set at a pc that happens to be a
    /// start.
    pub exit_known_pc: u64,
    /// Exits carrying `FLAG_AFTER_STORE`: polling point (c), byte for byte
    /// what an interpreted store does.
    pub exit_after_store: u64,
    /// Exits carrying `FLAG_SLICE_ENDED`.
    pub exit_slice_ended: u64,
    /// Entries that retired nothing: the first block did not fit the
    /// remaining budget, so the hart's no-progress guard interprets it.
    pub exit_no_progress: u64,
    /// Guest instructions retired **between** stays, by the interpreter.
    pub interpreted_between: u64,
    /// Exits and the instructions the interpreter then retired, per
    /// [`lp_emu_jit::host::why`] code. Indexed by the code itself, so a new
    /// one shows up without this needing to know about it.
    pub why: [(u64, u64); WHY_CODES],
}

/// How many [`lp_emu_jit::host::why`] codes there are, plus one for the zero
/// slot no exit uses.
pub const WHY_CODES: usize = 16;

/// A `why` code's name, for the report.
#[must_use]
pub fn why_name(code: usize) -> &'static str {
    match code as i32 {
        host::why::BUDGET => "budget",
        host::why::EDGE_OUT => "edge-out-of-set",
        host::why::AFTER_STORE => "after-store",
        host::why::INDIRECT_MISS => "indirect-miss",
        host::why::INDIRECT_NO_TABLE => "indirect-no-table",
        host::why::LOAD_REFUSED => "load-refused",
        host::why::LOAD_STRADDLE => "load-straddle",
        host::why::STORE_PERM => "store-perm",
        host::why::STORE_REFUSED => "store-refused",
        host::why::STORE_STRADDLE => "store-straddle",
        host::why::UNDECODABLE => "undecodable",
        host::why::ESCAPE_DIVERGED => "escape-diverged",
        host::why::ESCAPE_TARGET => "escape-target",
        host::why::SLICE_ENDED => "slice-ended",
        _ => "unset",
    }
}

/// What the walk found, before any host ceiling was applied.
#[derive(Clone, Copy, Debug, Default)]
pub struct Discovery {
    pub stats: DiscoverStats,
    pub discover_us: u128,
}

/// Ask a core to record its own work for another engine to replay.
///
/// See [`crate::jit_record`]. Armed at every translation event and taken up
/// by whichever core is alive when the run passes `after_cycles`, which is how
/// a recording lands on the module JD5's *last* event installed rather than on
/// one a `fence.i` has since replaced.
#[derive(Clone, Debug)]
pub struct RecordRequest {
    pub dir: PathBuf,
    pub after_cycles: u64,
    pub entries: usize,
    /// Extra blocks-per-function sizes to emit **the same block set** at,
    /// beside the module the core itself is built from.
    ///
    /// This is what makes JD26's sizing table one recording rather than five.
    /// A global block index does not depend on the split, so a module emitted
    /// at any size answers the same recording — but only if it is emitted from
    /// the *same* block set, and a second run's walk is not the same walk: the
    /// guest publishes code, and what it published depends on what ran.
    pub sizes: Vec<usize>,
}

/// What the machine asked for, so a report can say what it got.
#[derive(Clone, Copy, Debug)]
pub struct BuildReport {
    /// The whole image, whatever was installed of it.
    pub discovery: Discovery,
    pub blocks: usize,
    pub insts: usize,
    pub native_insts: usize,
    pub escaped_insts: usize,
    pub module_bytes: usize,
    pub emit_us: u128,
    pub compile_us: u128,
    pub instantiate_us: u128,
    /// Sub-dispatchers in the module, not counting the outer selector.
    pub functions: usize,
    /// The largest sub-dispatcher body, in bytes.
    pub max_body_bytes: usize,
    /// The per-function block budget the module was actually emitted at,
    /// which is not what the caller asked for when the caller asked for more
    /// blocks than there are.
    pub fn_blocks: usize,
    /// What the indirect-target page map and its slot arrays took.
    pub indirect_bytes: u32,
}

impl BuildReport {
    /// The boot-cost line (JD20), in host milliseconds and said to be.
    ///
    /// One line per translation event, on every `--jit-report` run and in
    /// every PR body: it is a product number — what a phone pays before the
    /// first guest instruction runs — and a number nobody prints is a number
    /// nobody notices going from 0.4 s to 4 s.
    #[must_use]
    pub fn boot_line(&self, event: &str) -> String {
        let d = &self.discovery;
        let ms = |us: u128| us as f64 / 1000.0;
        format!(
            "{event}: discovered {} blocks / {} instr in {:.1} ms; installed {} blocks / {} instr; \
             emitted {} B in {:.1} ms in {} fn x {} blocks (largest {} B, targets {} B); \
             compiled in {:.1} ms; instantiated in {:.2} ms",
            d.stats.blocks,
            d.stats.insts,
            ms(d.discover_us),
            self.blocks,
            self.insts,
            self.module_bytes,
            ms(self.emit_us),
            self.functions,
            self.fn_blocks,
            self.max_body_bytes,
            self.indirect_bytes,
            ms(self.compile_us),
            ms(self.instantiate_us),
        )
    }
}

/// What translated code calls back into.
///
/// The two pointers are set for the length of one entry and cleared
/// afterwards. They are raw because a `wasmtime::Store`'s data has to be
/// `'static` and `&mut MachineHart<'_>` is not; see
/// [`lp_emu_jit::host_wasmtime::WasmtimeCore`].
pub struct C6Ops {
    hart: *mut MachineHart<SocBus>,
    bus: *mut SocBus,
    /// The exchange area, inside the arena. Stable for the life of the
    /// machine, because the arena is.
    exchange: *mut u8,
    /// The slice end the escape hatch reported, if any.
    slice_end: Option<SliceEnd>,
    escape_hatch: u64,
    /// Every import call this entry made, in call order, while a recording is
    /// running. Empty and never touched otherwise.
    calls: Vec<CallRec>,
    recording: bool,
}

// SAFETY: the pointers are only dereferenced from inside a `WasmtimeCore::enter`
// call on the thread that set them, and `JitCore::run` clears them before it
// returns. `Send` is wasmtime's requirement on a store's data, not a claim that
// this may be shared.
unsafe impl Send for C6Ops {}

impl C6Ops {
    /// # Safety
    ///
    /// Only valid between [`JitCore::run`] setting the pointers and clearing
    /// them, which is exactly the window translated code runs in.
    unsafe fn parts(&mut self) -> (&mut MachineHart<SocBus>, &mut SocBus) {
        debug_assert!(!self.hart.is_null() && !self.bus.is_null());
        // SAFETY: the caller's obligation, above.
        unsafe { (&mut *self.hart, &mut *self.bus) }
    }
}

impl HostOps for C6Ops {
    fn mmio_load(&mut self, pc: u32, cycle: u64, address: u32, kind: u32) -> MmioLoad {
        // SAFETY: called from inside `enter`.
        let (_, bus) = unsafe { self.parts() };
        // The exact `(pc, cycle)` the interpreter would have set (JD17): a
        // peripheral model reads both, and P1b measured that a cycle charged
        // at block entry is a different number.
        bus.set_issuing(pc, cycle);
        let read = match kind {
            load_kind::B => bus.read_byte(address).map(i32::from),
            load_kind::BU => bus.read_byte(address).map(|v| i32::from(v as u8)),
            load_kind::H => bus.read_halfword(address).map(i32::from),
            load_kind::HU => bus.read_halfword(address).map(|v| i32::from(v as u16)),
            load_kind::W => bus.read_word(address),
            other => unreachable!("the translator emits no load kind {other}"),
        };
        let out = match read {
            Ok(value) => MmioLoad {
                // A load never makes the hart poll — the interpreter looks
                // after a store, not after a load — but a peripheral's `read`
                // can still ask the machine to take over, and the next store
                // has to leave when it has.
                status: if bus.sideband_or_yield_pending() {
                    MMIO_PENDING
                } else {
                    MMIO_OK
                },
                value: value as u32,
            },
            // The access did not happen. The interpreter re-runs the
            // instruction and takes the trap, with the `mepc` and the `mtval`
            // only it can produce.
            Err(_) => MmioLoad {
                status: MMIO_REFUSED,
                value: 0,
            },
        };
        if self.recording {
            self.calls.push(CallRec {
                kind: 0,
                pc,
                cycle,
                address,
                access: kind,
                value: 0,
                result: (u64::from(out.status) << 32) | u64::from(out.value),
            });
        }
        out
    }

    fn mmio_store(
        &mut self,
        pc: u32,
        cycle: u64,
        address: u32,
        kind: u32,
        value: u32,
    ) -> MmioStore {
        // SAFETY: called from inside `enter`.
        let (_, bus) = unsafe { self.parts() };
        bus.set_issuing(pc, cycle);
        let written = match kind {
            store_kind::B => bus.write_byte(address, value as i8),
            store_kind::H => bus.write_halfword(address, value as i16),
            store_kind::W => bus.write_word(address, value as i32),
            other => unreachable!("the translator emits no store kind {other}"),
        };
        let out = match written {
            Ok(()) => {
                if bus.sideband_or_yield_pending() {
                    MMIO_LEAVE_AFTER
                } else {
                    MMIO_OK
                }
            }
            Err(_) => MMIO_REFUSED,
        };
        if self.recording {
            self.calls.push(CallRec {
                kind: 1,
                pc,
                cycle,
                address,
                access: kind,
                value,
                result: u64::from(out),
            });
        }
        out
    }

    fn step_one(&mut self, pc: u32, cycle: u64, instret: u64, regs: &mut [i32; 32]) -> StepOne {
        self.escape_hatch += 1;
        // SAFETY: called from inside `enter`.
        let (hart, bus) = unsafe { self.parts() };
        // Hand the hart everything translated code has been carrying, run one
        // instruction the way the interpreter always has, and take it back.
        *hart.regs_mut() = *regs;
        hart.set_pc(pc);
        hart.set_counters(cycle, instret);
        let ended = hart.step_one(bus);
        *regs = *hart.regs();
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

    fn exchange(&mut self) -> &mut [u8] {
        // SAFETY: `exchange` points at `EXCHANGE_LEN` bytes inside the bus's
        // arena, which is allocated once and never moves, and which no region
        // covers — see this module's docs.
        unsafe { core::slice::from_raw_parts_mut(self.exchange, EXCHANGE_LEN as usize) }
    }
}

/// A translated core for this machine.
pub struct JitCore {
    core: HostCore<C6Ops>,
    /// Guest pc to block index. One lookup per entry, at the ~19,500 entries
    /// per emulated second the slice cap implies; P4 and P5 are where this
    /// moves into the module.
    index: BTreeMap<u32, u32>,
    /// The guest bytes each block was translated from, kept so an
    /// invalidation can be answered exactly. ~4 bytes per translated
    /// instruction — 10 KB for a P3 block set.
    code: Vec<(u32, Box<[u8]>)>,
    /// The cost model the module was emitted against. A block's budget check
    /// has it folded in as a constant, so a model change is not something a
    /// byte re-check would notice.
    model: CycleModel,
    stats: JitStats,
    report: BuildReport,
    /// Guest code changed under us and the blocks have not been re-checked
    /// yet. `invalidate` has no bus to check against, so the check happens at
    /// the next entry, which does.
    verify_pending: bool,
    /// Something the core cannot recover from: a trap out of translated code,
    /// or a cost model that changed under the module. It stops being entered.
    dead: bool,
    /// The pc and instruction count the last stay left at, so the next entry
    /// can say how much the interpreter did in between.
    last_exit: Option<(u32, u64, usize)>,
    /// Exit pc to `(exits, instructions the interpreter then retired)`, kept
    /// only under `LP_EMU_JIT_EXITS`. A coverage shortfall is a list of
    /// addresses, and this is the list.
    exit_sites: Option<BTreeMap<(u32, u32), (u64, u64)>>,
    /// Guest bytes this module was translated from have changed. Like
    /// [`dead`](Self::dead) it stops the core being entered, but it is not a
    /// bug — it is the module waiting to be replaced at the next translation
    /// event (JD5).
    stale: bool,
    /// A recording in progress, and the cycle count it starts at.
    recorder: Option<(Recorder, u64)>,
    /// The module's own bytes, kept only while a recording wants them.
    wasm: Option<Vec<u8>>,
    /// Where the tables ended up, so a recording can say what a replay has to
    /// reproduce.
    at: Areas,
}

/// Where a translated module's tables sit in the arena.
#[derive(Clone, Copy, Debug)]
struct Areas {
    perm_at: u32,
    exchange_at: u32,
    /// The indirect-target page map; the slot arrays follow it.
    indirect_at: u32,
    /// What the page map and its slot arrays take, together.
    indirect_len: u32,
    pages: u64,
}

/// Place the permission table and the exchange area in the arena's largest
/// gap, and say how much of the arena a wasm memory can cover.
fn areas(bus: &SocBus, set: &BlockSet) -> Result<Areas, String> {
    let arena_len = bus.guest_arena().len();
    let pages = (arena_len / 65536) as u64;
    if pages == 0 {
        return Err("the guest arena is smaller than one wasm page".into());
    }
    let indirect_len = u32::try_from(target_table_bytes(set))
        .map_err(|_| "the indirect-target tables do not fit a 32-bit offset".to_string())?;
    let need = u64::from(PERM_ENTRIES) + u64::from(EXCHANGE_LEN) + u64::from(indirect_len);
    let (gap_base, gap_len) = bus
        .largest_arena_gap()
        .ok_or_else(|| "the guest arena has no gap for the translator's tables".to_string())?;
    if u64::from(gap_len) < need {
        return Err(format!(
            "the arena's largest gap is {gap_len} bytes and the translator's tables need {need}"
        ));
    }
    let perm_at = gap_base - bus.guest_arena_base();
    let exchange_at = perm_at + PERM_ENTRIES;
    let indirect_at = exchange_at + EXCHANGE_LEN;
    if u64::from(indirect_at) + u64::from(indirect_len) > pages * 65536 {
        return Err("the translator's tables fall outside the wasm memory".into());
    }
    Ok(Areas {
        perm_at,
        exchange_at,
        indirect_at,
        indirect_len,
        pages,
    })
}

/// Write the permission table into the arena.
///
/// The bus's table is one byte per 16 KiB page **of the arena**; the emitted
/// code indexes one byte per 16 KiB page of the **whole 32-bit guest space**,
/// so it needs no bounds compare. This is where one becomes the other, and it
/// is also where the pages a wasm memory cannot reach are taken back to
/// [`PERM_NONE`]: the arena is not a whole number of wasm pages, so its tail
/// — on the C6, all 16 KiB of LP SRAM — is outside the memory and every
/// access to it has to go out through the bus.
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
    // The tables' own pages are in a gap no region covers, so they are already
    // zero; asserting it is how that stays true if a chip's map ever changes.
    let own = ((u64::from(base) + u64::from(at.perm_at)) >> PERM_SHIFT) as usize;
    debug_assert_eq!(
        arena[at.perm_at as usize + own],
        PERM_NONE,
        "the translator's own tables are guest-reachable RAM"
    );
}

impl JitCore {
    /// Translate `set` and build a core for it.
    ///
    /// # Errors
    ///
    /// A block set that does not fit the arena's shape, or a module wasmtime
    /// will not compile.
    pub fn build(
        bus: &mut SocBus,
        set: &BlockSet,
        model: CycleModel,
        policy: Emit,
        discovery: Discovery,
        fn_blocks: usize,
        record: Option<&RecordRequest>,
    ) -> Result<Self, String> {
        let at = areas(bus, set)?;
        write_permission_table(bus, at);

        let arena_len = bus.guest_arena().len();
        let guest_base = bus.guest_arena_base();
        let arena_ptr = bus.guest_arena_mut().as_mut_ptr();
        // Where the arena sits inside the memory the module imports, and how
        // big that memory is.
        //
        // Natively the module's memory *is* the arena — `host_wasmtime` hands
        // the engine an alias of it — so the arena starts at offset zero and
        // the fold on every guest access is one subtract. In the browser the
        // module imports the emulator's **whole** linear memory, the arena is
        // an ordinary allocation somewhere inside it, and every folded
        // `memarg` base shifts by that allocation's address. The `Layout`
        // already takes a base for exactly this; this is the one place the two
        // hosts differ in what they put in it.
        #[cfg(target_family = "wasm")]
        let (mem_base, memory_pages) =
            (arena_ptr as u32, core::arch::wasm32::memory_size(0) as u64);
        #[cfg(not(target_family = "wasm"))]
        let (mem_base, memory_pages) = (0u32, at.pages);

        // After `mem_base` is known, and that ordering is the point: the page
        // map's entries are pointers into the module's memory, not into the
        // host's view of the arena.
        write_target_tables(bus.guest_arena_mut(), mem_base, at.indirect_at, set);

        let layout = Layout {
            memory_pages,
            guest_base,
            arena_offset: mem_base,
            perm_offset: mem_base + at.perm_at,
            exchange_offset: mem_base + at.exchange_at,
            indirect: Some(mem_base + at.indirect_at),
        };

        let started = std::time::Instant::now();
        let emitted: Emitted = emit_module(set, model, layout, policy, fn_blocks);
        let emit_us = started.elapsed().as_micros();
        // wasm's implementation limit applies per **function**, not per
        // module, and the split is what keeps every function under it.
        // Checked here rather than left to the engine, because every engine
        // reports it differently, none of them say what to do about it, and
        // the answer — a smaller `fn_blocks` — is one the caller can act on.
        if emitted.max_body_bytes > BODY_BUDGET {
            return Err(format!(
                "at {fn_blocks} blocks per function the largest sub-dispatcher is {} bytes, \
                 over the {BODY_BUDGET}-byte budget; use fewer (--jit-fn-blocks)",
                emitted.max_body_bytes,
            ));
        }

        let index = set
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.pc, i as u32))
            .collect();
        // The guest bytes behind each block, so an invalidation can be
        // answered by asking whether they changed rather than by giving up.
        let arena_base = guest_base;
        let code: Vec<(u32, Box<[u8]>)> = {
            let arena = bus.guest_arena();
            set.blocks
                .iter()
                .map(|b| {
                    let at = (u64::from(b.pc) - u64::from(arena_base)) as usize;
                    let len = (b.end_pc().wrapping_sub(b.pc)) as usize;
                    let bytes = arena
                        .get(at..at + len)
                        .map_or_else(|| Vec::new().into_boxed_slice(), |b| b.to_vec().into());
                    (b.pc, bytes)
                })
                .collect()
        };

        let spans = bus.region_spans();
        // Straight from the arena, never a constant: this is what decides
        // whether the engine may elide its bounds checks, and only the
        // allocation knows whether there is really a guard behind it. In the
        // browser it is always `None` — the arena there is a heap `Vec` inside
        // the emulator's own linear memory, and the engine's guard pages are
        // already under every access.
        let arena_guard = bus.guest_arena_guard();
        let exchange = bus.guest_arena_mut()[at.exchange_at as usize..].as_mut_ptr();
        let ops = C6Ops {
            hart: core::ptr::null_mut(),
            bus: core::ptr::null_mut(),
            exchange,
            slice_end: None,
            escape_hatch: 0,
            calls: Vec::new(),
            recording: false,
        };
        // SAFETY: `arena_base` is the bus's own arena, allocated once during
        // construction from the chip's declared memory map and documented as
        // not moving for the life of the machine; the memory wasmtime is given
        // covers only whole wasm pages of it. Nothing holds a Rust reference
        // into the arena while translated code runs: `JitCore::run` reaches the
        // bus through the raw pointer it just set, and does not touch it
        // otherwise. `arena_guard` is the arena's own report: when it is
        // `Some`, the bytes past `arena_len` really are unmapped out to the
        // end of the reservation and its guard.
        let core = unsafe { HostCore::new(&emitted.wasm, ops, arena_ptr, arena_len, arena_guard) }
            .map_err(|e| format!("the translated module did not build: {e:?}"))?;
        let (compile_us, instantiate_us) = (core.compile_us(), core.instantiate_us());

        // What a replay has to reproduce: the guest's own regions and the
        // translator's tables, and never the ~200 MiB the arena spans between
        // them — nothing reads it, and reading it would commit it.
        let recorder = record.map(|r| {
            // Clipped to the wasm memory, which is whole pages of the arena
            // and so stops short of its tail — on the C6 that tail is all
            // 16 KiB of LP SRAM, and a replay has no memory to put it in.
            let reachable = (at.pages * 65536) as u32;
            let mut live: Vec<(u32, u32, bool)> = spans
                .iter()
                .map(|&(base, len, _)| (base - arena_base, len, true))
                .filter(|&(off, _, _)| off < reachable)
                .map(|(off, len, v)| (off, len.min(reachable - off), v))
                .collect();
            live.push((at.perm_at, PERM_ENTRIES, false));
            live.push((at.exchange_at, EXCHANGE_LEN, false));
            live.push((at.indirect_at, at.indirect_len, false));
            live.sort_unstable();
            (
                Recorder::new(r.dir.clone(), live, r.entries),
                r.after_cycles,
            )
        });
        let wasm = record.map(|_| emitted.wasm.clone());

        Ok(Self {
            core,
            index,
            code,
            model,
            stats: JitStats::default(),
            verify_pending: false,
            dead: false,
            stale: false,
            recorder,
            wasm,
            at,
            last_exit: None,
            exit_sites: std::env::var_os("LP_EMU_JIT_EXITS").map(|_| BTreeMap::new()),
            report: BuildReport {
                discovery,
                blocks: set.blocks.len(),
                insts: set.inst_count(),
                native_insts: emitted.native_insts,
                escaped_insts: emitted.escaped_insts,
                module_bytes: emitted.wasm.len(),
                emit_us,
                compile_us,
                instantiate_us,
                functions: emitted.functions,
                max_body_bytes: emitted.max_body_bytes,
                fn_blocks: fn_blocks.min(set.blocks.len()),
                indirect_bytes: at.indirect_len,
            },
        })
    }

    /// Check whether the guest bytes this module was translated from are
    /// still what it was translated from, and stop using it if they are not.
    ///
    /// This is the answer to an invalidation, and it is **exact**: a module
    /// whose every block's bytes are unchanged is still a correct
    /// translation of them, whatever the reason the machine gave for asking.
    /// It is what keeps a `fence.i` — the guest publishing a shader it wrote
    /// into RAM — from costing the translation of the app's read-only
    /// `.text`, which cannot have changed and which is most of what a render
    /// loop runs. Measured on all four pinned images: 2 invalidations, no
    /// block's bytes changed.
    ///
    /// # Why one changed block retires the whole module, and not just itself
    ///
    /// P3 dropped the changed blocks from the entry index and kept the rest.
    /// That is not sound, and M7 P4's `fence.i` test is what caught it: the
    /// index governs where the hart may **enter**, and a block's edges to
    /// other blocks are compiled *into the module*. Dropping a block from the
    /// index does not stop a surviving block's `br` from reaching its body,
    /// so a caller that was translated before the guest rewrote its callee
    /// went on running the callee it was translated from. It never fired on
    /// the pinned images — nothing ever changed — which is exactly the kind
    /// of bug that waits.
    ///
    /// So a changed byte makes the whole module **stale**: it stops being
    /// entered, the run continues interpreted, and the next translation
    /// event replaces it. That is not a fallback, it is the design — JD5 has
    /// an event for precisely this.
    fn verify(&mut self, bus: &SocBus) {
        self.verify_pending = false;
        self.stats.invalidations += 1;
        let base = bus.guest_arena_base();
        let arena = bus.guest_arena();
        let changed = self.code.iter().filter(|(pc, bytes)| {
            let Ok(at) = usize::try_from(u64::from(*pc) - u64::from(base)) else {
                return true;
            };
            !arena
                .get(at..at + bytes.len())
                .is_some_and(|now| now == &bytes[..])
        });
        let dropped = changed.count() as u64;
        if dropped > 0 {
            self.stats.dropped_blocks += dropped;
            self.stale = true;
            log::debug!(
                "jit: {dropped} translated block(s) no longer match the guest's bytes; \
                 the module is stale until the next translation event"
            );
        }
    }

    /// Write the recording out beside the module it is of, and say so.
    ///
    /// Called the moment the last entry lands rather than at the end of the
    /// run, because this is where the arena and the module bytes are both in
    /// hand and neither has to be kept alive for a caller that may never come.
    fn finish_recording(&mut self, bus: &SocBus) {
        let Some((r, _)) = self.recorder.as_ref() else {
            return;
        };
        if r.escaped {
            log::error!(
                "jit: the escape hatch fired inside a recorded entry; \
                 `crate::jit_record` does not carry that case, so no recording was written"
            );
            self.recorder = None;
            self.wasm = None;
            return;
        }
        let wasm = self.wasm.take().unwrap_or_default();
        let out = r.finish(
            &wasm,
            bus.guest_arena(),
            self.at.pages,
            self.at.exchange_at,
            self.report.fn_blocks,
            self.report.blocks,
        );
        match out {
            Ok(()) => eprintln!(
                "jit: recorded {} entries into translated code to {}",
                r.len(),
                r.dir().display()
            ),
            Err(e) => log::error!("jit: the recording could not be written: {e}"),
        }
        self.recorder = None;
    }

    /// Whether this module could have been entered at `pc` — and if not, how
    /// near it came.
    ///
    /// The distinction P5 measured and P4's census cannot make: the walk
    /// reports the instructions it **found**, and a module can only be entered
    /// at a block **start**. An indirect call into the middle of a block that
    /// discovery found perfectly well is a pc this module has no label for, so
    /// the stay leaves and the interpreter runs the whole function. "found"
    /// and "enterable" are two different numbers, and only one of them is
    /// coverage.
    fn where_is(&self, pc: u32) -> String {
        if self.index.contains_key(&pc) {
            return "a block start".to_string();
        }
        let at = self.code.partition_point(|(start, _)| *start <= pc);
        if at > 0 {
            let (start, bytes) = &self.code[at - 1];
            if pc < start.wrapping_add(bytes.len() as u32) {
                return format!("inside the block at {start:#010x}");
            }
        }
        "not discovered".to_string()
    }

    #[must_use]
    pub fn stats(&self) -> JitStats {
        self.stats
    }

    #[must_use]
    pub fn build_report(&self) -> BuildReport {
        self.report
    }
}

impl TranslatedCore<SocBus> for JitCore {
    fn run(&mut self, hart: &mut MachineHart<SocBus>, bus: &mut SocBus, end: u64) -> RunOutcome {
        if self.dead {
            self.stats.refused_no_entry += 1;
            return RunOutcome::Refused;
        }
        if self.stale {
            self.stats.refused_stale += 1;
            return RunOutcome::Refused;
        }
        // The block set's budget checks have the cost model folded in as
        // constants, so a model that changed since emission is not something
        // the byte re-check below would catch.
        if hart.cycle_model() != self.model {
            log::warn!("jit: the cost model changed under a translated core; interpreting");
            self.dead = true;
            return RunOutcome::Refused;
        }
        if self.verify_pending {
            self.verify(bus);
            if self.stale {
                self.stats.refused_stale += 1;
                return RunOutcome::Refused;
            }
        }
        let pc = hart.pc();
        let Some(&entry) = self.index.get(&pc) else {
            self.stats.refused_no_entry += 1;
            return RunOutcome::Refused;
        };
        // The refusal rules, in the order the spike learned them.
        //
        // A pending side-band or yield must be observed at exactly the store
        // that raised it, and translated code cannot re-raise one it did not
        // make.
        if bus.sideband_or_yield_pending() {
            self.stats.refused_pending += 1;
            return RunOutcome::Refused;
        }
        // Translated code does RAM loads the bus never sees, so a load
        // watchpoint could not fire.
        if bus.load_watchpoints_armed() {
            self.stats.refused_watch += 1;
            return RunOutcome::Refused;
        }
        // One store range can be honoured inline — esp-hal's stack guard is
        // armed for whole runs — and more than one cannot.
        let watch = match bus.store_watch() {
            lp_emu_core::StoreWatch::None => (0, 0),
            lp_emu_core::StoreWatch::One { lo, hi } => (lo, hi),
            lp_emu_core::StoreWatch::Many => {
                self.stats.refused_watch += 1;
                return RunOutcome::Refused;
            }
        };
        // An inline access charges nothing and fires no execute watchpoint,
        // so a bus that would have done either is one this core cannot be
        // exact on. The hart already refuses to reach this seam on such a bus
        // — it only enters from the block-cached loop — so this never fires in
        // practice, and it is here because "never in practice" is not a
        // guarantee a correctness rule should rest on.
        if !bus.fetch_is_pure() {
            self.stats.refused_impure += 1;
            return RunOutcome::Refused;
        }

        self.stats.entries += 1;
        let (cycle, instret) = (hart.cycle_count(), hart.instruction_count());
        if let Some((from, at_instret, why)) = self.last_exit.take() {
            let gap = instret.saturating_sub(at_instret);
            self.stats.interpreted_between += gap;
            self.stats.why[why].1 += gap;
            if let Some(sites) = self.exit_sites.as_mut() {
                let slot = sites.entry((from, why as u32)).or_default();
                slot.0 += 1;
                slot.1 += gap;
            }
        }
        // Arm the recording once the run is past the point the caller asked
        // for, which is how it lands on the module the *last* translation
        // event installed rather than one a `fence.i` has since replaced.
        let delta = match &mut self.recorder {
            Some((r, after)) if cycle >= *after && r.wants_more() => {
                Some(r.before_entry(bus.guest_arena()))
            }
            _ => None,
        };
        let recording = delta.is_some();
        let watch_in = watch;
        let mut regs_in = [0i32; 31];
        if recording {
            regs_in.copy_from_slice(&hart.regs()[1..]);
        }
        {
            let ops = self.core.ops_mut();
            ops.recording = recording;
            ops.calls.clear();
            let x = ops.exchange();
            for (i, r) in hart.regs().iter().enumerate() {
                x[4 * i..][..4].copy_from_slice(&r.to_le_bytes());
            }
            ops.slice_end = None;
            ops.hart = hart;
            ops.bus = bus;
        }
        let exit = self.core.enter(entry, cycle, instret, end, watch);
        let ops = self.core.ops_mut();
        ops.hart = core::ptr::null_mut();
        ops.bus = core::ptr::null_mut();
        ops.recording = false;
        let calls = core::mem::take(&mut ops.calls);
        let escaped = core::mem::take(&mut ops.escape_hatch);
        self.stats.escape_hatch += escaped;
        let slice_end = ops.slice_end.take();

        let exit = match exit {
            Ok(exit) => exit,
            // The emitted module cannot trap on any path this translator
            // emits, so a trap is a translator bug. Saying so and refusing is
            // the only answer that keeps the transcript right; the run
            // continues interpreted.
            Err(e) => {
                log::error!("jit: translated code trapped at {pc:#010x}: {e}");
                self.dead = true;
                return RunOutcome::Refused;
            }
        };

        // The module leaves the hart's own pc and counters where the exchange
        // area says, so the outcome and the hart agree.
        let exit_why;
        let mut regs = *hart.regs();
        {
            let x = self.core.ops_mut().exchange();
            for (i, r) in regs.iter_mut().enumerate() {
                *r = i32::from_le_bytes([x[4 * i], x[4 * i + 1], x[4 * i + 2], x[4 * i + 3]]);
            }
            let cycle_count = u64::from_le_bytes(
                x[host::EXCHANGE_CYCLE as usize..][..8]
                    .try_into()
                    .expect("eight bytes"),
            );
            let instruction_count = u64::from_le_bytes(
                x[host::EXCHANGE_INSTRET as usize..][..8]
                    .try_into()
                    .expect("eight bytes"),
            );
            self.stats.cross += u64::from_le_bytes(
                x[host::EXCHANGE_CROSS as usize..][..8]
                    .try_into()
                    .expect("eight bytes"),
            );
            self.stats.indirect_miss += u64::from_le_bytes(
                x[host::EXCHANGE_INDIRECT_MISS as usize..][..8]
                    .try_into()
                    .expect("eight bytes"),
            );
            exit_why = i32::from_le_bytes(
                x[host::EXCHANGE_EXIT_WHY as usize..][..4]
                    .try_into()
                    .expect("four bytes"),
            );
            regs[0] = 0;
            *hart.regs_mut() = regs;
            hart.set_pc(exit.pc);
            hart.set_counters(cycle_count, instruction_count);
            self.stats.retired += instruction_count.saturating_sub(instret);
        }

        if let Some(delta) = delta {
            let escaped_here = escaped > 0;
            let regs = hart.regs();
            let mut regs_out = [0i32; 31];
            regs_out.copy_from_slice(&regs[1..]);
            let rec = EntryRec {
                entry,
                cycle_in: cycle,
                instret_in: instret,
                end,
                watch_lo: watch_in.0,
                watch_hi: watch_in.1,
                regs_in,
                delta,
                calls,
                exit_pc: exit.pc,
                flags: exit.flags,
                cycle_out: hart.cycle_count(),
                instret_out: hart.instruction_count(),
                regs_out,
            };
            let (r, _) = self.recorder.as_mut().expect("armed above");
            if escaped_here {
                r.escaped = true;
            }
            r.after_entry(bus.guest_arena(), rec);
            if r.done {
                self.finish_recording(bus);
            }
        }

        if exit.flags & FLAG_AFTER_STORE != 0 {
            self.stats.exit_after_store += 1;
        }
        if exit.flags & FLAG_SLICE_ENDED != 0 {
            self.stats.exit_slice_ended += 1;
        }
        if hart.instruction_count() == instret {
            self.stats.exit_no_progress += 1;
        }
        if self.index.contains_key(&exit.pc) {
            self.stats.exit_known_pc += 1;
        } else {
            self.stats.exit_unknown_pc += 1;
        }
        let why = (exit_why as usize).min(WHY_CODES - 1);
        self.stats.why[why].0 += 1;
        self.last_exit = Some((exit.pc, hart.instruction_count(), why));

        if exit.flags & FLAG_SLICE_ENDED != 0 {
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
            after_store: exit.flags & FLAG_AFTER_STORE != 0,
        }
    }

    fn invalidate(&mut self, _range: Option<(u32, u32)>) {
        // The range is not used, and deliberately: `verify` re-checks every
        // block's own bytes, which answers a range and a whole flush with the
        // same, exact, question. What it costs is one pass over ~40 KB at
        // each of the ~98 cache refills and one `fence.i` a render run
        // performs.
        //
        // What happens after a change is JD5's second event: the machine
        // retranslates at the `fence.i`, and until it does the module is
        // stale and the run is interpreted.
        self.verify_pending = true;
    }

    fn retired(&self) -> u64 {
        self.stats.retired
    }

    fn report(&self) -> String {
        for (code, &(exits, gap)) in self.stats.why.iter().enumerate() {
            if exits == 0 && gap == 0 {
                continue;
            }
            eprintln!(
                "jit: exits by reason: {:<18} {exits:>10} exit(s), {gap:>12} instructions \
                 interpreted after",
                why_name(code)
            );
        }
        if let Some(sites) = self.exit_sites.as_ref() {
            let mut top: Vec<(u32, u32, u64, u64)> = sites
                .iter()
                .map(|(&(pc, why), &(n, gap))| (pc, why, n, gap))
                .collect();
            top.sort_unstable_by(|a, b| b.3.cmp(&a.3));
            for (pc, why, n, gap) in top.iter().take(24) {
                eprintln!(
                    "jit: exit {pc:#010x} ({}, {}): {n} time(s), {gap} instructions \
                     interpreted after",
                    why_name(*why as usize),
                    self.where_is(*pc)
                );
            }
        }
        let r = self.report;
        let s = self.stats;
        let d = r.discovery.stats;
        format!(
            "discovered {} blocks / {} instr from {} seeds ({} starts, {} ended undecodable, \
             {} named no code{}); \
             installed {} blocks / {} instr ({} emitted, {} escaped, {:.1} % static escape); \
             module {} B in {} fn x {} blocks (largest body {} B, target tables {} B), \
             discover {:.2} ms, emit {:.2} ms, compile {:.2} ms, \
             instantiate {:.2} ms; \
             entries {}, retired {}, escape_hatch {}, cross {}, indirect_miss {}, \
             interpreted_between {}, exits known/unknown {}/{}, after_store {}, \
             slice_ended {}, no_progress {}, \
             invalidations {} (dropped {} blocks), \
             refused_pending {}, refused_watch {}, refused_no_entry {}, refused_impure {}, \
             refused_stale {}",
            d.blocks,
            d.insts,
            d.seeds,
            d.starts,
            d.undecodable,
            d.empty_starts,
            if d.truncated {
                ", budget-truncated"
            } else {
                ""
            },
            r.blocks,
            r.insts,
            r.native_insts,
            r.escaped_insts,
            if r.insts == 0 {
                0.0
            } else {
                100.0 * r.escaped_insts as f64 / r.insts as f64
            },
            r.module_bytes,
            r.functions,
            r.fn_blocks,
            r.max_body_bytes,
            r.indirect_bytes,
            r.discovery.discover_us as f64 / 1000.0,
            r.emit_us as f64 / 1000.0,
            r.compile_us as f64 / 1000.0,
            r.instantiate_us as f64 / 1000.0,
            s.entries,
            s.retired,
            s.escape_hatch,
            s.cross,
            s.indirect_miss,
            s.interpreted_between,
            s.exit_known_pc,
            s.exit_unknown_pc,
            s.exit_after_store,
            s.exit_slice_ended,
            s.exit_no_progress,
            s.invalidations,
            s.dropped_blocks,
            s.refused_pending,
            s.refused_watch,
            s.refused_no_entry,
            s.refused_impure,
            s.refused_stale,
        )
    }
}
