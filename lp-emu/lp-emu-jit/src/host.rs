//! The contract between an emitted module and whatever is hosting it.
//!
//! A translated module imports three functions and one linear memory, and
//! nothing else. This module is the Rust half of that: [`HostOps`] is what a
//! host has to be able to do, and the constants are the wire format the
//! emitted code uses to say what happened.
//!
//! # Why this trait knows nothing about a hart
//!
//! JD2 fixes this crate's dependencies at `lp-emu-core` and `wasm-encoder`, so
//! it cannot name `MachineHart` — which is the right shape anyway. The
//! translator, the emitted module and the wasmtime host live here; the glue
//! that satisfies [`HostOps`] by driving a real hart and a real bus lives in
//! the machine crate that can see both (`lp-emu-esp32c6`). That keeps this
//! crate arch-shaped rather than machine-shaped, and it adds no workspace-local
//! AGPL edge.
//!
//! # The exit protocol
//!
//! This is the thing a future reader needs written down more than anything
//! else in the crate, so it is written down twice — here, and in the crate
//! README.
//!
//! The emitted function is
//!
//! ```text
//! run(entry_block: i32, cycle: i64, instret: i64, end: i64,
//!     watch_lo: i64, watch_hi: i64) -> i32
//! ```
//!
//! and it returns **the guest pc to resume at**. Everything else it has to
//! report goes in the exchange area, a fixed 256-byte window of the imported
//! memory whose offset is folded into the module at emission time:
//!
//! | offset | what |
//! |---|---|
//! | `+0`   | `regs[32]`, one `i32` each, `x0` first |
//! | `+128` | `mcycle` |
//! | `+136` | `minstret` |
//! | `+144` | [`FLAG_AFTER_STORE`] / [`FLAG_SLICE_ENDED`] / [`FLAG_PENDING`] |
//! | `+148` | the status [`HostOps::step_one`] last reported |
//! | `+152` | [`EXCHANGE_CROSS`] — cross-function transfers this stay |
//! | `+160` | [`EXCHANGE_INDIRECT_MISS`] — unresolved indirect jumps |
//! | `+168` | [`EXCHANGE_EXIT_WHY`] — why the stay ended |
//!
//! The **whole** register file is in the exchange area, not just the registers
//! the block set touches: [`HostOps::step_one`] runs an arbitrary guest
//! instruction, which may read or write any register. The registers the block
//! set does touch additionally live in wasm locals for the length of a stay,
//! and are flushed to the exchange area and reloaded around every escape.
//!
//! **`cx.now` discipline (JD17).** The cycle and instruction counters live in
//! wasm locals, and are handed back at every point the bus can observe them:
//! as arguments to every MMIO import, in the exchange area before every
//! `step_one`, and in the exchange area at every exit. P1b measured that
//! charging a block's cycles at entry is not exact, because peripheral models
//! read `cx.now` in the middle of a block.

// ---- the exchange area ----------------------------------------------------

/// `regs[32]`, `x0` first.
pub const EXCHANGE_REGS: u64 = 0;
/// `mcycle`, as an `i64`.
pub const EXCHANGE_CYCLE: u64 = 128;
/// `minstret`, as an `i64`.
pub const EXCHANGE_INSTRET: u64 = 136;
/// The exit flags, as an `i32`.
pub const EXCHANGE_FLAGS: u64 = 144;
/// The status [`HostOps::step_one`] last reported, as an `i32`.
pub const EXCHANGE_STATUS: u64 = 148;
/// How many times the outer selector re-dispatched into another
/// sub-dispatcher function during this stay, as an `i64` (P5, JD8).
///
/// Counted in the selector rather than at the edge, because that is the one
/// place every cross-function transfer passes through, and because it keeps
/// the count off the intra-function paths entirely. The host adds it up and
/// reports it as a rate; a high one is a sizing finding, which is exactly why
/// it is a permanent counter and not a debug build's.
pub const EXCHANGE_CROSS: u64 = 152;
/// How many indirect jumps this stay could **not** resolve in-module, as an
/// `i64`: the target lookup said "no block starts here" and the stay left.
///
/// Incremented only on the miss, so a resolved `jalr` — the common case once
/// the whole image is installed — pays nothing for the counter.
pub const EXCHANGE_INDIRECT_MISS: u64 = 160;
/// Why the stay ended, as an `i32`: one of the [`why`] codes.
///
/// Written at every exit, so a coverage shortfall names its own cause. With
/// the whole image installed this is the only thing that separates "the
/// translator refused an encoding" from "the walk never found the code" from
/// "the polling contract said leave" — three very different problems that all
/// read as interpreted instructions.
pub const EXCHANGE_EXIT_WHY: u64 = 168;
/// How much of the imported memory the exchange area claims.
pub const EXCHANGE_LEN: u32 = 256;

