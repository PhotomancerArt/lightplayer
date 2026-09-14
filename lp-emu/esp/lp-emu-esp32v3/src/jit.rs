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
//! twice in this phase — the shared-compilation path is P07's.
//!
//! # What crosses the seam (M7 P06)
//!
//! The module keeps the current window in locals and the physical `AR` file
//! in the exchange area (XD8), so the whole architectural head — the 64
//! words, `WindowBase`, `WindowStart`, `SAR`, the loop registers,
//! `PS.CALLINC` — is marshalled **hart → exchange at every entry and
//! exchange → hart at every exit**, through
//! [`lp_xt_jit::replay::marshal`], the one copy of that wire format. The
//! escape hatch marshals the same both ways around `XtHart::step_one`, and
//! the fused poll marshals the words a polling point can observe
//! (`PS.CALLINC`, the loop registers) before it runs
//! [`XtHart::poll_after_store`] — the hart's own polling point (c), which is
//! the plan's "one polling point, one copy" (DD111).
//!
//! # The refusals are the design
//!
//! Every condition below is a **refusal**, never an assertion: refusing hands
//! the slice back to the interpreter, which is always correct and only ever
//! slow. A translated core that cannot be exact about something does not try.
//! The entry-time refusals P04 learned from the C6 gain three Xtensa ones:
//! `PS.WOE` clear or `PS.EXCM` set (the window precondition is meaningless
//! there; the vector region runs interpreted), an `IBREAK` armed (the
//! interpreter tests it per instruction and a stay cannot), and `LCOUNT != 0`
//! with a live `LEND` the walk never marked (the interpreter would loop back
//! where the module could not).
//!
//! # Diagnostics, read once from the environment
//!
//! - `LP_EMU_XT_JIT_ESCAPE_ALL=1`: emit the escape-everything module
//!   ([`Emit::NOTHING`]) rather than the real one — the C6's
//!   `--jit-escape-all`, as an environment switch because the phase's file
//!   list does not reach the CLI. Off by default; the boot line says which
//!   was built.
//! - `LP_EMU_XT_JIT_RECORD=<dir>`, with `LP_EMU_XT_JIT_RECORD_AFTER=<cycles>`
//!   (default 0) and `LP_EMU_XT_JIT_RECORD_ENTRIES=<n>` (default 200): record
//!   core 0's entries into translated code — the exchange head in and out,
//!   the import answers in call order (the escape hatch's included, with the
//!   memory it wrote), the guest memory the interpreter changed between
//!   entries — as one engine case `scripts/emu/jit-engine-check.mjs` replays
//!   in V8 and JavaScriptCore. Off by default.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use lp_emu_core::{Bus, InstClass};
use lp_emu_esp_common::bus::{AccessRule, PERMISSION_PAGE_LEN, SocBus};
use lp_xt_emu::mach::sr::{PS_EXCM, PS_WOE};
use lp_xt_emu::mach::translated::{RunOutcome, TranslatedCore};
use lp_xt_emu::mach::{PcCensus, SliceEnd, XtHart};
use lp_xt_jit::blocks::BlockSet;
use lp_xt_jit::decode::lp_xt_inst::{self, Inst, NullaryNarrowOp, NullaryOp};
use lp_xt_jit::discover::{Bounds, DiscoverStats, Discovered, Extent, discover_from, word_seeds};
use lp_xt_jit::lp_emu_jit::dispatch::{target_table_bytes, write_target_tables};
use lp_xt_jit::lp_emu_jit::host::{
    self, EXCHANGE_LEN, ExchangeLayout, HostOps, MmioLoad, MmioStore, PERM_ENTRIES, PERM_NONE,
    PERM_READ, PERM_SHIFT, Polled, STEP_CONTINUE, STEP_SLICE_ENDED, StepOne,
};
use lp_xt_jit::lp_emu_jit::host_wasmtime::WasmtimeCore;
use lp_xt_jit::lp_emu_jit::translate::Layout;
use lp_xt_jit::replay::case::{self, Call};
use lp_xt_jit::replay::marshal;
use lp_xt_jit::translate::mem::PERM_READ_WORD;
use lp_xt_jit::translate::{Emit, known_lends, refusal_of, static_census, why};

/// How many why-codes the report buckets exits into: the ABI's fourteen and
/// the emitter's two, with room.
const WHY_CODES: usize = 32;

/// The why-codes by name, for the report.
fn why_name(code: usize) -> &'static str {
    match code as i32 {
        0 => "none",
        why::BUDGET => "budget",
        why::EDGE_OUT => "edge-out",
        why::AFTER_STORE => "after-store",
        why::INDIRECT_MISS => "indirect-miss",
        why::INDIRECT_NO_TABLE => "indirect-no-table",
        why::LOAD_REFUSED => "load-refused",
        why::LOAD_STRADDLE => "load-straddle",
        why::STORE_PERM => "store-watch",
        why::STORE_REFUSED => "store-refused",
        why::STORE_STRADDLE => "store-straddle",
        why::UNDECODABLE => "undecodable",
        why::ESCAPE_DIVERGED => "escape-diverged",
        why::ESCAPE_TARGET => "escape-target",
        why::SLICE_ENDED => "slice-ended",
        why::WINDOW => "window",
        why::LOOP_BACK_MISS => "loop-back-miss",
        _ => "?",
    }
}

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
    /// The slice end the escape hatch or a poll reported, if any.
    slice_end: Option<SliceEnd>,
    escape_hatch: u64,
    /// The escape census: which instruction the hatch ran, by the emitter's
    /// refusal name (or the reason an emitted family escaped).
    escapes_by: BTreeMap<&'static str, u64>,
    /// Polls that answered anything but "carry on", and why.
    poll_left: u64,
    poll_code_dirty: u64,
    /// Every import call this entry made, while a recording is on.
    recording: bool,
    calls: Vec<Call>,
    /// The guest regions as arena offsets, for the escape hatch's granules
    /// while recording.
    live: Vec<(u32, u32)>,
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

    fn exchange_slice<'a>(&self) -> &'a mut [u8] {
        // SAFETY: `exchange` points at `self.exchange_len` bytes inside the
        // bus's arena, which is allocated once and never moves, and which no
        // region covers — see `areas`.
        unsafe { core::slice::from_raw_parts_mut(self.exchange, self.exchange_len) }
    }

    /// The whole head, exchange → hart.
    fn take_state(&mut self) {
        let x = self.exchange_slice();
        // SAFETY: called from inside `enter`.
        let (hart, _) = unsafe { self.parts() };
        let (lbeg, lend, lcount) = marshal::read_state(x, hart.cpu_mut());
        let sr = hart.sr_mut();
        sr.lbeg = lbeg;
        sr.lend = lend;
        sr.lcount = lcount;
    }

    /// The whole head, hart → exchange.
    fn give_state(&mut self) {
        let x = self.exchange_slice();
        // SAFETY: called from inside `enter`.
        let (hart, _) = unsafe { self.parts() };
        let (lbeg, lend, lcount) = {
            let sr = hart.sr();
            (sr.lbeg, sr.lend, sr.lcount)
        };
        marshal::write_state(x, hart.cpu(), lbeg, lend, lcount);
    }

    /// The guest regions' bytes, for a granule diff.
    fn live_bytes(&mut self) -> Vec<Vec<u8>> {
        let live = self.live.clone();
        // SAFETY: called from inside `enter`.
        let (_, bus) = unsafe { self.parts() };
        let arena = bus.guest_arena();
        live.iter()
            .map(|&(at, len)| arena[at as usize..][..len as usize].to_vec())
            .collect()
    }

    /// **Polling point (c)**, run by the hart itself, from inside a stay
    /// (DD111). The words a poll can observe are marshalled in first: the
    /// emitter keeps `PS.CALLINC` and the loop registers current in the
    /// exchange area at every point they change, and interrupt entry saves
    /// `PS` whole while the handler's `save_context` reads the loop
    /// registers.
    ///
    /// Three answers, as the C6's: carry on; the slice ended (a bus yield);
    /// leave — the hart moved (an interrupt's vector), the bus is no longer
    /// pure, **or the store published code** (the invalidation has to be
    /// applied before another translated block runs, and `run_blocks` does
    /// that after the stay).
    fn polling_point(&mut self, post_pc: u32, post_cycle: u64, post_instret: u64) -> Polled {
        let x = self.exchange_slice();
        // SAFETY: called from inside `enter`.
        let (hart, bus) = unsafe { self.parts() };
        let (lbeg, lend, lcount) = marshal::read_polled_state(x, hart.cpu_mut());
        {
            let sr = hart.sr_mut();
            sr.lbeg = lbeg;
            sr.lend = lend;
            sr.lcount = lcount;
        }
        hart.set_pc(post_pc);
        hart.set_counters(post_cycle, post_instret);
        let polled = hart.poll_after_store(bus);
        let out = if polled.yielded {
            Polled {
                status: host::MMIO_SLICE_ENDED,
                pc: hart.pc(),
            }
        } else if polled.code_dirty || hart.pc() != post_pc || !bus.fetch_is_pure() {
            Polled {
                status: host::MMIO_LEAVE_AFTER,
                pc: hart.pc(),
            }
        } else {
            Polled {
                status: host::MMIO_OK,
                pc: post_pc,
            }
        };
        if polled.yielded {
            self.slice_end = Some(SliceEnd::BusYield);
        }
        if polled.code_dirty {
            self.poll_code_dirty += 1;
        }
        if out.status != host::MMIO_OK {
            self.poll_left += 1;
        }
        out
    }
}

