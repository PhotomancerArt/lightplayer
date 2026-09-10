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

use std::collections::BTreeMap;

use lp_emu_core::{Bus, CycleModel};
use lp_emu_esp_common::bus::{PERM_NONE, PERM_READ, PERM_READ_WRITE, PERMISSION_PAGE_LEN, SocBus};
use lp_emu_jit::blocks::BlockSet;
use lp_emu_jit::host::{
    self, EXCHANGE_LEN, FLAG_AFTER_STORE, FLAG_SLICE_ENDED, HostOps, MMIO_LEAVE_AFTER, MMIO_OK,
    MMIO_PENDING, MMIO_REFUSED, MmioLoad, MmioStore, PERM_ENTRIES, PERM_SHIFT, STEP_CONTINUE,
    STEP_SLICE_ENDED, StepOne, load_kind, store_kind,
};
use lp_emu_jit::host_wasmtime::WasmtimeCore;
use lp_emu_jit::translate::{Emit, Emitted, Layout, emit};
use lp_riscv_emu::mach::translated::{RunOutcome, TranslatedCore};
use lp_riscv_emu::mach::{MachineHart, SliceEnd};

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

/// Sweep a block set from `seeds`, translate it, and install it on `hart`.
///
/// # Errors
///
/// Nothing reachable to translate, an arena with no room for the tables, or a
/// module wasmtime will not compile.
pub fn install(
    hart: &mut MachineHart<SocBus>,
    bus: &mut SocBus,
    seeds: &[u32],
    max_blocks: usize,
    model: CycleModel,
    policy: Emit,
) -> Result<BuildReport, String> {
    let set = {
        let spans = bus.region_spans();
        let base = bus.guest_arena_base();
        let arena = bus.guest_arena();
        BlockSet::sweep(seeds, max_blocks, &mut |pc| {
            arena_word(arena, base, &spans, pc)
        })
    };
    if set.is_empty() {
        return Err(format!(
            "nothing translatable is reachable from {:#010x}",
            seeds.first().copied().unwrap_or(0)
        ));
    }
    let entries = set.entries().to_vec();
    let core = JitCore::build(bus, &set, model, policy)?;
    let report = core.build_report();
    hart.set_translated_core(Box::new(core), &entries);
    Ok(report)
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
    /// Guest instructions handed back to the interpreter from inside
    /// translated code (JD10). Allowed to be non-zero; not allowed to be
    /// unmeasured.
    pub escape_hatch: u64,
    /// Guest instructions retired inside translated code, escapes included.
    pub retired: u64,
}

/// What the machine asked for, so a report can say what it got.
#[derive(Clone, Copy, Debug)]
pub struct BuildReport {
    pub blocks: usize,
    pub insts: usize,
    pub native_insts: usize,
    pub escaped_insts: usize,
    pub module_bytes: usize,
    pub emit_us: u128,
    pub compile_us: u128,
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
        match read {
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
        }
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
        match written {
            Ok(()) => {
                if bus.sideband_or_yield_pending() {
                    MMIO_LEAVE_AFTER
                } else {
                    MMIO_OK
                }
            }
            Err(_) => MMIO_REFUSED,
        }
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
    core: WasmtimeCore<C6Ops>,
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
}

/// Where a translated module's three areas sit in the arena.
#[derive(Clone, Copy, Debug)]
struct Areas {
    perm_at: u32,
    exchange_at: u32,
    pages: u64,
}