/// The exit was an MMIO store whose side-band or yield the hart must now
/// observe — polling point (c). The hart does exactly what it does after an
/// interpreted store.
pub const FLAG_AFTER_STORE: i32 = 1;
/// The exit was [`HostOps::step_one`] reporting that the *slice* is over — a
/// `wfi`, an `ebreak`, a bus yield or a fault. The host knows which; translated
/// code only knows it has to leave.
pub const FLAG_SLICE_ENDED: i32 = 2;
/// An MMIO **load** left a yield on the bus, so the next store must leave even
/// if it is an inline RAM store the bus never sees.
///
/// Not a flag the host reads — it is how one sub-dispatcher hands that
/// obligation to the next across a cross-function edge (P5). Inside a
/// function it lives in a local; a sub-dispatcher's epilogue writes it here
/// and the next one's prologue picks it up, which is the same flush-and-reload
/// every other piece of stay state does at an exit (JD17). The selector clears
/// the whole field when the host enters, so a stale bit from the last exit
/// cannot be read as this stay's.
pub const FLAG_PENDING: i32 = 4;

/// Why a stay ended. Reported, never acted on.
pub mod why {
    /// The block's own maximum cost did not fit the remaining slice budget.
    pub const BUDGET: i32 = 1;
    /// A `jal`, a branch or a fall-through named a pc the block set does not
    /// hold.
    pub const EDGE_OUT: i32 = 2;
    /// An MMIO store the bus wants observed now — polling point (c).
    pub const AFTER_STORE: i32 = 3;
    /// An indirect jump the target table did not resolve.
    pub const INDIRECT_MISS: i32 = 4;
    /// An indirect jump in a module built with no target table at all.
    pub const INDIRECT_NO_TABLE: i32 = 5;
    /// A load the bus refused: it faulted, or it hit a watchpoint.
    pub const LOAD_REFUSED: i32 = 6;
    /// A load straddling two kinds of page.
    pub const LOAD_STRADDLE: i32 = 7;
    /// A store on a page the permission table does not call writable RAM.
    pub const STORE_PERM: i32 = 8;
    /// A store the bus refused.
    pub const STORE_REFUSED: i32 = 9;
    /// A store straddling two kinds of page, or onto read-only RAM.
    pub const STORE_STRADDLE: i32 = 10;
    /// The word after this block is one `decode` does not recognise, so the
    /// block ended before it (JD7).
    pub const UNDECODABLE: i32 = 11;
    /// An escaped instruction left the decoder's straight line.
    pub const ESCAPE_DIVERGED: i32 = 12;
    /// An escaped terminator landed somewhere this module does not hold.
    pub const ESCAPE_TARGET: i32 = 13;
    /// `step_one` reported the slice over.
    pub const SLICE_ENDED: i32 = 14;
}

// ---- the permission table -------------------------------------------------

/// `log2` of the permission table's page size: one byte per 16 KiB of guest
/// address space. The size the 0.849 ns/instruction phone reading was taken
/// with, and the same granularity `lp_emu_esp_common::bus::PERMISSION_PAGE_LEN`
/// uses, because the host builds one table from the other.
pub const PERM_SHIFT: u32 = 14;