impl HostOps for V3Ops {
    fn layout(&self) -> ExchangeLayout {
        lp_xt_jit::LAYOUT
    }

    /// The escape hatch: the whole head crosses both ways around
    /// `XtHart::step_one`, because an escaped instruction may write any
    /// register, rotate the window (`rotw`, `rfwo`) or move the loop
    /// registers.
    fn step_one_wide(&mut self, pc: u32, cycle: u64, instret: u64) -> StepOne {
        self.escape_hatch += 1;
        let before = if self.recording {
            self.live_bytes()
        } else {
            Vec::new()
        };
        self.take_state();
        // The census, by what the emitter would have said about it.
        let name = {
            // SAFETY: called from inside `enter`.
            let (_, bus) = unsafe { self.parts() };
            let base = bus.guest_arena_base();
            let arena = bus.guest_arena();
            let mut bytes = [0u8; 3];
            let mut got = 0;
            for (i, slot) in bytes.iter_mut().enumerate() {
                match pc
                    .checked_add(i as u32)
                    .and_then(|a| arena_byte(arena, base, a))
                {
                    Some(b) => {
                        *slot = b;
                        got = i + 1;
                    }
                    None => break,
                }
            }
            match lp_xt_jit::decode::decode(&bytes[..got]) {
                lp_xt_jit::decode::Decode::Ok(d) => refusal_of(&d).unwrap_or(match d.inst {
                    Inst::Rrr(
                        lp_xt_inst::AluRrr::Quou
                        | lp_xt_inst::AluRrr::Quos
                        | lp_xt_inst::AluRrr::Remu
                        | lp_xt_inst::AluRrr::Rems,
                        ..,
                    ) => "zero divisor",
                    _ => "escape-all build",
                }),
                lp_xt_jit::decode::Decode::Undecodable { .. } => "undecodable",
                lp_xt_jit::decode::Decode::Refused { .. } => "refused",
            }
        };
        *self.escapes_by.entry(name).or_insert(0) += 1;
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
        self.give_state();
        if self.recording {
            let after = self.live_bytes();
            let mut mem = Vec::new();
            for ((at, _), (b, a)) in self.live.iter().zip(before.iter().zip(after.iter())) {
                mem.extend(case::granules(b, a, *at));
            }
            let words = marshal::words(self.exchange_slice());
            self.calls.push(Call::Step {
                pc: out.pc,
                cycle: out.cycle,
                instret: out.instret,
                status: out.status,
                words,
                mem,
            });
        }
        out
    }

    fn step_one(&mut self, _pc: u32, _cycle: u64, _instret: u64, _regs: &mut [i32; 32]) -> StepOne {
        unreachable!(
            "an Xtensa host escapes through `step_one_wide`: the AR file is 64 physical \
             registers and the hart owns it"
        )
    }

    /// An access on a page the permission table does not call plain RAM —
    /// or one the fast path declined (a misaligned word, a sub-word access
    /// on SRAM0, a straddle): the bus serves it exactly as it serves the
    /// interpreter's.
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
        let out = match read {
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
        };
        if self.recording {
            self.calls.push(Call::Load(
                (i64::from(out.status) << 32) | i64::from(out.value),
            ));
        }
        out
    }

    /// A store the bus serves — every store to SRAM0 among them, because a
    /// store there is the invalidation event (XD3) and the bus is what
    /// records it — and then polling point (c) in the same crossing.
    fn mmio_store(
        &mut self,
        pc: u32,
        cycle: u64,
        address: u32,
        kind: u32,
        value: u32,
        post_pc: u32,
        post_cycle: u64,
        post_instret: u64,
    ) -> MmioStore {
        let written = {
            // SAFETY: called from inside `enter`.
            let (_, bus) = unsafe { self.parts() };
            bus.set_issuing(pc, cycle);
            match kind {
                host::store_kind::B => bus.write_byte(address, value as i8),
                host::store_kind::H => bus.write_halfword(address, value as i16),
                host::store_kind::W => bus.write_word(address, value as i32),
                other => unreachable!("{other} is not a store kind the protocol defines"),
            }
        };
        let out = match written {
            Ok(()) => self.polling_point(post_pc, post_cycle, post_instret),
            // The access did not happen; the interpreter traps rather than
            // retiring it, so there is no polling point to run.
            Err(_) => Polled {
                status: host::MMIO_REFUSED,
                pc,
            },
        };
        if self.recording {
            self.calls.push(Call::Store(
                (i64::from(out.status) << 32) | i64::from(out.pc),
            ));
        }
        out
    }

    /// Polling point (c) on its own, for the store the bus never saw and the
    /// System-class instruction that owes one.
    fn poll(&mut self, pc: u32, cycle: u64, instret: u64) -> Polled {
        let out = self.polling_point(pc, cycle, instret);
        if self.recording {
            self.calls.push(Call::Poll(
                (i64::from(out.status) << 32) | i64::from(out.pc),
            ));
        }
        out
    }

    fn exchange(&mut self) -> &mut [u8] {
        self.exchange_slice()
    }
}

