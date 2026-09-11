//! The browser host for emitted modules — **the product host** (JD11, JD12,
//! JD13).
//!
//! Where `host_wasmtime` exists so identity can be proven on the desk, this is
//! where the milestone's number comes from: the emulator is itself a
//! `wasm32-wasip1` module in a dedicated Worker, a translated module is
//! another module in the same engine, and the two are wired together with no
//! JS frame on any hot path.
//!
//! # The three halves of the seam
//!
//! 1. **Three exports of the emulator's own module** — [`jit_mmio_load`],
//!    [`jit_mmio_store`], [`jit_step_one`]. The JS host hands them straight to
//!    a translated module as its `emu.mmio_load` / `emu.mmio_store` /
//!    `emu.step_one` imports, together with `emu.memory`, which is the
//!    emulator's own linear memory. Every call a translated module makes is
//!    therefore wasm→wasm; P2 measured that at 1.60 ns against 6.05 ns through
//!    a JS shim.
//! 2. **Two imports from JS**, namespace `emu_host`: [`jit_compile`] and
//!    [`jit_release`]. They are called **once per translation event** (JD5:
//!    boot, then each `fence.i`) and never on a hot path. `jit_compile` does
//!    `new WebAssembly.Module` **synchronously** — legal because guest time is
//!    the scheduler's and the wall clock never enters the machine (PD5 / ADR
//!    2026-09-06), so however long a compile takes it cannot reach a
//!    transcript. That is also why the emulator has to stay in a **dedicated
//!    Worker**: a synchronous multi-hundred-millisecond compile on the main
//!    thread would be a frozen UI, and JD13 makes the Worker a correctness
//!    requirement rather than a convenience.
//! 3. **Entry by `call_indirect`.** `jit_compile` returns a slot in the
//!    emulator module's `__indirect_function_table`, and on wasm32 a function
//!    pointer *is* a table index — so the slot is turned into an
//!    `extern "C" fn` of the exit protocol's shape and called. No JS at all on
//!    the entry path. [`jit_table_probe`] and [`jit_table_selftest`] exist so
//!    that encoding is *checked in the engine that is about to rely on it*
//!    rather than assumed; `jit-host.js` runs the round trip at wiring time
//!    and refuses to proceed if it disagrees.
//!
//! # Why the live-machine pointer is sound
//!
//! The three exports have no way to reach a hart and a bus of their own: they
//! are plain module exports, called from another module, with no argument that
//! names the machine. So [`BrowserCore::enter`] parks a pointer to its `HostOps`
//! in [`CURRENT`] for exactly the length of one stay and clears it on the way
//! out — the same window, and the same lifetime argument, as the two raw
//! pointers `C6Ops` already parks for the native host.
//!
//! Three things make that sound rather than merely conventional:
//!
//! - **The `wasip1` build is single-threaded.** There is no second thread that
//!   could observe a half-set pointer, and no `Sync` claim is made.
//! - **The window is exactly one call.** Translated code can only run inside
//!   `enter`; the core lifts the hart and the bus out for the stay exactly as
//!   the native `C6Ops` does, and nothing else holds a Rust reference into
//!   them while it runs.
//! - **A call outside the window panics instead of reading anything.**
//!   [`current`] refuses a `None`, so a seam bug is a loud abort at the first
//!   call rather than a silent read of whatever the last stay left behind.
//!   That is the "a null deref is a trap, never a silent read" rule, made
//!   explicit because on wasm a null read is *not* automatically a trap: guest
//!   address 0 is ordinary linear memory.
//!
//! # What a trap does here, and why `enter` still returns a `Result`
//!
//! Natively a wasm trap comes back as an `Err` and the run continues
//! interpreted. In the browser it cannot: a trap in a module called through
//! `call_indirect` unwinds nothing and kills the whole instance. `enter`
//! therefore only ever returns `Err` for a host-side refusal, and a translator
//! bug shows up as the Worker's `error` event — which the rig reports rather
//! than hangs on. The shape is kept identical to the native host's so the
//! machine crate has one code path, not two.

extern crate alloc;
// The crate is `no_std`; this module is not. `wasm32-wasip1` has `std`, and
// JD20's boot-cost line needs a clock the emitting half of the line is already
// timed with.
extern crate std;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;

use lp_emu_core::arena::ArenaGuard;

use crate::host::{EXCHANGE_FLAGS, Exit, HostOps};

