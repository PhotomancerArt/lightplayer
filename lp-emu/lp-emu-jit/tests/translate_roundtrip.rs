//! Emit a module, run it in wasmtime, and check what came back.
//!
//! This is the translator's own bench test: a hand-built block set, a fake
//! host that records every import call, and assertions on the registers, the
//! counters and the exit pc. It is not an identity oracle — that is the
//! emulator's whole-image sweep — but an emitter bug found here costs minutes
//! where the same bug found through a 5.5-second reference image costs hours.
//!
//! The same block set is run twice: once fully emitted, and once with
//! [`Emit::NOTHING`], where every instruction goes out through the escape
//! hatch. **The two runs must agree on everything.** That is R9's "no
//! completeness cliff" as a test rather than as a claim.

#![cfg(feature = "host-wasmtime")]

use lp_emu_core::CycleModel;
use lp_emu_core::arena::GuestArena;
use lp_emu_jit::blocks::{BlockSet, adjacency_order};
use lp_emu_jit::discover::discover;
use lp_emu_jit::dispatch::{Selector, emit_module, target_table_bytes, write_target_tables};
use lp_emu_jit::host::{
    self, EXCHANGE_CROSS, EXCHANGE_CYCLE, EXCHANGE_INDIRECT_MISS, EXCHANGE_INSTRET, EXCHANGE_LEN,
    EXCHANGE_REGS, FAST_ARMED, FAST_SERVED, FAST_WORDS, HostOps, MMIO_LEAVE_AFTER, MMIO_OK,
    MMIO_PENDING, MMIO_SLICE_ENDED, MmioLoad, MmioStore, PERM_ENTRIES, PERM_NONE, PERM_READ_WRITE,
    PERM_SHIFT, Polled, STEP_CONTINUE, StepOne,
};
use lp_emu_jit::host_wasmtime::WasmtimeCore;
use lp_emu_jit::replay::GRANULE_BYTES;
use lp_emu_jit::translate::{Emit, FastRead, FastReads, FastSource, Layout};

// The test memory: exchange, then the permission table, then 64 KiB of guest
// RAM at `GUEST_BASE`. Six wasm pages, so the whole thing is one allocation
// the emitted module addresses with folded constants exactly as the real one
// does.
const EXCHANGE_AT: u32 = 0;
/// The published-read block (M7b P3), past the exchange area.
const FAST_AT: u32 = 0x100;
const PERM_AT: u32 = 0x1000;
const ARENA_AT: u32 = PERM_AT + PERM_ENTRIES;
const ARENA_LEN: u32 = 0x1_0000;
const GUEST_BASE: u32 = 0x4000_0000;
const PAGES: u64 = 6;
/// Where the indirect-target tables go in the rig that has them. Only the
/// tests that exercise `jalr` pay for them: the page map alone is 1 MiB, and
/// putting it in every case would quadruple every engine case for nothing.
const IND_AT: u32 = ARENA_AT + ARENA_LEN;
const IND_PAGES: u64 = 23;

/// A host that serves the imports out of a plain `Vec` and remembers what it
/// was asked.
struct FakeHost {
    mem: *mut u8,
    len: usize,
    /// `(pc, cycle, address, kind)` per `mmio_load`.
    loads: Vec<(u32, u64, u32, u32)>,
    /// `(pc, cycle, address, kind, value)` per `mmio_store`.
    stores: Vec<(u32, u64, u32, u32, u32)>,
    /// The `(post_pc, post_cycle, post_instret)` each `mmio_store` was handed
    /// for its polling point — the state the interpreter would hold there.
    posts: Vec<(u32, u64, u64)>,
    /// The pcs handed to `step_one`, in order.
    escapes: Vec<u32>,
    /// `(pc, cycle, instret)` per stand-alone `poll` — the polling point an
    /// inline RAM store owes after a load left a yield (M7b P2).
    polls: Vec<(u32, u64, u64)>,
    /// What the polling point fused into the next `mmio_store` should answer.
    /// `None` is the ordinary case: the store retired and the hart did not
    /// move.
    store_answer: Option<Polled>,
    /// What a stand-alone `poll` should answer.
    poll_answer: Option<Polled>,
    /// When set, every `mmio_load` reports [`MMIO_PENDING`], which is how a
    /// load leaves the obligation that makes the next inline RAM store a
    /// polling point.
    load_leaves_yield: bool,
    /// Every import call's *result*, in call order, so another engine can be
    /// handed the same answers without reimplementing the host. See
    /// `engine_case`.
    trace: Vec<Call>,
}

/// One import call's answer, as the JSON a JS host replays.
#[derive(Clone, Debug)]
enum Call {
    Load(i64),
    Store(i64),
    Poll(i64),
    /// `(pc, cycle, instret, status, regs, memory granules the interpreter
    /// changed)`.
    ///
    /// The granules are why a replay can be believed. An escaped instruction
    /// runs on the *interpreter*, and a store it makes lands in memory the
    /// replaying engine has no way to reproduce — a replay that reloaded the
    /// initial image and ran only the module would be executing it against
    /// memory the real run never had. That is the spike's entry-21 divergence,
    /// and it is what `crate::replay`'s memory-granule diff exists for. This
    /// is the same diff at the same granularity.
    Step(u32, u64, u64, u32, [i32; 32], Vec<(u32, Vec<u8>)>),
}

// SAFETY: the pointer is into a buffer the test owns and keeps alive for
// longer than the store; nothing here crosses a thread.
unsafe impl Send for FakeHost {}

impl FakeHost {
    fn bytes(&mut self) -> &mut [u8] {
        // SAFETY: `mem` is the test's own `len`-byte allocation.
        unsafe { core::slice::from_raw_parts_mut(self.mem, self.len) }
    }
}

impl HostOps for FakeHost {
    fn mmio_load(&mut self, pc: u32, cycle: u64, address: u32, kind: u32) -> MmioLoad {
        self.loads.push((pc, cycle, address, kind));
        let out = MmioLoad {
            status: if self.load_leaves_yield {
                MMIO_PENDING
            } else {
                MMIO_OK
            },
            // A recognisable value, and a different one per address so a
            // mixed-up operand shows up as a wrong number rather than a zero.
            value: 0xAB00 | (address & 0xff),
        };
        self.trace.push(Call::Load(
            (i64::from(out.status) << 32) | i64::from(out.value),
        ));
        out
    }

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
        self.stores.push((pc, cycle, address, kind, value));
        self.posts.push((post_pc, post_cycle, post_instret));
        let out = self.store_answer.unwrap_or(Polled {
            status: MMIO_OK,
            pc: post_pc,
        });
        self.trace.push(Call::Store(
            (i64::from(out.status) << 32) | i64::from(out.pc),
        ));
        out
    }

    fn poll(&mut self, pc: u32, cycle: u64, instret: u64) -> Polled {
        self.polls.push((pc, cycle, instret));
        let out = self.poll_answer.unwrap_or(Polled {
            status: MMIO_OK,
            pc,
        });
        self.trace.push(Call::Poll(
            (i64::from(out.status) << 32) | i64::from(out.pc),
        ));
        out
    }

    fn step_one(&mut self, pc: u32, cycle: u64, instret: u64, regs: &mut [i32; 32]) -> StepOne {
        self.escapes.push(pc);
        // A miniature interpreter over exactly the encodings this test uses.
        // Its only job is to be the same answer the emitted code gives, so the
        // escape-hatch run and the emitted run can be compared.
        let word = {
            let at = (ARENA_AT + (pc - GUEST_BASE)) as usize;
            let b = self.bytes();
            u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
        };
        let before = self.bytes().to_vec();
        let d = tiny_decode(word);
        let mut next = pc.wrapping_add(u32::from(d.width));
        match d.kind {
            Kind::Lui { rd, imm } => {
                if rd != 0 {
                    regs[rd as usize] = imm;
                }
            }
            Kind::Addi { rd, rs1, imm } => {
                if rd != 0 {
                    regs[rd as usize] = regs[rs1 as usize].wrapping_add(imm);
                }
            }
            Kind::Add { rd, rs1, rs2 } => {
                if rd != 0 {
                    regs[rd as usize] = regs[rs1 as usize].wrapping_add(regs[rs2 as usize]);
                }
            }
            Kind::Sw { rs1, rs2, imm } => {
                let address = (regs[rs1 as usize].wrapping_add(imm)) as u32;
                let value = regs[rs2 as usize] as u32;
                let at = (ARENA_AT + (address - GUEST_BASE)) as usize;
                self.bytes()[at..at + 4].copy_from_slice(&value.to_le_bytes());
            }
            Kind::Lw { rd, rs1, imm } => {
                let address = (regs[rs1 as usize].wrapping_add(imm)) as u32;
                let at = (ARENA_AT + (address - GUEST_BASE)) as usize;
                let b = self.bytes();
                let value = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
                if rd != 0 {
                    regs[rd as usize] = value as i32;
                }
            }
            Kind::System { rd } => {
                if rd != 0 {
                    regs[rd as usize] = 0x5a5a;
                }
            }
            Kind::Jal { rd, imm } => {
                if rd != 0 {
                    regs[rd as usize] = next as i32;
                }
                next = pc.wrapping_add(imm as u32);
            }
            Kind::Bne { rs1, rs2, imm } => {
                if regs[rs1 as usize] != regs[rs2 as usize] {
                    next = pc.wrapping_add(imm as u32);
                }
            }
        }
        let out = StepOne {
            pc: next,
            cycle: cycle + d.cycles,
            instret: instret + 1,
            status: STEP_CONTINUE,
        };
        let after = self.bytes().to_vec();
        let mut granules = Vec::new();
        for (i, (a, b)) in before
            .chunks(GRANULE_BYTES)
            .zip(after.chunks(GRANULE_BYTES))
            .enumerate()
        {
            if a != b {
                granules.push(((i * GRANULE_BYTES) as u32, b.to_vec()));
            }
        }
        self.trace.push(Call::Step(
            out.pc,
            out.cycle,
            out.instret,
            out.status,
            *regs,
            granules,
        ));
        out
    }

    fn exchange(&mut self) -> &mut [u8] {
        let at = EXCHANGE_AT as usize;
        &mut self.bytes()[at..at + EXCHANGE_LEN as usize]
    }
}