// ---- placing the tables in the arena ---------------------------------------

/// Where the translator's tables sit inside the guest arena.
#[derive(Clone, Copy, Debug)]
struct Areas {
    perm_at: u32,
    exchange_at: u32,
    /// The indirect-target page map and slot arrays (M7 P05).
    indirect_at: u32,
    indirect_len: u32,
    pages: u64,
    gap_len: u32,
    gap_left: u32,
}

/// Place the permission table, the exchange areas and the indirect-target
/// tables in the arena's largest gap, and say how much of the arena a wasm
/// memory can cover.
///
/// The exchange area is per **instance**, and the two harts get two of them
/// side by side in the gap — a stay belongs to one hart and its counters are
/// that hart's.
fn areas(bus: &SocBus, instances: u32, indirect_len: u64) -> Result<Areas, String> {
    let arena_len = bus.guest_arena().len();
    let pages = (arena_len / 65536) as u64;
    if pages == 0 {
        return Err("the guest arena is smaller than one wasm page".into());
    }
    let exchange_len = lp_xt_jit::LAYOUT.len();
    let exchanges = u64::from(exchange_len) * u64::from(instances);
    let need = u64::from(PERM_ENTRIES) + exchanges + indirect_len;
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
    let indirect_at = (u64::from(exchange_at) + exchanges) as u32;
    if u64::from(perm_at) + need > pages * 65536 {
        return Err("the translator's tables fall outside the wasm memory".into());
    }
    Ok(Areas {
        perm_at,
        exchange_at,
        indirect_at,
        indirect_len: indirect_len as u32,
        pages,
        gap_len,
        gap_left: gap_len - (need as u32),
    })
}

/// Write the permission table into the arena.
///
/// The bus's table is one byte per 16 KiB page **of the arena**; emitted code
/// indexes one byte per 16 KiB page of the whole 32-bit guest space, so it
/// needs no bounds compare. This is where one becomes the other, where the
/// pages a wasm memory cannot reach are taken back to [`PERM_NONE`] — and
/// where **this chip's rules** land on the bus's answer:
///
/// - an **executable** page never takes an inline store, whatever the bus
///   said about writability: a guest store into executable memory is the
///   store-address invalidation event (XD3), and the bus is what records
///   it. Such a page is [`PERM_READ`] — or, when its region is word-only
///   (SRAM0, DD37), [`PERM_READ_WORD`]: aligned word loads inline, every
///   other access the bus's, so a sub-word or misaligned access faults
///   exactly as the interpreter's does.
fn write_permission_table(bus: &mut SocBus, at: Areas) {
    let table = bus.permission_table();
    let base = bus.guest_arena_base();
    let reachable = at.pages * 65536;
    let rules: Vec<(u32, u32, bool, AccessRule)> = bus
        .regions()
        .iter()
        .map(|r| (r.base, r.end(), r.exec, r.access))
        .collect();
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
        let guest = (u64::from(base) + offset) as u32;
        let entry = (u64::from(guest) >> PERM_SHIFT) as usize;
        let rule = rules
            .iter()
            .find(|&&(lo, hi, _, _)| lo <= guest && guest < hi);
        arena[at.perm_at as usize + entry] = match rule {
            Some(&(_, _, true, AccessRule::WordOnly)) => PERM_READ_WORD,
            Some(&(_, _, true, _)) => PERM_READ,
            _ => perm,
        };
    }
}

// ---- the build report ------------------------------------------------------

/// What one translation event cost and produced.
#[derive(Clone, Debug)]
pub struct BuildReport {
    pub seeds: usize,
    pub blocks: usize,
    pub insts: usize,
    pub native_insts: usize,
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
    /// What the whole-image walk found before any host budget bound it.
    pub whole_blocks: usize,
    /// The sweep's own account of where it stopped short (P05).
    pub stats: DiscoverStats,
    /// The indirect-target tables' bytes, and the 16 KiB pages they cover.
    pub indirect_bytes: u32,
    pub indirect_pages: usize,
    /// The static escape census: instructions in the block set the emitter
    /// refuses, by name.
    pub static_escapes: BTreeMap<&'static str, usize>,
    /// The escape-everything build was asked for.
    pub escape_all: bool,
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
        let total = self.native_insts + self.escaped_insts;
        let per_inst = if total == 0 {
            0.0
        } else {
            self.module_bytes as f64 / total as f64
        };
        let census: Vec<String> = self
            .static_escapes
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect();
        format!(
            "core{core} {event}: {} block(s) from {} seed(s), {} instruction(s) ({} escaped, {} \
             emitted natively, static escape share {:.2} %{}); {} function(s) at {} blocks each, \
             largest body {} B, module {} B ({:.1} B per instruction); discover {:.1} ms, emit \
             {:.1} ms, compile {:.1} ms, instantiate {:.1} ms; arena gap {} B, {} B left; \
             indirect tables {} B over {} page(s); static escapes: {}; sweep: {}",
            self.blocks,
            self.seeds,
            self.insts,
            self.escaped_insts,
            self.native_insts,
            if total == 0 {
                0.0
            } else {
                100.0 * self.escaped_insts as f64 / total as f64
            },
            if self.escape_all {
                ", ESCAPE-ALL build"
            } else {
                ""
            },
            self.functions,
            self.fn_blocks,
            self.max_body_bytes,
            self.module_bytes,
            per_inst,
            self.discover_us as f64 / 1000.0,
            self.emit_us as f64 / 1000.0,
            self.compile_us as f64 / 1000.0,
            self.instantiate_us as f64 / 1000.0,
            self.gap_len,
            self.gap_left,
            self.indirect_bytes,
            self.indirect_pages,
            if census.is_empty() {
                "none".to_string()
            } else {
                census.join(", ")
            },
            stats_line(&self.stats, self.whole_blocks),
        )
    }
}

