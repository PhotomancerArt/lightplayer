//! A wasmtime host for emitted modules — development, CI, and the native
//! `--jit` path (JD18).
//!
//! **Never the product host.** In the browser the engine is the browser's own,
//! and this exists so identity can be proven on the desk against the same
//! module bytes.
//!
//! # The memory, and the configuration that is not here
//!
//! JD18 says any wasmtime host **must** be configured with guard-page traps:
//!
//! ```text
//! // Guard-page bounds checks, the way a browser engine always does them.
//! // Without these wasmtime emits an explicit check on EVERY guest load and
//! // store, and the region runs 4.8x slower — a property of the native host,
//! // not of the emitted module. Measured: 308 -> 1481 M instr/s.
//! config.signals_based_traps(true);
//! config.memory_reservation(1 << 32);
//! config.memory_guard_size(1 << 31);
//! config.memory_may_move(false);
//! ```
//!
//! **This host cannot use it, and the reason is structural rather than an
//! oversight.** JD4 gives the guest arena to the bus, as a `Vec<u8>`, and the
//! emitted module has to import *that* memory rather than a copy — copying a
//! 256 MiB arena in and out around every slice would cost more than
//! translation saves. So the memory is supplied through
//! [`wasmtime::MemoryCreator`], aliasing the bus's own allocation. And
//! `MemoryCreator`'s safety contract is explicit that a host-supplied memory
//! has to be followed by `guard_size_in_bytes` of **unmapped** address space,
//! because cranelift elides bounds checks on the strength of it. A `Vec` is
//! followed by whatever the allocator put there.
//!
//! Configuring the guard and then handing over a `Vec` would not fail: it
//! would turn a translator bug from a wasm trap into a silent write into the
//! host heap. So this host asks for **explicit bounds checks** — the slow,
//! safe half of the pair — and the native `--jit` path pays the ~4.8x the
//! spike measured.
//!
//! Two things follow, and both belong in the milestone ADR:
//!
//! - **the browser has no such conflict.** There the emulator *is* a wasm
//!   module: the arena is a `Vec` inside its own linear memory, the module
//!   imports that memory, and the arena's base is an offset. The engine's own
//!   guard pages are already there and no aliasing is involved. The 4.8x is a
//!   property of hosting wasm *next to* a native emulator, not of the design;
//! - **a native host that wants the guard pages has to own the arena.** That
//!   is a real option — the memory creator maps a guarded reservation and the
//!   bus is handed a view of it — and it is P8's to weigh, with the numbers,
//!   against caching compiled modules. P3 does not need it: identity does not
//!   care how fast it is proven.

extern crate alloc;
// The crate is `no_std`; this module is not, and only exists behind the
// `host-wasmtime` feature — which drags in wasmtime, which is `std`. Named
// here so JD20's boot-cost line can be timed with a real clock: the emitting
// half of that line is timed by the caller, and the compiling half can only
// be timed where the compiler is called.
extern crate std;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;

use wasmtime::{
    Caller, Config, Engine, Extern, Instance, LinearMemory, MemoryCreator, Module, Store, TypedFunc,
};

use crate::host::{EXCHANGE_FLAGS, HostOps};
use crate::translate::ENTRY_FUNC;

/// A linear memory that *is* the emulator's guest arena.
///
/// # Safety
///
/// The pointer must stay valid, and the allocation must not move, for as long
/// as the [`Store`] this memory is instantiated into is alive. That is the
/// same promise `SocBus` already makes about its arena — "it must not move
/// once the machine is running" — and it is why the arena is allocated once,
/// during construction, from the chip's declared memory map.
///
/// The memory is **not** guarded. See this module's docs: the configuration
/// that goes with it asks for explicit bounds checks, so nothing is relying on
/// the bytes after the allocation being unmapped.
struct ArenaMemory {
    base: *mut u8,
    len: usize,
}

// SAFETY: the pointer is only dereferenced by wasm code running inside the
// store, and the store is not `Send` across a run. The `Send`/`Sync` bounds are
// wasmtime's blanket requirement on the trait, not a claim that two threads
// may use this at once.
unsafe impl Send for ArenaMemory {}
unsafe impl Sync for ArenaMemory {}

// SAFETY: `byte_size` and `byte_capacity` report exactly the allocation the
// caller promised, `as_ptr` returns its base, and `grow_to` refuses anything
// larger — so wasm can never address a byte outside it. The trait's guard-page
// clause is discharged by the configuration rather than by the allocation: see
// the module docs.
unsafe impl LinearMemory for ArenaMemory {
    fn byte_size(&self) -> usize {
        self.len
    }