// The JS side of the seam. TWO imports, both called once per translation
// event, neither on any hot path.
//
// `jit_compile(ptr, len, base, timings) -> i32` compiles the `len` bytes of
// module at `ptr` in the emulator's own memory, instantiates it against the
// emulator instance's `memory` and its three exports, grows
// `__indirect_function_table` by one and writes the new instance's `run`
// export into the new slot — as `table.grow(1)` and a SEPARATE
// `table.set(idx, run)`, never the fused `grow(delta, ref)` form, which JSC
// accepts and then traps on at `call_indirect` (P2, S4). It returns the slot,
// or a negative `compile_error` code.
//
// `base` is the guest arena's byte offset inside the emulator's linear memory
// — the same number every folded `memarg` in the module is built on. The JS
// host does not need it to wire anything; it carries it so a boot line and an
// uploaded result can say what the module was emitted against.
//
// `timings` points at two `f64`s the host fills in with the milliseconds it
// spent compiling and instantiating. THIS IS THE ONE PLACE THIS FILE DEVIATES
// FROM P6's BRIEF, which wrote the import as `jit_compile(ptr, len, base)`:
// JD20's boot-cost line reports compile and instantiate separately, and in the
// browser that split exists only on the JS side of the call. An out-pointer
// keeps the seam at two imports, which is the constraint the brief was
// protecting.
//
// `jit_release(idx)` nulls the slot so JD5's second event can replace the
// module.
#[link(wasm_import_module = "emu_host")]
unsafe extern "C" {
    fn jit_compile(ptr: u32, len: u32, base: u32, timings: u32) -> i32;
    fn jit_release(idx: i32);
}

/// What a negative [`jit_compile`] return means. The JS host and this table
/// are the only two places these numbers appear.
pub mod compile_error {
    /// `new WebAssembly.Module` threw — the bytes, or the engine's own limits.
    pub const COMPILE: i32 = -1;
    /// `new WebAssembly.Instance` threw — the import object did not satisfy
    /// the module's imports.
    pub const INSTANTIATE: i32 = -2;
    /// The module has no `run` export of the exit protocol's shape.
    pub const NO_ENTRY: i32 = -3;
    /// `table.grow` or `table.set` threw, or the engine has no exported
    /// `__indirect_function_table` (the build is missing `--export-table`).
    pub const TABLE: i32 = -4;
    /// The JS host was never wired to this instance.
    pub const NO_HOST: i32 = -5;
}

fn compile_error_name(code: i32) -> &'static str {
    match code {
        compile_error::COMPILE => "the engine refused to compile the module",
        compile_error::INSTANTIATE => "the engine refused to instantiate the module",
        compile_error::NO_ENTRY => "the module has no `run` export",
        compile_error::TABLE => "the indirect function table could not be grown or written",
        compile_error::NO_HOST => "no JS host is wired to this instance",
        _ => "an error the JS host did not name",
    }
}

/// The exit protocol's entry point, as this target's ABI sees it.
///
/// `run(entry_block, cycle, instret, end, watch_lo, watch_hi) -> pc`. See
/// [`crate::host`].
type RunFn = extern "C" fn(i32, i64, i64, i64, i64, i64) -> i32;