/// The sweep's counters, in one line, the same way everywhere they print.
fn stats_line(s: &DiscoverStats, whole_blocks: usize) -> String {
    format!(
        "{} start(s) → {} block(s) (whole image {}), undecodable {}, refused {}, extent-ends {}, \
         data-ends {}, literals {} ({} start(s) dropped), loop-ends {}, empty {}, capped {}{}",
        s.starts,
        s.blocks,
        whole_blocks,
        s.undecodable,
        s.refused,
        s.extent_ends,
        s.data_ends,
        s.literals,
        s.literal_starts_dropped,
        s.loop_ends,
        s.empty_starts,
        s.capped,
        if s.truncated { ", TRUNCATED" } else { "" },
    )
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
    refused_ps: u64,
    refused_ibreak: u64,
    refused_loop: u64,
    exit_slice_ended: u64,
    exit_no_progress: u64,
    /// Invalidations answered by re-reading the guest bytes and finding them
    /// unchanged — the common case, and the reason a loop-register write is
    /// not fatal.
    verified: u64,
    verify_failed: u64,
    why: [u64; WHY_CODES],
    /// The escape census, merged from the host after every entry.
    escapes_by: BTreeMap<&'static str, u64>,
    /// Polls that answered anything but "carry on", and how many of those
    /// were the store publishing code.
    poll_left: u64,
    poll_code_dirty: u64,
    /// `why::WINDOW` exits by the instruction at the exit pc: an `entry`'s
    /// own check, a `retw`'s, or the block-level precondition.
    window_entry: u64,
    window_retw: u64,
    window_block: u64,
    /// How many groups each exit wrote back: the dirty mask's popcount.
    dirty: [u64; 5],
}

// ---- the recorder ----------------------------------------------------------

/// A recording of core 0's entries, for the engines (see the module docs).
struct Recorder {
    dir: PathBuf,
    after: u64,
    want: usize,
    /// The arena offsets a replay has to reproduce: the guest regions (the
    /// volatile ones), the permission table, the exchange area and the
    /// indirect tables.
    ranges: Vec<(u32, u32)>,
    /// The guest regions alone, whose between-entries writes are the delta.
    live: Vec<(u32, u32)>,
    initial: Option<Vec<(u32, Vec<u8>)>>,
    shadow: Vec<Vec<u8>>,
    entries: Vec<case::Entry>,
    done: bool,
}

impl Recorder {
    fn from_env(at: Areas, bus: &SocBus, exchange_at: u32) -> Option<Self> {
        let dir = std::env::var_os("LP_EMU_XT_JIT_RECORD").map(PathBuf::from)?;
        let after = std::env::var("LP_EMU_XT_JIT_RECORD_AFTER")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let want = std::env::var("LP_EMU_XT_JIT_RECORD_ENTRIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(200);
        let base = bus.guest_arena_base();
        let reachable = (at.pages * 65536) as u32;
        let live: Vec<(u32, u32)> = bus
            .regions()
            .iter()
            .filter(|r| r.len() != 0)
            .map(|r| (r.base - base, r.len()))
            .filter(|&(off, _)| off < reachable)
            .map(|(off, len)| (off, len.min(reachable - off)))
            .collect();
        let mut ranges = live.clone();
        ranges.push((at.perm_at, PERM_ENTRIES));
        ranges.push((exchange_at, lp_xt_jit::LAYOUT.len()));
        ranges.push((at.indirect_at, at.indirect_len));
        ranges.sort_unstable();
        Some(Self {
            dir,
            after,
            want,
            ranges,
            live,
            initial: None,
            shadow: Vec::new(),
            entries: Vec::new(),
            done: false,
        })
    }

    fn snapshot(&self, arena: &[u8], ranges: &[(u32, u32)]) -> Vec<Vec<u8>> {
        ranges
            .iter()
            .map(|&(at, len)| arena[at as usize..][..len as usize].to_vec())
            .collect()
    }

    /// The first recorded entry takes the whole image; every one after
    /// takes only what changed since the previous entry ended.
    fn before_entry(&mut self, arena: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let now = self.snapshot(arena, &self.live.clone());
        let delta = if self.initial.is_none() {
            let ranges = self.ranges.clone();
            self.initial = Some(
                ranges
                    .iter()
                    .map(|&(at, len)| (at, arena[at as usize..][..len as usize].to_vec()))
                    .collect(),
            );
            Vec::new()
        } else {
            let mut d = Vec::new();
            for ((at, _), (was, is)) in self.live.iter().zip(self.shadow.iter().zip(now.iter())) {
                d.extend(case::granules(was, is, *at));
            }
            d
        };
        self.shadow = now;
        delta
    }

    fn after_entry(&mut self, arena: &[u8], entry: case::Entry) {
        self.shadow = self.snapshot(arena, &self.live.clone());
        self.entries.push(entry);
        if self.entries.len() >= self.want {
            self.done = true;
        }
    }

    fn finish(&self, wasm: &[u8], arena: &[u8], pages: u64, exchange_at: u32) {
        let Some(initial) = self.initial.as_ref() else {
            return;
        };
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            log::error!("jit: {}: {e}", self.dir.display());
            return;
        }
        let finals: Vec<(u32, Vec<u8>)> = self
            .ranges
            .iter()
            .map(|&(at, len)| (at, arena[at as usize..][..len as usize].to_vec()))
            .collect();
        let ranges: Vec<(u32, &[u8])> = initial.iter().map(|(a, b)| (*a, b.as_slice())).collect();
        let finals: Vec<(u32, &[u8])> = finals.iter().map(|(a, b)| (*a, b.as_slice())).collect();
        let json = case::json(
            "record",
            pages,
            exchange_at,
            "module.wasm",
            &ranges,
            &self.entries,
            &finals,
        );
        let written = std::fs::write(self.dir.join("module.wasm"), wasm)
            .and_then(|()| std::fs::write(self.dir.join("record.json"), json));
        match written {
            Ok(()) => eprintln!(
                "jit: recorded {} entries into translated code to {}",
                self.entries.len(),
                self.dir.display()
            ),
            Err(e) => log::error!("jit: the recording could not be written: {e}"),
        }
    }
}

// ---- the core --------------------------------------------------------------

/// One core's installed module, and the entry index into it.
pub struct XtJitCore {
    core: WasmtimeCore<V3Ops>,
    /// Guest pc to block index. The hart's own byte-indexed entry table says
    /// *whether* a pc is an entry; this says which block it is.
    index: BTreeMap<u32, u32>,
    /// Every `LEND` the walk marked: a stay may not start with `LCOUNT != 0`
    /// and a live `LEND` outside this set.
    lends: BTreeSet<u32>,
    which: usize,
    /// The cost model the budget checks were emitted against.
    model: lp_emu_core::CycleModel,
    /// What installed this core: `boot`, or `app-core release`.
    event: String,
    report: BuildReport,
    stats: Stats,
    /// Set when the guest changed code this module was translated from, or
    /// when the cost model moved under it. A stale module is never entered
    /// again: retranslation is P07's.
    stale: bool,
    /// An invalidation arrived and the bytes have not been re-checked yet.
    verify_pending: bool,
    /// The guest bytes each block was translated from. See P04/P05.
    code: Vec<(u32, Box<[u8]>)>,
    /// The module's own bytes, kept while a recording wants them.
    wasm: Option<Vec<u8>>,
    recorder: Option<Recorder>,
    pages: u64,
    exchange_at: u32,
}

impl XtJitCore {
    /// Are the guest bytes still the ones this module was translated from?
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

    /// The block starts this module holds — the stop set for an incremental
    /// walk over code published after it was installed (XD10, P07).
    #[must_use]
    pub fn starts(&self) -> BTreeSet<u32> {
        self.index.keys().copied().collect()
    }

