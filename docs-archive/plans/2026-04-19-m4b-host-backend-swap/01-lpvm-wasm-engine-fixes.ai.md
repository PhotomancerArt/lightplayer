# Phase 1 — `lpvm-wasm` engine fixes

`[sub-agent: yes, parallel: -]`

## Scope of phase

Two prerequisite fixes inside `lp-shader/lpvm-wasm/` so the wasmtime
runtime is safe and complete before `lp-engine` swaps over to it in
phase 2.

1. **Memory pre-reservation.** Replace
   `WasmtimeLpvmMemory::alloc`'s bump+grow loop with bump-only over a
   pre-grown budget set on `WasmOptions::host_memory_pages`. Default
   budget = 1024 wasm pages = 64 MiB. The budget is grown once at
   `WasmLpvmEngine::new`; `Memory::grow` is never called after init,
   so cached `LpvmBuffer.native` pointers stay valid for the engine's
   lifetime.

2. **`compile_with_config` parity.** `WasmLpvmEngine` currently
   inherits `LpvmEngine`'s default `compile_with_config` which
   silently drops the per-call `lpir::CompilerConfig`. Add an
   override that mirrors `lpvm-native`'s impl: clone
   `self.compile_options`, overwrite its `config` field with the
   per-call value, then compile.

Both changes ship with one new regression test in
`lpvm-wasm/tests/compile_with_config.rs` asserting that compiling
the same LPIR with two different `q32.mul_mode` values produces
distinct WASM byte outputs.

**Out of scope:**

- `BrowserLpvmEngine` (`rt_browser/engine.rs`). Don't add the
  `compile_with_config` override there — its consumers don't yet set
  non-default `CompilerConfig`. Leave it on the trait default.
- `lp-engine`, `lp-server`, firmware crates, any consumer wiring.
  All consumer churn is phase 2 and phase 3.
- Wasmtime perf knobs beyond the existing `consume_fuel(true)`. Add
  the deferred-knobs comment block (see "Implementation details" §3)
  but no behaviour change.

## Code organization reminders

- One concept per file. The two `lpvm-wasm` files touched here are
  already small and single-purpose; keep them that way.
