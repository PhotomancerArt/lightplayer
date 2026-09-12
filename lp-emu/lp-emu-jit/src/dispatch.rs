//! Two-level dispatch: the whole image in one module (JD8, JD26).
//!
//! # Why there are two levels at all
//!
//! wasm caps **one function body at 7,654,321 bytes**. At the ~470 B a block
//! emits that is about 16,000 blocks, and a whole-image walk of a render
//! image finds **156,000**. One function is not a tuning choice this
//! translator declined; it is arithmetically impossible. So the module is an
//! outer **selector** over N **sub-dispatchers**, each holding a contiguous
//! run of the block set and each keeping [`crate::translate`]'s shape inside:
//! a `loop` over a `br_table`, one `block` per guest block, forward edges to
//! labels, back edges through the table.
//!
//! The spike measured `br_table` throughput flat in table size — 1.03 ns per
//! guest instruction at 161 blocks, 1.13 at 8,161, in JavaScriptCore — so
//! nothing about *dispatch* argues for small functions. What argues for them
//! is the engine: a multi-megabyte function is a unit of compilation, and a
//! lazily tiering engine may take a very long time over one or decline to
//! optimise it at all. That is the measurement JD26 asks for and
//! `--jit-fn-blocks` is the knob it is taken with.
//!
//! # The three ways control leaves a sub-dispatcher
//!
//! | | how | what the `i64` result says |
//! |---|---|---|
//! | **exit** | the stay is over: a budget ran out, an edge left the block set, the interpreter has to take over | `cross` clear, value = the guest pc |
//! | **cross** | the target block lives in another sub-dispatcher | `cross` set, value = the **global** block index |
//! | **return** | never — a sub-dispatcher only returns through the epilogue | |
//!
//! A cross costs exactly what an exit costs *inside* the module — the
//! counters, the live registers and the pending-yield obligation are flushed
//! to the exchange area and reloaded by the next function's prologue (JD17) —
//! and none of what an exit costs *outside* it: no host frame, no refusal
//! rules, no re-entry. The selector counts them ([`EXCHANGE_CROSS`]) so the
//! rate is a reported number rather than an assumption.
//!
//! # Indirect targets, in O(1), across functions
//!
//! Every guest **return** is a `jalr`, and P3 left the module at every one of
//! them. Two flat tables make that a branch instead:
//!
//! - a **page map**, one `i32` per 16 KiB of the whole 32-bit guest address
//!   space (the same [`PERM_SHIFT`] granularity the permission table uses, so
//!   one shift serves both), holding the linear-memory offset of that page's
//!   slot array;
//! - a **slot array** per 16 KiB page that holds at least one block start:
//!   8,192 `i32`s, one per two bytes of the page, holding the global block
//!   index that starts there or `-1`.
//!
//! Every page with no block start points at **one shared array of `-1`**, so
//! a wild address needs no bounds check and no branch of its own — it reads
//! `-1` like any other address that does not start a block, and the stay
//! leaves. Two loads and no search, for any address in the 4 GiB space.
//!
//! Two-byte granularity is not an economy: **48.99 % of real block starts sit
//! at 2 mod 4**, so a four-byte table would miss half of them.
//!
//! # The sizes, and what they are checked against
//!
//! [`emit_module`] reports the largest sub-dispatcher body it produced, and
//! the host refuses a module whose largest body is over [`BODY_BUDGET`] — 80 %
//! of the limit — rather than letting an engine discover it. That leaves
//! headroom for a block set whose average block is fatter than the one the
//! size was chosen on, which `--jit-escape-all` is: an escaped instruction
//! emits a register flush, a call and a reload where a real one emits a few
//! opcodes.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::vec::Vec;

use lp_emu_core::CycleModel;
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ElementSection, Elements, EntityType, ExportKind,
    ExportSection, Function, FunctionSection, ImportSection, Instruction as I, MemArg, MemoryType,
    Module, RefType, TableSection, TableType, TypeSection, ValType,
};

use crate::blocks::BlockSet;
use crate::host::{
    EXCHANGE_CROSS, EXCHANGE_CYCLE, EXCHANGE_FLAGS, EXCHANGE_INDIRECT_MISS, EXCHANGE_INSTRET,
    PERM_SHIFT,
};
use crate::translate::{
    ENTRY_FUNC, Emit, Emitted, F_FIRST_BODY, F_MMIO_LOAD, IMPORT_MODULE, Layout, emit_body,
    fast_load,
};