    /// The boot-cost line for this core's one translation event.
    #[must_use]
    pub fn boot_line(&self) -> String {
        self.report.boot_line(self.which, &self.event)
    }

    /// Which instruction a `why::WINDOW` exit refused at.
    fn note_window_exit(&mut self, bus: &SocBus, pc: u32) {
        let base = bus.guest_arena_base();
        let arena = bus.guest_arena();
        let mut bytes = [0u8; 3];
        let mut got = 0;
        for (i, slot) in bytes.iter_mut().enumerate() {
            match pc
                .checked_add(i as u32)
                .and_then(|a| arena_byte(arena, base, a))
            {
                Some(b) => {
                    *slot = b;
                    got = i + 1;
                }
                None => break,
            }
        }
        match lp_xt_inst::decode(&bytes[..got]).map(|(i, _)| i) {
            Ok(Inst::Entry(..)) => self.stats.window_entry += 1,
            Ok(Inst::Nullary(NullaryOp::Retw) | Inst::NullaryN(NullaryNarrowOp::RetwN)) => {
                self.stats.window_retw += 1;
            }
            _ => self.stats.window_block += 1,
        }
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

        // The refusals, in the order the C6's spike learned them, plus this
        // chip's three.
        if bus.sideband_or_yield_pending() {
            self.stats.refused_pending += 1;
            return RunOutcome::Refused;
        }
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
        if !bus.fetch_is_pure() {
            self.stats.refused_impure += 1;
            return RunOutcome::Refused;
        }
        if hart.next_timer_cycle().is_some_and(|at| at < end) {
            self.stats.refused_timer += 1;
            return RunOutcome::Refused;
        }
        // A block is emitted under `woe && !excm`: the window precondition
        // is meaningless otherwise, and the exception-vector region runs
        // interpreted (0.87 % of the render loop).
        let ps = hart.ps();
        if ps & PS_WOE == 0 || ps & PS_EXCM != 0 {
            self.stats.refused_ps += 1;
            return RunOutcome::Refused;
        }
        // `IBREAK`: the interpreter tests it before every instruction and a
        // stay cannot. The block cache refuses the same way.
        if hart.breakpoints().ibreakenable != 0 {
            self.stats.refused_ibreak += 1;
            return RunOutcome::Refused;
        }
        // A live `LEND` the walk never marked, with a count to fire on: the
        // interpreter would loop back at an instruction the module runs
        // straight through.
        {
            let sr = hart.sr();
            if sr.lcount != 0 && !self.lends.contains(&sr.lend) {
                self.stats.refused_loop += 1;
                return RunOutcome::Refused;
            }
        }

        self.stats.entries += 1;
        let (cycle, instret) = (hart.cycle_count(), hart.instruction_count());
        let recording = self
            .recorder
            .as_ref()
            .is_some_and(|r| !r.done && cycle >= r.after);
        let delta = if recording {
            self.recorder
                .as_mut()
                .map(|r| r.before_entry(bus.guest_arena()))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        {
            let ops = self.core.ops_mut();
            ops.slice_end = None;
            ops.hart = hart;
            ops.bus = bus;
            ops.recording = recording;
            ops.calls.clear();
            // The whole head, hart → exchange (XD8).
            ops.give_state();
        }
        let words_in = recording.then(|| marshal::words(self.core.ops_mut().exchange_slice()));
        let entered = self.core.enter(entry, cycle, instret, end, watch);
        let ops = self.core.ops_mut();
        ops.hart = core::ptr::null_mut();
        ops.bus = core::ptr::null_mut();
        let escaped = core::mem::take(&mut ops.escape_hatch);
        self.stats.escape_hatch += escaped;
        for (name, n) in core::mem::take(&mut ops.escapes_by) {
            *self.stats.escapes_by.entry(name).or_insert(0) += n;
        }
        self.stats.poll_left += core::mem::take(&mut ops.poll_left);
        self.stats.poll_code_dirty += core::mem::take(&mut ops.poll_code_dirty);
        let slice_end = ops.slice_end.take();
        let calls = core::mem::take(&mut ops.calls);

        let exit_pc = match entered {
            Ok(exit) => exit.pc,
            // The emitted module cannot trap on any path this translator
            // emits, so a trap is a translator bug. Saying so and refusing is
            // the only answer that keeps the transcript right.
            Err(e) => {
                log::error!("jit: translated code trapped at {pc:#010x}: {e}");
                self.stale = true;
                return RunOutcome::Refused;
            }
        };

        // The flags are read at **this layout's** offset (P04's workaround:
        // `WasmtimeCore::enter` reads `exit.flags` at RV32's fixed offset,
        // which past a 64-word file is inside the AR file).
        let x = self.core.ops_mut().exchange_slice();
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
        let dirty = marshal::dirty(x);
        let words_out = recording.then(|| marshal::words(x));

        // The whole head, exchange → hart, then the pc and the counters.
        {
            let ops = self.core.ops_mut();
            ops.hart = hart;
            ops.bus = bus;
            ops.take_state();
            ops.hart = core::ptr::null_mut();
            ops.bus = core::ptr::null_mut();
        }
        hart.set_pc(exit_pc);
        hart.set_counters(cycle_count, instruction_count);
        self.stats.retired += instruction_count.saturating_sub(instret);
        self.stats.why[(exit_why as usize).min(WHY_CODES - 1)] += 1;
        self.stats.dirty[(dirty.count_ones() as usize).min(4)] += 1;
        if exit_why == why::WINDOW {
            self.note_window_exit(bus, exit_pc);
        }
        if instruction_count == instret {
            self.stats.exit_no_progress += 1;
        }

        if recording {
            let entry_rec = case::Entry {
                entry,
                cycle,
                instret,
                end,
                watch_lo: watch.0,
                watch_hi: watch.1,
                words_in: words_in.unwrap_or_default(),
                delta,
                calls,
                exit_pc,
                flags,
                cycle_out: cycle_count,
                instret_out: instruction_count,
                words_out: words_out.unwrap_or_default(),
            };
            if let Some(r) = self.recorder.as_mut() {
                r.after_entry(bus.guest_arena(), entry_rec);
                if r.done {
                    let wasm = self.wasm.take().unwrap_or_default();
                    r.finish(&wasm, bus.guest_arena(), self.pages, self.exchange_at);
                }
            }
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
            // Never: polling point (c) runs inside the stay, through the
            // fused poll (DD111), at exactly the instruction that owed it.
            after_store: false,
        }
    }

    fn invalidate(&mut self, _range: Option<(u32, u32)>) {
        // Noted, not acted on: the answer is only needed at the next entry,
        // and most invalidations are not about this module's bytes at all
        // (see `XtJitCore::code`).
        self.verify_pending = true;
    }

    fn report(&self) -> String {
        let s = &self.stats;
        let refused = s.refused_no_entry
            + s.refused_pending
            + s.refused_watch
            + s.refused_impure
            + s.refused_timer
            + s.refused_stale
            + s.refused_ps
            + s.refused_ibreak
            + s.refused_loop;
        let native = s.retired.saturating_sub(s.escape_hatch);
        let why: Vec<String> = s
            .why
            .iter()
            .enumerate()
            .filter(|(_, n)| **n != 0)
            .map(|(i, &n)| format!("{} {n}", why_name(i)))
            .collect();
        let census: Vec<String> = s
            .escapes_by
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect();
        let exits: u64 = s.dirty.iter().sum();
        let groups: u64 = s.dirty.iter().enumerate().map(|(g, n)| g as u64 * n).sum();
        let mean_words = if exits == 0 {
            0.0
        } else {
            4.0 * groups as f64 / exits as f64
        };
        format!(
            "core{}: {} entries, {} instruction(s) inside translated code ({} escaped to the \
             interpreter, {} retired natively, {:.2} % of the entered); {} refusal(s) (no-entry \
             {}, pending {}, watch {}, impure {}, timer {}, stale {}, ps {}, ibreak {}, loop {}); \
             {} invalidation(s) answered by re-reading the bytes, {} that found them changed; {} \
             slice end(s), {} no-progress exit(s); exits by reason: {}; window refusals: block \
             {}, entry {}, retw {}; polls that left {} (code-dirty {}); writeback per exit: \
             0 groups {}, 1 {}, 2 {}, 3 {}, 4 {} (mean {:.2} words of 64); escapes by \
             instruction: {}; {}",
            self.which,
            s.entries,
            s.retired,
            s.escape_hatch,
            native,
            if s.retired == 0 {
                0.0
            } else {
                100.0 * native as f64 / s.retired as f64
            },
            refused,
            s.refused_no_entry,
            s.refused_pending,
            s.refused_watch,
            s.refused_impure,
            s.refused_timer,
            s.refused_stale,
            s.refused_ps,
            s.refused_ibreak,
            s.refused_loop,
            s.verified,
            s.verify_failed,
            s.exit_slice_ended,
            s.exit_no_progress,
            if why.is_empty() {
                "none".to_string()
            } else {
                why.join(", ")
            },
            s.window_block,
            s.window_entry,
            s.window_retw,
            s.poll_left,
            s.poll_code_dirty,
            s.dirty[0],
            s.dirty[1],
            s.dirty[2],
            s.dirty[3],
            s.dirty[4],
            mean_words,
            if census.is_empty() {
                "none".to_string()
            } else {
                census.join(", ")
            },
            self.boot_line(),
        )
    }
}

// ---- installing ------------------------------------------------------------

/// What one translation event walks from: the seeds and the bounds the
/// machine derives from its ELFs and its memory map (P05, rules 1 and 3).
#[derive(Clone, Copy, Debug)]
pub struct Walk<'a> {
    /// In the order the sweep should take them — biggest symbol first.
    pub seeds: &'a [u32],
    /// Every sized symbol in an executable region, sorted by start.
    pub extents: &'a [Extent],
    /// The executable regions, `[lo, hi)`, sorted.
    pub spans: &'a [(u32, u32)],
}