/// Permission byte: not plain RAM. Every access on this page goes out through
/// the import.
///
/// The three values are this crate's own rather than
/// `lp_emu_esp_common::bus`'s, because JD2 fixes this crate's dependencies at
/// `lp-emu-core` and `wasm-encoder`. The host builds one table from the other
/// and is where the two are asserted equal.
pub const PERM_NONE: u8 = 0;
/// Permission byte: plain RAM, loads may be performed inline; stores may not.
pub const PERM_READ: u8 = 1;
/// Permission byte: plain RAM, loads and stores may both be performed inline.
pub const PERM_READ_WRITE: u8 = 2;

/// Entries in the permission table the emitted code indexes.
///
/// The table covers the **whole 32-bit guest address space**, not just the
/// arena, so a load or a store can be checked with one shift and one byte load
/// and no bounds compare: a wild address lands on a zero entry and goes out
/// through the import, where the bus faults it exactly as it always would.
/// 256 KiB buys that, once, per machine.
pub const PERM_ENTRIES: u32 = 1 << (32 - PERM_SHIFT);

// ---- what the imports say -------------------------------------------------

/// The access happened and nothing else needs doing.
pub const MMIO_OK: u32 = 0;
/// The access did **not** happen: it faulted, or it hit a watchpoint.
/// Translated code leaves at the instruction's own pc having retired nothing,
/// and the interpreter runs the instruction and takes the trap, with the right
/// `mepc` and the right `mtval`.
pub const MMIO_REFUSED: u32 = 1;
/// The store happened and the bus is now holding a side-band or a yield.
/// Translated code leaves *after* the store with [`FLAG_AFTER_STORE`], so the
/// hart observes it at exactly the store it always has.
pub const MMIO_LEAVE_AFTER: u32 = 2;
/// The load happened and the bus is now holding a yield. The interpreter does
/// not look after a load, so neither does translated code — but the *next*
/// store must leave, even if it is an inline RAM store the bus never sees.
/// Translated code remembers this in a local for the rest of the stay.
pub const MMIO_PENDING: u32 = 3;

/// [`HostOps::step_one`]: the caller may carry on.
pub const STEP_CONTINUE: u32 = 0;
/// [`HostOps::step_one`]: the instruction ended the slice. Translated code
/// leaves at once with [`FLAG_SLICE_ENDED`]; the host is holding the reason.
pub const STEP_SLICE_ENDED: u32 = 1;

/// The width and signedness code an emitted load hands its import.
///
/// Bit 2 is "zero-extend", the low two bits are `log2(width)` — the same shape
/// the RISC-V `funct3` of a load has, and the reason it is that shape is so a
/// reader can check it against the ISA rather than against a table.
pub mod load_kind {
    pub const B: u32 = 0;
    pub const H: u32 = 1;
    pub const W: u32 = 2;
    pub const BU: u32 = 4;
    pub const HU: u32 = 5;
}

/// The width code an emitted store hands its import: `log2(width)`.
pub mod store_kind {
    pub const B: u32 = 0;
    pub const H: u32 = 1;
    pub const W: u32 = 2;
}

/// What one call into translated code produced.
///
/// Shared by both hosts — `host_wasmtime`'s `enter` and `host_browser`'s each
/// return this — because it is the exit protocol's own shape rather than a
/// property of whichever engine ran the module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exit {
    /// The guest pc to resume at.
    pub pc: u32,
    /// The exchange area's flag word: [`FLAG_AFTER_STORE`],
    /// [`FLAG_SLICE_ENDED`].
    pub flags: i32,
}

/// What a bus access told translated code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MmioLoad {
    /// One of [`MMIO_OK`], [`MMIO_REFUSED`], [`MMIO_PENDING`].
    pub status: u32,
    /// The loaded value, already sign- or zero-extended to 32 bits the way the
    /// instruction asks. Meaningless when `status` is [`MMIO_REFUSED`].
    pub value: u32,
}

