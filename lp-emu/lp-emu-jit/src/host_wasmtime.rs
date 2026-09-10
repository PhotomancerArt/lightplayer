//! A wasmtime host for emitted modules — development, CI, and the native
//! `--jit` path (JD18).
//!
//! **Never the product host.** In the browser the engine is the browser's own,
//! and this exists so identity can be proven on the desk against the same
//! module bytes.
//!
//! # The memory, and the guard it comes with
//!
//! JD4 gives the guest arena to the bus, and the emitted module has to import
//! *that* memory rather than a copy — copying a 256 MiB arena in and out
//! around every slice would cost more than translation saves. So the memory is
//! supplied through [`wasmtime::MemoryCreator`], aliasing the bus's own
//! allocation. And `MemoryCreator`'s safety contract is explicit that a
//! host-supplied memory has to be followed by `guard_size_in_bytes` of
//! **unmapped** address space, because cranelift elides bounds checks on the
//! strength of it.
//!
//! P3 read that contract against a `Vec<u8>` arena and, rightly, refused the
//! guard: a `Vec` is followed by whatever the allocator put there, so
//! promising a guard would have turned a translator bug from a wasm trap into
//! a silent write into the host heap. The price was JD18's ~4.8x — explicit
//! bounds checks on every guest load and store.
//!
//! JD21 removes the conflict at the other end: the bus's arena
//! ([`lp_emu_core::arena::GuestArena`]) now maps its own reservation with a
//! real unmapped guard behind it, so the promise this host makes is one the
//! allocation actually keeps, and JD18's configuration goes on:
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
//! **Only when the arena says it is guarded.** [`WasmtimeCore::new`] takes the
//! guard from the arena, not from a constant, and a host without virtual
//! memory hands it `None` — for which this host goes back to explicit bounds
//! checks and [`ArenaMemoryCreator`] refuses any non-zero guard the engine
//! asks for. A build that promises a guard it does not have is the bug this
//! whole arrangement exists to prevent, and it is not expressible here.
//!
//! The browser never had the conflict at all: there the emulator *is* a wasm
//! module, the arena is bytes inside its own linear memory, the module imports
//! that memory, and the engine's own guard pages are already under it.

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

use lp_emu_core::arena::ArenaGuard;
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
/// When `guard` is `Some`, the bytes from `base + len` out to
/// `base + reservation + guard` are genuinely unmapped, which is what lets the
/// engine elide its bounds checks. When it is `None` there is no guard, and
/// [`ArenaMemoryCreator::new_memory`] has already refused to hand this memory
/// to an engine that wanted one.
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

// SAFETY: four clauses, and each is discharged here:
//
// - `byte_size` and `byte_capacity` report exactly the allocation the caller
//   promised — the arena's own length, no more;
// - `as_ptr` returns its base, which the caller has promised is stable for the
//   life of the store;
// - `grow_to` refuses anything larger, so the reported size never rises and
//   the base never has to move;
// - the guard-page clause is discharged by the *allocation*, checked in
//   `ArenaMemoryCreator::new_memory` against what the engine asks for. An
//   arena with no guard is only ever paired with a configuration that asks for
//   none.
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

/// Hands the engine the one arena it is allowed to have.
///
/// The guard check in [`Self::new_memory`] is the load-bearing part: the
/// engine asks for a reservation and a guard derived from the `Config` this
/// module sets, and the request is refused unless the arena's own mapping
/// covers it. That is what makes a mismatch between the configuration and the
/// allocation an instantiation error rather than a silent loss of memory
/// safety.
struct ArenaMemoryCreator {
    base: usize,
    len: usize,
    /// The arena's own report of what lies behind it, straight from
    /// `GuestArena::guard`.
    guard: Option<ArenaGuard>,
}