impl Walk<'_> {
    fn bounds(&self) -> Bounds<'_> {
        Bounds {
            extents: self.extents,
            spans: self.spans,
        }
    }
}

/// One guest byte, straight out of the arena — **pure**: this runs before the
/// guest reaches any of these addresses, so a fetch that charged a cycle or
/// fired a watchpoint would be a change the guest can see.
#[inline]
fn arena_byte(arena: &[u8], base: u32, pc: u32) -> Option<u8> {
    pc.checked_sub(base)
        .and_then(|o| arena.get(o as usize))
        .copied()
}

/// Walk the image from `walk`, stopping wherever `known` already holds a
/// block start, and say how long it took in microseconds.
#[must_use]
pub fn walk_image(
    bus: &SocBus,
    walk: &Walk,
    budget: usize,
    known: &BTreeSet<u32>,
) -> (Discovered, u128) {
    let base = bus.guest_arena_base();
    let arena = bus.guest_arena();
    let started = std::time::Instant::now();
    let found = discover_from(walk.seeds, walk.bounds(), budget, known, &mut |pc| {
        arena_byte(arena, base, pc)
    });
    (found, started.elapsed().as_micros())
}

/// The third path's result (rule 8).
#[derive(Clone, Debug, Default)]
pub struct WrittenWalk {
    /// The write spans, normalised: sorted, merged, `[lo, hi)`.
    pub spans: Vec<(u32, u32)>,
    /// Word-aligned addresses in them that decode.
    pub seeds: usize,
    pub found: Discovered,
    pub us: u128,
}

/// Sort and merge write spans so they can bound a walk.
fn normalise_spans(spans: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut sorted: Vec<(u32, u32)> = spans.iter().copied().filter(|&(lo, hi)| hi > lo).collect();
    sorted.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(sorted.len());
    for (lo, hi) in sorted {
        match out.last_mut() {
            Some(last) if lo <= last.1 => last.1 = last.1.max(hi),
            _ => out.push((lo, hi)),
        }
    }
    out
}

/// Walk the guest's own code (rule 8): every word-aligned address in the
/// spans it stored into executable memory that decodes is a seed; the walk
/// is bounded by the spans and stops at `known`.
///
/// `written` are **write** addresses; `exec_of` maps each to where the code
/// executes — identity on the classic, the alias offset on the S3 (P09).
#[must_use]
pub fn walk_written(
    bus: &SocBus,
    written: &[(u32, u32)],
    known: &BTreeSet<u32>,
    exec_of: &dyn Fn(u32) -> u32,
) -> WrittenWalk {
    let spans = normalise_spans(written);
    if spans.is_empty() {
        return WrittenWalk::default();
    }
    let base = bus.guest_arena_base();
    let arena = bus.guest_arena();
    let started = std::time::Instant::now();
    let mut fetch = |pc: u32| arena_byte(arena, base, pc);
    let seeds = word_seeds(&spans, exec_of, &mut fetch);
    let exec_spans: Vec<(u32, u32)> = spans
        .iter()
        .map(|&(lo, hi)| (exec_of(lo), exec_of(hi)))
        .collect();
    let bounds = Bounds {
        extents: &[],
        spans: &exec_spans,
    };
    let found = discover_from(&seeds, bounds, usize::MAX, known, &mut fetch);
    WrittenWalk {
        spans,
        seeds: seeds.len(),
        found,
        us: started.elapsed().as_micros(),
    }
}