/// What a store told translated code: one of [`MMIO_OK`], [`MMIO_REFUSED`],
/// [`MMIO_LEAVE_AFTER`].
pub type MmioStore = u32;

/// What [`HostOps::step_one`] left behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepOne {
    /// The guest pc to continue at — the interpreter's own answer, so a trap
    /// reports the vector and a taken branch reports its target.
    pub pc: u32,
    pub cycle: u64,
    pub instret: u64,
    /// [`STEP_CONTINUE`] or [`STEP_SLICE_ENDED`].
    pub status: u32,
}

/// What translated code needs from the machine that owns it.
///
/// Every method is handed the **exact `(pc, cycle)` the interpreter would have
/// set** before the access, because peripheral models read both.
pub trait HostOps {
    /// An access on a page the permission table says is not plain RAM.
    fn mmio_load(&mut self, pc: u32, cycle: u64, address: u32, kind: u32) -> MmioLoad;

    /// The same, storing.
    fn mmio_store(&mut self, pc: u32, cycle: u64, address: u32, kind: u32, value: u32)
    -> MmioStore;

    /// The escape hatch (JD10): run **exactly one** guest instruction at `pc`,
    /// the way the interpreter would, and report where it left.
    ///
    /// `regs` is the whole architectural register file, read and written in
    /// place. `cycle` and `instret` are what translated code has charged so
    /// far, and the returned ones are what the instruction left.
    fn step_one(&mut self, pc: u32, cycle: u64, instret: u64, regs: &mut [i32; 32]) -> StepOne;

    /// The [`EXCHANGE_LEN`] bytes of the imported memory the emitted module
    /// uses to talk to its host — the *same* bytes, not a copy.
    ///
    /// The layout is this module's and the marshalling is
    /// [`escape_hatch`]'s, so a host implements a window and nothing more.
    fn exchange(&mut self) -> &mut [u8];
}

fn read_i32(at: &[u8]) -> i32 {
    i32::from_le_bytes([at[0], at[1], at[2], at[3]])
}

fn read_i64(at: &[u8]) -> i64 {
    i64::from_le_bytes([at[0], at[1], at[2], at[3], at[4], at[5], at[6], at[7]])
}

/// Serve one `step_one` import call: unpack the exchange area, run the
/// instruction, pack the answer back.
///
/// This is the *only* place the exchange area's register, counter and status
/// fields are read and written on the host side, so the emitted code and the
/// host cannot drift apart on the layout. Returns the pc to continue at, which
/// is what the import hands back to translated code.
///
/// # Panics
///
/// Panics if [`HostOps::exchange`] is shorter than [`EXCHANGE_LEN`].
pub fn escape_hatch<H: HostOps + ?Sized>(ops: &mut H, pc: u32) -> u32 {
    let mut regs = [0i32; 32];
    let (cycle, instret) = {
        let x = ops.exchange();
        assert!(
            x.len() >= EXCHANGE_LEN as usize,
            "the exchange area is {} bytes, not {EXCHANGE_LEN}",
            x.len()
        );
        for (i, r) in regs.iter_mut().enumerate() {
            *r = read_i32(&x[EXCHANGE_REGS as usize + 4 * i..]);
        }
        (
            read_i64(&x[EXCHANGE_CYCLE as usize..]) as u64,
            read_i64(&x[EXCHANGE_INSTRET as usize..]) as u64,
        )
    };

    let out = ops.step_one(pc, cycle, instret, &mut regs);

    let x = ops.exchange();
    for (i, r) in regs.iter().enumerate() {
        x[EXCHANGE_REGS as usize + 4 * i..][..4].copy_from_slice(&r.to_le_bytes());
    }
    x[EXCHANGE_CYCLE as usize..][..8].copy_from_slice(&out.cycle.to_le_bytes());
    x[EXCHANGE_INSTRET as usize..][..8].copy_from_slice(&out.instret.to_le_bytes());
    x[EXCHANGE_STATUS as usize..][..4].copy_from_slice(&out.status.to_le_bytes());
    out.pc
}