/// wasm's implementation limit on a single function body, in bytes.
///
/// Not a wasmtime number and not a JavaScriptCore number — every engine
/// reports the same value, in its own words:
///
/// ```text
/// V8 :  CompileError: WebAssembly.Module(): size 10405871 > maximum function size 7654321
/// JSC:  CompileError: Code function's size 10405871 is too big
/// ```
pub const MAX_FUNCTION_BODY: usize = 7_654_321;

/// The largest sub-dispatcher body this crate will emit: 80 % of
/// [`MAX_FUNCTION_BODY`].
///
/// A budget, not a discovery. The block set a size was chosen on is not the
/// block set it will meet — `--jit-escape-all` emits several times the bytes
/// per block — and an engine's refusal arrives as a compile error a hundred
/// kilobytes into a body rather than as a number anybody can plan against.
pub const BODY_BUDGET: usize = MAX_FUNCTION_BODY / 5 * 4;

/// The outer selector's shape. M7 P6c Q5.
///
/// The selector is the one exported function and the only thing that knows a
/// global block index is a `(sub-dispatcher, local index)` pair. There are two
/// ways to write "call sub-dispatcher number `fidx`" in WebAssembly, and until
/// P6c only one of them had been measured.
///
/// [`Nested`](Self::Nested) is P5's: a `br_table` inside `count` nested
/// `block`s, each arm computing the local index and making a **direct** call.
/// Every arm is about 20 bytes and there is one per sub-dispatcher, so the
/// selector is **O(count)** — and `count` is `blocks / fn_blocks`, so the
/// selector grows as the size knob shrinks. At 8 blocks a function on
/// `render-basic` t2 that is **730,452 bytes and 25,156 nested blocks**, and
/// V8's optimizing tier answers it with `Fatal process out of memory: Zone`
/// in `WasmLoweringPhase`, from a background compile job no `install` retry
/// can catch (P6b H5). It is the reason V8's usable window has a floor at all.
///
/// [`Flat`](Self::Flat) is one `call_indirect` through a function table with
/// one entry per sub-dispatcher: **O(1)** in `count`, whatever the size knob
/// says. It costs a table section, an element segment, and an engine-side
/// signature check on every cross-function edge the selector routes.
///
/// Both produce the same guest behaviour by construction — the same arguments
/// reach the same function, and `tests/translate_roundtrip.rs` asserts the two
/// forms retire identically under wasmtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector {
    /// `count` nested blocks and a direct call. O(count) bytes.
    Nested,
    /// One `call_indirect` through a function table. O(1) bytes.
    Flat,
}

impl Selector {
    /// What [`Emit::EVERYTHING`](crate::translate::Emit::EVERYTHING) uses.
    ///
    /// **[`Flat`](Self::Flat) since M7 P6c**, on the measurement in that
    /// phase's report: it removes V8's low-end wall outright (the selector
    /// stops being the module's largest function at any size) and it is not
    /// slower than the nested form in either engine at the sizes both can run.
    pub const DEFAULT: Self = Self::Flat;
}

/// One `i32` per 16 KiB page of the whole 32-bit guest space.
pub const PAGEMAP_ENTRIES: u32 = 1 << (32 - PERM_SHIFT);
/// The page map's size in bytes.
pub const PAGEMAP_BYTES: u32 = PAGEMAP_ENTRIES * 4;
/// One `i32` per two bytes of a 16 KiB page.
pub const SLOTS_PER_PAGE: u32 = (1 << PERM_SHIFT) / 2;
/// One page's slot array, in bytes.
pub const SLOT_ARRAY_BYTES: u32 = SLOTS_PER_PAGE * 4;

/// The 16 KiB pages of the guest space that hold at least one block start.
fn target_pages(set: &BlockSet) -> Vec<u32> {
    let mut pages: Vec<u32> = set.blocks.iter().map(|b| b.pc >> PERM_SHIFT).collect();
    // Sorted before the dedup, because `dedup` only drops **adjacent**
    // duplicates and the block set is not always in address order: a layout
    // policy (`blocks::BlockOrder`) permutes it. Without the sort a page whose
    // blocks are no longer consecutive gets two slot arrays, the page map ends
    // up pointing at whichever was written last, and the blocks that landed in
    // the other one become unreachable by `jalr` — a silent coverage loss, not
    // a crash. M7b P5.
    pages.sort_unstable();
    pages.dedup();
    pages
}

/// How many bytes [`write_target_tables`] needs for `set`.
///
/// The page map is fixed; the slot arrays are one per page that holds a block
/// start, plus **one** shared array of `-1` every other page points at. It is
/// the shared array that lets an indirect jump skip the "is this address
/// executable" branch entirely: an address on a page with no blocks reads
/// `-1` exactly like an address on a page that has some but not this one.
#[must_use]
pub fn target_table_bytes(set: &BlockSet) -> u64 {
    let pages = target_pages(set).len() as u64 + 1;
    u64::from(PAGEMAP_BYTES) + pages * u64::from(SLOT_ARRAY_BYTES)
}

