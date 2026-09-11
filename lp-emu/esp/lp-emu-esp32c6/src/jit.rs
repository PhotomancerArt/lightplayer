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
use lp_emu_jit::discover::{DiscoverStats, Discovered, discover, discover_from};
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

/// The smallest block budget worth retrying at, and the floor
/// `--jit-fn-blocks` is clamped to. Below this the set is too small to be
/// evidence of anything, and a failure is a real failure.
///
/// **8 since M7 P6b, and the reason is a measurement, not a preference.** At
/// 64 — the floor P5 chose — every engine's curve was still falling and no
/// smaller size could be asked for, so G-M7P had to record "the optimum is
/// unmeasured below 64". It is measured now, on the same recording at every
/// size (`scripts/emu/p6b-replay-sweep.mjs`), and the two engines disagree
/// about which end of the range is safe:
///
/// - **JavaScriptCore keeps getting faster all the way down** — 8.75 ns per
///   guest instruction at 8 blocks a function against 12.2–19.7 at 64, on the
///   same module bytes and the same recording.
/// - **V8 dies at the small end**, with the same
///   `Fatal process out of memory: Zone` in `WasmLoweringPhase` that #680
///   found at 512 blocks a function — and for the mirror-image reason. The
///   module's largest function is a sub-dispatcher at big sizes and the
///   **outer selector** at small ones: 730 KB and 25,156 nested blocks at 8,
///   against 88 KB at 64. V8 compiles 32 (selector 176 KB) and refuses 16
///   (352 KB).
///
/// So this is a floor on what can be *asked for*, and it is not a default.
/// `--jit-fn-blocks`'s default is unchanged, and choosing it is a decision
/// the G-M7P gate makes with both of those rows in front of it.
const MIN_BLOCKS: usize = 8;

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

/// Walk the image from `seeds`, stopping wherever `known` already holds a
/// block start, and say how long it took in microseconds.
///
/// The one walk in this crate: [`install`] uses it, the incremental path uses
/// it with the installed modules' starts as the stop set, and M7b P1's split
/// census uses it to ask what the incremental set *would* hold before anything
/// is emitted. `known` empty is the whole-image walk.
///
/// Pure — it reads the arena and touches no peripheral.
#[must_use]
pub fn walk(
    bus: &SocBus,
    seeds: &[u32],
    budget: usize,
    known: &BTreeSet<u32>,
) -> (Discovered, u128) {
    let spans = bus.region_spans();
    let base = bus.guest_arena_base();
    let arena = bus.guest_arena();
    let started = std::time::Instant::now();
    let found = discover_from(seeds, budget, known, &mut |pc| {
        arena_word(arena, base, &spans, pc)
    });
    (found, started.elapsed().as_micros())
}

/// Every guest instruction pc in `set`.
#[must_use]
pub fn instruction_pcs(set: &BlockSet) -> BTreeSet<u32> {
    set.blocks
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
    let nothing_known = BTreeSet::new();
    // The whole image first, and **unbounded**, whatever budget the caller
    // named. `max_blocks` is a bound on what a host will compile, not on what
    // the program can run, and reporting the walk through the budget would
    // hide the very number P5 is sized against.
    let (whole, discover_us) = walk(bus, seeds, usize::MAX, &nothing_known);
    if whole.set.is_empty() {
        return Err(format!(
            "nothing translatable is reachable from {:#010x}",
            seeds.first().copied().unwrap_or(0)
        ));
    }
    let budget = whole.set.blocks.len().min(max_blocks);
    let found = if budget < whole.set.blocks.len() {
        walk(bus, seeds, budget, &nothing_known).0
    } else {
        whole
    };

    // **Two modules, split on whether the guest could rewrite the bytes**
    // (M7b P1). Read-only guest code — the flash-cache window and the mask ROM
    // — cannot change, so a module holding only that can never go stale, and
    // it is 96 % of the image. Everything in writable memory goes in the other
    // one, which is what a `fence.i` replaces.
    //
    // Without this split there is nothing for incremental translation to be
    // incremental *against*: `render-basic` t2's first `fence.i` finds **four**
    // blocks of the 154,544 whose bytes changed — mis-swept data in HP SRAM,
    // not code anything runs — and the whole-module retire (DD18, and the
    // soundness argument in `JitCore::verify`) then retires all 154,544 and
    // re-emits 65 MB. Four words of churn cost the whole image.
    //
    // A recording is of ONE module's bytes, so a run that asked for one keeps
    // the single-module shape; it is a diagnostic and its speed is nobody's
    // number.
    let sets = if record.is_some() {
        vec![(found.set.clone(), false)]
    } else {
        split_by_writability(bus, &found.set)
    };

    let mut core: Option<JitCore> = None;
    for (set, read_only) in &sets {
        let used = core.as_ref().map_or(0, JitCore::gap_used);
        let (module, recorder) = build_with_halving(
            bus,
            set,
            model,
            policy,
            // The discovery figures are the WHOLE walk's, on the first module
            // only: they describe the image, not the half of it this module
            // holds, and `JitCore::totals` adds them up.
            Discovery {
                stats: if core.is_none() {
                    found.stats
                } else {
                    Default::default()
                },
                discover_us: if core.is_none() { discover_us } else { 0 },
            },
            fn_blocks,
            record,
            used,
            *read_only,
        )?;
        match core.as_mut() {
            None => core = Some(JitCore::of(module, recorder, model)),
            Some(c) => {
                c.add(module);
            }
        }
    }
    let core = core.expect("a non-empty block set produces at least one module");
    if let Some(r) = record {
        emit_sizes(bus, &found.set, model, policy, r);
    }
    let report = core.totals();
    let entries: Vec<u32> = core.index.keys().copied().collect();
    hart.set_translated_core(Box::new(core), &entries);
    Ok(report)
}

