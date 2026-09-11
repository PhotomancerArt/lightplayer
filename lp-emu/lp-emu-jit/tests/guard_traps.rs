//! The guard is real: an out-of-range guest access traps.
//!
//! M7 JD21 turns off wasmtime's explicit bounds checks and lets cranelift
//! elide them, on the strength of the unmapped reservation
//! [`lp_emu_core::arena::GuestArena`] maps behind the arena. That trade is
//! only sound if the guard actually guards, and the failure mode if it does
//! not is silent: an out-of-range store lands in the emulator's own heap and
//! nothing reports anything. So it is proven here rather than assumed.
//!
//! The module these tests run is hand-built rather than translated. It has the
//! translator's imports and entry signature, and its whole body is one guest
//! access at an offset the *caller* chooses — which is the only thing the
//! guard has an opinion about, and lets one module ask both the in-range and
//! the out-of-range question.

#![cfg(feature = "host-wasmtime")]

use lp_emu_core::arena::GuestArena;
use lp_emu_jit::host::{EXCHANGE_LEN, HostOps, MmioLoad, MmioStore, Polled, StepOne};
use lp_emu_jit::host_wasmtime::WasmtimeCore;
use lp_emu_jit::translate::{ENTRY_FUNC, IMPORT_MODULE};
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction as I, MemArg, MemoryType, Module, TypeSection, ValType,
};

/// Six wasm pages, the same shape the translator's own roundtrip rig uses.
const MEM_LEN: usize = 6 * 65536;

/// Far outside the arena and far inside the reservation: 1 GiB in, which no
/// heap allocation next door could ever cover.
const WAY_OUT: u32 = 1 << 30;

/// What the module writes when it is asked to store.
const SENTINEL: i32 = 0x5EED_5EED_u32 as i32;

/// A host that answers nothing: these modules never call an import.
struct NoHost {
    exchange: [u8; EXCHANGE_LEN as usize],
}

impl HostOps for NoHost {
    fn mmio_load(&mut self, _pc: u32, _cycle: u64, _address: u32, _kind: u32) -> MmioLoad {
        unreachable!("the guard module makes no MMIO access")
    }

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
        unreachable!("the guard module makes no MMIO access")
    }

    fn step_one(&mut self, _pc: u32, _cycle: u64, _instret: u64, _regs: &mut [i32; 32]) -> StepOne {
        unreachable!("the guard module escapes nothing")
    }

    fn poll(&mut self, _pc: u32, _cycle: u64, _instret: u64) -> Polled {
        unreachable!("the guard module has no store to poll after")
    }

    fn exchange(&mut self) -> &mut [u8] {
        &mut self.exchange
    }
}

/// What the hand-built module's body does with the address it is handed.
#[derive(Clone, Copy)]
enum Access {
    Load,
    Store,
}

/// A module with the translator's imports and entry signature whose body is
/// one access at `entry` — the first parameter of `run`.
fn guard_probe(access: Access) -> Vec<u8> {
    let mut module = Module::new();

    let mut types = TypeSection::new();
    // The translator's five types, in its order, so the imports line up with
    // what `WasmtimeCore` supplies.
    types.ty().function(
        [ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        [ValType::I64],
    );
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
    types.ty().function([ValType::I32], [ValType::I32]);
    types.ty().function(
        [ValType::I32, ValType::I64, ValType::I64],
        [ValType::I64],
    );
    types.ty().function(
        [
            ValType::I32,
            ValType::I64,
            ValType::I64,
            ValType::I64,
            ValType::I64,
            ValType::I64,
        ],
        [ValType::I32],
    );
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
            minimum: (MEM_LEN / 65536) as u64,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    funcs.function(4);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export(ENTRY_FUNC, ExportKind::Func, 4);
    module.section(&exports);

    let mem = MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    };
    let mut f = Function::new([]);
    match access {
        Access::Load => {
            f.instruction(&I::LocalGet(0));
            f.instruction(&I::I32Load(mem));
        }
        Access::Store => {
            f.instruction(&I::LocalGet(0));
            f.instruction(&I::I32Const(SENTINEL));
            f.instruction(&I::I32Store(mem));
            f.instruction(&I::I32Const(0));
        }
    }
    f.instruction(&I::End);
    let mut code = CodeSection::new();
    code.function(&f);
    module.section(&code);

    module.finish()
}