/// Write the indirect-target page map and slot arrays into `mem` at `at`,
/// and report how many bytes they took.
///
/// `at` is a byte offset into **`mem`**; `base` is where `mem`'s own first
/// byte sits inside the module's imported memory. The page map's entries are
/// **memory**-relative, because the emitted lookup's second load adds nothing
/// to what the first one produced — so a value written here is
/// `base + <offset in mem>`.
///
/// The two bases are the same number natively, where the module's memory *is*
/// the arena and `base` is zero. They are not the same in the browser, where
/// the module imports the emulator's whole linear memory and the arena is an
/// allocation inside it (M7 P6, JD11): the host writes these tables through
/// its own `&mut [u8]` view of the arena and translated code reads them
/// through the memory. A page map holding arena-relative pointers there sends
/// every resolved `jalr` to a block index read out of guest data, which is how
/// this was found.
///
/// # Panics
///
/// Panics when `mem` is shorter than [`target_table_bytes`] past `at`, or
/// when the tables would not fit a 32-bit offset. Both are the host's sizing
/// to get right before it promises [`Layout::indirect`].
pub fn write_target_tables(mem: &mut [u8], base: u32, at: u32, set: &BlockSet) -> u32 {
    let need = target_table_bytes(set);
    let end = u64::from(at) + need;
    assert!(
        end <= mem.len() as u64 && end <= u64::from(u32::MAX),
        "the indirect target tables need {need} bytes at {at} and the memory has {}",
        mem.len()
    );
    let put = |mem: &mut [u8], off: u32, v: i32| {
        mem[off as usize..][..4].copy_from_slice(&v.to_le_bytes());
    };

    // The shared "nothing starts here" array sits first, so every page map
    // entry has something valid to point at before any page is placed.
    let dead = at + PAGEMAP_BYTES;
    for slot in 0..SLOTS_PER_PAGE {
        put(mem, dead + slot * 4, -1);
    }
    for page in 0..PAGEMAP_ENTRIES {
        put(mem, at + page * 4, (base + dead) as i32);
    }

    let mut next = dead + SLOT_ARRAY_BYTES;
    for page in target_pages(set) {
        for slot in 0..SLOTS_PER_PAGE {
            put(mem, next + slot * 4, -1);
        }
        put(mem, at + page * 4, (base + next) as i32);
        next += SLOT_ARRAY_BYTES;
    }
    for (i, b) in set.blocks.iter().enumerate() {
        // An odd start is unreachable by `jalr`, which clears the low bit of
        // every target, and it would share a slot with the address below it.
        // Dropping it from the table costs that block nothing but the
        // indirect edge it could never have had.
        if b.pc & 1 != 0 {
            continue;
        }
        // The page map holds a memory-relative pointer; writing through
        // `mem` needs it back in `mem`'s own terms.
        let array = i32::from_le_bytes(
            mem[(at + (b.pc >> PERM_SHIFT) * 4) as usize..][..4]
                .try_into()
                .expect("four bytes"),
        ) as u32
            - base;
        let slot = (b.pc & ((1 << PERM_SHIFT) - 1)) >> 1;
        put(mem, array + slot * 4, i as i32);
    }
    next - at
}

/// The selector's locals, past its six parameters.
const S_NEXT: u32 = 6;
const S_RET: u32 = 7;
const S_FIDX: u32 = 8;
const S_PC: u32 = 9;
const S_CYC: u32 = 10;
const S_INS: u32 = 11;

const P_ENTRY: u32 = 0;
const P_CYCLE: u32 = 1;
const P_INSTRET: u32 = 2;
const P_END: u32 = 3;
const P_WATCH_LO: u32 = 4;
const P_WATCH_HI: u32 = 5;

fn memarg(offset: u64) -> MemArg {
    MemArg {
        offset,
        align: 0,
        memory_index: 0,
    }
}