/// Split a block set into **read-only** blocks and **writable** blocks, in
/// that order, dropping whichever half is empty.
///
/// The one rule the whole-module retire cares about: bytes the guest cannot
/// write cannot make a module stale. A block is read-only when every byte it
/// was translated from sits in a region the bus calls read-only; a block that
/// straddles the two — which no real one does, but which nothing forbids —
/// counts as writable, because "invalidating too much is slow, invalidating too
/// little is wrong".
fn split_by_writability(bus: &SocBus, set: &BlockSet) -> Vec<(BlockSet, bool)> {
    let spans = bus.region_spans();
    let read_only = |pc: u32, len: u32| {
        spans.iter().any(|&(base, l, writable)| {
            !writable
                && pc >= base
                && u64::from(pc) + u64::from(len) <= u64::from(base) + u64::from(l)
        })
    };
    let (mut ro, mut rw) = (Vec::new(), Vec::new());
    for b in &set.blocks {
        let len = b.end_pc().wrapping_sub(b.pc);
        if read_only(b.pc, len) {
            ro.push(b.clone());
        } else {
            rw.push(b.clone());
        }
    }
    [(ro, true), (rw, false)]
        .into_iter()
        .filter(|(blocks, _)| !blocks.is_empty())
        .map(|(blocks, read_only)| {
            let index = blocks
                .iter()
                .enumerate()
                .map(|(i, b)| (b.pc, i))
                .collect::<BTreeMap<_, _>>();
            (BlockSet::from_blocks(blocks, index), read_only)
        })
        .collect()
}

/// [`Module::build`], halving the per-function budget until the host stops
/// refusing.
///
/// Halving is not a workaround, it is the honest shape: every engine refuses a
/// large enough function and they refuse at wildly different sizes. See
/// [`install`]'s own docs.
#[allow(clippy::too_many_arguments)]
fn build_with_halving(
    bus: &mut SocBus,
    set: &BlockSet,
    model: CycleModel,
    policy: Emit,
    discovery: Discovery,
    fn_blocks: usize,
    record: Option<&RecordRequest>,
    gap_used: u32,
    read_only: bool,
) -> Result<(Module, Option<(Recorder, u64)>), String> {
    let mut per_fn = fn_blocks.max(MIN_BLOCKS);
    loop {
        match Module::build(
            bus, set, model, policy, discovery, per_fn, record, gap_used, read_only,
        ) {
            Ok(built) => return Ok(built),
            Err(e) if per_fn > MIN_BLOCKS => {
                per_fn = (per_fn / 2).max(MIN_BLOCKS);
                log::warn!("jit: {e}; retrying with {per_fn} blocks per function");
            }
            Err(e) => return Err(e),
        }
    }
}

/// The way back from the hart's boxed core to this crate's own.
///
/// See [`TranslatedCore::as_any_mut`]: the hart holds a `dyn TranslatedCore`
/// and has no business knowing what a module is, so the machine crate asks for
/// its own type back.
trait AsJitCore {
    fn as_jit_core(&mut self) -> Option<&mut JitCore>;
}

impl AsJitCore for lp_riscv_emu::mach::translated::BoxedCore<SocBus> {
    fn as_jit_core(&mut self) -> Option<&mut JitCore> {
        self.as_any_mut()?.downcast_mut::<JitCore>()
    }
}

/// What one `fence.i` did, so the machine can say it and the report can be
/// read without knowing this module's internals.
pub enum Incremental {
    /// A module was added beside the ones already installed.
    Added(BuildReport),
    /// The publish claimed nothing the installed modules do not already hold,
    /// so nothing was emitted and nothing was compiled. **This is the cheapest
    /// possible answer to a `fence.i` and it is a real one**: a second fence
    /// over a buffer that was already swept adds no code.
    Nothing,
    /// The incremental path does not apply and the caller must do a whole-image
    /// translation instead: a module went stale, or no core is installed, or
    /// this run is recording.
    WholeImage(String),
}

