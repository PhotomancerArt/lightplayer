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
use lp_emu_jit::blocks::BlockSet;
use lp_emu_jit::discover::discover;
use lp_emu_jit::host::{
    self, EXCHANGE_CYCLE, EXCHANGE_INSTRET, EXCHANGE_LEN, EXCHANGE_REGS, HostOps, MMIO_OK,
    MmioLoad, MmioStore, PERM_ENTRIES, PERM_NONE, PERM_READ_WRITE, PERM_SHIFT, STEP_CONTINUE,
    StepOne,
};
use lp_emu_jit::host_wasmtime::WasmtimeCore;
use lp_emu_jit::replay::GRANULE_BYTES;
use lp_emu_jit::translate::{Emit, Layout, emit};

// The test memory: exchange, then the permission table, then 64 KiB of guest
// RAM at `GUEST_BASE`. Six wasm pages, so the whole thing is one allocation
// the emitted module addresses with folded constants exactly as the real one
// does.
const EXCHANGE_AT: u32 = 0;
const PERM_AT: u32 = 0x1000;
const ARENA_AT: u32 = PERM_AT + PERM_ENTRIES;
const ARENA_LEN: u32 = 0x1_0000;
const GUEST_BASE: u32 = 0x4000_0000;
const PAGES: u64 = 6;
const MEM_LEN: usize = (PAGES as usize) * 65536;

/// A host that serves the imports out of a plain `Vec` and remembers what it
/// was asked.
struct FakeHost {
    mem: *mut u8,
    /// `(pc, cycle, address, kind)` per `mmio_load`.
    loads: Vec<(u32, u64, u32, u32)>,
    /// `(pc, cycle, address, kind, value)` per `mmio_store`.
    stores: Vec<(u32, u64, u32, u32, u32)>,
    /// The pcs handed to `step_one`, in order.
    escapes: Vec<u32>,
    /// Every import call's *result*, in call order, so another engine can be
    /// handed the same answers without reimplementing the host. See
    /// `engine_case`.
    trace: Vec<Call>,
}

/// One import call's answer, as the JSON a JS host replays.
#[derive(Clone, Debug)]
enum Call {
    Load(i64),
    Store(i32),
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
        // SAFETY: `mem` is the test's own `MEM_LEN`-byte allocation.
        unsafe { core::slice::from_raw_parts_mut(self.mem, MEM_LEN) }
    }
}

impl HostOps for FakeHost {
    fn mmio_load(&mut self, pc: u32, cycle: u64, address: u32, kind: u32) -> MmioLoad {
        self.loads.push((pc, cycle, address, kind));
        let out = MmioLoad {
            status: MMIO_OK,
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
    ) -> MmioStore {
        self.stores.push((pc, cycle, address, kind, value));
        self.trace.push(Call::Store(MMIO_OK as i32));
        MMIO_OK
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
        other => panic!("the fake step_one does not know opcode {other:#x}"),
    };
    Tiny {
        kind,
        width: 4,
        cycles,
    }
}

// --- the rig ----------------------------------------------------------------

fn layout() -> Layout {
    Layout {
        memory_pages: PAGES,
        guest_base: GUEST_BASE,
        arena_offset: ARENA_AT,
        perm_offset: PERM_AT,
        exchange_offset: EXCHANGE_AT,
    }
}

struct Rig {
    /// A `GuestArena` rather than a `Vec`, so these tests run the emitted
    /// modules under the same guard-page configuration the native `--jit`
    /// path uses (M7 JD21) wherever the host can provide one.
    mem: GuestArena,
}

impl Rig {
    /// A memory with the permission table filled in: the 64 KiB of guest RAM
    /// is read-write, and everything else — including the MMIO address the
    /// tests use — is [`PERM_NONE`].
    fn new(program: &[(u32, u32)]) -> Self {
        let mut mem = GuestArena::zeroed(MEM_LEN);
        for page in 0..(ARENA_LEN >> PERM_SHIFT) {
            let entry = ((GUEST_BASE >> PERM_SHIFT) + page) as usize;
            mem[PERM_AT as usize + entry] = PERM_READ_WRITE;
        }
        assert_eq!(mem[PERM_AT as usize], PERM_NONE, "page zero is not RAM");
        for &(pc, word) in program {
            let at = (ARENA_AT + (pc - GUEST_BASE)) as usize;
            mem[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        Self { mem }
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
        let emitted = emit(set, CycleModel::Esp32C6, layout(), policy);
        for (i, r) in regs.iter().enumerate() {
            let at = (EXCHANGE_AT as u64 + EXCHANGE_REGS) as usize + 4 * i;
            self.mem[at..at + 4].copy_from_slice(&r.to_le_bytes());
        }
        let initial = self.mem.to_vec();
        let guard = self.mem.guard();
        let base = self.mem.as_mut_ptr();
        let host = FakeHost {
            mem: base,
            loads: Vec::new(),
            stores: Vec::new(),
            escapes: Vec::new(),
            trace: Vec::new(),
        };
        // SAFETY: `self.mem` outlives `core`, and nothing else holds a
        // reference into it while `enter` is running.
        let mut core = unsafe { WasmtimeCore::new(&emitted.wasm, host, base, MEM_LEN, guard) }
            .expect("the emitted module compiles and instantiates");
        let exit = core
            .enter(0, 0, 0, end, (0, 0))
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
            regs: out_regs,
            loads: core.ops_mut().loads.clone(),
            stores: core.ops_mut().stores.clone(),
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
    reason = "a case file is exactly these eight things, and a struct to carry \
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
) {
    use std::io::Write as _;
    std::fs::create_dir_all(dir).expect("the case directory");
    std::fs::write(format!("{dir}/{case}.wasm"), wasm).expect("the module");
    let calls: Vec<String> = trace
        .iter()
        .map(|c| match c {
            Call::Load(v) => format!(r#"{{"kind":"load","ret":"{v}"}}"#),
            Call::Store(v) => format!(r#"{{"kind":"store","ret":{v}}}"#),
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
        r#"{{"case":"{case}","pages":{PAGES},"exchange":{EXCHANGE_AT},"entry":0,"end":"{end}",
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
    regs: [i32; 32],
    loads: Vec<(u32, u64, u32, u32)>,
    stores: Vec<(u32, u64, u32, u32, u32)>,
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
    assert_eq!(*emitted.mem, *escaped.mem, "and the memory, byte for byte");
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
    assert_eq!(*rig.mem, *escaped.mem);
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

/// The unused-import guard: `host` is named so a future reader can find the
/// protocol from the test that exercises it.
#[test]
fn the_exchange_area_is_where_the_protocol_says_it_is() {
    assert_eq!(host::EXCHANGE_REGS, 0);
    assert_eq!(host::EXCHANGE_CYCLE, 128);
    assert_eq!(host::EXCHANGE_INSTRET, 136);
    assert!(host::EXCHANGE_STATUS < u64::from(EXCHANGE_LEN));
}