/// The machine translated code is currently running against, or `None`.
///
/// See the module docs for why a `static mut` is the right shape here and what
/// makes it sound. Not `pub`: the only ways to set it are entering a stay and
/// leaving one.
static mut CURRENT: Option<*mut (dyn HostOps + 'static)> = None;

/// The live machine, or a panic naming the bug.
///
/// # Panics
///
/// If translated code called its host outside a stay, which is a seam bug and
/// must not be answered with a plausible-looking zero.
fn current() -> *mut (dyn HostOps + 'static) {
    // SAFETY: single-threaded `wasip1`, and the read is of a `Copy` value
    // through a raw pointer rather than a reference to a `static mut`.
    let held = unsafe { *(&raw const CURRENT) };
    match held {
        Some(ops) => ops,
        None => panic!("translated code called its host outside a stay"),
    }
}

/// `emu.mmio_load`: an access on a page the permission table says is not plain
/// RAM. Returns `(status << 32) | value`, exactly as the native host's wrapper
/// does.
#[unsafe(no_mangle)]
pub extern "C" fn jit_mmio_load(pc: i32, cycle: i64, address: i32, kind: i32) -> i64 {
    // SAFETY: `current` has refused a `None`, and the pointer it hands back is
    // the `HostOps` the stay we are inside parked. No other reference into it
    // exists while translated code runs.
    let ops = unsafe { &mut *current() };
    let out = ops.mmio_load(pc as u32, cycle as u64, address as u32, kind as u32);
    (i64::from(out.status) << 32) | i64::from(out.value)
}

/// `emu.mmio_store`: the same, storing. Returns the status.
#[unsafe(no_mangle)]
pub extern "C" fn jit_mmio_store(pc: i32, cycle: i64, address: i32, kind: i32, value: i32) -> i32 {
    // SAFETY: as [`jit_mmio_load`].
    let ops = unsafe { &mut *current() };
    ops.mmio_store(pc as u32, cycle as u64, address as u32, kind as u32, value as u32) as i32
}

/// `emu.step_one`: the escape hatch (JD10). Runs exactly one guest instruction
/// the way the interpreter would and returns the pc to continue at; everything
/// else goes through the exchange area.
#[unsafe(no_mangle)]
pub extern "C" fn jit_step_one(pc: i32) -> i32 {
    // SAFETY: as [`jit_mmio_load`].
    let ops = unsafe { &mut *current() };
    crate::host::escape_hatch(ops, pc as u32) as i32
}

/// The constant [`jit_table_probe`] folds in, so a wrong slot cannot
/// accidentally produce the right answer.
const PROBE_XOR: i32 = 0x5a5a_5a5a_u32 as i32;

/// A known export for the table round trip. Does nothing else and is called by
/// nothing else.
#[unsafe(no_mangle)]
pub extern "C" fn jit_table_probe(x: i32) -> i32 {
    x ^ PROBE_XOR
}

/// Call table slot `idx` as `(i32) -> i32` and return what it produced.
///
/// This is the test the brief asks for: `jit-host.js` grows
/// `__indirect_function_table`, writes [`jit_table_probe`] — an export of
/// *this* module, reached as a JS `funcref` — into the new slot, and calls
/// this with that slot. If "a function pointer is a table index" is true in
/// this engine, the answer is `x ^ 0x5a5a5a5a`; if it is not, the whole entry
/// mechanism is wrong and the host says so before a 64 MB module is compiled
/// on the strength of it. It is also the P2 fused-`grow` bug's own detector:
/// that bug produced a slot whose `.get()` looked right and whose
/// `call_indirect` trapped, and this is a `call_indirect`.
#[unsafe(no_mangle)]
pub extern "C" fn jit_table_selftest(idx: i32, x: i32) -> i32 {
    // SAFETY: on wasm32 a function pointer is an index into
    // `__indirect_function_table`; the caller has just written a `(i32) -> i32`
    // function into `idx`. A mismatched signature or an empty slot traps,
    // which is the answer this function exists to produce.
    let f: extern "C" fn(i32) -> i32 = unsafe { core::mem::transmute(idx as usize) };
    f(x)
}

/// One compiled, instantiated translated module, entered through the shared
/// function table.
///
/// The same surface as [`crate::host_wasmtime::WasmtimeCore`], deliberately:
/// the machine crate picks one at compile time by target and has one code
/// path.
pub struct BrowserCore<H: HostOps + 'static> {
    /// Boxed so its address is stable for the life of the core — [`CURRENT`]
    /// holds a pointer to it across every stay.
    ops: Box<H>,
    /// The `__indirect_function_table` slot the JS host put `run` in.
    slot: i32,
    run: RunFn,
    module_bytes: usize,
    compile_us: u128,
    instantiate_us: u128,
}