/// Discover the image from `walk`, translate it, and install a core on
/// `hart`.
///
/// The walk is the real sweep (P05): symbol-seeded, extent-bounded, literals
/// struck out, `LEND` a terminator, refused instructions stepped over. The
/// whole image is walked **unbounded** first and reported, whatever budget
/// the caller named.
///
/// # Errors
///
/// A walk that reached nothing, a block set that does not fit the arena's
/// shape, a module over the per-function byte budget (use fewer
/// `--jit-fn-blocks`), or a module the engine will not compile.
pub fn install(
    hart: &mut XtHart<SocBus>,
    bus: &mut SocBus,
    which: usize,
    walk: &Walk,
    max_blocks: usize,
    fn_blocks: usize,
    event: &str,
) -> Result<XtJitCore, String> {
    let nothing = BTreeSet::new();
    let (whole, mut discover_us) = walk_image(bus, walk, usize::MAX, &nothing);
    if whole.set.is_empty() {
        return Err(format!(
            "none of the {} seed(s) reached a decodable instruction",
            walk.seeds.len()
        ));
    }
    let whole_blocks = whole.set.blocks.len();
    let found = if whole_blocks > max_blocks {
        let (bounded, us) = walk_image(bus, walk, max_blocks, &nothing);
        discover_us += us;
        bounded
    } else {
        whole
    };

    // The tables, sized from what the walk covered.
    let indirect_len = target_table_bytes(&found.set);
    let at = areas(bus, 2, indirect_len)?;
    write_permission_table(bus, at);
    let arena_base = bus.guest_arena_base();
    let indirect_pages = {
        let mut pages: Vec<u32> = found
            .set
            .blocks
            .iter()
            .map(|b| b.pc >> PERM_SHIFT)
            .collect();
        pages.sort_unstable();
        pages.dedup();
        pages.len()
    };

    let arena_len = bus.guest_arena().len();
    let arena_ptr = bus.guest_arena_mut().as_mut_ptr();
    #[cfg(target_family = "wasm")]
    let (mem_base, memory_pages) = (arena_ptr as u32, core::arch::wasm32::memory_size(0) as u64);
    #[cfg(not(target_family = "wasm"))]
    let (mem_base, memory_pages) = (0u32, at.pages);

    // The indirect-target page map and slot arrays, written whole (P05).
    let indirect_bytes =
        write_target_tables(bus.guest_arena_mut(), mem_base, at.indirect_at, &found.set);

    // One exchange area per instance, side by side in the gap.
    let exchange_at = at.exchange_at + lp_xt_jit::LAYOUT.len() * which as u32;
    let layout = Layout {
        memory_pages,
        guest_base: arena_base,
        arena_offset: mem_base,
        perm_offset: mem_base + at.perm_at,
        exchange_offset: mem_base + exchange_at,
        indirect: Some(mem_base + at.indirect_at),
        fast_reads: None,
    };

    // The emission policy, read once from the environment (a diagnostic, off
    // by default): `LP_EMU_XT_JIT_ESCAPE_ALL=1` is the escape-everything
    // build; `LP_EMU_XT_JIT_EMIT=alu,memory,control` names the families to
    // emit natively, for bisecting a machine-scale divergence by family.
    let escape_all = std::env::var_os("LP_EMU_XT_JIT_ESCAPE_ALL").is_some_and(|v| v == "1");
    let policy = if escape_all {
        Emit::NOTHING
    } else if let Ok(families) = std::env::var("LP_EMU_XT_JIT_EMIT") {
        Emit {
            alu: families.contains("alu"),
            memory: families.contains("memory"),
            control: families.contains("control"),
            ..Emit::EVERYTHING
        }
    } else {
        Emit::EVERYTHING
    };
    let escape_all = escape_all || policy == Emit::NOTHING;
    let model = hart.cycle_model();
    let started = std::time::Instant::now();
    let emitted = lp_xt_jit::translate::emit_module(&found.set, model, layout, policy, fn_blocks);
    let emit_us = started.elapsed().as_micros();
    if emitted.max_body_bytes > lp_xt_jit::lp_emu_jit::dispatch::BODY_BUDGET {
        return Err(format!(
            "at {fn_blocks} blocks per function the largest body is {} bytes, over the {}-byte \
             budget; use fewer (--jit-fn-blocks)",
            emitted.max_body_bytes,
            lp_xt_jit::lp_emu_jit::dispatch::BODY_BUDGET,
        ));
    }

    // The guest bytes behind each block, so an invalidation can be answered by
    // asking whether they changed rather than by giving up.
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

    // The recorder, core 0 only: one module, one recording.
    let recorder = if which == 0 {
        Recorder::from_env(at, bus, exchange_at)
    } else {
        None
    };
    let live = recorder
        .as_ref()
        .map(|r| r.live.clone())
        .unwrap_or_default();

    let arena_guard = bus.guest_arena_guard();
    let exchange = bus.guest_arena_mut()[exchange_at as usize..].as_mut_ptr();
    let ops = V3Ops {
        hart: core::ptr::null_mut(),
        bus: core::ptr::null_mut(),
        exchange,
        exchange_len: lp_xt_jit::LAYOUT.len() as usize,
        slice_end: None,
        escape_hatch: 0,
        escapes_by: BTreeMap::new(),
        poll_left: 0,
        poll_code_dirty: 0,
        recording: false,
        calls: Vec::new(),
        live,
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
        seeds: walk.seeds.len(),
        blocks: found.set.blocks.len(),
        insts: found.stats.insts,
        native_insts: emitted.native_insts,
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
        whole_blocks,
        stats: found.stats,
        indirect_bytes,
        indirect_pages,
        static_escapes: static_census(&found.set),
        escape_all,
    };
    let wasm = recorder.as_ref().map(|_| emitted.wasm.clone());
    Ok(XtJitCore {
        core,
        index: index_of(&found.set),
        lends: known_lends(&found.set),
        which,
        model,
        event: event.to_string(),
        report,
        stats: Stats::default(),
        stale: false,
        verify_pending: false,
        code,
        wasm,
        recorder,
        pages: at.pages,
        exchange_at,
    })
}

fn index_of(set: &BlockSet) -> BTreeMap<u32, u32> {
    set.blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.pc, i as u32))
        .collect()
}

// ---- the coverage census -------------------------------------------------------

/// How many pages the interpreted-remainder table names.
const CENSUS_TOP_PAGES: usize = 14;