    fn byte_capacity(&self) -> usize {
        self.len
    }

    fn grow_to(&mut self, new_size: usize) -> wasmtime::Result<()> {
        if new_size <= self.len {
            Ok(())
        } else {
            Err(wasmtime::Error::msg(
                "the guest arena is a fixed allocation and cannot grow",
            ))
        }
    }

    fn as_ptr(&self) -> *mut u8 {
        self.base
    }
}

struct ArenaMemoryCreator {
    base: usize,
    len: usize,
}

// SAFETY: every memory this creator hands out is the same, correctly sized
// view of one allocation the caller has promised is stable — see
// [`ArenaMemory`].
unsafe impl MemoryCreator for ArenaMemoryCreator {
    fn new_memory(
        &self,
        _ty: wasmtime::MemoryType,
        minimum: usize,
        _maximum: Option<usize>,
        _reserved_size_in_bytes: Option<usize>,
        _guard_size_in_bytes: usize,
    ) -> Result<Box<dyn LinearMemory>, String> {
        if minimum > self.len {
            return Err(alloc::format!(
                "the module wants {minimum} bytes of memory and the guest arena is {}",
                self.len
            ));
        }
        Ok(Box::new(ArenaMemory {
            base: self.base as *mut u8,
            len: self.len,
        }))
    }
}

/// What one call into translated code produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exit {
    /// The guest pc to resume at.
    pub pc: u32,
    /// The exchange area's flag word: [`crate::host::FLAG_AFTER_STORE`],
    /// [`crate::host::FLAG_SLICE_ENDED`].
    pub flags: i32,
}

/// One compiled, instantiated translated module and the store it lives in.
///
/// Generic over the ops rather than boxing them, because a `Store`'s data has
/// to be `'static`: a host that has to reach a hart and a bus holds raw
/// pointers to them and sets them for the length of one call, and that struct
/// is `'static` while `&mut MachineHart<'_>` is not.
pub struct WasmtimeCore<H: HostOps + 'static> {
    store: Store<H>,
    run: TypedFunc<(i32, i64, i64, i64, i64, i64), i32>,
    module_bytes: usize,
    /// Host microseconds the engine spent compiling the translated module,
    /// and instantiating it. Separate because JD20's boot-cost line reports
    /// them separately: compilation is the number that scales with the image
    /// and instantiation is the number the spike measured at 0.1 ms.
    compile_us: u128,
    instantiate_us: u128,
}