fn host() -> NoHost {
    NoHost {
        exchange: [0u8; EXCHANGE_LEN as usize],
    }
}

/// Build a core over `arena`, claiming exactly the guard the arena reports.
fn core_over(arena: &mut GuestArena, access: Access) -> WasmtimeCore<NoHost> {
    let guard = arena.guard();
    let base = arena.as_mut_ptr();
    // SAFETY: `arena` outlives the returned core in every caller here — each
    // drops the core first — and nothing else borrows its bytes while the
    // module runs. `guard` is the arena's own report, so a `Some` means the
    // reservation behind `base` really is unmapped past the arena's length.
    unsafe { WasmtimeCore::new(&guard_probe(access), host(), base, MEM_LEN, guard) }
        .expect("the probe module compiles and instantiates")
}

fn enter_at(core: &mut WasmtimeCore<NoHost>, offset: u32) -> wasmtime::Result<()> {
    core.enter(offset, 0, 0, 0, (0, 0)).map(|_| ())
}

#[test]
fn an_access_inside_the_arena_reaches_the_arena() {
    let mut arena = GuestArena::zeroed(MEM_LEN);
    let mut core = core_over(&mut arena, Access::Store);
    enter_at(&mut core, 2048).expect("an in-range store is not a trap");
    drop(core);
    assert_eq!(
        arena[2048..2052],
        SENTINEL.to_le_bytes(),
        "the store landed in the bus's own bytes, which is the point of JD4"
    );
}

/// The other half of the pair: an arena with no guard gets explicit bounds
/// checks, and an out-of-range access is still a trap rather than a write into
/// whatever the allocator put next door. This is the `wasm32-wasip1` path's
/// configuration, exercised natively.
#[test]
fn an_unguarded_arena_still_traps_out_of_range() {
    let mut heap = vec![0u8; MEM_LEN];
    let bystander = vec![0u8; 1 << 20];
    let base = heap.as_mut_ptr();
    // SAFETY: `heap` outlives the core, nothing else borrows it while the
    // module runs, and no guard is claimed — which is the whole of what a
    // plain heap allocation may promise.
    let mut core =
        unsafe { WasmtimeCore::new(&guard_probe(Access::Store), host(), base, MEM_LEN, None) }
            .expect("the probe module compiles and instantiates");
    let err = enter_at(&mut core, WAY_OUT).expect_err("an unguarded store must trap too");
    assert!(
        err.downcast_ref::<wasmtime::Trap>().is_some(),
        "the fault must reach us as a wasm trap: {err:?}"
    );
    drop(core);
    assert!(bystander.iter().all(|&b| b == 0));
    assert!(heap.iter().all(|&b| b == 0));
}

#[cfg(all(unix, not(target_family = "wasm")))]
#[test]
fn a_load_past_the_arena_traps_rather_than_reading_the_host() {
    let mut arena = GuestArena::zeroed(MEM_LEN);
    assert!(
        arena.guard().is_some(),
        "this host maps its arenas, so the guard-page path is the one under test"
    );
    let mut core = core_over(&mut arena, Access::Load);
    let err = enter_at(&mut core, WAY_OUT).expect_err("a load 1 GiB past the arena must trap");
    assert!(
        err.downcast_ref::<wasmtime::Trap>().is_some(),
        "the fault must reach us as a wasm trap, not as some other error: {err:?}"
    );
}

#[cfg(all(unix, not(target_family = "wasm")))]
#[test]
fn a_store_past_the_arena_traps_rather_than_writing_the_host() {
    // A second allocation, so that if the guard were missing and the store
    // landed in the host heap there would be something recognisable to have
    // hit. It is checked afterwards either way.
    let bystander = vec![0u8; 1 << 20];
    let mut arena = GuestArena::zeroed(MEM_LEN);
    assert!(arena.guard().is_some(), "this host maps its arenas");
    let mut core = core_over(&mut arena, Access::Store);
    let err = enter_at(&mut core, WAY_OUT).expect_err("a store 1 GiB past the arena must trap");
    assert!(
        err.downcast_ref::<wasmtime::Trap>().is_some(),
        "the fault must reach us as a wasm trap: {err:?}"
    );
    drop(core);
    assert!(
        bystander.iter().all(|&b| b == 0),
        "the store must not have reached the host's own memory"
    );
    assert!(
        arena.iter().all(|&b| b == 0),
        "and it must not have reached the arena either"
    );
}