/// The **second** translation event, done incrementally (M7b P1, DD18).
///
/// Where [`install`] walks the whole image, emits ~65 MB of wasm and replaces
/// the core, this keeps the **read-only** module — which is 96 % of the image
/// and cannot have changed, because the guest cannot write those bytes — and
/// re-emits only the writable side, which is what a publish can have touched
/// and where the published code itself lands.
///
/// Three things make it exact rather than merely cheaper:
///
/// - the walk is given the read-only module's block starts as a **stop set**
///   ([`lp_emu_jit::discover::discover_from`]), so exactly one module answers
///   for any pc and the hart's entry index has one answer;
/// - the new module gets **its own** indirect-target tables, in its own slice
///   of the arena gap, because a target slot holds a block index and a block
///   index only means something inside the module it was emitted with;
/// - every edge that leaves a module is an ordinary **exit**, so the hart
///   re-enters through the index and lands in whichever module holds the
///   target. Cross-module control flow costs one entry, which is the number
///   `LP_EMU_JIT_SPLIT_CENSUS` measured before any of this was built.
///
/// **The whole-module retire stays.** The writable module is retired and
/// replaced *whole* at every event, exactly as the single module was before
/// this phase — this is that retire, applied to the only module whose bytes
/// can change. If the **read-only** module ever goes stale, which would mean
/// the emulator moved bytes the guest calls read-only, this returns
/// [`Incremental::WholeImage`] and the caller retranslates everything.
///
/// # Errors
///
/// Anything that stops the new module being built. The caller keeps what it
/// has: a failure here costs coverage and never a transcript.
pub fn install_incremental(
    hart: &mut MachineHart<SocBus>,
    bus: &mut SocBus,
    seeds: &[u32],
    fn_blocks: usize,
    model: CycleModel,
    policy: Emit,
) -> Result<Incremental, String> {
    let Some(core) = hart.translated_core_mut().and_then(|c| c.as_jit_core()) else {
        return Ok(Incremental::WholeImage(
            "no translated core is installed".to_string(),
        ));
    };
    if core.recording() {
        // A recording replays ONE module's bytes against one block set. Two
        // live modules would make it a recording of neither, so a run that
        // asked for one keeps the whole-image path — it is a diagnostic, and
        // the speed of a diagnostic run is nobody's number.
        return Ok(Incremental::WholeImage("this run is recording".to_string()));
    }
    core.verify_now(bus);
    if let Some(why) = core.read_only_module_is_stale(bus) {
        return Ok(Incremental::WholeImage(why));
    }
    // Retire every writable module and re-walk what they held. The walk stops
    // at the read-only module's own starts, so what it claims is exactly "the
    // writable side of the image as it now stands", published code included.
    core.retire_all_but_read_only();
    let known = core.installed_starts();
    let gap_used = core.gap_used();

    let (found, discover_us) = walk(bus, seeds, usize::MAX, &known);
    if found.set.is_empty() {
        return Ok(Incremental::Nothing);
    }
    let discovery = Discovery {
        stats: found.stats,
        discover_us,
    };

    let (module, _) = build_with_halving(
        bus, &found.set, model, policy, discovery, fn_blocks, None, gap_used, false,
    )?;

    let core = hart
        .translated_core_mut()
        .and_then(|c| c.as_jit_core())
        .expect("the core was here a moment ago");
    let report = core.add(module);
    let entries: Vec<u32> = core.index.keys().copied().collect();
    hart.set_translated_entries(&entries);
    Ok(Incremental::Added(report))
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
    let Ok(at) = areas(bus, set, 0) else { return };
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
                "jit: size {size}: {} blocks in {} fn, {} B, largest body {} B \
                 (sub-dispatcher {} B, selector {} B) ({} the \
                 {}-byte budget), emitted in {:.1} ms -> {}",
                set.blocks.len(),
                emitted.functions,
                emitted.wasm.len(),
                emitted.max_body_bytes,
                emitted.max_sub_body_bytes,
                emitted.selector_bytes,
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
    let at = areas(bus, &found.set, 0)?;
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

/// Where an entry's time goes, sampled (M7 P6b, H3).
///
/// Off unless `--jit-entry-time <stride>` asked for it, because the whole
/// point of the number is a run whose wall clock nobody has perturbed: a
/// `std::time::Instant` costs about 20 ns on this desk and an entry costs
/// about 450, so timing every entry would move what it measures by a third.
/// One entry in `stride` is sampled instead, and `samples` is reported beside
/// the sums so a reader can see what the mean is a mean of.
///
/// The four sums nest: `run_ns` is the whole of [`TranslatedCore::run`], and
/// `enter_ns`, `index_ns` and `regs_ns` are pieces inside it — the module
/// call, the two entry-index lookups, and the 32-register copy out to the
/// exchange area and back. What is left over is the refusal rules, the
/// counters and the exit bookkeeping.
#[derive(Clone, Copy, Debug, Default)]
pub struct EntryTiming {
    /// Sample one entry in this many. Zero means the timer is off.
    pub stride: u64,
    /// Entries seen, sampled or not.
    pub seen: u64,
    /// Entries actually timed.
    pub samples: u64,
    /// The whole of `run`, over the sampled entries.
    pub run_ns: u64,
    /// `enter` — the module — over the same entries.
    pub enter_ns: u64,
    /// The two `index` lookups: the entry pc, and the exit pc's census.
    pub index_ns: u64,
    /// The register file out to the exchange area and back.
    pub regs_ns: u64,
    /// Entries timed with the OUTER clock pair only — no inner ones.
    pub plain_samples: u64,
    /// The whole of `run` over those, which is the honest per-entry number:
    /// the breakdown's own six `Instant::now()` calls inflate `run_ns` by
    /// their own cost and this is the control that shows by how much.
    pub plain_ns: u64,
}

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
    /// The largest body in the module, in bytes — **the outer selector
    /// included** since M7 P6c. This is the number
    /// [`BODY_BUDGET`] is checked against.
    pub max_body_bytes: usize,
    /// The largest **sub-dispatcher** body, in bytes.
    pub max_sub_body_bytes: usize,
    /// The outer selector's own body, in bytes. `O(1)` with the flat
    /// selector, `O(functions)` with the nested one.
    pub selector_bytes: usize,
    /// The per-function block budget the module was actually emitted at,
    /// which is not what the caller asked for when the caller asked for more
    /// blocks than there are.
    pub fn_blocks: usize,
    /// What the indirect-target page map and its slot arrays took.
    pub indirect_bytes: u32,
    /// The arena gap the tables went in, and what is left of it.
    pub gap_len: u32,
    pub gap_left: u32,
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
             emitted {} B in {:.1} ms in {} fn x {} blocks (largest {} B: sub-dispatcher {} B, \
             selector {} B; targets {} B of a {} B gap, {} B left); compiled in {:.1} ms; \
             instantiated in {:.2} ms",
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
            self.max_sub_body_bytes,
            self.selector_bytes,
            self.indirect_bytes,
            self.gap_len,
            self.gap_left,
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
        mmio_census::note(false, pc, address);
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
        mmio_census::note(true, pc, address);
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

/// One compiled, installed module, and everything that is per module rather
/// than per core (M7b P1).
///
/// A core holds one of these at boot and one more per `fence.i` that published
/// code the installed modules did not already hold. They never overlap: the
/// incremental walk is given every installed module's block starts as a stop
/// set, so exactly one module answers for any pc.
pub struct Module {
    core: HostCore<C6Ops>,
    /// Guest pc to this module's **local** block index — what its selector
    /// takes. The core's own index maps a pc to `(module, local)`.
    index: BTreeMap<u32, u32>,
    /// The guest bytes each block was translated from, kept so an
    /// invalidation can be answered exactly. ~4 bytes per translated
    /// instruction.
    code: Vec<(u32, Box<[u8]>)>,
    report: BuildReport,
    /// Every byte this module was translated from sits in a region the bus
    /// calls read-only, so the guest cannot change them and this module can
    /// never go stale. That is what a `fence.i` keeps.
    read_only: bool,
    /// Guest bytes **this module** was translated from have changed. It stops
    /// being entered and the next translation event replaces it; the other
    /// modules are untouched, which is sound because every edge out of a
    /// module is an exit and the hart re-enters through the index.
    stale: bool,
    /// The module's own bytes, kept only while a recording wants them.
    wasm: Option<Vec<u8>>,
    /// Where this module's tables ended up.
    at: Areas,
}

/// A translated core for this machine: **one or more modules**, and one index
/// over all of them.
pub struct JitCore {
    /// Installed modules, oldest first. Index 0 is the boot module.
    mods: Vec<Module>,
    /// Guest pc to `(module, local block index)`. One lookup per entry, at the
    /// ~19,500 entries per emulated second the slice cap implies.
    index: BTreeMap<u32, (u32, u32)>,
    /// The cost model the modules were emitted against. A block's budget check
    /// has it folded in as a constant, so a model change is not something a
    /// byte re-check would notice.
    model: CycleModel,
    stats: JitStats,
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
    /// Where an entry's time goes, under `LP_EMU_JIT_ENTRY_TIME=<stride>`
    /// (M7 P6b). An env var rather than a flag for the same reason
    /// [`exit_sites`](Self::exit_sites) is one: it is a diagnostic nobody
    /// runs by accident, and it perturbs the very wall clock the rest of the
    /// run reports.
    timing: EntryTiming,
    /// A recording in progress, and the cycle count it starts at.
    ///
    /// A recording is of **one** module — it replays a block set against a
    /// module's own bytes — so a run that asked for one never takes the
    /// incremental path. See [`crate::jit::install_incremental`].
    recorder: Option<(Recorder, u64)>,
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
    /// The arena gap the three tables were placed in, and what is left of it
    /// past them. A second live module's tables have to come out of the same
    /// gap (M7b P1), so the number is reported rather than assumed.
    gap_len: u32,
    gap_left: u32,
}

/// Place the permission table and the exchange area in the arena's largest
/// gap, and say how much of the arena a wasm memory can cover.
///
/// `used` is how much of the gap earlier modules have already taken. The
/// permission table and the exchange area are **shared** — one copy, at the
/// gap's own base, whatever the module — because they hold the same bytes for
/// every module and every module's [`Layout`] folds in the same offsets. The
/// **indirect-target tables are not**: a slot holds a block index, and a block
/// index only means something inside the module it was emitted with, so each
/// live module gets its own slice of the gap past everything already placed
/// (M7b P1, DD18).
///
/// On the C6 that is not a tight budget: the gap between the mask ROM's data
/// and HP SRAM is **218,103,808 bytes**, and a whole-image table is about
/// 6.6 MB of it. The boot line reports what is left after each module so a
/// chip whose map is meaner says so rather than failing at the third event.
fn areas(bus: &SocBus, set: &BlockSet, used: u32) -> Result<Areas, String> {
    let arena_len = bus.guest_arena().len();
    let pages = (arena_len / 65536) as u64;
    if pages == 0 {
        return Err("the guest arena is smaller than one wasm page".into());
    }
    let indirect_len = u32::try_from(target_table_bytes(set))
        .map_err(|_| "the indirect-target tables do not fit a 32-bit offset".to_string())?;
    let shared = u64::from(PERM_ENTRIES) + u64::from(EXCHANGE_LEN);
    let need = u64::from(used).max(shared) + u64::from(indirect_len);
    let (gap_base, gap_len) = bus
        .largest_arena_gap()
        .ok_or_else(|| "the guest arena has no gap for the translator's tables".to_string())?;
    if u64::from(gap_len) < need {
        return Err(format!(
            "the arena's largest gap is {gap_len} bytes, {used} of them are already a live \
             module's tables, and this module's need {indirect_len} more"
        ));
    }
    let perm_at = gap_base - bus.guest_arena_base();
    let exchange_at = perm_at + PERM_ENTRIES;
    let indirect_at = perm_at + (used.max(shared as u32));
    if u64::from(indirect_at) + u64::from(indirect_len) > pages * 65536 {
        return Err("the translator's tables fall outside the wasm memory".into());
    }
    Ok(Areas {
        perm_at,
        exchange_at,
        indirect_at,
        indirect_len,
        pages,
        gap_len,
        gap_left: gap_len - (need as u32),
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

impl Module {
    /// Translate `set` and build one module for it, past whatever `gap_used`
    /// bytes of the arena gap earlier modules already hold.
    ///
    /// # Errors
    ///
    /// A block set that does not fit the arena's shape, or a module wasmtime
    /// will not compile.
    fn build(
        bus: &mut SocBus,
        set: &BlockSet,
        model: CycleModel,
        policy: Emit,
        discovery: Discovery,
        fn_blocks: usize,
        record: Option<&RecordRequest>,
        gap_used: u32,
        read_only: bool,
    ) -> Result<(Self, Option<(Recorder, u64)>), String> {
        let at = areas(bus, set, gap_used)?;
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
        //
        // `max_body_bytes` is the largest body in the module *including the
        // outer selector* since M7 P6c. It used to be the largest
        // sub-dispatcher, which is the same number only while the
        // sub-dispatchers are the big functions — below 64 blocks a function
        // they are not, and the check was blind at exactly the sizes DD20
        // moved the default towards.
        if emitted.max_body_bytes > BODY_BUDGET {
            let (what, other) = if emitted.selector_bytes > emitted.max_sub_body_bytes {
                ("outer selector", "largest sub-dispatcher")
            } else {
                ("largest sub-dispatcher", "outer selector")
            };
            return Err(format!(
                "at {fn_blocks} blocks per function the {what} is {} bytes, over the \
                 {BODY_BUDGET}-byte budget (the {other} is {} B); use fewer (--jit-fn-blocks)",
                emitted.max_body_bytes,
                emitted.max_sub_body_bytes.min(emitted.selector_bytes),
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

        Ok((
            Self {
                core,
                index,
                code,
                read_only,
                stale: false,
                wasm,
                at,
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
                    max_sub_body_bytes: emitted.max_sub_body_bytes,
                    selector_bytes: emitted.selector_bytes,
                    fn_blocks: fn_blocks.min(set.blocks.len()),
                    indirect_bytes: at.indirect_len,
                    gap_len: at.gap_len,
                    gap_left: at.gap_left,
                },
            },
            recorder,
        ))
    }

    /// Check whether the guest bytes **this module** was translated from are
    /// still what it was translated from, and say how many blocks changed.
    fn changed_blocks(&self, bus: &SocBus) -> u64 {
        let base = bus.guest_arena_base();
        let arena = bus.guest_arena();
        self.code
            .iter()
            .filter(|(pc, bytes)| {
                let Ok(at) = usize::try_from(u64::from(*pc) - u64::from(base)) else {
                    return true;
                };
                !arena
                    .get(at..at + bytes.len())
                    .is_some_and(|now| now == &bytes[..])
            })
            .count() as u64
    }
}

impl JitCore {
    /// A core holding one module, the way boot installs it.
    fn of(module: Module, recorder: Option<(Recorder, u64)>, model: CycleModel) -> Self {
        let index = module
            .index
            .iter()
            .map(|(&pc, &b)| (pc, (0u32, b)))
            .collect();
        Self {
            mods: vec![module],
            index,
            model,
            stats: JitStats::default(),
            verify_pending: false,
            dead: false,
            recorder,
            last_exit: None,
            exit_sites: std::env::var_os("LP_EMU_JIT_EXITS").map(|_| BTreeMap::new()),
            timing: EntryTiming {
                stride: std::env::var("LP_EMU_JIT_ENTRY_TIME")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .filter(|&n| n > 0)
                    .unwrap_or(0),
                ..EntryTiming::default()
            },
        }
    }

    /// Add an already-built module beside the ones this core holds, and say
    /// what it cost.
    ///
    /// The index is extended rather than rebuilt: the incremental walk was
    /// given every installed module's starts as a stop set, so no pc it claims
    /// is one an earlier module answers for. The `debug_assert` is that rule,
    /// checked.
    fn add(&mut self, module: Module) -> BuildReport {
        let m = self.mods.len() as u32;
        for (&pc, &b) in &module.index {
            let clash = self.index.insert(pc, (m, b));
            debug_assert!(
                clash.is_none(),
                "two modules answer for {pc:#010x}: the incremental walk was given the \
                 wrong stop set"
            );
        }
        let report = module.report;
        self.mods.push(module);
        report
    }

    /// Every block start the installed modules hold — the stop set the next
    /// incremental walk is given.
    #[must_use]
    pub fn installed_starts(&self) -> BTreeSet<u32> {
        self.index.keys().copied().collect()
    }

    /// How much of the arena gap the installed modules' tables have taken.
    ///
    /// Computed rather than remembered, because a retired module hands its
    /// slice back and the next module may have it.
    #[must_use]
    pub fn gap_used(&self) -> u32 {
        self.mods
            .iter()
            .map(|m| (m.at.indirect_at - m.at.perm_at) + m.at.indirect_len)
            .max()
            .unwrap_or(0)
    }

    /// Whether module 0 — the read-only one — went stale, and what changed.
    ///
    /// It should not be able to: the guest cannot write the bytes it was
    /// translated from. If it ever does, something moved memory the bus calls
    /// read-only, the split's whole premise is gone, and the honest answer is
    /// to retranslate the image rather than to trust a module over a
    /// measurement.
    #[must_use]
    pub fn read_only_module_is_stale(&self, bus: &SocBus) -> Option<String> {
        let m = self.mods.iter().find(|m| m.read_only && m.stale)?;
        Some(format!(
            "a read-only module went stale: {} of its {} block(s) no longer match the \
             guest's bytes",
            m.changed_blocks(bus),
            m.code.len(),
        ))
    }

    /// Drop every module but the read-only one, and the index entries they
    /// answered for.
    ///
    /// This **is** the whole-module retire (DD18), applied to the modules whose
    /// bytes a publish can have changed. Dropping the module drops its
    /// `HostCore`, which in the browser hands its function-table slot back.
    pub fn retire_all_but_read_only(&mut self) {
        let keep: Vec<usize> = self
            .mods
            .iter()
            .enumerate()
            .filter(|(_, m)| m.read_only)
            .map(|(i, _)| i)
            .collect();
        // Read-only modules are emitted first and never retired, so the ones
        // that stay keep the indices they had and the index needs no
        // renumbering — asserted rather than assumed.
        debug_assert!(
            keep.iter().enumerate().all(|(i, &m)| i == m),
            "the read-only modules are not the first ones"
        );
        self.mods.truncate(keep.len());
        let live = keep.len() as u32;
        self.index.retain(|_, &mut (m, _)| m < live);
    }

    /// Whether a recording is running, which is what keeps a run that asked
    /// for one on the whole-image path.
    #[must_use]
    pub fn recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// Re-check every module's bytes now rather than at the next entry, and
    /// say whether any module went stale.
    ///
    /// The machine asks this at a translation event, because whether a module
    /// is stale is what decides between adding a module and replacing the lot
    /// (DD18: the whole-module retire stays).
    pub fn verify_now(&mut self, bus: &SocBus) -> Option<String> {
        if self.verify_pending {
            self.verify(bus);
        }
        let stale: Vec<usize> = self
            .mods
            .iter()
            .enumerate()
            .filter(|(_, m)| m.stale)
            .map(|(i, _)| i)
            .collect();
        if stale.is_empty() {
            return None;
        }
        // Named rather than counted: which module went stale, and how many of
        // its blocks the guest rewrote, is the difference between "the shader
        // buffer moved" and "the whole image is being re-emitted every event
        // for four blocks of stack".
        let mut out = String::from("the guest rewrote code these modules were translated from:");
        for i in stale {
            let m = &self.mods[i];
            let changed = m.changed_blocks(bus);
            out.push_str(&format!(
                " module {i} ({changed} of {} block(s)",
                m.code.len()
            ));
            let mut named = 0;
            for (pc, bytes) in &m.code {
                if named == 4 {
                    break;
                }
                let at = (u64::from(*pc) - u64::from(bus.guest_arena_base())) as usize;
                let same = bus
                    .guest_arena()
                    .get(at..at + bytes.len())
                    .is_some_and(|now| now == &bytes[..]);
                if !same {
                    out.push_str(&format!(", {pc:#010x}"));
                    named += 1;
                }
            }
            out.push(')');
        }
        Some(out)
    }

    /// Check whether the guest bytes each module was translated from are
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
    ///
    /// # Per module since M7b P1
    ///
    /// A core holds one module per translation event now, and the retire is
    /// per module rather than per core. That is sound for exactly the reason
    /// the whole-module retire exists: every edge **out of** a module is an
    /// exit, so entering a module can only run that module's own blocks. A
    /// module whose every block's bytes are unchanged is still a correct
    /// translation of them whatever happened to the module next door.
    fn verify(&mut self, bus: &SocBus) {
        self.verify_pending = false;
        self.stats.invalidations += 1;
        for m in &mut self.mods {
            if m.stale {
                continue;
            }
            let dropped = m.changed_blocks(bus);
            if dropped > 0 {
                self.stats.dropped_blocks += dropped;
                m.stale = true;
                log::debug!(
                    "jit: {dropped} translated block(s) no longer match the guest's bytes; \
                     that module is stale until the next translation event"
                );
            }
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
            self.mods[0].wasm = None;
            return;
        }
        // A recording is of ONE module, which is why a run that asked for one
        // never takes the incremental path (`install_incremental`).
        let m = &mut self.mods[0];
        let wasm = m.wasm.take().unwrap_or_default();
        let out = r.finish(
            &wasm,
            bus.guest_arena(),
            m.at.pages,
            m.at.exchange_at,
            m.report.fn_blocks,
            m.report.blocks,
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
        for m in &self.mods {
            let at = m.code.partition_point(|(start, _)| *start <= pc);
            if at > 0 {
                let (start, bytes) = &m.code[at - 1];
                if pc < start.wrapping_add(bytes.len() as u32) {
                    return format!("inside the block at {start:#010x}");
                }
            }
        }
        "not discovered".to_string()
    }

    #[must_use]
    pub fn stats(&self) -> JitStats {
        self.stats
    }

    /// The **last** module's build report — what the translation event that
    /// just ran cost. JD20's boot line is per event, so this is per event too.
    #[must_use]
    pub fn build_report(&self) -> BuildReport {
        self.mods.last().expect("a core holds a module").report
    }

    /// Every installed module's report added together — what the run has paid
    /// for translation and what it is running.
    ///
    /// The counts that add (blocks, instructions, bytes, milliseconds) add;
    /// the ones that do not (the largest body, the split the last module was
    /// emitted at, the gap) take the largest or the latest and say so by being
    /// named that in the report line.
    #[must_use]
    pub fn totals(&self) -> BuildReport {
        let mut out = self.mods[0].report;
        for m in &self.mods[1..] {
            let r = m.report;
            out.discovery.stats.blocks += r.discovery.stats.blocks;
            out.discovery.stats.insts += r.discovery.stats.insts;
            out.discovery.stats.seeds += r.discovery.stats.seeds;
            out.discovery.stats.starts += r.discovery.stats.starts;
            out.discovery.stats.undecodable += r.discovery.stats.undecodable;
            out.discovery.stats.empty_starts += r.discovery.stats.empty_starts;
            out.discovery.stats.truncated |= r.discovery.stats.truncated;
            out.discovery.discover_us += r.discovery.discover_us;
            out.blocks += r.blocks;
            out.insts += r.insts;
            out.native_insts += r.native_insts;
            out.escaped_insts += r.escaped_insts;
            out.module_bytes += r.module_bytes;
            out.emit_us += r.emit_us;
            out.compile_us += r.compile_us;
            out.instantiate_us += r.instantiate_us;
            out.functions += r.functions;
            out.max_body_bytes = out.max_body_bytes.max(r.max_body_bytes);
            out.max_sub_body_bytes = out.max_sub_body_bytes.max(r.max_sub_body_bytes);
            out.selector_bytes = out.selector_bytes.max(r.selector_bytes);
            out.fn_blocks = r.fn_blocks;
            out.indirect_bytes += r.indirect_bytes;
            out.gap_left = r.gap_left;
        }
        out
    }
}

impl TranslatedCore<SocBus> for JitCore {
    fn run(&mut self, hart: &mut MachineHart<SocBus>, bus: &mut SocBus, end: u64) -> RunOutcome {
        if self.dead {
            self.stats.refused_no_entry += 1;
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
        }
        // M7 P6b H3. `timed` is `None` on every entry the stride skips, and
        // an `Option` test is the whole cost of the timer being compiled in.
        // Alternate samples: one carries the inner breakdown, the next carries
        // only the outer pair. Six `Instant::now()` calls inside a 450 ns
        // entry are not free, and `plain` is the control that prices them.
        let (timed, breakdown) = if self.timing.stride != 0 {
            let take = self.timing.seen % self.timing.stride == 0;
            let breakdown = (self.timing.seen / self.timing.stride) % 2 == 0;
            self.timing.seen += 1;
            (take.then(std::time::Instant::now), breakdown)
        } else {
            (None, false)
        };
        let inner = if breakdown { timed } else { None };
        let pc = hart.pc();
        let t_index = inner.map(|_| std::time::Instant::now());
        let found = self.index.get(&pc).copied();
        if let Some(t) = t_index {
            self.timing.index_ns += t.elapsed().as_nanos() as u64;
        }
        let Some((module, entry)) = found else {
            self.stats.refused_no_entry += 1;
            return RunOutcome::Refused;
        };
        let module = module as usize;
        // Staleness is per module since M7b P1: the module that holds this pc
        // is the only one whose bytes matter for entering here.
        if self.mods[module].stale {
            self.stats.refused_stale += 1;
            return RunOutcome::Refused;
        }
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
        let t_regs = inner.map(|_| std::time::Instant::now());
        {
            let ops = self.mods[module].core.ops_mut();
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
        if let Some(t) = t_regs {
            self.timing.regs_ns += t.elapsed().as_nanos() as u64;
        }
        let t_enter = inner.map(|_| std::time::Instant::now());
        let exit = self.mods[module]
            .core
            .enter(entry, cycle, instret, end, watch);
        if let Some(t) = t_enter {
            self.timing.enter_ns += t.elapsed().as_nanos() as u64;
        }
        let ops = self.mods[module].core.ops_mut();
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
        let t_regs_out = inner.map(|_| std::time::Instant::now());
        let mut regs = *hart.regs();
        {
            let x = self.mods[module].core.ops_mut().exchange();
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
        if let Some(t) = t_regs_out {
            self.timing.regs_ns += t.elapsed().as_nanos() as u64;
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
        let t_index2 = inner.map(|_| std::time::Instant::now());
        let known = self.index.contains_key(&exit.pc);
        if let Some(t) = t_index2 {
            self.timing.index_ns += t.elapsed().as_nanos() as u64;
        }
        if known {
            self.stats.exit_known_pc += 1;
        } else {
            self.stats.exit_unknown_pc += 1;
        }
        let why = (exit_why as usize).min(WHY_CODES - 1);
        self.stats.why[why].0 += 1;
        self.last_exit = Some((exit.pc, hart.instruction_count(), why));

        if let Some(t) = timed {
            if breakdown {
                self.timing.run_ns += t.elapsed().as_nanos() as u64;
                self.timing.samples += 1;
            } else {
                self.timing.plain_ns += t.elapsed().as_nanos() as u64;
                self.timing.plain_samples += 1;
            }
        }
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

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
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
        // M7 P6b H3: where an entry's time goes, on its own line so nothing
        // that parses the report line has to know about it and so it is
        // absent from every run that did not ask for it.
        let t = self.timing;
        if t.samples > 0 {
            let mean = |ns: u64| ns as f64 / t.samples as f64;
            let whole = mean(t.run_ns);
            let share = |ns: u64| 100.0 * mean(ns) / whole;
            eprintln!(
                "jit: entry cost: {} broken-down + {} plain sample(s) of {} entries (1 in {}); \
                 run {:.1} ns unclocked inside, {:.1} ns with the breakdown's own six clocks = \
                 enter {:.1} ns ({:.1} %) + index {:.1} ns ({:.1} %) + regs {:.1} ns ({:.1} %) \
                 + {:.1} ns ({:.1} %) of refusal rules, counters and exit bookkeeping",
                t.samples,
                t.plain_samples,
                t.seen,
                t.stride,
                if t.plain_samples > 0 {
                    t.plain_ns as f64 / t.plain_samples as f64
                } else {
                    f64::NAN
                },
                whole,
                mean(t.enter_ns),
                share(t.enter_ns),
                mean(t.index_ns),
                share(t.index_ns),
                mean(t.regs_ns),
                share(t.regs_ns),
                whole - mean(t.enter_ns) - mean(t.index_ns) - mean(t.regs_ns),
                100.0 - share(t.enter_ns) - share(t.index_ns) - share(t.regs_ns),
            );
        }
        let r = self.totals();
        let s = self.stats;
        let d = r.discovery.stats;
        format!(
            "discovered {} blocks / {} instr from {} seeds ({} starts, {} ended undecodable, \
             {} named no code{}); \
             installed {} blocks / {} instr ({} emitted, {} escaped, {:.1} % static escape); \
             {} module(s), {} B in {} fn x {} blocks (largest body {} B, target tables {} B), \
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
            self.mods.len(),
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

/// M7 P6c (Q2): a census of every MMIO operation translated code performs.
///
/// **Off unless `LP_EMU_JIT_MMIO_CENSUS` is set**, like `LP_EMU_JIT_EXITS`
/// and P6b's `LP_EMU_JIT_ENTRY_TIME`: it is a diagnostic nobody runs by
/// accident, it allocates per distinct address and per distinct guest pc, and
/// it perturbs the wall clock the rest of the run reports.
///
/// **Why a thread-local and not a field.** The census has to span the whole
/// run, and a run has three cores on `render-basic` t2 — boot plus two
/// `fence.i` retranslations, each of which *replaces* the [`JitCore`]
/// (JD5/P5). A counter on the core would be thrown away twice and the census
/// would report the last third of the run. The emulator is single-threaded on
/// every target this runs on (`wasm32-wasip1` has no threads at all), so a
/// `thread_local!` is the run.
///
/// **What it counts.** Only MMIO that *translated code* issued, because the
/// two [`HostOps`] callbacks are the translated module's only way out to the
/// bus. Interpreted MMIO — inside the 6.54 % of retired instructions that run
/// outside translated code — is not in it, and the report says so in its
/// first line rather than implying a total.
pub mod mmio_census {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use lp_emu_esp_common::bus::SocBus;
    use lp_emu_esp_common::periph::Peripheral;

    /// `[loads, stores]` — the two columns every row of the census has.
    type Ops = [u64; 2];

    #[derive(Default)]
    struct Census {
        /// Guest address → `[loads, stores]`. One entry per distinct
        /// register, which is a few hundred on a render run.
        by_address: BTreeMap<u32, Ops>,
        /// Guest pc → `[loads, stores]`. One entry per distinct instruction
        /// that performs MMIO.
        by_pc: BTreeMap<u32, Ops>,
        loads: u64,
        stores: u64,
    }

    thread_local! {
        static CENSUS: RefCell<Option<Census>> = const { RefCell::new(None) };
    }

    /// Whether the census is on, read once from the environment.
    ///
    /// A `OnceLock` rather than a per-call `var_os`, because this is
    /// consulted on every MMIO operation — tens of millions on
    /// `render-basic` t2 — and an environment lookup there would be the
    /// measurement's own cost.
    #[must_use]
    pub fn on() -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("LP_EMU_JIT_MMIO_CENSUS").is_some())
    }

    /// Record one MMIO operation issued from translated code.
    #[inline]
    pub fn note(store: bool, pc: u32, address: u32) {
        if !on() {
            return;
        }
        let k = usize::from(store);
        CENSUS.with(|c| {
            let mut slot = c.borrow_mut();
            let c = slot.get_or_insert_with(Census::default);
            c.by_address.entry(address).or_default()[k] += 1;
            c.by_pc.entry(pc).or_default()[k] += 1;
            if store {
                c.stores += 1;
            } else {
                c.loads += 1;
            }
        });
    }

    /// The census as report lines, or `None` when it was never turned on.
    ///
    /// `bus` turns an address back into `PERIPHERAL+0xoff name` out of the
    /// peripheral's own [`Peripheral::reg_name`], which is the same
    /// resolution a trace line gets — so there is no second copy of the
    /// memory map here to drift away from the machine's.
    #[must_use]
    pub fn report(bus: &SocBus) -> Option<String> {
        CENSUS.with(|c| {
            let slot = c.borrow();
            let c = slot.as_ref()?;
            let spans = bus.peripheral_spans();
            let where_is = |address: u32| -> (String, String) {
                for &(base, len, i) in &spans {
                    if address.wrapping_sub(base) < len {
                        let off = address - base;
                        let p = bus.peripheral(i);
                        let block = p.map_or("?", Peripheral::name);
                        let reg = p.and_then(|p| p.reg_name(off)).unwrap_or("");
                        return (block.to_string(), format!("+{off:#06x} {reg}"));
                    }
                }
                ("(unmapped)".to_string(), format!(" {address:#010x}"))
            };

            let total = c.loads + c.stores;
            let of = |n: u64| 100.0 * n as f64 / total.max(1) as f64;
            let mut out = String::new();
            out.push_str(&format!(
                "jit: mmio census (translated code only): {total} operation(s) — {} load(s), \
                 {} store(s), over {} distinct register(s) and {} distinct guest pc(s)\n",
                c.loads,
                c.stores,
                c.by_address.len(),
                c.by_pc.len(),
            ));

            // By peripheral: "which model answers most of them", the question
            // the ladder's M4 census answered for the harness image.
            let mut by_block: BTreeMap<String, Ops> = BTreeMap::new();
            for (&a, ops) in &c.by_address {
                let e = by_block.entry(where_is(a).0).or_default();
                e[0] += ops[0];
                e[1] += ops[1];
            }
            let mut blocks: Vec<_> = by_block.into_iter().collect();
            blocks.sort_by_key(|(_, o)| std::cmp::Reverse(o[0] + o[1]));
            for (block, o) in blocks {
                let n = o[0] + o[1];
                out.push_str(&format!(
                    "jit: mmio by peripheral: {block:<22} {n:>12} ({:>6.2} %)  {:>12} load(s), \
                     {:>12} store(s)\n",
                    of(n),
                    o[0],
                    o[1],
                ));
            }

            // By register, top 24.
            let mut regs: Vec<_> = c.by_address.iter().map(|(&a, o)| (a, *o)).collect();
            regs.sort_by_key(|(_, o)| std::cmp::Reverse(o[0] + o[1]));
            for (a, o) in regs.iter().take(24) {
                let (block, off) = where_is(*a);
                let n = o[0] + o[1];
                out.push_str(&format!(
                    "jit: mmio by register: {:<34} {n:>12} ({:>6.2} %)  {:>12} load(s), \
                     {:>12} store(s)\n",
                    format!("{block}{off}"),
                    of(n),
                    o[0],
                    o[1],
                ));
            }

            // By guest pc, top 20 — the brief's number, and the one that says
            // whether the traffic is a handful of loops or a spread.
            let mut pcs: Vec<_> = c.by_pc.iter().map(|(&p, o)| (p, *o)).collect();
            pcs.sort_by_key(|(_, o)| std::cmp::Reverse(o[0] + o[1]));
            for (pc, o) in pcs.iter().take(20) {
                let n = o[0] + o[1];
                out.push_str(&format!(
                    "jit: mmio by guest pc: {pc:#010x} {n:>12} ({:>6.2} %)  {:>12} load(s), \
                     {:>12} store(s)\n",
                    of(n),
                    o[0],
                    o[1],
                ));
            }
            Some(out)
        })
    }
}