// --- the tiny decoder the fake `step_one` runs on ---------------------------

enum Kind {
    Lui { rd: u8, imm: i32 },
    Addi { rd: u8, rs1: u8, imm: i32 },
    Add { rd: u8, rs1: u8, rs2: u8 },
    Sw { rs1: u8, rs2: u8, imm: i32 },
    Lw { rd: u8, rs1: u8, imm: i32 },
    Jal { rd: u8, imm: i32 },
    Bne { rs1: u8, rs2: u8, imm: i32 },
    /// Any `SYSTEM` word. The fake interpreter writes a marker to `rd`.
    System { rd: u8 },
}

struct Tiny {
    kind: Kind,
    width: u8,
    cycles: u64,
}

fn tiny_decode(w: u32) -> Tiny {
    let model = CycleModel::Esp32C6;
    let rd = ((w >> 7) & 0x1f) as u8;
    let rs1 = ((w >> 15) & 0x1f) as u8;
    let rs2 = ((w >> 20) & 0x1f) as u8;
    let imm_i = (w as i32) >> 20;
    let imm_s = (((w >> 25) << 5) | ((w >> 7) & 0x1f)) as i32;
    let imm_s = (imm_s << 20) >> 20;
    let imm_b = {
        let v = ((w >> 31) & 1) << 12
            | ((w >> 7) & 1) << 11
            | ((w >> 25) & 0x3f) << 5
            | ((w >> 8) & 0xf) << 1;
        ((v << 19) as i32) >> 19
    };
    let imm_j = {
        let v = ((w >> 31) & 1) << 20
            | ((w >> 12) & 0xff) << 12
            | ((w >> 20) & 1) << 11
            | ((w >> 21) & 0x3ff) << 1;
        ((v << 11) as i32) >> 11
    };
    let cy = |c| u64::from(model.cycles_for(c));
    let (kind, cycles) = match w & 0x7f {
        0x37 => (
            Kind::Lui {
                rd,
                imm: (w & 0xffff_f000) as i32,
            },
            cy(lp_emu_core::InstClass::Lui),
        ),
        0x13 => (
            Kind::Addi {
                rd,
                rs1,
                imm: imm_i,
            },
            cy(lp_emu_core::InstClass::Alu),
        ),
        0x33 => (Kind::Add { rd, rs1, rs2 }, cy(lp_emu_core::InstClass::Alu)),
        0x23 => (
            Kind::Sw {
                rs1,
                rs2,
                imm: imm_s,
            },
            cy(lp_emu_core::InstClass::Store),
        ),
        0x03 => (
            Kind::Lw {
                rd,
                rs1,
                imm: imm_i,
            },
            cy(lp_emu_core::InstClass::Load),
        ),
        0x6f => (
            Kind::Jal { rd, imm: imm_j },
            cy(if rd == 0 {
                lp_emu_core::InstClass::JalTail
            } else {
                lp_emu_core::InstClass::JalCall
            }),
        ),
        0x63 => {
            let taken = ((w >> 15) & 0x1f) as usize;
            let _ = taken;
            (
                Kind::Bne {
                    rs1,
                    rs2,
                    imm: imm_b,
                },
                // Charged by the caller's knowledge of whether it was taken;
                // this test's branch is always taken.
                cy(lp_emu_core::InstClass::BranchTaken),
            )
        }
        // `SYSTEM` (M7b P6). `lp_emu_jit::decode` refuses the whole opcode, so
        // a block ends before one and the emitted code hands it to
        // `step_one`. The fake interpreter writes a marker into `rd` so a
        // test can see that the *interpreter* ran it and the emitted code did
        // not, and charges an ALU cycle so the counter arithmetic is
        // checkable.
        0x73 => (Kind::System { rd }, cy(lp_emu_core::InstClass::Alu)),
        other => panic!("the fake step_one does not know opcode {other:#x}"),
    };
    Tiny {
        kind,
        width: 4,
        cycles,
    }
}

// --- the rig ----------------------------------------------------------------

struct Rig {
    /// A `GuestArena` rather than a `Vec`, so these tests run the emitted
    /// modules under the same guard-page configuration the native `--jit`
    /// path uses (M7 JD21) wherever the host can provide one.
    mem: GuestArena,
    pages: u64,
    /// Whether this rig builds the indirect-target tables, so an emitted
    /// `jalr` resolves in-module instead of leaving.
    indirect: bool,
    /// The global block index the next run enters at.
    entry: u32,
    /// What the polling point fused into a store should answer (M7b P2).
    store_answer: Option<Polled>,
    /// What a stand-alone `poll` should answer.
    poll_answer: Option<Polled>,
    /// Whether an `mmio_load` leaves a yield on the fake bus.
    load_leaves_yield: bool,
    /// The published MMIO word reads this rig folds into the module (M7b P3).
    fast_reads: Option<FastReads>,
}

impl Rig {
    fn mem_len(&self) -> usize {
        (self.pages as usize) * 65536
    }

    /// Fill the published-read block the way a machine's host does (M7b P3):
    /// the two published words, and `armed`.
    fn publish(&mut self, armed: bool, words: [u32; 2]) {
        let at = FAST_AT as usize;
        self.mem[at + FAST_ARMED as usize..][..4].copy_from_slice(&i32::from(armed).to_le_bytes());
        for (i, w) in words.iter().enumerate() {
            self.mem[at + FAST_WORDS as usize + 4 * i..][..4].copy_from_slice(&w.to_le_bytes());
        }
    }

    /// Reads translated code served from the published block.
    fn fast_served(&self) -> u64 {
        let at = FAST_AT as usize + FAST_SERVED as usize;
        u64::from_le_bytes(self.mem[at..at + 8].try_into().unwrap())
    }

    fn layout(&self) -> Layout {
        Layout {
            memory_pages: self.pages,
            guest_base: GUEST_BASE,
            arena_offset: ARENA_AT,
            perm_offset: PERM_AT,
            exchange_offset: EXCHANGE_AT,
            indirect: self.indirect.then_some(IND_AT),
            fast_reads: self.fast_reads,
        }
    }

    /// A memory with the permission table filled in: the 64 KiB of guest RAM
    /// is read-write, and everything else — including the MMIO address the
    /// tests use — is [`PERM_NONE`].
    fn new(program: &[(u32, u32)]) -> Self {
        Self::sized(program, PAGES, false)
    }

    /// The same rig with room for the indirect-target tables, and building
    /// them.
    fn with_indirect(program: &[(u32, u32)]) -> Self {
        Self::sized(program, IND_PAGES, true)
    }