/// Place the permission table and the exchange area in the arena's largest
/// gap, and say how much of the arena a wasm memory can cover.
fn areas(bus: &SocBus) -> Result<Areas, String> {
    let arena_len = bus.guest_arena().len();
    let pages = (arena_len / 65536) as u64;
    if pages == 0 {
        return Err("the guest arena is smaller than one wasm page".into());
    }
    let need = PERM_ENTRIES + EXCHANGE_LEN;
    let (gap_base, gap_len) = bus
        .largest_arena_gap()
        .ok_or_else(|| "the guest arena has no gap for the translator's tables".to_string())?;
    if gap_len < need {
        return Err(format!(
            "the arena's largest gap is {gap_len} bytes and the translator's tables need {need}"
        ));
    }
    let perm_at = gap_base - bus.guest_arena_base();
    let exchange_at = perm_at + PERM_ENTRIES;
    if u64::from(exchange_at) + u64::from(EXCHANGE_LEN) > pages * 65536 {
        return Err("the translator's tables fall outside the wasm memory".into());
    }
    Ok(Areas {
        perm_at,
        exchange_at,
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
    ) -> Result<Self, String> {
        let at = areas(bus)?;
        write_permission_table(bus, at);

        let layout = Layout {
            memory_pages: at.pages,
            guest_base: bus.guest_arena_base(),
            // The memory *is* the arena, so the arena starts at offset zero
            // and the fold is one subtract. In the browser this is the arena
            // `Vec`'s own address inside the emulator's memory.
            arena_offset: 0,
            perm_offset: at.perm_at,
            exchange_offset: at.exchange_at,
        };

        let started = std::time::Instant::now();
        let emitted: Emitted = emit(set, model, layout, policy);
        let emit_us = started.elapsed().as_micros();
        // wasm's implementation limit on a single function body, and this
        // translator emits one function for the whole block set. Checked here
        // rather than left to the engine, because every engine reports it
        // differently and none of them say what to do about it. Two-level
        // dispatch is P5's (JD8); until then the answer is a smaller set.
        const MAX_FUNCTION_BODY: usize = 7_654_321;
        if emitted.wasm.len() >= MAX_FUNCTION_BODY {
            return Err(format!(
                "{} blocks emit a {}-byte module and wasm caps one function body at \
                 {MAX_FUNCTION_BODY} bytes; translate fewer blocks (--jit-blocks) until P5 \
                 splits the module",
                set.blocks.len(),
                emitted.wasm.len(),
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
        let arena_base = bus.guest_arena_base();
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

        let arena_len = bus.guest_arena().len();
        let exchange = bus.guest_arena_mut()[at.exchange_at as usize..].as_mut_ptr();
        let arena_ptr = bus.guest_arena_mut().as_mut_ptr();
        let ops = C6Ops {
            hart: core::ptr::null_mut(),
            bus: core::ptr::null_mut(),
            exchange,
            slice_end: None,
            escape_hatch: 0,
        };
        let started = std::time::Instant::now();
        // SAFETY: `arena_base` is the bus's own arena, allocated once during
        // construction from the chip's declared memory map and documented as
        // not moving for the life of the machine; the memory wasmtime is given
        // covers only whole wasm pages of it. Nothing holds a Rust reference
        // into the arena while translated code runs: `JitCore::run` reaches the
        // bus through the raw pointer it just set, and does not touch it
        // otherwise.
        let core = unsafe { WasmtimeCore::new(&emitted.wasm, ops, arena_ptr, arena_len) }
            .map_err(|e| format!("the translated module did not build: {e:?}"))?;
        let compile_us = started.elapsed().as_micros();

        Ok(Self {
            core,
            index,
            code,
            model,
            stats: JitStats::default(),
            verify_pending: false,
            dead: false,
            report: BuildReport {
                blocks: set.blocks.len(),
                insts: set.inst_count(),
                native_insts: emitted.native_insts,
                escaped_insts: emitted.escaped_insts,
                module_bytes: emitted.wasm.len(),
                emit_us,
                compile_us,
            },
        })
    }

    /// Drop every block whose guest bytes are no longer what it was
    /// translated from.
    ///
    /// This is the answer to an invalidation, and it is **exact**: a block
    /// whose bytes are unchanged is still a correct translation of them,
    /// whatever the reason the machine gave for asking. It is what keeps a
    /// `fence.i` — the guest publishing a shader it wrote into RAM — from
    /// costing the translation of the app's read-only `.text`, which cannot
    /// have changed and which is most of what a render loop runs.
    ///
    /// The spike re-checked these bytes on **every entry**; JD1 dropped that
    /// in favour of `fence.i` as the trigger. This is the same check on the
    /// same bytes, driven by the trigger rather than by the clock: once per
    /// invalidation, over ~10 KB.
    fn verify(&mut self, bus: &SocBus) {
        self.verify_pending = false;
        self.stats.invalidations += 1;
        let base = bus.guest_arena_base();
        let arena = bus.guest_arena();
        let mut dropped = 0u64;
        self.index.retain(|_, &mut i| {
            let (pc, bytes) = &self.code[i as usize];
            let at = match usize::try_from(u64::from(*pc) - u64::from(base)) {
                Ok(at) => at,
                Err(_) => return false,
            };
            let same = arena
                .get(at..at + bytes.len())
                .is_some_and(|now| now == &bytes[..]);
            if !same {
                dropped += 1;
            }
            same
        });
        self.stats.dropped_blocks += dropped;
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
        {
            let ops = self.core.ops_mut();
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
            regs[0] = 0;
            *hart.regs_mut() = regs;
            hart.set_pc(exit.pc);
            hart.set_counters(cycle_count, instruction_count);
            self.stats.retired += instruction_count.saturating_sub(instret);
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
        // same, exact, question. What it costs is one pass over ~10 KB at each
        // of the ~98 cache refills and one `fence.i` a render run performs.
        //
        // P3 does not retranslate what it drops — the two translation events
        // are P4's (JD5) — so coverage only falls.
        self.verify_pending = true;
    }

    fn report(&self) -> String {
        let r = self.report;
        let s = self.stats;
        format!(
            "{} blocks / {} instr ({} emitted, {} escaped, {:.1} % static escape); \
             module {} B, emit {:.2} ms, compile {:.2} ms; \
             entries {}, retired {}, escape_hatch {}, \
             invalidations {} (dropped {} blocks), \
             refused_pending {}, refused_watch {}, refused_no_entry {}, refused_impure {}",
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
            r.emit_us as f64 / 1000.0,
            r.compile_us as f64 / 1000.0,
            s.entries,
            s.retired,
            s.escape_hatch,
            s.invalidations,
            s.dropped_blocks,
            s.refused_pending,
            s.refused_watch,
            s.refused_no_entry,
            s.refused_impure,
        )
    }
}