impl<H: HostOps + 'static> WasmtimeCore<H> {
    /// Compile `wasm` and instantiate it against the arena at
    /// `(arena_base, arena_len)`.
    ///
    /// # Safety
    ///
    /// `arena_base` must point at `arena_len` readable, writable bytes that
    /// stay valid and do not move for as long as this `WasmtimeCore` is alive,
    /// and nothing else may hold a Rust reference into them while translated
    /// code is running. The emulator's guest arena is exactly that by
    /// construction — see [`ArenaMemory`].
    ///
    /// # Errors
    ///
    /// A module that does not compile, or does not have the imports this
    /// crate's translator emits.
    pub unsafe fn new(
        wasm: &[u8],
        ops: H,
        arena_base: *mut u8,
        arena_len: usize,
    ) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        // Explicit bounds checks: this store's memory is the bus's own `Vec`
        // and has no guard region after it. See the module docs — this is the
        // one place JD18's configuration is deliberately not used, and why.
        config.signals_based_traps(false);
        config.memory_reservation(0);
        config.memory_guard_size(0);
        config.memory_may_move(false);
        config.with_host_memory(Arc::new(ArenaMemoryCreator {
            base: arena_base as usize,
            len: arena_len,
        }));

        let engine = Engine::new(&config)?;
        let compiling = std::time::Instant::now();
        let module = Module::new(&engine, wasm)?;
        let compile_us = compiling.elapsed().as_micros();
        let mut store = Store::new(&engine, ops);

        let mmio_load = wasmtime::Func::wrap(
            &mut store,
            |mut caller: Caller<'_, H>, pc: i32, cycle: i64, address: i32, kind: i32| -> i64 {
                let out = caller.data_mut().mmio_load(
                    pc as u32,
                    cycle as u64,
                    address as u32,
                    kind as u32,
                );
                ((i64::from(out.status)) << 32) | i64::from(out.value)
            },
        );
        let mmio_store = wasmtime::Func::wrap(
            &mut store,
            |mut caller: Caller<'_, H>,
             pc: i32,
             cycle: i64,
             address: i32,
             kind: i32,
             value: i32|
             -> i32 {
                caller.data_mut().mmio_store(
                    pc as u32,
                    cycle as u64,
                    address as u32,
                    kind as u32,
                    value as u32,
                ) as i32
            },
        );
        let step_one =
            wasmtime::Func::wrap(&mut store, |mut caller: Caller<'_, H>, pc: i32| -> i32 {
                crate::host::escape_hatch(caller.data_mut(), pc as u32) as i32
            });

        // The memory has to be **defined by a module**, not created with
        // `Memory::new`: `wasmtime::Memory::new` builds its instance with
        // `OnDemandInstanceAllocator::default()`, which has no memory creator,
        // so a host memory would be silently ignored and the module would run
        // against a fresh zeroed allocation. Nothing errors; the guest's
        // memory simply is not the bus's. A one-line shim module whose only
        // content is the memory goes through the configured allocator, which
        // does consult the creator — and it is also the shape the browser
        // host has anyway, where the emulator instance owns the memory and the
        // translated module imports it.
        let pages = u32::try_from(arena_len / 65536).unwrap_or(u32::MAX);
        let instantiating = std::time::Instant::now();
        let shim = Module::new(&engine, &memory_shim(pages))?;
        let shim = Instance::new(&mut store, &shim, &[])?;
        let memory = shim
            .get_memory(&mut store, "memory")
            .expect("the shim module exports its memory");

        let instance = Instance::new(
            &mut store,
            &module,
            &[
                Extern::Func(mmio_load),
                Extern::Func(mmio_store),
                Extern::Func(step_one),
                Extern::Memory(memory),
            ],
        )?;
        let run = instance
            .get_typed_func::<(i32, i64, i64, i64, i64, i64), i32>(&mut store, ENTRY_FUNC)?;
        Ok(Self {
            store,
            run,
            module_bytes: wasm.len(),
            compile_us,
            instantiate_us: instantiating.elapsed().as_micros(),
        })
    }

    /// Host microseconds the engine spent compiling the translated module
    /// (JD20's boot-cost line).
    #[must_use]
    pub fn compile_us(&self) -> u128 {
        self.compile_us
    }

    /// Host microseconds spent instantiating it — the memory shim included,
    /// because that is what a host has to build to hand the module the
    /// arena.
    #[must_use]
    pub fn instantiate_us(&self) -> u128 {
        self.instantiate_us
    }

    /// The ops this core calls back into, so the caller can point them at the
    /// hart and the bus for the length of one entry.
    pub fn ops_mut(&mut self) -> &mut H {
        self.store.data_mut()
    }

    #[must_use]
    pub fn module_bytes(&self) -> usize {
        self.module_bytes
    }

    /// Enter translated code at block index `entry`.
    ///
    /// # Errors
    ///
    /// A wasm trap. The emitted module cannot trap on any path this
    /// translator emits, so a trap is a translator bug and is reported rather
    /// than absorbed.
    pub fn enter(
        &mut self,
        entry: u32,
        cycle: u64,
        instret: u64,
        end: u64,
        watch: (u64, u64),
    ) -> wasmtime::Result<Exit> {
        let pc = self.run.call(
            &mut self.store,
            (
                entry as i32,
                cycle as i64,
                instret as i64,
                end as i64,
                watch.0 as i64,
                watch.1 as i64,
            ),
        )?;
        let flags = {
            let x = &self.store.data_mut().exchange()[EXCHANGE_FLAGS as usize..];
            i32::from_le_bytes([x[0], x[1], x[2], x[3]])
        };
        Ok(Exit {
            pc: pc as u32,
            flags,
        })
    }
}

/// A module whose entire content is one exported memory of `pages` pages.
///
/// See [`WasmtimeCore::new`] for why this exists rather than
/// `wasmtime::Memory::new`.
fn memory_shim(pages: u32) -> alloc::vec::Vec<u8> {
    use wasm_encoder::{ExportKind, ExportSection, MemorySection, MemoryType, Module};
    let mut module = Module::new();
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: u64::from(pages),
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    module.section(&exports);
    module.finish()
}

/// The bytes a `Vec`-backed arena has to be a whole number of wasm pages for.
#[must_use]
pub fn whole_pages(arena_len: usize) -> usize {
    arena_len / 65536 * 65536
}