/// Emit the whole module for `set`, with at most `fn_blocks` guest blocks in
/// each sub-dispatcher.
///
/// The chunks are contiguous runs of `set.blocks`, so a global block index is
/// `chunk * fn_blocks + local` and the selector needs one divide (one shift,
/// when `fn_blocks` is a power of two) rather than a table of its own.
///
/// **Which blocks share a function is therefore the block set's own order**,
/// and that order is the caller's ([`crate::blocks::BlockOrder`]). M7b P5
/// measured the alternative — a depth-first trace layout, each block followed
/// by the successor it runs into — against address order on one recording and
/// one image, in both engines. The measurement is in `lp-emu-jit/README.md`;
/// the emitter is indifferent, because a layout is a permutation of the set
/// and not a change to what is emitted.
///
/// # Panics
///
/// Panics on an empty block set, or on `fn_blocks == 0` — both are callers'
/// bugs, not states a walk can produce. Callers ask [`BlockSet::is_empty`]
/// first.
#[must_use]
pub fn emit_module(
    set: &BlockSet,
    model: CycleModel,
    layout: Layout,
    policy: Emit,
    fn_blocks: usize,
) -> Emitted {
    assert!(!set.is_empty(), "an empty block set has nothing to emit");
    assert!(fn_blocks > 0, "a sub-dispatcher holds at least one block");
    let total = set.blocks.len();
    let chunk = fn_blocks.min(total);
    let count = total.div_ceil(chunk);

    // `$fast_load` — the machine's published MMIO word reads (M7b P3) — is the
    // module's **last** function, past the selector, so every index a
    // sub-dispatcher, the element segment and the export already use is
    // exactly what it was. An MMIO load calls it instead of the import when
    // the machine published any; the two have the same signature, so the call
    // site is byte-for-byte unchanged either way.
    let fast_func = layout.fast_reads.map(|_| F_FIRST_BODY + count as u32 + 1);
    let load_func = fast_func.unwrap_or(F_MMIO_LOAD);

    let mut bodies = Vec::with_capacity(count);
    let (mut native_insts, mut escaped_insts) = (0usize, 0usize);
    for c in 0..count {
        let lo = c * chunk;
        let len = chunk.min(total - lo);
        let (f, native, escaped) = emit_body(set, lo, len, model, layout, policy, load_func);
        native_insts += native;
        escaped_insts += escaped;
        bodies.push(f);
    }
    // The selector is built here rather than at the code section, because it
    // is a function body like any other and `max_body_bytes` has to be able to
    // see it. M7 P6b found that it could not: `emit_module` measured the
    // sub-dispatchers and not the one function that grows as they shrink, so
    // the budget check below had a blind spot at exactly the sizes where the
    // selector is the largest function in the module.
    let sel = selector(count, chunk, layout, policy.selector);
    let fast = layout.fast_reads.map(fast_load);
    let max_sub_body_bytes = bodies.iter().map(Function::byte_len).max().unwrap_or(0);
    let selector_bytes = sel.byte_len();
    let fast_bytes = fast.as_ref().map_or(0, Function::byte_len);
    let max_body_bytes = max_sub_body_bytes.max(selector_bytes).max(fast_bytes);

    let mut module = Module::new();

    let mut types = TypeSection::new();
    // 0: mmio_load(pc, cycle, address, kind) -> (status << 32) | value
    types.ty().function(
        [ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        [ValType::I64],
    );
    // 1: mmio_store(pc, cycle, address, kind, value,
    //               post_pc, post_cycle, post_instret) -> (status << 32) | pc
    //
    // The last three are polling point (c)'s: the store's own call runs it, so
    // a store that raises an interrupt does not have to leave (M7b P2).
    types.ty().function(
        [
            ValType::I32,
            ValType::I64,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I64,
            ValType::I64,
        ],
        [ValType::I64],
    );
    // 2: step_one(pc) -> pc
    types.ty().function([ValType::I32], [ValType::I32]);
    // 3: poll(pc, cycle, instret) -> (status << 32) | pc
    types
        .ty()
        .function([ValType::I32, ValType::I64, ValType::I64], [ValType::I64]);
    let stay = [
        ValType::I32,
        ValType::I64,
        ValType::I64,
        ValType::I64,
        ValType::I64,
        ValType::I64,
    ];
    // 4: a sub-dispatcher: (entry_local, …) -> (cross << 32) | value
    types.ty().function(stay, [ValType::I64]);
    // 5: run(entry_global, …) -> exit pc
    types.ty().function(stay, [ValType::I32]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import(IMPORT_MODULE, "mmio_load", EntityType::Function(0));
    imports.import(IMPORT_MODULE, "mmio_store", EntityType::Function(1));
    imports.import(IMPORT_MODULE, "step_one", EntityType::Function(2));
    imports.import(IMPORT_MODULE, "poll", EntityType::Function(3));
    imports.import(
        IMPORT_MODULE,
        "memory",
        EntityType::Memory(MemoryType {
            minimum: layout.memory_pages,
            // Deliberately unbounded: in the browser this imports the
            // emulator's own memory, which is larger than the arena and may
            // grow. A declared maximum would refuse it.
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    for _ in 0..count {
        funcs.function(4);
    }
    funcs.function(5);
    if fast.is_some() {
        // Type 0 is `mmio_load`'s, which is the signature `$fast_load` has.
        funcs.function(0);
    }
    module.section(&funcs);

    // The flat selector's own table: one funcref per sub-dispatcher, in
    // sub-dispatcher order, so the table index IS the function index the
    // nested form branches on. Declared with its maximum pinned to its
    // initial size — nothing grows it, and an engine that knows the bound can
    // fold the bounds check.
    //
    // Note it is table **0** of this module and has nothing to do with the
    // emulator's own `__indirect_function_table`: an emitted module has no
    // tables at all otherwise, and imports none.
    if policy.selector == Selector::Flat {
        let mut tables = TableSection::new();
        tables.table(TableType {
            element_type: RefType::FUNCREF,
            minimum: count as u64,
            maximum: Some(count as u64),
            table64: false,
            shared: false,
        });
        module.section(&tables);
    }

    let selector_index = F_FIRST_BODY + count as u32;
    let mut exports = ExportSection::new();
    exports.export(ENTRY_FUNC, ExportKind::Func, selector_index);
    module.section(&exports);

    if policy.selector == Selector::Flat {
        let mut elements = ElementSection::new();
        let fns: Vec<u32> = (0..count as u32).map(|j| F_FIRST_BODY + j).collect();
        elements.active(
            Some(0),
            &ConstExpr::i32_const(0),
            Elements::Functions(Cow::Owned(fns)),
        );
        module.section(&elements);
    }

    let mut codes = CodeSection::new();
    for f in &bodies {
        codes.function(f);
    }
    codes.function(&sel);
    if let Some(f) = &fast {
        codes.function(f);
    }
    module.section(&codes);

    Emitted {
        wasm: module.finish(),
        native_insts,
        escaped_insts,
        functions: count,
        max_body_bytes,
        max_sub_body_bytes,
        selector_bytes,
    }
}

/// The outer selector: the one export, and the only thing that knows a global
/// block index is a `(function, local index)` pair.
fn selector(count: usize, chunk: usize, layout: Layout, shape: Selector) -> Function {
    let locals = alloc::vec![
        (1, ValType::I32), // next
        (1, ValType::I64), // the sub-dispatcher's result
        (2, ValType::I32), // function index, exit pc
        (2, ValType::I64), // cycle, instret
    ];
    let mut f = Function::new(locals);
    let mut e = |ins: I<'static>| {
        f.instruction(&ins);
    };
    let exchange = |field: u64| memarg(u64::from(layout.exchange_offset) + field);

    // The stay's own flags, and the stay's own counters. Cleared here because
    // this is the one place the host's entry and the module's begin together:
    // a `FLAG_PENDING` left by the *last* exit is not this stay's obligation,
    // and the two counters below are read back per entry.
    e(I::I32Const(0));
    e(I::I32Const(0));
    e(I::I32Store(exchange(EXCHANGE_FLAGS)));
    e(I::I32Const(0));
    e(I::I64Const(0));
    e(I::I64Store(exchange(EXCHANGE_CROSS)));
    e(I::I32Const(0));
    e(I::I64Const(0));
    e(I::I64Store(exchange(EXCHANGE_INDIRECT_MISS)));

    e(I::LocalGet(P_ENTRY));
    e(I::LocalSet(S_NEXT));
    e(I::LocalGet(P_CYCLE));
    e(I::LocalSet(S_CYC));
    e(I::LocalGet(P_INSTRET));
    e(I::LocalSet(S_INS));

    let last = count - 1;
    let pow2 = chunk.is_power_of_two();
    let shift = chunk.trailing_zeros() as i32;

    e(I::Block(BlockType::Empty)); // $outer
    e(I::Loop(BlockType::Empty)); // $L

    // fidx = next / chunk. A power-of-two size makes that a shift and the
    // remainder below a mask, which is why the sizes JD26 is measured at are
    // powers of two; anything else still works, one `i32.div_u` slower per
    // cross-function edge.
    e(I::LocalGet(S_NEXT));
    if pow2 {
        e(I::I32Const(shift));
        e(I::I32ShrU);
    } else {
        e(I::I32Const(chunk as i32));
        e(I::I32DivU);
    }
    e(I::LocalSet(S_FIDX));

    // The local index and the five stay arguments, which both shapes push in
    // the same order onto the same six-parameter signature (type 4).
    let args = |e: &mut dyn FnMut(I<'static>)| {
        // local = next % chunk
        e(I::LocalGet(S_NEXT));
        if pow2 {
            e(I::I32Const(chunk as i32 - 1));
            e(I::I32And);
        } else {
            e(I::I32Const(chunk as i32));
            e(I::I32RemU);
        }
        e(I::LocalGet(S_CYC));
        e(I::LocalGet(S_INS));
        e(I::LocalGet(P_END));
        e(I::LocalGet(P_WATCH_LO));
        e(I::LocalGet(P_WATCH_HI));
    };

    match shape {
        // O(1) in `count`: the function index is a table index, and one
        // `call_indirect` is the whole dispatch. No `$sel` block, no per-arm
        // block, no `br_table`.
        Selector::Flat => {
            args(&mut e);
            e(I::LocalGet(S_FIDX));
            e(I::CallIndirect {
                type_index: 4,
                table_index: 0,
            });
            e(I::LocalSet(S_RET));
        }
        // O(count): one nested block and one direct call per sub-dispatcher.
        Selector::Nested => {
            e(I::Block(BlockType::Empty)); // $sel
            for _ in 0..=last {
                e(I::Block(BlockType::Empty));
            }
            e(I::Block(BlockType::Empty)); // $tbl
            e(I::LocalGet(S_FIDX));
            let table: Vec<u32> = (1..=(last as u32 + 1)).collect();
            e(I::BrTable(Cow::Owned(table), 0));
            e(I::End);
            // A function index the host invented. Unreachable by construction
            // — the index came out of this very block set — and said so rather
            // than papered over, exactly as the inner dispatcher's default arm
            // is.
            e(I::Unreachable);
            for j in 0..=last {
                e(I::End);
                args(&mut e);
                e(I::Call(F_FIRST_BODY + j as u32));
                e(I::LocalSet(S_RET));
                e(I::Br((last - j) as u32));
            }
            e(I::End); // $sel
        }
    }

    // The counters come back the way they go out at an exit (JD17): through
    // the exchange area, which the sub-dispatcher's epilogue has just
    // written.
    e(I::I32Const(0));
    e(I::I64Load(exchange(EXCHANGE_CYCLE)));
    e(I::LocalSet(S_CYC));
    e(I::I32Const(0));
    e(I::I64Load(exchange(EXCHANGE_INSTRET)));
    e(I::LocalSet(S_INS));

    e(I::LocalGet(S_RET));
    e(I::I64Const(32));
    e(I::I64ShrU);
    e(I::I32WrapI64);
    e(I::If(BlockType::Empty));
    e(I::LocalGet(S_RET));
    e(I::I32WrapI64);
    e(I::LocalSet(S_NEXT));
    e(I::I32Const(0));
    e(I::I32Const(0));
    e(I::I64Load(exchange(EXCHANGE_CROSS)));
    e(I::I64Const(1));
    e(I::I64Add);
    e(I::I64Store(exchange(EXCHANGE_CROSS)));
    e(I::Br(1)); // $L
    e(I::End);

    e(I::LocalGet(S_RET));
    e(I::I32WrapI64);
    e(I::LocalSet(S_PC));
    e(I::Br(1)); // $outer

    e(I::End); // $L — never falls through
    e(I::Unreachable);
    e(I::End); // $outer

    e(I::LocalGet(S_PC));
    e(I::End);
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{Block, BlockEnd};
    use crate::decode::{Decoded, Inst, OpI};
    use alloc::collections::BTreeMap;
    use lp_emu_core::InstClass;

    /// `n` blocks laid end to end from `base`, each falling into the next.
    ///
    /// Synthetic on purpose: the point is the module's shape at a size no
    /// fixture image makes cheap to reach. The mix is a quarter memory
    /// operations, because it is the inline permission check around a load or
    /// a store that decides how many bytes a block emits — an all-`addi`
    /// ladder emits 70 B a block against the ~470 B the census measured, and
    /// a size test built on it would be testing nothing.
    fn ladder(base: u32, n: usize) -> BlockSet {
        const PER_BLOCK: u32 = 8;
        let mut blocks = Vec::with_capacity(n);
        let mut index = BTreeMap::new();
        let step = PER_BLOCK * 4;
        for i in 0..n {
            let pc = base + (i as u32) * step;
            let insts = (0..PER_BLOCK)
                .map(|j| {
                    let inst = match j % 4 {
                        0 => Inst::Load {
                            kind: crate::decode::LoadKind::W,
                            rd: 2,
                            rs1: 3,
                            imm: 8,
                        },
                        2 => Inst::Store {
                            kind: crate::decode::StoreKind::W,
                            rs1: 3,
                            rs2: 2,
                            imm: 12,
                        },
                        _ => Inst::OpImm {
                            op: OpI::Addi,
                            rd: 1,
                            rs1: 1,
                            imm: 1,
                        },
                    };
                    let class = match j % 4 {
                        0 => InstClass::Load,
                        2 => InstClass::Store,
                        _ => InstClass::Alu,
                    };
                    (
                        pc + j * 4,
                        Decoded {
                            inst,
                            width: 4,
                            class,
                        },
                    )
                })
                .collect();
            index.insert(pc, i);
            blocks.push(Block {
                pc,
                insts,
                end: BlockEnd::Fall(pc + step),
            });
        }
        BlockSet::from_blocks(blocks, index)
    }

    fn layout() -> Layout {
        Layout {
            memory_pages: 64,
            guest_base: 0x4000_0000,
            arena_offset: 0,
            perm_offset: 0,
            exchange_offset: 0,
            indirect: None,
            fast_reads: None,
        }
    }

    /// The rule the whole phase exists to keep: whatever the block set, no
    /// **function** in the module is anywhere near the size an engine refuses
    /// at. A test, not a habit (P5's definition of done).
    #[test]
    fn no_sub_dispatcher_exceeds_the_body_budget() {
        let set = ladder(0x4200_0000, 40_000);
        let one = emit_module(
            &set,
            CycleModel::Esp32C6,
            layout(),
            Emit::EVERYTHING,
            40_000,
        );
        assert_eq!(one.functions, 1);
        assert!(
            one.max_body_bytes > BODY_BUDGET,
            "40,000 blocks in one function is {} bytes, which was supposed to be over the \
             {BODY_BUDGET}-byte budget — if it is not, this test has stopped testing anything",
            one.max_body_bytes
        );

        // Deliberately not the sizes JD26 measures on a render image: this
        // ladder emits ~860 B a block against that image's ~470, which is the
        // point — the budget is checked against the block set in hand, never
        // against a size that was right for another one.
        for fn_blocks in [500usize, 1_000, 2_000, 4_000] {
            let split = emit_module(
                &set,
                CycleModel::Esp32C6,
                layout(),
                Emit::EVERYTHING,
                fn_blocks,
            );
            assert_eq!(split.functions, 40_000usize.div_ceil(fn_blocks));
            assert!(
                split.max_body_bytes <= BODY_BUDGET,
                "at {fn_blocks} blocks per function the largest body is {} bytes, over the \
                 {BODY_BUDGET}-byte budget",
                split.max_body_bytes
            );
            assert_eq!(split.native_insts, one.native_insts);
            assert_eq!(split.escaped_insts, one.escaped_insts);
        }
    }

    /// **The budget's other end** (M7 P6b finding 2, landed in P6c).
    ///
    /// `BODY_BUDGET` exists so `install` can refuse a module and retry at a
    /// smaller size. It is checked against `Emitted::max_body_bytes`, and
    /// until P6c that number was the largest *sub-dispatcher* — which is the
    /// module's largest function only while the sub-dispatchers are big. Make
    /// them small enough and the **selector** takes over: P6b measured
    /// 730,452 B of selector against a 40,244 B largest sub-dispatcher at 8
    /// blocks a function on `render-basic` t2, and the check saw the 40,244.
    ///
    /// So: at one block per function on a ladder big enough to matter, assert
    /// the two numbers have crossed, and that `max_body_bytes` is the
    /// selector rather than the sub-dispatcher. Without the fix this test
    /// fails on its last assertion — `max_body_bytes` would be the small
    /// number.
    #[test]
    fn the_body_budget_counts_the_selector_too() {
        let set = ladder(0x4200_0000, 8_000);

        let nested = emit_module(
            &set,
            CycleModel::Esp32C6,
            layout(),
            Emit {
                selector: Selector::Nested,
                ..Emit::EVERYTHING
            },
            1,
        );
        assert_eq!(nested.functions, 8_000);
        assert!(
            nested.selector_bytes > nested.max_sub_body_bytes,
            "at one block a function the nested selector ({} B) was supposed to be larger than \
             the largest sub-dispatcher ({} B) — if it is not, this test has stopped testing \
             anything",
            nested.selector_bytes,
            nested.max_sub_body_bytes
        );
        assert_eq!(
            nested.max_body_bytes, nested.selector_bytes,
            "`max_body_bytes` has to be the largest body in the module, and here that is the \
             selector ({} B), not the largest sub-dispatcher ({} B)",
            nested.selector_bytes, nested.max_sub_body_bytes
        );

        // And the flat selector is the reason the low end is usable at all:
        // one `call_indirect` is the same handful of bytes whether it routes
        // to eight functions or to eight thousand.
        let flat = emit_module(
            &set,
            CycleModel::Esp32C6,
            layout(),
            Emit {
                selector: Selector::Flat,
                ..Emit::EVERYTHING
            },
            1,
        );
        assert_eq!(flat.functions, nested.functions);
        assert_eq!(flat.max_sub_body_bytes, nested.max_sub_body_bytes);
        assert!(
            flat.selector_bytes < nested.selector_bytes / 100,
            "the flat selector is O(1) in the function count and the nested one is O(count); at \
             8,000 functions they were {} B and {} B",
            flat.selector_bytes,
            nested.selector_bytes
        );
        assert_eq!(
            flat.max_body_bytes, flat.max_sub_body_bytes,
            "with the flat selector the largest body is a sub-dispatcher again"
        );
    }

    /// The two selector shapes emit the same guest work at every size — same
    /// sub-dispatchers, same instruction counts — and differ only in the one
    /// function that routes between them.
    ///
    /// That the flat form *runs* is
    /// `tests/translate_roundtrip.rs::the_two_selector_shapes_retire_identically`,
    /// which needs an engine and so cannot live here.
    #[test]
    fn the_two_selector_shapes_differ_only_in_the_selector() {
        let set = ladder(0x4200_0000, 600);
        for fn_blocks in [1usize, 8, 32, 600] {
            let [nested, flat] = [Selector::Nested, Selector::Flat].map(|selector| {
                emit_module(
                    &set,
                    CycleModel::Esp32C6,
                    layout(),
                    Emit {
                        selector,
                        ..Emit::EVERYTHING
                    },
                    fn_blocks,
                )
            });
            assert_eq!(flat.functions, 600usize.div_ceil(fn_blocks));
            assert_eq!(flat.functions, nested.functions);
            assert_eq!(flat.native_insts, nested.native_insts);
            assert_eq!(flat.escaped_insts, nested.escaped_insts);
            assert_eq!(flat.max_sub_body_bytes, nested.max_sub_body_bytes);
            assert!(flat.native_insts > 0);
        }
    }

    /// The two tables answer every address in the 32-bit space, and they say
    /// `-1` everywhere a block does not start.
    ///
    /// Run at **two bases**, and the second one is the point. Natively the
    /// host's `&mut [u8]` view and the module's imported memory are the same
    /// bytes at the same offsets, so a page map holding
    /// pointers-into-the-view and a page map holding pointers-into-the-memory
    /// are indistinguishable. In the browser they are not, and every table in
    /// this file was written before there was a browser host to notice — a
    /// resolved `jalr` there read a block index out of guest data, and the
    /// only symptom was a run that diverged half a second in. The lookup below
    /// is deliberately written the way the *emitted code* does it: follow the
    /// page map's value as a memory offset, adding nothing.
    #[test]
    fn the_target_tables_answer_every_address() {
        for base in [0u32, 0x0011_0000] {
            let set = ladder(0x4200_0000, 300);
            let at = 32u32;
            let need = target_table_bytes(&set);
            let mut mem = alloc::vec![0u8; at as usize + need as usize + 16];
            let used = write_target_tables(&mut mem, base, at, &set);
            assert_eq!(u64::from(used), need);
            // 300 blocks of 16 bytes is 4,800: one 16 KiB page of slots, plus
            // the shared dead array every other page points at.
            assert_eq!(
                need,
                u64::from(PAGEMAP_BYTES) + 2 * u64::from(SLOT_ARRAY_BYTES)
            );

            let lookup = |addr: u32| -> i32 {
                // `memarg(pagemap)`: the page map is at `base + at` in the
                // memory, and `mem` starts at `base`.
                let array = i32::from_le_bytes(
                    mem[(at + (addr >> PERM_SHIFT) * 4) as usize..][..4]
                        .try_into()
                        .expect("four bytes"),
                ) as u32;
                // `memarg(0)`: whatever the page map said, used as-is.
                let slot = (addr & ((1 << PERM_SHIFT) - 1)) >> 1;
                i32::from_le_bytes(
                    mem[(array - base + slot * 4) as usize..][..4]
                        .try_into()
                        .expect("four bytes"),
                )
            };
            for (i, b) in set.blocks.iter().enumerate() {
                assert_eq!(
                    lookup(b.pc),
                    i as i32,
                    "base {base:#x}, block {i} at {:#010x}",
                    b.pc
                );
                assert_eq!(
                    lookup(b.pc + 2),
                    -1,
                    "base {base:#x}, mid-block at {:#010x}",
                    b.pc + 2
                );
            }
            // Every page with no block start reads -1, page zero and the top
            // of the address space included.
            for addr in [0u32, 2, 0x4080_0000, 0x5000_0000, 0xffff_fffe] {
                assert_eq!(
                    lookup(addr),
                    -1,
                    "base {base:#x}, {addr:#010x} starts no block"
                );
            }
        }
    }
}