- New test goes in `lp-shader/lpvm-wasm/tests/compile_with_config.rs`
  (separate from any existing in-tree tests; this is a regression
  test, not part of the engine's own unit tests).
- Mark any genuinely temporary scaffolding with a `TODO` comment.
  None expected here.

## Sub-agent reminders

- Do **not** commit. Phase 4 commits the whole plan as one unit.
- Do **not** expand scope. If you find unrelated bugs in
  `rt_wasmtime/`, file a TODO in the phase report and move on.
- Do **not** suppress warnings or add `#[allow(...)]` to make the
  build green. Fix the actual problem.
- Do **not** disable, `#[ignore]`, or weaken existing tests.
- If anything is ambiguous or blocked, **stop and report** — do not
  improvise.
- Report back: files changed, test command output (full), and any
  deviations from this phase file.

## Implementation details

### File 1 — `lp-shader/lpvm-wasm/src/options.rs`

Add a `host_memory_pages` field on `WasmOptions`. Default = 1024
wasm pages = 64 MiB. Clarify the unit in the doc comment.

Current state of the file (full):

```rust
//! WASM compilation options.

use lpir::{CompilerConfig, FloatMode};

/// Options for LPIR-to-WASM compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmOptions {
    /// Numeric format: Q32 (fixed-point i32) or Float (f32).
    pub float_mode: FloatMode,

    /// Middle-end LPIR pass settings (inline, etc.).
    pub config: CompilerConfig,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
            config: CompilerConfig::default(),
        }
    }
}
```

After:

```rust
//! WASM compilation options.

use lpir::{CompilerConfig, FloatMode};

/// Options for LPIR-to-WASM compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmOptions {
    /// Numeric format: Q32 (fixed-point i32) or Float (f32).
    pub float_mode: FloatMode,

    /// Middle-end LPIR pass settings (inline, etc.).
    pub config: CompilerConfig,

    /// Wasmtime host runtime: number of 64 KiB wasm pages to pre-grow the
    /// linear memory to at engine construction.
    ///
    /// The wasmtime backend (`rt_wasmtime`) caches raw host pointers in
    /// `LpvmBuffer::native`. If `Memory::grow` is called after the first
    /// allocation, wasmtime is free to relocate the linear memory and any
    /// previously-cached `native` pointer becomes a use-after-free hazard.
    /// To avoid this, the wasmtime engine pre-grows the linear memory once
    /// at construction to `host_memory_pages` and never grows it again;
    /// `WasmtimeLpvmMemory::alloc` returns `OutOfMemory` past the cap.
    ///
    /// Default = 1024 pages = 64 MiB. Ignored on the wasm32 (`rt_browser`)
    /// runtime, which uses `WebAssembly.Memory`'s grow on demand.
    pub host_memory_pages: u32,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
            config: CompilerConfig::default(),
            host_memory_pages: 1024,
        }
    }
}
```

### File 2 — `lp-shader/lpvm-wasm/src/rt_wasmtime/shared_runtime.rs`

Two structural changes:

1. `WasmLpvmSharedRuntime::new` takes the `host_memory_pages` budget
   and pre-grows the wasmtime memory to that page count after
   construction. Pre-grow uses `Memory::grow` once; any failure is
   surfaced as `WasmError::runtime`.
2. `WasmtimeLpvmMemory::alloc` removes the inner `while end >
   cur_len { ... grow ... }` loop. If the bump cursor would exceed
   the current memory size (which equals the pre-grown budget),
   return `AllocError::OutOfMemory`.

Updated module-level doc comment (replace the existing one):

```rust
//! One wasmtime [`Store`], one [`Memory`], bump sub-region for [`LpvmMemory`].
//!
//! **Bump over pre-grown memory.** [`WasmtimeLpvmMemory`] only advances a cursor;
//! [`Memory::grow`] is never called after the engine is constructed. The host runtime
//! pre-grows the linear memory once in [`WasmLpvmSharedRuntime::new`] to
//! [`WasmOptions::host_memory_pages`] (default 64 MiB). Allocations beyond that cap
//! return [`AllocError::OutOfMemory`].
//!
//! This is the safe path for production hosts: cached host pointers in
//! [`lpvm::LpvmBuffer::native`] stay valid because the underlying linear memory is
//! never relocated. The bump-only allocator does not reuse memory; if a real workload
//! exhausts the cap, raise [`WasmOptions::host_memory_pages`] or revisit the allocator.
```

Updated `WasmLpvmSharedRuntime::new` (changes the signature to take
`host_memory_pages: u32` and adds the pre-grow):

```rust
impl WasmLpvmSharedRuntime {
    pub(crate) fn new(engine: &Engine, host_memory_pages: u32) -> Result<Arc<Self>, WasmError> {
        let spec = EnvMemorySpec::engine_initial_for_host();
        let mem_ty = MemoryType::new(spec.initial_pages, spec.max_pages);
        let mut store = Store::new(engine, ());
        let memory = Memory::new(&mut store, mem_ty)
            .map_err(|e| WasmError::runtime(format!("Memory::new: {e}")))?;

        // Pre-grow once to the host budget so cached native pointers in
        // LpvmBuffer never observe a Memory::grow relocation. See module docs.
        let current_pages = memory.size(&store) as u32;
        if host_memory_pages > current_pages {
            let delta = u64::from(host_memory_pages - current_pages);
            memory
                .grow(&mut store, delta)
                .map_err(|e| WasmError::runtime(format!(
                    "pre-grow to {host_memory_pages} pages failed: {e}"
                )))?;
        }

        let guest_reserve = usize::try_from(EnvMemorySpec::guest_reserve_bytes())
            .map_err(|_| WasmError::runtime("guest reserve size"))?;
        Ok(Arc::new(Self {
            inner: Mutex::new(WasmLpvmSharedRuntimeInner {
                store,
                memory,
                bump_cursor: guest_reserve,
            }),
        }))
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, WasmLpvmSharedRuntimeInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
```

Note: `Memory::size(&store)` may return `u64` in this wasmtime
version. Cast appropriately; if the cast cannot represent the
budget, surface `WasmError::runtime`. Inspect
`Cargo.lock` / `wasmtime` 42 docs to confirm the exact type before
writing the cast.

Updated `WasmtimeLpvmMemory::alloc` (loop removed, OOM past the
pre-grown cap):

```rust
impl LpvmMemory for WasmtimeLpvmMemory {
    fn alloc(&self, size: usize, align: usize) -> Result<LpvmBuffer, AllocError> {
        if size == 0 {
            return Err(AllocError::InvalidSize);
        }
        if !align.is_power_of_two() {
            return Err(AllocError::InvalidSize);
        }
        let mut guard = self.runtime.lock();
        let mem = guard.memory;
        let aligned = round_up(guard.bump_cursor, align);
        let end = aligned.checked_add(size).ok_or(AllocError::InvalidSize)?;

        // Memory was pre-grown once at engine init; never grow again or cached
        // LpvmBuffer.native pointers go stale. Past the cap is OOM.
        if end > mem.data_size(&guard.store) {
            return Err(AllocError::OutOfMemory);
        }

        guard.bump_cursor = end;
        let native_base = mem.data_mut(&mut guard.store).as_mut_ptr();
        let native = unsafe { native_base.add(aligned) };
        Ok(LpvmBuffer::new(native, aligned as u64, size, align))
    }

    fn free(&self, _buffer: LpvmBuffer) {
        // Bump semantics: memory is not reused.
    }

    fn realloc(&self, _buffer: LpvmBuffer, _new_size: usize) -> Result<LpvmBuffer, AllocError> {
        // Not supported for bump allocator; use alloc + copy + free old.
        Err(AllocError::InvalidPointer)
    }
}
```

The `EnvMemorySpec::WASM_PAGE_SIZE` import / `page` local variable
in the old `alloc` is no longer needed; remove it. Same for the
unused `cur_len` rebind.

### File 3 — `lp-shader/lpvm-wasm/src/rt_wasmtime/engine.rs`

Three edits in `WasmLpvmEngine`:

1. Pass `compile_options.host_memory_pages` into
   `WasmLpvmSharedRuntime::new`.
2. Add a short comment block above `Engine::new` listing the
   wasmtime knobs deliberately deferred (epoch interruption, parallel
   compilation, custom memory limits). Stops the next reader from
   re-investigating the same ground.
3. Add a `compile_with_config` override on the `impl LpvmEngine for
   WasmLpvmEngine` block. Mirror `lpvm-native`'s pattern: clone the
   engine's `WasmOptions`, overwrite `opts.config` with the per-call
   value, compile through `compile_lpir`, and store the resulting
   `WasmOptions` back into the produced `WasmLpvmModule.opts` so
   downstream consumers see what was actually used.

Updated `new` (changes within the body):

```rust
impl WasmLpvmEngine {
    /// New engine (builtins are linked natively from `lps-builtins`).
    pub fn new(compile_options: WasmOptions) -> Result<Self, WasmError> {
        ensure_builtins_referenced();
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        // Deferred wasmtime knobs (revisit when perf or sandboxing demands it):
        //   - config.epoch_interruption(true) for cooperative cancellation
        //   - config.parallel_compilation(true) for multi-threaded compile
        //   - config.memory_reservation(...) / static_memory_maximum_size for
        //     a bigger pre-mapped guard region than the default (~4 GiB)
        // None of these change correctness; they are tuning knobs.
        let engine = Engine::new(&config)
            .map_err(|e| WasmError::runtime(format!("failed to create WASM engine: {e}")))?;
        let runtime = WasmLpvmSharedRuntime::new(&engine, compile_options.host_memory_pages)?;
        let memory = WasmtimeLpvmMemory::new(Arc::clone(&runtime));
        Ok(Self {
            engine,
            compile_options,
            runtime,
            memory,
        })
    }
}
```

Add the override at the end of the `impl LpvmEngine for
WasmLpvmEngine` block (after `compile`, before `memory`). Existing
`compile` is unchanged.

```rust
    fn compile_with_config(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        config: &lpir::CompilerConfig,
    ) -> Result<Self::Module, Self::Error> {
        let mut opts = self.compile_options.clone();
        opts.config = config.clone();
        let artifact = compile_lpir(ir, meta, &opts)?;
        let bytes = artifact.wasm_module().bytes.clone();
        WasmLpvmModule::validate_shader(&self.engine, &bytes)?;
        WasmLpvmModule::validate_memory_import(&self.engine, &bytes)?;
        let exports: HashMap<_, _> = artifact
            .wasm_module()
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect();
        Ok(WasmLpvmModule {
            engine: self.engine.clone(),
            runtime: Arc::clone(&self.runtime),
            wasm_bytes: bytes,
            signatures: artifact.signatures().clone(),
            exports,
            shadow_stack_base: artifact.wasm_module().shadow_stack_base,
            opts,
            lpir: ir.clone(),
        })
    }
```

The body is intentionally a copy of `compile` with a different
`opts` source. Don't refactor them into a shared helper — the trait
shape may diverge (e.g. browser engine adds its own override later)
and the duplication is small. If the duplication grows past this
phase, factor it then.

### File 4 (NEW) — `lp-shader/lpvm-wasm/tests/compile_with_config.rs`

Regression test mirroring M4a's `lpvm-native` Phase-0 test
(`q32.mul_mode` flowing through `compile_with_config`). Uses
`lps-frontend` to compile a tiny GLSL snippet to LPIR, then runs it
through the wasm engine twice with different `Q32MulMode` settings,
and asserts the two WASM byte outputs differ.

Look at any existing `q32`-flavoured test in the workspace before
writing this. The exact `CompilerConfig` field name and the
`Q32MulMode` enum live in `lpir::CompilerConfig`. The test should
**not** depend on a specific GLSL → IR ordering — the assertion is
just "outputs differ", so any tiny snippet that exercises a
multiply works.

Skeleton (fill in correct types; verify imports compile):

```rust
//! Regression: `WasmLpvmEngine::compile_with_config` actually flows
//! `lpir::CompilerConfig` into emission (not silently dropped).

use lpir::CompilerConfig;
use lpvm::LpvmEngine;
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

#[test]
fn compile_with_config_q32_mul_mode_flows_to_emission() {
    // Smallest snippet that emits a multiply on the q32 path.
    let glsl = r#"
        void main() {
            float x = 0.5;
            float y = x * x;
        }
    "#;

    // Front-end → LPIR + module signature.
    let (ir, meta) = lps_frontend::compile_glsl_to_lpir(glsl)
        .expect("front-end compile");

    let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine new");

    let mut cfg_a = CompilerConfig::default();
    let mut cfg_b = CompilerConfig::default();
    // Set the two configs to different `q32.mul_mode` values. Look up the
    // exact enum variants in lpir::CompilerConfig before writing this.
    // e.g. cfg_a.q32.mul_mode = Q32MulMode::Truncate;
    //      cfg_b.q32.mul_mode = Q32MulMode::Round;

    let mod_a = engine.compile_with_config(&ir, &meta, &cfg_a).expect("compile a");
    let mod_b = engine.compile_with_config(&ir, &meta, &cfg_b).expect("compile b");

    assert_ne!(
        mod_a.wasm_bytes, mod_b.wasm_bytes,
        "compile_with_config must thread CompilerConfig into emission; \
         identical bytes mean per-call config was dropped"
    );
}
```

Notes:

- `lps_frontend` is already in `lpvm-wasm`'s dev-dependencies — if
  the actual function name differs from `compile_glsl_to_lpir`,
  grep `lp-shader/lps-frontend/src/lib.rs` for the public entry
  point and use that.
- `WasmLpvmModule.wasm_bytes` is `pub(crate)`. Either expose a
  `wasm_bytes(&self) -> &[u8]` accessor on `WasmLpvmModule` (small,
  additive — preferred), or move the test inside
  `src/rt_wasmtime/engine.rs` as a `#[cfg(test)] mod tests`. Prefer
  the accessor + integration test for parity with how M4a wrote its
  `lpvm-native` regression.
- If `lpir::CompilerConfig`'s q32 mode enum is named differently
  (e.g. `Q32Options.mul_mode`), use whatever's there. The test only
  cares that two values produce different bytes.

If after a reasonable look you cannot find an `lpir::CompilerConfig`
field whose change actually affects WASM bytes for a trivial
shader, **stop and report** before inventing one.

## Validate

```bash
cargo test -p lpvm-wasm
cargo check -p lpvm-wasm --target wasm32-unknown-unknown
```

The wasm32 check is to confirm `WasmOptions::host_memory_pages`
doesn't accidentally break compilation on the browser path
(which doesn't read the field). The browser engine still uses
`WasmOptions::default()` from its caller; the new field has a
default value so existing call sites work unchanged.