impl<H: HostOps + 'static> BrowserCore<H> {
    /// Hand `wasm` to the JS host to compile, instantiate and install, and
    /// take back the table slot to enter it through.
    ///
    /// `arena_base` and `arena_len` describe the guest arena **inside the
    /// emulator's own linear memory**; unlike the native host, nothing is
    /// aliased and no memory is created — the module imports the memory this
    /// code is already running in. `guard` is accepted and ignored for
    /// signature parity: the engine's own guard pages are already under every
    /// access, which is the conflict `host_wasmtime`'s docs say the browser
    /// never had.
    ///
    /// # Safety
    ///
    /// `arena_base` must point at `arena_len` bytes inside this module's
    /// linear memory that stay valid and do not move for as long as this core
    /// is alive, and nothing else may hold a Rust reference into them while
    /// translated code is running. The emulator's guest arena is exactly that
    /// by construction.
    ///
    /// # Errors
    ///
    /// Anything the JS host refused: see [`compile_error`].
    pub unsafe fn new(
        wasm: &[u8],
        ops: H,
        arena_base: *mut u8,
        arena_len: usize,
        _guard: Option<ArenaGuard>,
    ) -> Result<Self, String> {
        let memory_bytes = core::arch::wasm32::memory_size(0) * 65536;
        let end = (arena_base as usize).saturating_add(arena_len);
        if end > memory_bytes {
            return Err(format!(
                "the guest arena ends at {end} and the emulator's linear memory is {memory_bytes} \
                 bytes: a translated module importing it could not reach the arena"
            ));
        }

        // Two `f64`s of milliseconds, filled in by the JS host. Zeroed first so
        // a host that writes neither reports zeros rather than stack noise.
        let mut timings = [0f64; 2];
        let started = std::time::Instant::now();
        // SAFETY: the pointers are into this module's own memory and stay
        // valid for the call; `jit_compile` is synchronous and copies what it
        // needs before it returns.
        let slot = unsafe {
            jit_compile(
                wasm.as_ptr() as u32,
                wasm.len() as u32,
                arena_base as u32,
                timings.as_mut_ptr() as u32,
            )
        };
        let total_us = started.elapsed().as_micros();
        if slot < 0 {
            return Err(format!(
                "the JS host refused the translated module: {} ({slot})",
                compile_error_name(slot)
            ));
        }

        let ms_to_us = |ms: f64| {
            if ms.is_finite() && ms > 0.0 {
                (ms * 1000.0) as u128
            } else {
                0
            }
        };
        let (mut compile_us, instantiate_us) = (ms_to_us(timings[0]), ms_to_us(timings[1]));
        // A host that reported nothing still gets an honest boot line: the
        // whole call was compilation as far as anything here can tell.
        if compile_us == 0 && instantiate_us == 0 {
            compile_us = total_us;
        }

        // SAFETY: on wasm32 a function pointer is an index into
        // `__indirect_function_table`, and `slot` is the index the host just
        // wrote the module's `run` export into. `jit-host.js` has already
        // proved that encoding in this engine through `jit_table_selftest`
        // before any of this ran, and the signature is the exit protocol's,
        // which `jit_compile` checked the export against.
        let run: RunFn = unsafe { core::mem::transmute::<usize, RunFn>(slot as usize) };

        Ok(Self {
            ops: Box::new(ops),
            slot,
            run,
            module_bytes: wasm.len(),
            compile_us,
            instantiate_us,
        })
    }

    /// Host microseconds the engine spent compiling the translated module
    /// (JD20's boot-cost line).
    #[must_use]
    pub fn compile_us(&self) -> u128 {
        self.compile_us
    }

    /// Host microseconds spent instantiating it.
    #[must_use]
    pub fn instantiate_us(&self) -> u128 {
        self.instantiate_us
    }

    /// The ops translated code calls back into, so the caller can point them
    /// at the hart and the bus for the length of one entry.
    pub fn ops_mut(&mut self) -> &mut H {
        &mut self.ops
    }

    #[must_use]
    pub fn module_bytes(&self) -> usize {
        self.module_bytes
    }

    /// Enter translated code at block index `entry`.
    ///
    /// # Errors
    ///
    /// Nothing this host can detect: see the module docs on what a trap does
    /// in a browser engine. The `Result` is here so the machine crate has one
    /// shape for both hosts.
    pub fn enter(
        &mut self,
        entry: u32,
        cycle: u64,
        instret: u64,
        end: u64,
        watch: (u64, u64),
    ) -> Result<Exit, String> {
        let ops: *mut (dyn HostOps + 'static) = &mut *self.ops;
        // SAFETY: single-threaded, and the window is exactly the call below —
        // see the module docs. A stay cannot nest: translated code reaches the
        // host only through the three exports, and none of them re-enters.
        unsafe { *(&raw mut CURRENT) = Some(ops) };
        let pc = (self.run)(
            entry as i32,
            cycle as i64,
            instret as i64,
            end as i64,
            watch.0 as i64,
            watch.1 as i64,
        );
        // SAFETY: as above. Cleared on the way out so a call from outside a
        // stay panics rather than reading a machine that is no longer running.
        unsafe { *(&raw mut CURRENT) = None };

        let flags = {
            let x = &self.ops.exchange()[EXCHANGE_FLAGS as usize..];
            i32::from_le_bytes([x[0], x[1], x[2], x[3]])
        };
        Ok(Exit {
            pc: pc as u32,
            flags,
        })
    }
}

impl<H: HostOps + 'static> Drop for BrowserCore<H> {
    fn drop(&mut self) {
        // JD5's second event replaces the module; the slot has to come back or
        // the table grows by one entry per `fence.i` for the life of the run.
        // SAFETY: `slot` is a slot this core was handed and has not released.
        unsafe { jit_release(self.slot) };
    }
}