    fn sized(program: &[(u32, u32)], pages: u64, indirect: bool) -> Self {
        let mut mem = GuestArena::zeroed((pages as usize) * 65536);
        for page in 0..(ARENA_LEN >> PERM_SHIFT) {
            let entry = ((GUEST_BASE >> PERM_SHIFT) + page) as usize;
            mem[PERM_AT as usize + entry] = PERM_READ_WRITE;
        }
        assert_eq!(mem[PERM_AT as usize], PERM_NONE, "page zero is not RAM");
        for &(pc, word) in program {
            let at = (ARENA_AT + (pc - GUEST_BASE)) as usize;
            mem[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        Self {
            mem,
            pages,
            indirect,
            entry: 0,
            store_answer: None,
            poll_answer: None,
            load_leaves_yield: false,
            fast_reads: None,
        }
    }

    /// The memory with the exchange area's **diagnostic** window cleared.
    ///
    /// Everything past the status field is a counter or a reason code the
    /// host reports and nothing reads back (P5): how many cross-function
    /// transfers the stay made, how many indirect jumps missed, and why it
    /// ended. Two builds of the same block set are entitled to differ there
    /// and nowhere else — an all-escape build ends its stay through
    /// `escape_terminator` and an emitted one through the edge itself — so
    /// this is what "byte for byte" means when the two are compared.
    fn guest_memory(&self) -> Vec<u8> {
        let mut out = self.mem.to_vec();
        let from = (EXCHANGE_AT + 152) as usize;
        let to = (EXCHANGE_AT + EXCHANGE_LEN) as usize;
        out[from..to].fill(0);
        out
    }

    fn word_at(&self, guest: u32) -> u32 {
        let at = (ARENA_AT + (guest - GUEST_BASE)) as usize;
        u32::from_le_bytes([
            self.mem[at],
            self.mem[at + 1],
            self.mem[at + 2],
            self.mem[at + 3],
        ])
    }

    /// Emit `set` under `policy`, enter at block 0 with `regs`, and report
    /// everything that came back.
    fn run_named(
        &mut self,
        case: &str,
        set: &BlockSet,
        policy: Emit,
        regs: [i32; 32],
        end: u64,
    ) -> Outcome {
        self.run_split(case, set, policy, regs, end, usize::MAX)
    }

    /// The same, entering at block `entry` rather than at block zero.
    fn run_at(
        &mut self,
        case: &str,
        set: &BlockSet,
        policy: Emit,
        regs: [i32; 32],
        end: u64,
        entry: u32,
    ) -> Outcome {
        self.entry = entry;
        let out = self.run_split(case, set, policy, regs, end, usize::MAX);
        self.entry = 0;
        out
    }

    /// The same, with at most `fn_blocks` guest blocks per sub-dispatcher.
    fn run_split(
        &mut self,
        case: &str,
        set: &BlockSet,
        policy: Emit,
        regs: [i32; 32],
        end: u64,
        fn_blocks: usize,
    ) -> Outcome {
        if self.indirect {
            assert!(
                u64::from(IND_AT) + target_table_bytes(set) <= self.mem_len() as u64,
                "the rig's memory has no room for the indirect-target tables"
            );
            write_target_tables(&mut self.mem, 0, IND_AT, set);
        }
        let entry = self.entry;
        let layout = self.layout();
        let emitted = emit_module(set, CycleModel::Esp32C6, layout, policy, fn_blocks);
        for (i, r) in regs.iter().enumerate() {
            let at = (EXCHANGE_AT as u64 + EXCHANGE_REGS) as usize + 4 * i;
            self.mem[at..at + 4].copy_from_slice(&r.to_le_bytes());
        }
        let mem_len = self.mem_len();
        let initial = self.mem.to_vec();
        let guard = self.mem.guard();
        let base = self.mem.as_mut_ptr();
        let host = FakeHost {
            mem: base,
            len: mem_len,
            loads: Vec::new(),
            stores: Vec::new(),
            posts: Vec::new(),
            escapes: Vec::new(),
            polls: Vec::new(),
            store_answer: self.store_answer,
            poll_answer: self.poll_answer,
            load_leaves_yield: self.load_leaves_yield,
            trace: Vec::new(),
        };
        // SAFETY: `self.mem` outlives `core`, and nothing else holds a
        // reference into it while `enter` is running.
        let mut core = unsafe { WasmtimeCore::new(&emitted.wasm, host, base, mem_len, guard) }
            .expect("the emitted module compiles and instantiates");
        let exit = core
            .enter(entry, 0, 0, end, (0, 0))
            .expect("translated code does not trap");

        let read_i64 = |m: &[u8], at: u64| {
            let at = (EXCHANGE_AT as u64 + at) as usize;
            let mut b = [0u8; 8];
            b.copy_from_slice(&m[at..at + 8]);
            i64::from_le_bytes(b) as u64
        };
        let mut out_regs = [0i32; 32];
        for (i, r) in out_regs.iter_mut().enumerate() {
            let at = (EXCHANGE_AT as u64 + EXCHANGE_REGS) as usize + 4 * i;
            *r = i32::from_le_bytes([
                self.mem[at],
                self.mem[at + 1],
                self.mem[at + 2],
                self.mem[at + 3],
            ]);
        }
        let outcome = Outcome {
            pc: exit.pc,
            flags: exit.flags,
            cycle: read_i64(&self.mem, EXCHANGE_CYCLE),
            instret: read_i64(&self.mem, EXCHANGE_INSTRET),
            cross: read_i64(&self.mem, EXCHANGE_CROSS),
            indirect_miss: read_i64(&self.mem, EXCHANGE_INDIRECT_MISS),
            regs: out_regs,
            loads: core.ops_mut().loads.clone(),
            stores: core.ops_mut().stores.clone(),
            posts: core.ops_mut().posts.clone(),
            polls: core.ops_mut().polls.clone(),
            escapes: core.ops_mut().escapes.clone(),
            escaped_insts: emitted.escaped_insts,
            native_insts: emitted.native_insts,
        };
        if let Ok(dir) = std::env::var("LP_EMU_JIT_ENGINE_CASE") {
            write_case(
                &dir,
                case,
                &emitted.wasm,
                &initial,
                &self.mem,
                end,
                &core.ops_mut().trace,
                &outcome,
                self.pages,
                entry,
            );
        }
        outcome
    }
}

/// The 64-bit FNV-1a of a memory image, computed the same way here and in
/// `scripts/emu/jit-engine-check.mjs`.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Write one engine case: the module bytes, the memory it starts from, the
/// answers its imports gave, and everything it produced.
///
/// The point is that **the same module bytes** can then be run in V8 and in
/// JavaScriptCore against the same inputs, with no host to reimplement — the
/// imports are replayed from the recorded answers in call order. JD19: a
/// single-engine wasm number, or a single-engine wasm *answer*, is not a wasm
/// answer.
#[expect(
    clippy::too_many_arguments,
    reason = "a case file is exactly these ten things, and a struct to carry \
              them between two functions in one file would name them twice"
)]
fn write_case(
    dir: &str,
    case: &str,
    wasm: &[u8],
    initial: &[u8],
    final_mem: &[u8],
    end: u64,
    trace: &[Call],
    outcome: &Outcome,
    pages: u64,
    entry: u32,
) {
    use std::io::Write as _;
    std::fs::create_dir_all(dir).expect("the case directory");
    std::fs::write(format!("{dir}/{case}.wasm"), wasm).expect("the module");
    let calls: Vec<String> = trace
        .iter()
        .map(|c| match c {
            Call::Load(v) => format!(r#"{{"kind":"load","ret":"{v}"}}"#),
            Call::Store(v) => format!(r#"{{"kind":"store","ret":"{v}"}}"#),
            Call::Poll(v) => format!(r#"{{"kind":"poll","ret":"{v}"}}"#),
            Call::Step(pc, cycle, instret, status, regs, granules) => {
                let regs: Vec<String> = regs.iter().map(ToString::to_string).collect();
                let mem: Vec<String> = granules
                    .iter()
                    .map(|(at, bytes)| format!(r#"{{"at":{at},"b":"{}"}}"#, base64(bytes)))
                    .collect();
                format!(
                    r#"{{"kind":"step","pc":{pc},"cycle":"{cycle}","instret":"{instret}","status":{status},"regs":[{}],"mem":[{}]}}"#,
                    regs.join(","),
                    mem.join(",")
                )
            }
        })
        .collect();
    let regs: Vec<String> = outcome.regs.iter().map(ToString::to_string).collect();
    let mut f = std::fs::File::create(format!("{dir}/{case}.json")).expect("the case");
    write!(
        f,
        r#"{{"case":"{case}","pages":{pages},"exchange":{EXCHANGE_AT},"entry":{entry},"end":"{end}",
"cycle":"0","instret":"0","watchLo":"0","watchHi":"0",
"memoryFnv":"{}","initial":"{}","calls":[{}],
"expect":{{"pc":{},"flags":{},"cycle":"{}","instret":"{}","regs":[{}],"memoryFnv":"{}"}}}}"#,
        fnv1a(initial),
        base64(initial),
        calls.join(","),
        outcome.pc,
        outcome.flags,
        outcome.cycle,
        outcome.instret,
        regs.join(","),
        fnv1a(final_mem),
    )
    .expect("writing the case");
}

/// Plain base64, so the case file needs no dependency on either side.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Outcome {
    pc: u32,
    flags: i32,
    cycle: u64,
    instret: u64,
    /// How many times control left one sub-dispatcher for another.
    cross: u64,
    /// How many indirect jumps the target table could not resolve.
    indirect_miss: u64,
    regs: [i32; 32],
    loads: Vec<(u32, u64, u32, u32)>,
    stores: Vec<(u32, u64, u32, u32, u32)>,
    /// The `(post_pc, post_cycle, post_instret)` each `mmio_store` handed its
    /// fused polling point (M7b P2).
    posts: Vec<(u32, u64, u64)>,
    /// The `(pc, cycle, instret)` of each stand-alone `poll`.
    polls: Vec<(u32, u64, u64)>,
    escapes: Vec<u32>,
    escaped_insts: usize,
    native_insts: usize,
}

/// `addi a0, x0, 7` / `addi a1, a0, 3` / `add a2, a0, a1` / `jal x0, +8`.
fn straight_line() -> Vec<(u32, u32)> {
    vec![
        (GUEST_BASE, 0x0070_0513),
        (GUEST_BASE + 4, 0x0035_0593),
        (GUEST_BASE + 8, 0x00b5_0633),
        (GUEST_BASE + 12, 0x0080_006f),
    ]
}

fn set_of(rig: &Rig, seed: u32) -> BlockSet {
    discover(&[seed], 64, &mut |pc| {
        if (GUEST_BASE..GUEST_BASE + ARENA_LEN).contains(&pc) {
            Some(rig.word_at(pc))
        } else {
            None
        }
    })
    .set
}

#[test]
fn a_straight_line_block_computes_and_charges_what_the_cost_model_says() {
    let mut rig = Rig::new(&straight_line());
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named(
        "straight-emitted",
        &set,
        Emit::EVERYTHING,
        [0; 32],
        u64::MAX,
    );

    assert_eq!(out.regs[10], 7);
    assert_eq!(out.regs[11], 10);
    assert_eq!(out.regs[12], 17);
    assert_eq!(out.instret, 4);
    let model = CycleModel::Esp32C6;
    let expect = 3 * u64::from(model.cycles_for(lp_emu_core::InstClass::Alu))
        + u64::from(model.cycles_for(lp_emu_core::InstClass::JalTail));
    assert_eq!(out.cycle, expect);
    // The `jal` leaves the block set, so the exit pc is its target.
    assert_eq!(out.pc, GUEST_BASE + 20);
    assert_eq!(out.flags, 0);
    assert_eq!(out.escaped_insts, 0);
}

#[test]
fn the_all_escape_build_agrees_with_the_emitted_one_on_everything() {
    let program = straight_line();
    let mut emitted = Rig::new(&program);
    let set = set_of(&emitted, GUEST_BASE);
    let fast = emitted.run_named(
        "straight-emitted",
        &set,
        Emit::EVERYTHING,
        [0; 32],
        u64::MAX,
    );

    let mut escaped = Rig::new(&program);
    let slow = escaped.run_named("straight-escaped", &set, Emit::NOTHING, [0; 32], u64::MAX);

    assert_eq!(slow.escaped_insts, 4, "every instruction went out");
    assert_eq!(slow.native_insts, 0);
    assert_eq!(
        slow.escapes,
        vec![GUEST_BASE, GUEST_BASE + 4, GUEST_BASE + 8, GUEST_BASE + 12]
    );
    assert_eq!(fast.pc, slow.pc);
    assert_eq!(fast.cycle, slow.cycle);
    assert_eq!(fast.instret, slow.instret);
    assert_eq!(fast.regs, slow.regs);
    assert_eq!(fast.flags, slow.flags);
    assert_eq!(
        emitted.guest_memory(),
        escaped.guest_memory(),
        "and the memory, byte for byte"
    );
}

#[test]
fn a_ram_store_and_load_stay_inline_and_an_off_ram_one_goes_out() {
    // addi a0, x0, 0        (a0 = 0; the RAM address comes from the `lui`)
    // lui  a1, 0x40000      (a1 = 0x4000_0000)
    // addi a2, x0, 0x123
    // sw   a2, 0x100(a1)
    // lw   a3, 0x100(a1)
    // jal  x0, +4
    let program = vec![
        (GUEST_BASE, 0x0000_0513),
        (GUEST_BASE + 4, 0x4000_05b7),
        (GUEST_BASE + 8, 0x1230_0613),
        (GUEST_BASE + 12, 0x10c5_a023),
        (GUEST_BASE + 16, 0x1005_a683),
        (GUEST_BASE + 20, 0x0040_006f),
    ];
    let mut rig = Rig::new(&program);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("memory-emitted", &set, Emit::EVERYTHING, [0; 32], u64::MAX);

    assert_eq!(out.regs[13], 0x123, "the load saw the store");
    assert!(out.loads.is_empty(), "plain RAM never reaches the import");
    assert!(out.stores.is_empty());
    assert_eq!(out.instret, 6);

    // The same program with every instruction escaped has to agree.
    let mut escaped = Rig::new(&program);
    let slow = escaped.run_named("memory-escaped", &set, Emit::NOTHING, [0; 32], u64::MAX);
    assert_eq!(out.regs, slow.regs);
    assert_eq!(out.cycle, slow.cycle);
    assert_eq!(out.instret, slow.instret);
    assert_eq!(out.pc, slow.pc);
    assert_eq!(rig.guest_memory(), escaped.guest_memory());
}

#[test]
fn an_off_ram_access_carries_the_exact_pc_and_cycle_to_the_import() {
    // lui  a1, 0x60000      -> 0x6000_0000, which the permission table says is
    //                          not RAM
    // addi a2, x0, 0x55
    // sw   a2, 0(a1)
    // lw   a3, 0(a1)
    // jal  x0, +4
    let program = vec![
        (GUEST_BASE, 0x6000_05b7),
        (GUEST_BASE + 4, 0x0550_0613),
        (GUEST_BASE + 8, 0x00c5_a023),
        (GUEST_BASE + 12, 0x0005_a683),
        (GUEST_BASE + 16, 0x0040_006f),
    ];
    let mut rig = Rig::new(&program);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("mmio-emitted", &set, Emit::EVERYTHING, [0; 32], u64::MAX);

    let model = CycleModel::Esp32C6;
    let lui = u64::from(model.cycles_for(lp_emu_core::InstClass::Lui));
    let alu = u64::from(model.cycles_for(lp_emu_core::InstClass::Alu));
    let store = u64::from(model.cycles_for(lp_emu_core::InstClass::Store));

    assert_eq!(
        out.stores,
        vec![(GUEST_BASE + 8, lui + alu, 0x6000_0000, 2, 0x55)],
        "the store's own pc, and the cycle count as of the instruction before it"
    );
    assert_eq!(
        out.loads,
        vec![(GUEST_BASE + 12, lui + alu + store, 0x6000_0000, 2)]
    );
    assert_eq!(out.regs[13], 0xAB00);
}

#[test]
fn a_block_that_does_not_fit_the_budget_is_handed_back_untouched() {
    let mut rig = Rig::new(&straight_line());
    let set = set_of(&rig, GUEST_BASE);
    // One cycle of budget: the block's maximum cost cannot fit, so it must
    // report the entry pc having retired and charged nothing. The hart's
    // no-progress guard then runs it interpreted.
    let out = rig.run_named("budget", &set, Emit::EVERYTHING, [0; 32], 1);
    assert_eq!(out.pc, GUEST_BASE);
    assert_eq!(out.cycle, 0);
    assert_eq!(out.instret, 0);
    assert_eq!(out.regs, [0; 32]);
}

#[test]
fn a_backward_branch_stays_inside_the_module() {
    // addi a0, x0, 3
    // addi a0, a0, -1      <- loop top
    // bne  a0, x0, -4
    // jal  x0, +4
    let program = vec![
        (GUEST_BASE, 0x0030_0513),
        (GUEST_BASE + 4, 0xfff5_0513),
        (GUEST_BASE + 8, 0xfe05_1ee3),
        (GUEST_BASE + 12, 0x0040_006f),
    ];
    let mut rig = Rig::new(&program);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("loop-emitted", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    assert_eq!(out.regs[10], 0);
    // 1 `addi` to load, then three times round the loop, then the `jal`.
    assert_eq!(out.instret, 1 + 3 * 2 + 1);
    assert_eq!(out.pc, GUEST_BASE + 16);

    // And the escape-hatch build agrees, which means the "compare the pc the
    // interpreter reported against this block's static targets" path works.
    let mut escaped = Rig::new(&program);
    let slow = escaped.run_named("loop-escaped", &set, Emit::NOTHING, [0; 32], u64::MAX);
    assert_eq!(out.regs, slow.regs);
    assert_eq!(out.instret, slow.instret);
    assert_eq!(out.pc, slow.pc);
    assert_eq!(
        slow.escapes.len(),
        8,
        "every one of them went through the interpreter"
    );
}

/// `jal ra, +12` / `addi a1, x0, 7` / `jal x0, +8` / `jalr x0, 0(ra)` — a
/// call, a body, and the return that P3 always left the module at.
fn call_and_return() -> Vec<(u32, u32)> {
    vec![
        (GUEST_BASE, 0x00c0_00ef),
        (GUEST_BASE + 4, 0x0070_0593),
        (GUEST_BASE + 8, 0x0080_006f),
        (GUEST_BASE + 12, 0x0000_8067),
    ]
}

#[test]
fn a_return_resolves_through_the_target_table_instead_of_leaving() {
    let program = call_and_return();

    // Without the tables: P3's shape. The `jalr` is where the stay ends, and
    // the two instructions after the return never run inside the module.
    let mut plain = Rig::new(&program);
    let set = set_of(&plain, GUEST_BASE);
    assert_eq!(set.blocks.len(), 3, "call, body, return");
    let out = plain.run_named("ret-no-table", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    assert_eq!(out.pc, GUEST_BASE + 4, "the return leaves at its target");
    assert_eq!(out.instret, 2);
    assert_eq!(out.regs[11], 0, "the body never ran");

    // With them: the return is a branch, and the stay runs to the edge that
    // really does leave the block set.
    let mut rig = Rig::with_indirect(&program);
    let set = set_of(&rig, GUEST_BASE);
    let fast = rig.run_named("ret-table", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    assert_eq!(fast.pc, GUEST_BASE + 16, "the `jal` out of the set");
    assert_eq!(fast.instret, 4, "the call, the return, and the body's two");
    assert_eq!(fast.regs[1] as u32, GUEST_BASE + 4, "the link register");
    assert_eq!(fast.regs[11], 7, "the body ran");
    assert_eq!(fast.indirect_miss, 0);
    assert_eq!(fast.cross, 0, "one sub-dispatcher holds all three blocks");
    assert!(fast.escapes.is_empty(), "no instruction left for the host");
    let cost = |c| u64::from(CycleModel::Esp32C6.cycles_for(c));
    assert_eq!(
        fast.cycle,
        out.cycle + cost(lp_emu_core::InstClass::Alu) + cost(lp_emu_core::InstClass::JalTail),
        "the body's `addi` and its `jal`, charged on top of what P3's shape reached"
    );
}

#[test]
fn an_indirect_jump_the_table_does_not_know_leaves_and_says_so() {
    let program = call_and_return();
    let mut rig = Rig::with_indirect(&program);
    let set = set_of(&rig, GUEST_BASE);
    // `ra` already holds an address no block starts at, and the `jal` at the
    // top overwrites it — so enter at the return itself.
    let mut regs = [0i32; 32];
    regs[1] = (GUEST_BASE + 0x2000) as i32;
    let idx = set
        .blocks
        .iter()
        .position(|b| b.pc == GUEST_BASE + 12)
        .expect("the return is a block");
    let out = rig.run_at(
        "ret-miss",
        &set,
        Emit::EVERYTHING,
        regs,
        u64::MAX,
        idx as u32,
    );
    assert_eq!(out.pc, GUEST_BASE + 0x2000, "it left at the unknown target");
    assert_eq!(out.instret, 1);
    assert_eq!(out.indirect_miss, 1, "and the miss is counted");
}

#[test]
fn the_split_module_answers_exactly_what_one_function_does() {
    let program = call_and_return();
    let mut whole = Rig::with_indirect(&program);
    let set = set_of(&whole, GUEST_BASE);
    let one = whole.run_split(
        "split-1",
        &set,
        Emit::EVERYTHING,
        [0; 32],
        u64::MAX,
        usize::MAX,
    );

    for fn_blocks in [2usize, 1] {
        let mut rig = Rig::with_indirect(&program);
        let split = rig.run_split(
            "split-n",
            &set,
            Emit::EVERYTHING,
            [0; 32],
            u64::MAX,
            fn_blocks,
        );
        assert_eq!(split.pc, one.pc, "at {fn_blocks} blocks per function");
        assert_eq!(split.cycle, one.cycle, "at {fn_blocks} blocks per function");
        assert_eq!(split.instret, one.instret, "at {fn_blocks}");
        assert_eq!(split.regs, one.regs, "at {fn_blocks}");
        assert_eq!(split.flags, one.flags, "at {fn_blocks}");
        assert_eq!(split.loads, one.loads, "at {fn_blocks}");
        assert_eq!(split.stores, one.stores, "at {fn_blocks}");
        assert_eq!(split.escapes, one.escapes, "at {fn_blocks}");
        assert_eq!(split.indirect_miss, 0);
    }

    // One block per function makes every edge in this program a cross: the
    // call's forward `jal` and the `jalr` back.
    let mut each = Rig::with_indirect(&program);
    let split = each.run_split("split-each", &set, Emit::EVERYTHING, [0; 32], u64::MAX, 1);
    assert_eq!(split.cross, 2, "the call and the return");
}

/// A backward branch is an intra-function edge when the two blocks share a
/// sub-dispatcher and a cross-function one when they do not, and the answer
/// is the same either way.
#[test]
fn a_backward_branch_crosses_functions_without_changing_the_answer() {
    let program = vec![
        (GUEST_BASE, 0x0030_0513),
        (GUEST_BASE + 4, 0xfff5_0513),
        (GUEST_BASE + 8, 0xfe05_1ee3),
        (GUEST_BASE + 12, 0x0040_006f),
    ];
    let mut whole = Rig::new(&program);
    let set = set_of(&whole, GUEST_BASE);
    // The loop top, the loop body, and the block the branch falls out into.
    assert_eq!(set.blocks.len(), 3);
    let one = whole.run_split(
        "loop-1",
        &set,
        Emit::EVERYTHING,
        [0; 32],
        u64::MAX,
        usize::MAX,
    );
    let mut each = Rig::new(&program);
    let two = each.run_split("loop-2", &set, Emit::EVERYTHING, [0; 32], u64::MAX, 1);
    assert_eq!(one.regs, two.regs);
    assert_eq!(one.instret, two.instret);
    assert_eq!(one.cycle, two.cycle);
    assert_eq!(one.pc, two.pc);
    assert_eq!(one.cross, 0);
    assert_eq!(
        two.cross, 2,
        "the fall into the loop and the fall out of it; the three iterations \
         are back edges inside one function and cross nothing"
    );
}

/// The unused-import guard: `host` is named so a future reader can find the
/// protocol from the test that exercises it.
#[test]
fn the_exchange_area_is_where_the_protocol_says_it_is() {
    assert_eq!(host::EXCHANGE_REGS, 0);
    assert_eq!(host::EXCHANGE_CYCLE, 128);
    assert_eq!(host::EXCHANGE_INSTRET, 136);
    assert!(host::EXCHANGE_STATUS < u64::from(EXCHANGE_LEN));
}

// --- M7 P6b: the cross-function hop, priced on its own ----------------------

/// A ring of `n` two-instruction blocks: `addi x1, x1, 1` then a `jal` to the
/// next block, and the last one back to the first.
///
/// The shape exists to make **one** thing vary. Emitted as a single
/// sub-dispatcher every edge is a `br` to a label or a back edge through the
/// function's own `br_table` and nothing crosses; emitted one block to a
/// function every edge is a cross — the epilogue flushes the live registers
/// and both counters to the exchange area, the outer selector reads them back,
/// picks the next function and calls it, and that function's prologue reloads
/// them. Same guest work, same block set, same number of retired instructions;
/// the difference in wall clock divided by the difference in crosses is what
/// one hop costs.
///
/// `regs` is how many registers a block touches, and it is the second thing
/// the fixture varies. A hop's cost is not one number: the epilogue stores and
/// the prologue reloads exactly the registers that chunk's blocks name
/// (`live_regs_in` is per sub-dispatcher), so a ring whose blocks touch one
/// register prices the hop's *machinery* and a ring whose blocks touch all 31
/// prices the hop a real image pays.
fn hop_ring(n: u32, regs: u32) -> Vec<(u32, u32)> {
    fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
        ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
    }
    fn jal(rd: u32, offset: i32) -> u32 {
        let o = offset as u32;
        (((o >> 20) & 1) << 31)
            | (((o >> 1) & 0x3ff) << 21)
            | (((o >> 11) & 1) << 20)
            | (((o >> 12) & 0xff) << 12)
            | (rd << 7)
            | 0x6f
    }
    let step = 4 * (regs + 1);
    let mut out = Vec::with_capacity(((regs + 1) * n) as usize);
    for i in 0..n {
        let pc = GUEST_BASE + step * i;
        let next = GUEST_BASE + step * ((i + 1) % n);
        for r in 0..regs {
            out.push((pc + 4 * r, addi(1 + r, 1 + r, 1)));
        }
        let at = pc + 4 * regs;
        out.push((at, jal(0, next.wrapping_sub(at) as i32)));
    }
    out
}

/// The same ring, hopping by `stride` instead of by one (M7b P5).
///
/// `hop_ring`'s successor is the block laid out next, so a trace layout of it
/// IS address order and a layout test built on it tests nothing. A stride
/// coprime with `n` visits every block exactly once in an order the addresses
/// do not predict, which is the fixture a layout has to survive.
fn scatter_ring(n: u32, regs: u32, stride: u32) -> Vec<(u32, u32)> {
    assert_eq!(
        gcd(stride, n),
        1,
        "a stride that shares a factor with the ring does not reach every block"
    );
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
        ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
    }
    fn jal(rd: u32, offset: i32) -> u32 {
        let o = offset as u32;
        (((o >> 20) & 1) << 31)
            | (((o >> 1) & 0x3ff) << 21)
            | (((o >> 11) & 1) << 20)
            | (((o >> 12) & 0xff) << 12)
            | (rd << 7)
            | 0x6f
    }
    let step = 4 * (regs + 1);
    let mut out = Vec::with_capacity(((regs + 1) * n) as usize);
    for i in 0..n {
        let pc = GUEST_BASE + step * i;
        let next = GUEST_BASE + step * ((i + stride) % n);
        for r in 0..regs {
            out.push((pc + 4 * r, addi(1 + r, 1 + r, 1)));
        }
        let at = pc + 4 * regs;
        out.push((at, jal(0, next.wrapping_sub(at) as i32)));
    }
    out
}

/// The ring's whole block set, however many blocks that is.
fn ring_set(rig: &Rig, n: u32) -> BlockSet {
    discover(&[GUEST_BASE], n as usize + 8, &mut |pc| {
        if (GUEST_BASE..GUEST_BASE + ARENA_LEN).contains(&pc) {
            Some(rig.word_at(pc))
        } else {
            None
        }
    })
    .set
}

/// The hop fixture (M7 P6b, H2): the same ring, emitted whole and emitted one
/// block to a function, agreeing on everything a guest can observe and
/// disagreeing only in how many times control crossed a function boundary.
///
/// `LP_EMU_JIT_ENGINE_CASE=<dir>` writes both cases out, and
/// `scripts/emu/p6b-hop.mjs` times them in V8 and in JavaScriptCore. The
/// assertions here are what makes that timing trustworthy: if the two runs did
/// not retire the same instructions for the same cycles and leave the same
/// registers, the difference between their wall clocks would not be the hop.
#[test]
fn a_ring_emitted_whole_and_one_block_to_a_function_differs_only_in_crosses() {
    // A narrow ring (one live register) and a wide one (all 31), because the
    // difference between the two hops is the register flush and the reader
    // needs both halves to know which is which.
    one_ring("narrow", 1);
    one_ring("wide", 31);
}

fn one_ring(name: &str, regs: u32) {
    const N: u32 = 64;
    // 4,000,000 cycles is a stay long enough to time and short enough that
    // cranelift runs it inside a unit test.
    const END: u64 = 4_000_000;
    let program = hop_ring(N, regs);

    let mut whole = Rig::new(&program);
    let set = ring_set(&whole, N);
    assert_eq!(
        set.blocks.len() as u32,
        N,
        "the ring is one block per `jal`"
    );
    let one = whole.run_split(
        &alloc_name(name, "whole"),
        &set,
        Emit::EVERYTHING,
        [0; 32],
        END,
        usize::MAX,
    );

    let mut split = Rig::new(&program);
    let each = split.run_split(
        &alloc_name(name, "split"),
        &set,
        Emit::EVERYTHING,
        [0; 32],
        END,
        1,
    );

    assert_eq!(one.regs, each.regs, "the two splits compute the same thing");
    assert_eq!(
        one.instret, each.instret,
        "and retire the same instructions"
    );
    assert_eq!(one.cycle, each.cycle, "for the same cycles");
    assert_eq!(one.pc, each.pc, "and leave at the same pc");
    assert_eq!(
        one.cross, 0,
        "one function: every edge is a label or the function's own `br_table`"
    );
    // Every block the stay entered but the first crossed into its own
    // function, and the stay entered one more block than it retired: the last
    // one found the budget gone and left without retiring anything. So the
    // crosses are exactly the blocks that retired.
    let blocks_run = one.instret / u64::from(regs + 1);
    assert_eq!(
        each.cross, blocks_run,
        "one block to a function: every edge but the entry crosses"
    );
    assert!(
        blocks_run > 50_000,
        "the stay has to be long enough to time: {blocks_run} blocks"
    );
}

fn alloc_name(ring: &str, split: &str) -> String {
    format!("p6b-hop-{ring}-{split}")
}

/// **M7 P6c Q5: the two selector shapes retire identically.**
///
/// The outer selector is the one exported function and the only thing that
/// knows a global block index is a `(sub-dispatcher, local index)` pair.
/// [`Selector::Nested`] writes that as `count` nested blocks around a
/// `br_table`, one direct `call` per arm — `O(count)` bytes, and at 8 blocks a
/// function on `render-basic` t2 that is a 730,452-byte function which V8's
/// optimizing tier answers with a fatal `Zone` OOM (P6b H5).
/// [`Selector::Flat`] writes it as one `call_indirect` through a function
/// table — `O(1)`, whatever the size knob says.
///
/// A shape change to the one function every cross-function edge routes
/// through is only safe if the two forms are the same program. So: the same
/// ring, the same block set, the same sizes, emitted both ways and run in
/// wasmtime — same registers, same instructions retired, same cycles charged,
/// same exit pc, same crosses, same imports called in the same order.
///
/// One block a function is in the list on purpose: that is where the two
/// selectors are furthest apart in bytes (64 arms against one
/// `call_indirect`) and where every single edge goes through the selector.
#[test]
fn the_two_selector_shapes_retire_identically() {
    const N: u32 = 64;
    const END: u64 = 400_000;
    let program = hop_ring(N, 31);

    for fn_blocks in [1usize, 8, 64] {
        let mut nested_rig = Rig::new(&program);
        let set = ring_set(&nested_rig, N);
        let nested = nested_rig.run_split(
            &format!("p6c-selector-nested-{fn_blocks}"),
            &set,
            Emit {
                selector: Selector::Nested,
                ..Emit::EVERYTHING
            },
            [0; 32],
            END,
            fn_blocks,
        );

        let mut flat_rig = Rig::new(&program);
        let flat = flat_rig.run_split(
            &format!("p6c-selector-flat-{fn_blocks}"),
            &set,
            Emit {
                selector: Selector::Flat,
                ..Emit::EVERYTHING
            },
            [0; 32],
            END,
            fn_blocks,
        );

        assert_eq!(
            nested.regs, flat.regs,
            "at {fn_blocks} blocks a function the two selectors compute different registers"
        );
        assert_eq!(
            nested.instret, flat.instret,
            "at {fn_blocks} blocks a function they retire different instruction counts"
        );
        assert_eq!(nested.cycle, flat.cycle, "at {fn_blocks}: different cycles");
        assert_eq!(nested.pc, flat.pc, "at {fn_blocks}: different exit pc");
        assert_eq!(nested.flags, flat.flags, "at {fn_blocks}: different flags");
        assert_eq!(
            nested.cross, flat.cross,
            "at {fn_blocks}: the selector routes a different number of cross-function edges"
        );
        assert_eq!(
            nested.indirect_miss, flat.indirect_miss,
            "at {fn_blocks}: different indirect misses"
        );
        assert_eq!(nested.loads, flat.loads, "at {fn_blocks}: different loads");
        assert_eq!(
            nested.stores, flat.stores,
            "at {fn_blocks}: different stores"
        );
        assert_eq!(
            nested.escapes, flat.escapes,
            "at {fn_blocks}: different escapes"
        );
        assert!(
            nested.instret > 10_000,
            "the stay has to be long enough to be worth comparing: {} instructions",
            nested.instret
        );
        // At one block a function every edge in the ring is a cross, which is
        // what makes this the selector's own test rather than a test of the
        // sub-dispatchers' `br_table`.
        if fn_blocks == 1 {
            assert!(
                flat.cross > 10_000,
                "one block a function should cross on every edge: {} crosses",
                flat.cross
            );
        }
    }
}

// --- M7b P5: the layout is a permutation and nothing else -------------------

/// **A layout policy is exact by construction, and here is the engine saying
/// so.** M7b P5.
///
/// A global block index is a position in `BlockSet::blocks`, and four
/// different things read that position: the chunk a block is emitted into,
/// the selector's `index / chunk`, an edge's lookup in `BlockSet::index`, and
/// the indirect page map's enumeration. `BlockSet::permuted` moves all four
/// together, so the guest cannot tell — which is an argument, and this is the
/// measurement.
///
/// Run at three sizes, and one block a function is in the list on purpose:
/// there every edge goes through the selector, so a layout that got the index
/// arithmetic wrong could not hide inside a `br_table`.
#[test]
fn a_trace_layout_retires_identically_to_address_order() {
    const N: u32 = 64;
    const END: u64 = 400_000;
    // A ring that hops by 7 rather than by 1. `hop_ring`'s own ring runs
    // `i -> i + 1`, which is already the order a trace lays out, so it would
    // compare a set with itself; 7 is coprime with 64, so the trace visits
    // every block once in an order that is nothing like the addresses'.
    let program = scatter_ring(N, 31, 7);

    for fn_blocks in [1usize, 8, 64] {
        let mut address_rig = Rig::new(&program);
        let set = ring_set(&address_rig, N);
        let traced = set.permuted(&adjacency_order(&set));

        // The fixture has to be one the layout actually moves, or the test
        // passes by comparing a set with itself.
        assert_ne!(
            set.blocks.iter().map(|b| b.pc).collect::<Vec<_>>(),
            traced.blocks.iter().map(|b| b.pc).collect::<Vec<_>>(),
            "the trace layout left this ring in address order; the fixture is not exercising it"
        );
        assert_eq!(traced.blocks.len(), set.blocks.len());

        let address = address_rig.run_split(
            &format!("p5-layout-address-{fn_blocks}"),
            &set,
            Emit::EVERYTHING,
            [0; 32],
            END,
            fn_blocks,
        );

        let mut traced_rig = Rig::new(&program);
        let adjacency = traced_rig.run_split(
            &format!("p5-layout-adjacency-{fn_blocks}"),
            &traced,
            Emit::EVERYTHING,
            [0; 32],
            END,
            fn_blocks,
        );

        assert_eq!(
            address.regs, adjacency.regs,
            "at {fn_blocks} blocks a function the two layouts compute different registers"
        );
        assert_eq!(
            address.instret, adjacency.instret,
            "at {fn_blocks}: different instruction counts"
        );
        assert_eq!(
            address.cycle, adjacency.cycle,
            "at {fn_blocks}: different cycles"
        );
        assert_eq!(
            address.pc, adjacency.pc,
            "at {fn_blocks}: different exit pc"
        );
        assert_eq!(
            address.flags, adjacency.flags,
            "at {fn_blocks}: different flags"
        );
        assert_eq!(
            address.indirect_miss, adjacency.indirect_miss,
            "at {fn_blocks}: different indirect misses"
        );
        assert_eq!(
            address.loads, adjacency.loads,
            "at {fn_blocks}: different loads"
        );
        assert_eq!(
            address.stores, adjacency.stores,
            "at {fn_blocks}: different stores"
        );
        assert_eq!(
            address.escapes, adjacency.escapes,
            "at {fn_blocks}: different escapes"
        );
        assert!(
            address.instret > 10_000,
            "the stay has to be long enough to be worth comparing: {} instructions",
            address.instret
        );
    }
}

// --- M7b P2: polling point (c), inside the stay -----------------------------

/// A guest pc that stands in for a trap vector: somewhere the block set does
/// not hold, so a stay that leaves for it is unmistakable.
const VECTOR: u32 = GUEST_BASE + 0x9000;

/// `lui a1, 0x60000` / `addi a2, x0, 0x55` / `sw a2, 0(a1)` /
/// `addi a4, x0, 0x11` / `jal x0, +4`: one MMIO store with an instruction
/// after it that must not retire when the poll moves the hart.
fn mmio_store_then_one_more() -> Vec<(u32, u32)> {
    vec![
        (GUEST_BASE, 0x6000_05b7),
        (GUEST_BASE + 4, 0x0550_0613),
        (GUEST_BASE + 8, 0x00c5_a023),
        (GUEST_BASE + 12, 0x0110_0713),
        (GUEST_BASE + 16, 0x0040_006f),
    ]
}

/// The store's own host call carries the polling point's **post-store** state:
/// the pc the instruction retires to, and the counters with it charged (JD17).
///
/// The store's own `(pc, cycle)` pair is the *pre*-store one and is checked by
/// `an_off_ram_access_carries_the_exact_pc_and_cycle_to_the_import`; these are
/// the three numbers P2 added, and they are the ones the hart's own polling
/// point is run with.
#[test]
fn a_store_hands_its_polling_point_the_post_store_pc_and_counters() {
    let program = mmio_store_then_one_more();
    let mut rig = Rig::new(&program);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("poll-post-state", &set, Emit::EVERYTHING, [0; 32], u64::MAX);

    let model = CycleModel::Esp32C6;
    let lui = u64::from(model.cycles_for(lp_emu_core::InstClass::Lui));
    let alu = u64::from(model.cycles_for(lp_emu_core::InstClass::Alu));
    let store = u64::from(model.cycles_for(lp_emu_core::InstClass::Store));

    assert_eq!(
        out.posts,
        vec![(GUEST_BASE + 12, lui + alu + store, 3)],
        "the next instruction's pc, the cycle count with the store charged, \
         and minstret with the store retired"
    );
    // Nothing moved, so the stay carried on through the store.
    assert_eq!(out.instret, 5);
    assert_eq!(out.flags, 0);
    assert_eq!(
        out.regs[14], 0x11,
        "the instruction after the store retired"
    );
    assert!(
        out.polls.is_empty(),
        "an MMIO store needs no second crossing"
    );
}

/// A poll that **moves the hart** — a delivered interrupt — ends the stay at
/// the pc it reports, with the store retired and nothing after it.
#[test]
fn a_store_whose_poll_delivers_leaves_at_the_vector_with_no_flag() {
    let program = mmio_store_then_one_more();
    let mut rig = Rig::new(&program);
    rig.store_answer = Some(Polled {
        status: MMIO_LEAVE_AFTER,
        pc: VECTOR,
    });
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("poll-delivers", &set, Emit::EVERYTHING, [0; 32], u64::MAX);

    assert_eq!(
        out.pc, VECTOR,
        "the stay leaves where the poll left the hart"
    );
    assert_eq!(
        out.flags, 0,
        "and with no after-store flag: the polling point has already run"
    );
    assert_eq!(
        out.instret, 3,
        "the store retired; the next instruction did not"
    );
    assert_eq!(out.regs[14], 0, "nothing after the store ran");
}

/// A poll that says the **bus wants the slice** ends it the way a `step_one`
/// slice end does, at the same pc.
#[test]
fn a_store_whose_poll_yields_ends_the_slice() {
    let program = mmio_store_then_one_more();
    let mut rig = Rig::new(&program);
    rig.store_answer = Some(Polled {
        status: MMIO_SLICE_ENDED,
        pc: GUEST_BASE + 12,
    });
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("poll-yields", &set, Emit::EVERYTHING, [0; 32], u64::MAX);

    assert_eq!(out.pc, GUEST_BASE + 12);
    assert_eq!(out.flags, host::FLAG_SLICE_ENDED);
    assert_eq!(out.instret, 3);
    assert_eq!(out.regs[14], 0);
}

/// **The `FLAG_PENDING` store.** An MMIO load leaves a yield on the bus; the
/// next store is an **inline RAM store the bus never sees**, and it is still a
/// polling point. There is no store call to fuse into, so it makes its own
/// crossing — with the same post-store state — and the slice ends at exactly
/// that instruction.
#[test]
fn an_inline_ram_store_after_a_load_left_a_yield_polls_on_its_own() {
    // lui  a1, 0x60000     (MMIO)
    // lw   a3, 0(a1)       -> MMIO_PENDING: the bus is holding a yield
    // lui  a2, 0x40000     (plain RAM)
    // addi a4, x0, 0x77
    // sw   a4, 0x100(a2)   -> inline, and a polling point
    // addi a5, x0, 1       -> must not retire
    // jal  x0, +4
    let program = vec![
        (GUEST_BASE, 0x6000_05b7),
        (GUEST_BASE + 4, 0x0005_a683),
        (GUEST_BASE + 8, 0x4000_0637),
        (GUEST_BASE + 12, 0x0770_0713),
        (GUEST_BASE + 16, 0x10e6_2023),
        (GUEST_BASE + 20, 0x0010_0793),
        (GUEST_BASE + 24, 0x0040_006f),
    ];
    let mut rig = Rig::new(&program);
    rig.load_leaves_yield = true;
    rig.poll_answer = Some(Polled {
        status: MMIO_SLICE_ENDED,
        pc: GUEST_BASE + 20,
    });
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named(
        "pending-ram-store",
        &set,
        Emit::EVERYTHING,
        [0; 32],
        u64::MAX,
    );

    let model = CycleModel::Esp32C6;
    let lui = u64::from(model.cycles_for(lp_emu_core::InstClass::Lui));
    let alu = u64::from(model.cycles_for(lp_emu_core::InstClass::Alu));
    let load = u64::from(model.cycles_for(lp_emu_core::InstClass::Load));
    let store = u64::from(model.cycles_for(lp_emu_core::InstClass::Store));

    assert!(out.stores.is_empty(), "the RAM store never reached the bus");
    assert_eq!(
        out.polls,
        vec![(GUEST_BASE + 20, 2 * lui + alu + load + store, 5)],
        "one polling point, at the RAM store, with its post-store state"
    );
    assert_eq!(out.pc, GUEST_BASE + 20);
    assert_eq!(out.flags, host::FLAG_SLICE_ENDED);
    assert_eq!(
        out.instret, 5,
        "the RAM store retired; the `addi` after it did not"
    );
    assert_eq!(out.regs[15], 0);
    // The store did land, inline: the poll is not a substitute for it.
    let at = (ARENA_AT + 0x100) as usize;
    assert_eq!(rig.mem[at], 0x77);
}

/// And an inline RAM store with **nothing pending** makes no crossing at all
/// — which is the whole point of the `FLAG_PENDING` local.
#[test]
fn an_inline_ram_store_with_nothing_pending_calls_no_host_at_all() {
    let program = vec![
        (GUEST_BASE, 0x4000_0637),
        (GUEST_BASE + 4, 0x0770_0713),
        (GUEST_BASE + 8, 0x10e6_2023),
        (GUEST_BASE + 12, 0x0040_006f),
    ];
    let mut rig = Rig::new(&program);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("plain-ram-store", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    assert!(out.polls.is_empty());
    assert!(out.stores.is_empty());
    assert!(out.loads.is_empty());
    assert_eq!(out.instret, 4);
}

// --- carrying the obligation across a boundary (M7b F1) ---------------------

/// **The obligation survives a cross-function edge.**
///
/// `FLAG_PENDING` is how one sub-dispatcher hands the next the yield an MMIO
/// load left: the epilogue `or`s it into the exit flags and the prologue reads
/// it back with `& FLAG_PENDING`. The load here leaves one, the `jal` crosses
/// into another function, and the **inline RAM store** over there is the
/// polling point that owes it — so it must make its own crossing and end the
/// slice exactly where an interpreted run would.
///
/// One block to a function is the shape that forces the edge; it is the same
/// knob `a_ring_emitted_whole_and_one_block_to_a_function_differs_only_in_crosses`
/// turns.
#[test]
fn a_cross_function_edge_carries_the_obligation_a_pending_load_left() {
    // block 0:
    //   lui  a1, 0x60000    (MMIO)
    //   lw   a3, 0(a1)      -> MMIO_PENDING: the bus is holding a yield
    //   lui  a2, 0x40000    (plain RAM)
    //   jal  x0, +4         -> block 1, which is another function
    // block 1:
    //   addi a4, x0, 0x77
    //   sw   a4, 0x100(a2)  -> inline, and the polling point the load owes
    //   addi a5, x0, 1      -> must not retire
    //   jal  x0, +4
    let program = vec![
        (GUEST_BASE, 0x6000_05b7),
        (GUEST_BASE + 4, 0x0005_a683),
        (GUEST_BASE + 8, 0x4000_0637),
        (GUEST_BASE + 12, 0x0040_006f),
        (GUEST_BASE + 16, 0x0770_0713),
        (GUEST_BASE + 20, 0x10e6_2023),
        (GUEST_BASE + 24, 0x0010_0793),
        (GUEST_BASE + 28, 0x0040_006f),
    ];
    let mut rig = Rig::new(&program);
    rig.load_leaves_yield = true;
    rig.poll_answer = Some(Polled {
        status: MMIO_SLICE_ENDED,
        pc: GUEST_BASE + 24,
    });
    let set = set_of(&rig, GUEST_BASE);
    assert_eq!(set.blocks.len(), 2, "the `jal` ends the first block");
    let out = rig.run_split(
        "pending-across-a-cross",
        &set,
        Emit::EVERYTHING,
        [0; 32],
        u64::MAX,
        1,
    );

    let model = CycleModel::Esp32C6;
    let lui = u64::from(model.cycles_for(lp_emu_core::InstClass::Lui));
    let alu = u64::from(model.cycles_for(lp_emu_core::InstClass::Alu));
    let load = u64::from(model.cycles_for(lp_emu_core::InstClass::Load));
    let store = u64::from(model.cycles_for(lp_emu_core::InstClass::Store));
    let jal = u64::from(model.cycles_for(lp_emu_core::InstClass::JalTail));

    assert_eq!(out.cross, 1, "the `jal` went through the selector");
    assert!(out.stores.is_empty(), "the RAM store never reached the bus");
    assert_eq!(
        out.polls,
        vec![(GUEST_BASE + 24, 2 * lui + load + jal + alu + store, 6)],
        "the obligation crossed with the stay and was taken at the RAM store"
    );
    assert_eq!(out.pc, GUEST_BASE + 24);
    assert_eq!(out.flags, host::FLAG_SLICE_ENDED);
    assert_eq!(
        out.instret, 6,
        "the RAM store retired; the `addi` after it did not"
    );
}

/// **And an exit carries it in the field the protocol names for it.**
///
/// A stay that ends while the yield is still unclaimed has to say so, and
/// `FLAG_PENDING` is the bit that says it — not `FLAG_AFTER_STORE`, which
/// means a store the stay could not poll for itself and which a stay has not
/// set since M7b P2. The bit is what a cross-function edge reads back, and a
/// wrong one is lost on the way through.
///
/// What the *host* does with it at an exit is F3's question, and the answer is
/// **nothing**: `jit.rs`'s `POLL_OWED` is `FLAG_AFTER_STORE` alone, because the
/// obligation is the bus's, not the hart's, and the interpreter that runs after
/// the exit takes it at its own next store. Between P2 and F1 the two bits were
/// read together and the poll ran at the exit's pc, one store early. The
/// machine-level oracle for that is
/// `lp-emu-esp32c6/tests/jit_identity.rs::an_exit_with_an_unclaimed_obligation_ends_the_slice_where_the_interpreter_does`.
#[test]
fn an_exit_while_a_load_is_pending_reports_it_as_pending() {
    // lui a1, 0x60000
    // lw  a3, 0(a1)     -> MMIO_PENDING
    // jal x0, +4        -> off the end of the block set: the stay leaves
    let program = vec![
        (GUEST_BASE, 0x6000_05b7),
        (GUEST_BASE + 4, 0x0005_a683),
        (GUEST_BASE + 8, 0x0040_006f),
    ];
    let mut rig = Rig::new(&program);
    rig.load_leaves_yield = true;
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named(
        "pending-at-an-exit",
        &set,
        Emit::EVERYTHING,
        [0; 32],
        u64::MAX,
    );

    assert_eq!(out.loads.len(), 1);
    assert!(out.polls.is_empty(), "a load is not a polling point");
    assert_eq!(out.flags, host::FLAG_PENDING);
    assert_eq!(out.pc, GUEST_BASE + 12);
    assert_eq!(out.instret, 3);
}

/// **And an escape in between does not lose it.** `step_one` runs an
/// arbitrary guest instruction on the hart, and the stay comes back and
/// carries on — with the obligation an earlier load left still in hand, since
/// nothing it ran was a store that could have taken it.
///
/// `Emit`'s `alu` bit off and `memory` bit on is the one policy that puts an
/// escape between a pending load and an **inline** store; no flag asks for it
/// (`--jit-escape-all` is `Emit::NOTHING`, where the store escapes too), which
/// is why this is a test rather than a path the ladder measures.
#[test]
fn an_escape_between_a_pending_load_and_a_ram_store_keeps_the_poll() {
    // lui  a1, 0x60000     escaped (alu)
    // lw   a3, 0(a1)       emitted  -> MMIO_PENDING
    // lui  a2, 0x40000     escaped (alu)
    // addi a4, x0, 0x77    escaped (alu)
    // sw   a4, 0x100(a2)   emitted, inline, and the polling point
    // addi a5, x0, 1       must not retire
    // jal  x0, +4
    let program = vec![
        (GUEST_BASE, 0x6000_05b7),
        (GUEST_BASE + 4, 0x0005_a683),
        (GUEST_BASE + 8, 0x4000_0637),
        (GUEST_BASE + 12, 0x0770_0713),
        (GUEST_BASE + 16, 0x10e6_2023),
        (GUEST_BASE + 20, 0x0010_0793),
        (GUEST_BASE + 24, 0x0040_006f),
    ];
    let mut rig = Rig::new(&program);
    rig.load_leaves_yield = true;
    rig.poll_answer = Some(Polled {
        status: MMIO_SLICE_ENDED,
        pc: GUEST_BASE + 20,
    });
    let set = set_of(&rig, GUEST_BASE);
    let memory_only = Emit {
        alu: false,
        ..Emit::EVERYTHING
    };
    let out = rig.run_named(
        "pending-across-an-escape",
        &set,
        memory_only,
        [0; 32],
        u64::MAX,
    );

    let model = CycleModel::Esp32C6;
    let lui = u64::from(model.cycles_for(lp_emu_core::InstClass::Lui));
    let alu = u64::from(model.cycles_for(lp_emu_core::InstClass::Alu));
    let load = u64::from(model.cycles_for(lp_emu_core::InstClass::Load));
    let store = u64::from(model.cycles_for(lp_emu_core::InstClass::Store));

    assert_eq!(
        out.escapes,
        vec![GUEST_BASE, GUEST_BASE + 8, GUEST_BASE + 12],
        "the two `lui`s and the `addi` went out; the memory instructions did not"
    );
    assert!(out.stores.is_empty(), "the RAM store never reached the bus");
    assert_eq!(
        out.polls,
        vec![(GUEST_BASE + 20, 2 * lui + alu + load + store, 5)],
        "the obligation survived the escapes and was taken at the RAM store"
    );
    assert_eq!(out.pc, GUEST_BASE + 20);
    assert_eq!(out.flags, host::FLAG_SLICE_ENDED);
    assert_eq!(out.instret, 5);
}

// --- the published MMIO word reads (M7b P3) --------------------------------

/// `0x6000_0000` is published from slot 0, `0x6000_0004` reads a constant, and
/// `0x6000_0008` is published by nobody.
fn published_reads() -> FastReads {
    FastReads {
        offset: FAST_AT,
        reads: [
            Some(FastRead {
                address: 0x6000_0000,
                source: FastSource::Published(0),
            }),
            Some(FastRead {
                address: 0x6000_0004,
                source: FastSource::Constant(0x1234),
            }),
            None,
            None,
        ],
    }
}

/// `lui a1, 0x60000` / four loads off it / `jal x0, +4`.
///
/// A published word, a published constant, an address nobody published, and a
/// **byte** load of a published address.
fn four_mmio_loads() -> Vec<(u32, u32)> {
    vec![
        (GUEST_BASE, 0x6000_05b7),
        (GUEST_BASE + 4, 0x0005_a603),
        (GUEST_BASE + 8, 0x0045_a683),
        (GUEST_BASE + 12, 0x0085_a703),
        (GUEST_BASE + 16, 0x0005_c783),
        (GUEST_BASE + 20, 0x0040_006f),
    ]
}

/// **M7b P3.** An armed block serves the two published **word** reads inside
/// the module and crosses to the import for everything else — an address
/// nobody published, and a narrower access to one that was.
#[test]
fn a_published_word_read_is_served_without_a_host_call() {
    let program = four_mmio_loads();
    let mut rig = Rig::new(&program);
    rig.fast_reads = Some(published_reads());
    rig.publish(true, [0x00c0_ffee, 0]);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("fast-armed", &set, Emit::EVERYTHING, [0; 32], u64::MAX);

    assert_eq!(out.regs[12] as u32, 0x00c0_ffee, "the published word");
    assert_eq!(out.regs[13], 0x1234, "the published constant");
    assert_eq!(out.regs[14] as u32, 0xab08, "nobody published this one");
    assert_eq!(
        out.regs[15] as u32, 0xab00,
        "a byte load is not a word load"
    );
    // The cycles are the interpreter's, not "the cycles of the loads that
    // crossed": a read the block served still charges its own cost, so the
    // third load is `lui + 2 x load` and the fourth is `lui + 3 x load`. That
    // is the property JD17 is about, and it is why this asserts the numbers
    // rather than the count.
    let model = CycleModel::Esp32C6;
    let lui = u64::from(model.cycles_for(lp_emu_core::InstClass::Lui));
    let load = u64::from(model.cycles_for(lp_emu_core::InstClass::Load));
    assert_eq!(
        out.loads,
        vec![
            (GUEST_BASE + 12, lui + 2 * load, 0x6000_0008, 2),
            (GUEST_BASE + 16, lui + 3 * load, 0x6000_0000, 4),
        ],
        "exactly the two the block does not serve reached the import"
    );
    assert_eq!(rig.fast_served(), 2, "and exactly two did not");
}

/// The same module, the same reads, `armed` clear: **every** one of them goes
/// out. Disarming is always safe, which is why it is what the machine does
/// whenever anything it published could have gone stale.
#[test]
fn a_disarmed_block_serves_nothing_at_all() {
    let program = four_mmio_loads();
    let mut rig = Rig::new(&program);
    rig.fast_reads = Some(published_reads());
    rig.publish(false, [0x00c0_ffee, 0]);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("fast-disarmed", &set, Emit::EVERYTHING, [0; 32], u64::MAX);

    assert_eq!(out.regs[12] as u32, 0xab00, "the bus served it");
    assert_eq!(out.regs[13] as u32, 0xab04);
    assert_eq!(out.regs[14] as u32, 0xab08);
    assert_eq!(out.regs[15] as u32, 0xab00);
    assert_eq!(out.loads.len(), 4, "all four crossed");
    assert_eq!(rig.fast_served(), 0);
}

/// A module with no published reads at all is the M7b P2 module: the import is
/// the callee and there is no extra function in it.
#[test]
fn a_module_that_publishes_nothing_calls_the_import_as_before() {
    let program = four_mmio_loads();
    let mut rig = Rig::new(&program);
    rig.publish(true, [0x00c0_ffee, 0]);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("fast-absent", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    assert_eq!(out.loads.len(), 4);
    assert_eq!(out.regs[12] as u32, 0xab00);
    assert_eq!(rig.fast_served(), 0, "nothing wrote the counter");
}

// --- the undecodable block end (M7b P6) -------------------------------------

/// A word `lp_emu_jit::decode` refuses — the whole `SYSTEM` opcode is outside
/// what it recognises — followed by more of the block set.
///
/// `addi a0, x0, 5` / `<SYSTEM>` / `addi a1, x0, 7` / `jal x0, +4` (out).
/// The walk seeds the address after a refused word as a block start, so the
/// pc `step_one` reports is one the target table holds.
fn a_system_in_the_middle(word: u32) -> Vec<(u32, u32)> {
    vec![
        (GUEST_BASE, 0x0050_0513),
        (GUEST_BASE + 4, word),
        (GUEST_BASE + 8, 0x0070_0593),
        (GUEST_BASE + 12, 0x0040_006f),
    ]
}

/// `csrrs x28, mstatus, x0` — a `SYSTEM` word, refused by `decode` and run by
/// the fake interpreter as a marker write.
const SYSTEM_WORD: u32 = 0x3000_2e73;
/// `fence.i`. The whole encoding is fixed (`mach/mod.rs::FENCE_I`).
const FENCE_I_WORD: u32 = 0x0000_100f;

#[test]
fn a_block_that_ends_undecodable_steps_it_and_carries_on() {
    let program = a_system_in_the_middle(SYSTEM_WORD);

    // Without the tables there is nowhere to resolve the reported pc, so this
    // arm keeps the shape it has always had: the stay leaves at the refused
    // word having retired only what came before it.
    let mut plain = Rig::new(&program);
    let set = set_of(&plain, GUEST_BASE);
    assert_eq!(set.blocks.len(), 2, "the refused word cuts the set in two");
    let out = plain.run_named("undec-no-table", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    assert_eq!(out.pc, GUEST_BASE + 4, "it left at the word it refused");
    assert_eq!(out.instret, 1);
    assert!(out.escapes.is_empty(), "and nothing was stepped");
    assert_eq!(out.regs[11], 0, "the second block never ran");

    // With them: `step_one` runs the one word, and the pc it reports resolves
    // through the table into the block that starts there.
    let mut rig = Rig::with_indirect(&program);
    let set = set_of(&rig, GUEST_BASE);
    let fast = rig.run_named("undec-table", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    assert_eq!(
        fast.escapes,
        vec![GUEST_BASE + 4],
        "exactly the refused word went to `step_one`"
    );
    assert_eq!(fast.pc, GUEST_BASE + 16, "the `jal` out of the set");
    assert_eq!(
        fast.instret, 4,
        "the two `addi`s, the stepped word and the `jal`"
    );
    assert_eq!(fast.regs[10], 5, "the first block ran");
    assert_eq!(fast.regs[28], 0x5a5a, "the interpreter ran the refused word");
    assert_eq!(fast.regs[11], 7, "and the stay carried on into the next block");
    assert_eq!(fast.indirect_miss, 0, "the resolve hit");
    assert_eq!(
        fast.escaped_insts, 0,
        "no instruction *in* the set was escaped: the word is past the last one"
    );
}

/// The counters an escaped word is handed and hands back are the interpreter's
/// own, at the right instruction (JD17).
#[test]
fn an_undecodable_block_end_hands_step_one_the_pc_the_block_reached() {
    let program = a_system_in_the_middle(SYSTEM_WORD);
    let mut rig = Rig::with_indirect(&program);
    let set = set_of(&rig, GUEST_BASE);
    let out = rig.run_named("undec-counters", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    let cost = |c| u64::from(CycleModel::Esp32C6.cycles_for(c));
    assert_eq!(
        out.cycle,
        3 * cost(lp_emu_core::InstClass::Alu) + cost(lp_emu_core::InstClass::JalTail),
        "the stepped word charged the same class the fake interpreter charges, \
         once, and the counters came back through the exchange area"
    );
}

/// **The exclusion.** `fence.i` — and the whole `MISC-MEM` opcode it lives in
/// — still ends the stay, because the flush it asks for lands on a block cache
/// and a translated core that are lifted out of the hart for the length of a
/// stay. Stepping it inside one would leave the stay running the bytes it was
/// translated from.
#[test]
fn a_fence_i_at_a_block_end_still_leaves_the_stay() {
    let program = a_system_in_the_middle(FENCE_I_WORD);
    let mut rig = Rig::with_indirect(&program);
    let set = set_of(&rig, GUEST_BASE);
    assert!(
        matches!(
            set.blocks[0].end,
            lp_emu_jit::blocks::BlockEnd::Undecodable(_, lp_emu_jit::blocks::Unknown::Leave)
        ),
        "the walk marked it as the machine's own: {:?}",
        set.blocks[0].end
    );
    let out = rig.run_named("fence-i-end", &set, Emit::EVERYTHING, [0; 32], u64::MAX);
    assert_eq!(out.pc, GUEST_BASE + 4, "the stay left at the `fence.i`");
    assert_eq!(out.instret, 1);
    assert!(out.escapes.is_empty(), "and it was not stepped");
    assert_eq!(out.regs[11], 0, "the next block did not run inside the stay");
}

/// The budget check that comes with the step. The end of a block is a
/// `run_blocks` loop top, and the interpreter checks `cycle_count >= end`
/// there — so a stay whose block charged its whole budget must leave rather
/// than retire one more instruction.
#[test]
fn an_undecodable_block_end_past_the_budget_leaves_instead_of_stepping() {
    let program = a_system_in_the_middle(SYSTEM_WORD);
    let mut rig = Rig::with_indirect(&program);
    let set = set_of(&rig, GUEST_BASE);
    // Exactly the first block's cost: its budget check passes, and it leaves
    // the stay sitting on the end with nothing left to spend.
    let end = u64::from(CycleModel::Esp32C6.cycles_for(lp_emu_core::InstClass::Alu));
    let out = rig.run_named("undec-budget", &set, Emit::EVERYTHING, [0; 32], end);
    assert_eq!(out.instret, 1, "the block ran and the word did not");
    assert_eq!(out.pc, GUEST_BASE + 4, "it left at the word");
    assert!(
        out.escapes.is_empty(),
        "the slice was over before the word could be stepped"
    );
}