/// The coverage and FP tables from a run's per-pc census (P05).
///
/// The walk is run **here, at the end of the run, on the arena as it stands**
/// — once from the image's symbols and once, with the first walk's starts as
/// the stop set, from the spans the guest stored into executable memory —
/// and the union of the two block sets is scored against every retire each
/// hart's [`PcCensus`] counted. The census counts the hart's retires, so
/// under `--jit` the "interpreted" column is what the module did **not**
/// retire natively — the entries at the walk's pcs are the refused stays.
pub fn coverage_lines(
    bus: &SocBus,
    walk: &Walk,
    written: &[(u32, u32)],
    cores: &[(usize, &PcCensus)],
    symbolize: &dyn Fn(u32) -> Option<String>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let nothing = BTreeSet::new();
    let (boot, boot_us) = walk_image(bus, walk, usize::MAX, &nothing);
    let known: BTreeSet<u32> = boot.set.index.keys().copied().collect();
    let third = walk_written(bus, written, &known, &|w| w);
    let covered: BTreeSet<u32> = boot
        .set
        .blocks
        .iter()
        .chain(third.found.set.blocks.iter())
        .flat_map(|b| b.insts.iter().map(|&(pc, _)| pc))
        .collect();
    lines.push(format!(
        "discover: boot walk from {} seed(s) in {:.1} ms — {}",
        walk.seeds.len(),
        boot_us as f64 / 1000.0,
        stats_line(&boot.stats, boot.set.blocks.len()),
    ));
    let written_bytes: u64 = third.spans.iter().map(|&(lo, hi)| u64::from(hi - lo)).sum();
    lines.push(format!(
        "discover: written walk from {} word seed(s) over {} span(s) ({} B) in {:.1} ms — {}",
        third.seeds,
        third.spans.len(),
        written_bytes,
        third.us as f64 / 1000.0,
        stats_line(&third.found.stats, third.found.set.blocks.len()),
    ));
    for &(lo, hi) in third.spans.iter().take(8) {
        lines.push(format!(
            "discover: written span {lo:#010x}..{hi:#010x} ({} B)",
            hi - lo
        ));
    }

    let base = bus.guest_arena_base();
    let arena = bus.guest_arena();
    let region_of = |pc: u32| -> &str {
        bus.regions()
            .iter()
            .find(|r| r.contains(pc))
            .map_or("outside the arena", |r| r.name)
    };
    let decode_at = |pc: u32| -> Option<Inst> {
        let mut bytes = [0u8; 3];
        let mut got = 0;
        for (i, slot) in bytes.iter_mut().enumerate() {
            match pc
                .checked_add(i as u32)
                .and_then(|a| arena_byte(arena, base, a))
            {
                Some(b) => {
                    *slot = b;
                    got = i + 1;
                }
                None => break,
            }
        }
        lp_xt_inst::decode(&bytes[..got]).ok().map(|(inst, _)| inst)
    };

    for &(core, census) in cores {
        let total = census.retired();
        let pct = |n: u64| {
            if total == 0 {
                0.0
            } else {
                n as f64 * 100.0 / total as f64
            }
        };
        let mut inside = 0u64;
        let mut entry_hits = 0u64;
        let mut retw_hits = 0u64;
        let mut fp = FpShare::default();
        // page base → (retired, interpreted, hottest interpreted (count, pc))
        let mut pages: BTreeMap<u32, (u64, u64, (u64, u32))> = BTreeMap::new();
        for (pc, n) in census.iter() {
            let page = pages.entry(pc & !0xffff).or_default();
            page.0 += n;
            if covered.contains(&pc) {
                inside += n;
            } else {
                page.1 += n;
                if n > page.2.0 {
                    page.2 = (n, pc);
                }
            }
            if let Some(inst) = decode_at(pc) {
                match inst {
                    Inst::Entry(..) => entry_hits += n,
                    Inst::Nullary(NullaryOp::Retw) | Inst::NullaryN(NullaryNarrowOp::RetwN) => {
                        retw_hits += n;
                    }
                    _ => {}
                }
                fp.note(&inst, n);
            }
        }
        lines.push(format!(
            "core{core}: {total} retired by the hart; {inside} inside the walk's blocks ({:.2} %), \
             {} interpreted ({:.2} %)",
            pct(inside),
            total - inside,
            pct(total - inside),
        ));
        lines.push(format!(
            "core{core}: entry one-instruction blocks: {entry_hits} retire(s) at an `entry` \
             ({:.2} % of the hart's retires); retw: {retw_hits}",
            pct(entry_hits),
        ));
        lines.push(format!(
            "core{core}: fp: arith {} ({:.2} %), muladd {} ({:.2} %), convert {} ({:.2} %), \
             compare {} ({:.2} %), estimate {} ({:.2} %), ld/st {} ({:.2} %), moves/const {} \
             ({:.2} %) → {} ({:.2} % of retired)",
            fp.arith,
            pct(fp.arith),
            fp.muladd,
            pct(fp.muladd),
            fp.convert,
            pct(fp.convert),
            fp.compare,
            pct(fp.compare),
            fp.estimate,
            pct(fp.estimate),
            fp.ldst,
            pct(fp.ldst),
            fp.moves,
            pct(fp.moves),
            fp.total(),
            pct(fp.total()),
        ));
        let mut rows: Vec<(u32, (u64, u64, (u64, u32)))> = pages.into_iter().collect();
        rows.sort_by(|a, b| b.1.1.cmp(&a.1.1).then_with(|| a.0.cmp(&b.0)));
        lines.push(format!(
            "core{core}: interpreted remainder by 64 KiB page (top {}, of {}):",
            CENSUS_TOP_PAGES.min(rows.len()),
            rows.len()
        ));
        for (page, (retired, interpreted, (hot_n, hot_pc))) in
            rows.into_iter().take(CENSUS_TOP_PAGES)
        {
            let mut row = format!(
                "core{core}:   {page:#010x} {:<16} retired {retired:>11} interpreted {interpreted:>11} \
                 ({:>6.2} % of core)",
                region_of(page),
                pct(interpreted),
            );
            if interpreted > 0 {
                let _ = write!(
                    row,
                    "; hottest {hot_pc:#010x} ×{hot_n} {}",
                    symbolize(hot_pc).unwrap_or_else(|| "?".to_string())
                );
            }
            lines.push(row);
        }
    }
    lines
}

/// The FP share's counters (G-M7D-XT Q8).
#[derive(Clone, Copy, Debug, Default)]
struct FpShare {
    arith: u64,
    muladd: u64,
    convert: u64,
    compare: u64,
    estimate: u64,
    /// `lsi`/`ssi`/`lsx`/`ssx` and their update forms — `Load`/`Store` by
    /// class, FP by family.
    ldst: u64,
    /// `rfr`/`wfr`/`const.s`/`movt.s`… — `Alu` by class, FP by family.
    moves: u64,
}

impl FpShare {
    fn note(&mut self, inst: &Inst, n: u64) {
        match lp_xt_emu::block::cost_bound(inst) {
            InstClass::FloatArith => self.arith += n,
            InstClass::FloatMulAdd => self.muladd += n,
            InstClass::FloatConvert => self.convert += n,
            InstClass::FloatCompare => self.compare += n,
            InstClass::FloatEstimate => self.estimate += n,
            _ => match inst {
                Inst::FpLsi(..) | Inst::FpLsx(..) => self.ldst += n,
                Inst::Rfr(..)
                | Inst::Wfr(..)
                | Inst::ConstS(..)
                | Inst::FpMovAr(..)
                | Inst::FpMovBr(..)
                | Inst::FpRr(..)
                | Inst::FpRrr(..)
                | Inst::FpCmp(..)
                | Inst::FpToInt(..)
                | Inst::IntToFp(..) => self.moves += n,
                _ => {}
            },
        }
    }

    fn total(&self) -> u64 {
        self.arith
            + self.muladd
            + self.convert
            + self.compare
            + self.estimate
            + self.ldst
            + self.moves
    }
}

/// The exchange area the RV32 protocol sizes, for the assertion below.
const _: () = assert!(
    lp_xt_jit::LAYOUT.len() > EXCHANGE_LEN,
    "the Xtensa exchange area is longer than RV32's; a host that sized it by the RV32 constant \
     would hand translated code a short slice"
);