// SAFETY: every memory this creator hands out is the same, correctly sized
// view of one allocation the caller has promised is stable — see
// [`ArenaMemory`] — and `new_memory` refuses outright rather than return one
// whose guard the allocation does not actually provide.
unsafe impl MemoryCreator for ArenaMemoryCreator {
    fn new_memory(
        &self,
        _ty: wasmtime::MemoryType,
        minimum: usize,
        _maximum: Option<usize>,
        reserved_size_in_bytes: Option<usize>,
        guard_size_in_bytes: usize,
    ) -> Result<Box<dyn LinearMemory>, String> {
        if minimum > self.len {
            return Err(alloc::format!(
                "the module wants {minimum} bytes of memory and the guest arena is {}",
                self.len
            ));
        }
        // What the engine will treat as addressable-or-trapping: everything
        // from the base out to the end of the reservation, plus the guard it
        // has already told cranelift it may run off into.
        let wants = reserved_size_in_bytes
            .unwrap_or(self.len)
            .max(self.len)
            .checked_add(guard_size_in_bytes)
            .ok_or_else(|| String::from("the engine's reservation plus guard overflows"))?;
        // The arena's mapping is `reservation + guard` bytes long with only
        // its first `len` readable, so everything past `len` traps. Anything
        // the engine wants beyond that span is address space we do not own.
        let have = match self.guard {
            Some(g) => g.reservation.max(self.len).saturating_add(g.guard),
            None => self.len,
        };
        if wants > have {
            return Err(alloc::format!(
                "the engine wants {wants} bytes of reservation and guard behind the guest arena \
                 and it has {have}: a guard this host does not own would let an out-of-range \
                 guest access write the emulator's own memory instead of trapping"
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
    /// `guard` is the arena's own report of what lies behind it —
    /// `GuestArena::guard` — and decides which of the two bounds-check
    /// strategies this store gets. Pass what the arena says, never a
    /// constant: a guard named here that the allocation does not keep is
    /// exactly the unsound case, and the only defence left after that is
    /// [`ArenaMemoryCreator`]'s refusal.
    ///
    /// # Safety
    ///
    /// `arena_base` must point at `arena_len` readable, writable bytes that
    /// stay valid and do not move for as long as this `WasmtimeCore` is alive,
    /// and nothing else may hold a Rust reference into them while translated
    /// code is running. The emulator's guest arena is exactly that by
    /// construction — see [`ArenaMemory`].
    ///
    /// When `guard` is `Some`, the bytes from `arena_base + arena_len` out to
    /// `arena_base + reservation + guard` must additionally be **unmapped**,
    /// so that an access there raises a signal the engine turns into a wasm
    /// trap. `GuestArena` is the only thing in the tree that produces a
    /// `Some`, and it produces it only for a mapping it made itself.
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
        guard: Option<ArenaGuard>,
    ) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        match guard {
            // Guard-page bounds checks, the way a browser engine always does
            // them. Without these wasmtime emits an explicit check on EVERY
            // guest load and store, and the region runs 4.8x slower — a
            // property of the native host, not of the emitted module.
            // Measured: 308 -> 1481 M instr/s.
            Some(g) => {
                config.signals_based_traps(true);
                config.memory_reservation(g.reservation as u64);
                config.memory_guard_size(g.guard as u64);
                config.memory_may_move(false);
            }
            // No mapping behind this arena, so no guard may be promised: the
            // engine has to check every access itself. See the module docs.
            None => {
                config.signals_based_traps(false);
                config.memory_reservation(0);
                config.memory_guard_size(0);
                config.memory_may_move(false);
            }
        }
        config.with_host_memory(Arc::new(ArenaMemoryCreator {
            base: arena_base as usize,
            len: arena_len,
            guard,
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

/// The bytes of an arena that are a whole number of wasm pages.
#[must_use]
pub fn whole_pages(arena_len: usize) -> usize {
    arena_len / 65536 * 65536
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_core::arena::{GUARD, RESERVATION};

    const LEN: usize = 64 * 1024;

    fn creator(base: *mut u8, guard: Option<ArenaGuard>) -> ArenaMemoryCreator {
        ArenaMemoryCreator {
            base: base as usize,
            len: LEN,
            guard,
        }
    }

    fn ty() -> wasmtime::MemoryType {
        wasmtime::MemoryType::new(1, None)
    }

    /// The one thing that must never happen: an arena with nothing behind it
    /// handed to an engine that has already compiled its bounds checks away.
    #[test]
    fn an_unguarded_arena_refuses_an_engine_that_wants_a_guard() {
        let mut bytes = alloc::vec![0u8; LEN];
        let c = creator(bytes.as_mut_ptr(), None);
        let refused = c.new_memory(ty(), LEN, None, Some(RESERVATION), GUARD);
        assert!(refused.is_err(), "a guard we do not own must be refused");
        let ok = c.new_memory(ty(), LEN, None, Some(0), 0);
        assert!(ok.is_ok(), "and explicit bounds checks are still served");
    }

    /// The same check one step in: a mapping is only good for the span it
    /// actually reserved, so an engine asking to run off further is refused
    /// even though there *is* a guard.
    #[test]
    fn a_guarded_arena_refuses_more_than_it_mapped() {
        let mut bytes = alloc::vec![0u8; LEN];
        let guard = Some(ArenaGuard {
            reservation: RESERVATION,
            guard: GUARD,
        });
        let c = creator(bytes.as_mut_ptr(), guard);
        assert!(
            c.new_memory(ty(), LEN, None, Some(RESERVATION), GUARD).is_ok(),
            "exactly what it mapped is served"
        );
        assert!(
            c.new_memory(ty(), LEN, None, Some(RESERVATION * 2), GUARD)
                .is_err(),
            "a wider reservation than it mapped is refused"
        );
        assert!(
            c.new_memory(ty(), LEN, None, Some(RESERVATION), GUARD * 2)
                .is_err(),
            "and so is a wider guard"
        );
    }

    /// A module wanting more memory than the arena has is a build error, not
    /// a silently truncated memory.
    #[test]
    fn a_module_wanting_more_than_the_arena_is_refused() {
        let mut bytes = alloc::vec![0u8; LEN];
        let c = creator(bytes.as_mut_ptr(), None);
        assert!(c.new_memory(ty(), LEN + 1, None, Some(0), 0).is_err());
    }
}
