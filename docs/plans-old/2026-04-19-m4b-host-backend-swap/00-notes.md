# M4b — Host Backend Swap (Cranelift → Wasmtime)

Roadmap milestone:
[`docs/roadmaps/2026-04-16-lp-shader-textures/m4b-host-backend-swap.md`](../../roadmaps/2026-04-16-lp-shader-textures/m4b-host-backend-swap.md)

Predecessor (now done):
[`docs/plans-old/2026-04-19-m4a-pixel-loop-migration.md`](../../plans-old/2026-04-19-m4a-pixel-loop-migration.md)

## Scope of work

Replace the Cranelift-JIT backend that powers `lp-engine`'s host
graphics (`gfx/cranelift.rs` → `LpsEngine<CraneliftEngine>`) with the
Wasmtime-backed `WasmLpvmEngine` from `lpvm-wasm`. After M4a, the
swap is mechanical: change the `LpvmEngine` impl held inside the
`LpGraphics` wrapper, rename the public type / module / feature flag,
and re-route consumers (`lp-server`, `lp-cli`, `fw-emu` host-style
builds, integration tests).

Touch points pulled in by the swap:

- `WasmLpvmEngine` needs a `compile_with_config` override so per-call
  `lpir::CompilerConfig` (set by `ShaderCompileOptions`) actually
  reaches the WASM emitter — today the trait's default falls back to
  `compile()` which uses the engine's construction-time `WasmOptions`.
- `WasmtimeLpvmMemory` is a bump+grow allocator over the wasmtime
  linear memory and explicitly documents itself as "acceptable for
  short-lived host tests, not long-running production services". The
  cached `LpvmBuffer::native_ptr` is invalidated by `Memory::grow`.
  M4b's whole point is to make this the production path; we need to
  resolve the stale-pointer hazard before swapping in.
- `lp-engine`'s `cranelift` Cargo feature — and its transitive use in
  `lp-server`, `lp-cli`, and `fw-emu`/`fw-esp32` "comparison" builds
  — needs renaming or pruning. Firmware (RV32 / ESP32) cannot run
  Wasmtime, so the firmware default (`native-jit`) is unaffected, but
  the firmware-side `cranelift` *comparison* feature has nowhere to go.
- `LpGraphics::backend_name()` literal `"cranelift"` becomes
  `"wasmtime"`. Surfaced in logs and (potentially) `lp-cli`'s shader
  debug target tables.

Out of scope (per roadmap):

- Removing `lpvm-cranelift` from the workspace. It stays for `lp-cli
  shader-debug` (object-file generation), the Phase 2
  `render_texture_smoke.rs` regression, and as a regression backstop.
- `gfx/native_jit.rs` (RV32 firmware path) — `lpvm-native` continues
  unchanged.
- `lpfx-cpu` migration (M4c).
- Wasmtime perf tuning (separate later milestone).

## Current state of the codebase (post-M4a)

### `lp-engine/src/gfx/cranelift.rs`

Already minimal after M4a:

- `CraneliftGraphics { engine: LpsEngine<CraneliftEngine> }`.
- `compile_shader` builds `CompilerConfig` from `ShaderCompileOptions`
  and calls `engine.compile_px(source, Rgba16Unorm, &cfg)`.
- `alloc_output_buffer(w, h)` returns an `LpsTextureBuf` from the
  engine's shared memory.
- `CraneliftShader { px: LpsPxShader }` impls `LpShader`; `render`
  builds the engine uniforms struct (`outputSize`, `time`) via
  `gfx::uniforms::build_uniforms` and forwards to
  `LpsPxShader::render_frame(&uniforms, buf)`.
- File is gated with `#[cfg(feature = "cranelift")]` in
  `gfx/mod.rs`; re-exported as `CraneliftGraphics` from
  `lp-engine`.

The only thing tied to `lpvm-cranelift` is the `LpsEngine<E>` type
parameter (`E = CraneliftEngine`) plus the `CompileOptions::default()`
used to construct the engine. After M4b the type parameter switches
to `WasmLpvmEngine`; everything else in this file is stable.

### `lpvm-wasm` host runtime

Today provides:

- `WasmLpvmEngine::new(WasmOptions) -> Result<Self, WasmError>`
  (constructs the wasmtime `Engine` with `consume_fuel(true)`,
  builds a `WasmLpvmSharedRuntime` with one `Store + Memory`, and a
  `WasmtimeLpvmMemory` bump allocator).
- `impl LpvmEngine for WasmLpvmEngine`:
  - `compile(ir, meta)` — uses `self.compile_options` only.
  - **No `compile_with_config` override** — falls back to the default
    `LpvmEngine::compile_with_config` (ignores per-call config).
  - `memory()` returns `&WasmtimeLpvmMemory`.
- `WasmLpvmInstance::call_render_texture(name, buf, w, h)` is
  implemented (M2 path) and resets globals per call.

`WasmLpvmEngine` and `WasmLpvmInstance` are wired through
`Arc<WasmLpvmSharedRuntime>` whose interior is `Mutex<{Store, Memory,
bump_cursor}>`. So the engine type is `Send + Sync` (Mutex makes it
sync; Store/Memory are Send under wasmtime's thread model).

### `WasmtimeLpvmMemory` (the stale-pointer hazard)

`shared_runtime.rs` documents:

> Bump + grow [...] is acceptable for now because the wasmtime engine
> path is used from short-lived host tests, not long-running
> production services. In a long-lived process, a monotonic bump with
> unbounded growth would be a poor default (no reuse, memory only ever
> expands); a future design would want reuse, caps, or a different
> strategy.

The concrete problem for M4b: `alloc()` records `LpvmBuffer { native:
mem.data_mut(...).as_mut_ptr() + aligned, ... }`. If a *subsequent*
`alloc` triggers `Memory::grow`, the wasmtime memory may relocate and
all previously-returned `native` pointers become stale. Reading those
pointers from `LpsTextureBuf::data()` is then UB.

In `lp-engine`'s usage pattern this matters because:

- `LpsTextureBuf` lives long-term inside `ShaderRuntime`.
- A new shader compile / texture node addition triggers a new `alloc`
  on the same `LpsEngine.memory()`.
- The first `alloc` after init may grow memory (host engine starts
  with one page reserved for `guest_reserve` minus actual size — see
  `EnvMemorySpec`).

This needs to be addressed before the swap is safe.

### Cargo features and consumers

| Crate                   | Feature(s)                                                                   | Wired to                                  |
|-------------------------|------------------------------------------------------------------------------|-------------------------------------------|
| `lp-engine`             | default = `["std", "cranelift", "cranelift-optimizer", "cranelift-verifier"]` | `dep:lpvm-cranelift` + sub-features       |
| `lp-engine`             | `native-jit`                                                                  | `dep:lpvm-native`                         |
| `lp-server`             | default = `["std", "cranelift", "cranelift-optimizer", "cranelift-verifier"]` | proxies into `lp-engine`                  |
| `lp-server`             | `native-jit`                                                                  | proxies into `lp-engine`                  |
| `lp-cli`                | (no features for this) — depends on `lpvm-cranelift` directly for object-file generation in `shader_debug` |
| `fw-emu`                | default = `["native-jit"]`, optional `cranelift`                              | `lp-server/cranelift` (RV32 comparison)   |
| `fw-esp32`              | default = `["esp32c6", "server", "native-jit"]`, optional `cranelift`         | `lp-server/cranelift` (ESP32 comparison)  |
| `lpfx-cpu`              | default = `["cranelift"]`                                                     | direct `lpvm-cranelift` (M4c — out of scope) |

External callers of `CraneliftGraphics` / `lp_server::CraneliftGraphics`:

- `lp-cli/src/server/create_server.rs` — host CLI server constructor.
- `lp-cli/tests/integration.rs` — integration test.
- `lp-core/lp-server/tests/{server_tick,stop_all_projects,fs_version_tracking}.rs`.
- `lp-core/lp-engine/src/nodes/shader/runtime.rs` — `#[cfg(all(test,
  feature = "cranelift"))]` test.
- `fw-emu/src/main.rs`, `fw-esp32/src/main.rs` — `#[cfg(feature =
  "cranelift")]` runtime selection (compile-time mutually exclusive
  with `native-jit`).
- Doc-comment examples in `lp-server/src/server.rs`.

### `LpvmEngine` trait shape

Already has `compile_with_config(ir, meta, &CompilerConfig)` with a
default impl that delegates to `compile()`. `CraneliftEngine` and
`NativeJitEngine` override; `WasmLpvmEngine` does not. M4a routes
`compile_px` through `compile_with_config`.

### Notes on `lp-cli shader-debug`

`lp-cli/src/commands/shader_debug/handler.rs` calls
`collect_cranelift_data` → `lpvm-cranelift::object_bytes_from_ir` +
`link_object_with_builtins`. This is **AOT object-file generation**,
not JIT runtime — it's an authoring/debugging tool that emits
inspectable artifacts. It is independent of `gfx/cranelift.rs`, lives
on the `lpvm-cranelift` crate that *stays* per the roadmap, and
should not be touched by M4b.

The roadmap mentioned a "target table" in `lp-cli shader-debug` and
`scripts/glsl-filetests.sh`. Verify in the design phase whether either
literally lists `cranelift` as a runtime backend selector that points
at `lp-engine`'s host JIT (vs. the AOT object path). Most likely
nothing in the table changes — the AOT cranelift entry stays, and
there is no separate "host runtime" entry today.

### `LpShader: Send + Sync`

`LpShader` is the trait `lp-engine` boxes inside `ShaderRuntime`; the
engine renders single-threaded but holds shaders in `Arc<dyn …>` and
distributes them across nodes. M4a added `unsafe impl Send + Sync
for LpsTextureBuf / LpsPxShader` justified by the engine's
single-threaded render loop. The same justification carries over for
`WasmLpvmInstance` (which holds a `Mutex<Store>` via `Arc`, so is
already Send + Sync structurally), but should be re-confirmed
end-to-end during implementation.

## Questions

### Q1. `WasmtimeLpvmMemory` stale-pointer hazard — what to do? ✅ resolved

**Context.** The bump-allocator on wasmtime linear memory caches a raw
host pointer in `LpvmBuffer::native`. If any subsequent allocation
forces `Memory::grow`, wasmtime is free to relocate the linear memory,
which silently invalidates every previously-returned `native` pointer.
`LpsTextureBuf::data()` and `data_mut()` then read stale memory (UB).

In M4a-era usage (lp-engine), `LpsTextureBuf`s outlive the alloc that
produced them. Each new compiled shader allocates an output buffer
from the same engine. A texture allocated early (small project) and a
new texture added later (project edit) will, in principle, hit this
hazard.

In test-land it has been masked because the engine reserves
`EnvMemorySpec::guest_reserve_bytes()` worth of pages up front, and
typical tests stay below that.

**Options.**

1. **Pre-reserve enough memory at engine construction.** Pick a budget
   (e.g. 64 MiB or a configurable cap), pre-grow the wasmtime memory
   to that budget once, refuse alloc beyond it. Stale pointers cannot
   happen because we never call `grow` again. Hard cap is the
   trade-off; same shape as the in-tree `EnvMemorySpec` constants.
2. **Refresh the host pointer on every read.** Change
   `LpsTextureBuf::data()/data_mut()` to redo the
   `mem.data(store).as_ptr() + offset` arithmetic each time. Requires
   handing the `WasmLpvmSharedRuntime` (or a closure) into every
   `LpsTextureBuf`. Touches the buffer abstraction's public API.
3. **Switch the wasmtime path to a host-allocated arena that backs
   `LpvmBuffer.native` from a separate, stable allocation, and
   manually `Memory::write` into the wasmtime memory at call time.**
   Reproduces what `lpvm-cranelift::CraneliftHostMemory` does but
   loses zero-copy guest-host sharing. Heavy.
4. **Keep bump+grow, accept the hazard, gate behind a debug
   assertion.** Detect stale-pointer access at runtime and panic in
   debug builds; risk silent UB in release. Not acceptable for the
   production path.

**Suggested answer.** **Option 1 with a configurable cap.**
Pre-grow once at `WasmLpvmEngine::new` to a `max_pages` budget
exposed via `WasmOptions` (default e.g. 64 MiB ≈ 1024 pages). All
allocations live inside the pre-grown region; `Memory::grow` is
never called after init, so cached `native` pointers stay valid
forever. This is the smallest change, matches how `lpvm-cranelift`
runs (its global allocator is also stable across allocs), and
preserves zero-copy sharing for `LpsTextureBuf`. Document the cap
prominently; revisit when a real workload hits it.

**Answer.** **Option 1 — pre-reserve, no further `grow`.** Default
budget = 64 MiB (1024 wasm pages of 64 KiB) exposed on `WasmOptions`
as e.g. `host_memory_pages`. Engine construction calls
`Memory::grow` once to the budget and never again; alloc returns
`OutOfMemory` past the cap. `WasmtimeLpvmMemory::alloc` keeps the
bump cursor; the `grow` loop is removed.

Defer the wasm-in-browser counterpart (which already grows on demand
via `WebAssembly.Memory`) to a later pass — M4b only touches the
desktop wasmtime runtime.

### Q2. Cargo feature rename — `cranelift` → `wasmtime`? ✅ resolved (subsumed by Q2.5)

See **Q2.5** below — feature-based backend selection is replaced by
target-arch-based auto-selection, so the rename question dissolves
(the feature goes away entirely, no rename needed).

### Q2.5. Target-arch-based backend selection (replaces feature flags) ✅ resolved

**Context.** During Q2 discussion the user proposed replacing the
feature-flag-driven backend selection with target-arch-driven
selection:

- `cfg(target_arch = "riscv32")` → `lpvm-native` (rt_jit)
- `cfg(target_arch = "wasm32")` → `lpvm-wasm` (rt_browser:
  `BrowserLpvmEngine`)
- everything else (catchall) → `lpvm-wasm` (rt_wasmtime:
  `WasmLpvmEngine`)

Both branches of the catchall already provide working `LpvmEngine`
impls (`WasmLpvmEngine` in `lp-shader/lpvm-wasm/src/rt_wasmtime/engine.rs`,
`BrowserLpvmEngine` in `lp-shader/lpvm-wasm/src/rt_browser/engine.rs`).
The RV32 branch is what `gfx/native_jit.rs` already does. No new
backends invented.

**Concrete shape.**

```toml
# lp-engine/Cargo.toml
[target.'cfg(target_arch = "riscv32")'.dependencies]
lpvm-native = { path = "../../lp-shader/lpvm-native", default-features = false }

[target.'cfg(not(target_arch = "riscv32"))'.dependencies]
lpvm-wasm = { path = "../../lp-shader/lpvm-wasm", default-features = false }
```

```rust
// lp-engine/src/gfx/mod.rs
#[cfg(target_arch = "riscv32")]
mod native_jit;
#[cfg(target_arch = "wasm32")]
mod wasm_guest;
#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
mod host;

#[cfg(target_arch = "riscv32")]
pub use native_jit::Graphics;
#[cfg(target_arch = "wasm32")]
pub use wasm_guest::Graphics;
#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
pub use host::Graphics;
```

A single unqualified `lp_engine::Graphics`. `lp-server` re-exports
without ceremony; `lp-cli`, `fw-emu`, `fw-esp32` all just call
`Graphics::new()` with no `#[cfg(feature = …)]` at the call site.

**Knock-on effects:**

- **Q2 (feature rename) dissolves.** No `cranelift` / `wasmtime`
  feature on `lp-engine` or `lp-server`. The flag goes away entirely.
- **Q3 (firmware comparison feature) reinforced.** Already agreed to
  delete; the new model has nowhere to express "use a different
  backend on the same target", which is exactly what the comparison
  feature was for.
- **Q4 (type name) becomes `Graphics`.** No "Cranelift" / "Wasmtime"
  / "Host" debate; the type is what the target dictates. Single
  unqualified name.
- **Q7 (drop `lpvm-cranelift` from `lp-engine`) absorbed.** The new
  target deps don't include `lpvm-cranelift`. Same end state.
- **Q8 (std requirement) dissolves.** `lpvm-wasm` is in the catchall
  branch; the catchall by definition is host-class targets where
  `std` is fine. RV32 branch uses `lpvm-native` which is `no_std`.
  The `lp-engine` `std` feature can stay as it is for `lp-shared`
  etc., but no new gating is needed for the backend selection.

**Cost.**

- `fw-emu` / `fw-esp32`: drop both the `cranelift` and `native-jit`
  features (always-on for RV32). Their `compile_error!` for "no
  backend selected" goes away because there's always exactly one
  backend.
- `lp-cli/src/server/create_server.rs`, `lp-cli/tests/integration.rs`,
  `lp-server/tests/{server_tick,stop_all_projects,fs_version_tracking}.rs`,
  `lp-engine/src/nodes/shader/runtime.rs:445`: switch
  `CraneliftGraphics::new()` → `Graphics::new()`, drop any feature
  cfgs.
- One additional file in `lp-engine/src/gfx/`: `wasm_guest.rs` for
  the wasm32 branch. Mirrors `host.rs` but uses
  `BrowserLpvmEngine`. Needs a smoke (or at least
  `cargo check --target wasm32-unknown-unknown -p lp-engine`).
- `lpvm-cranelift` stays in the workspace for `lp-cli shader-debug`'s
  AOT path and (until M4c) `lpfx-cpu`'s direct use. `lp-engine`
  simply no longer depends on it.

**lpvm-native rt_emu on the catchall — explicit non-decision.**
User raised it as an open sub-question. Not adopted in M4b: there's
no `LpvmEngine`-shaped wrapper for rt_emu wired into `gfx/` today,
and its only realistic use case (test RV32 codegen without firmware)
is already covered by `lps-filetests`. If wanted later, add as an
opt-in `cfg(feature = "emu-graphics")` escape hatch on the catchall;
small additive change, doesn't disturb the rule.

**Future-fit.** User confirmed lpfx-cpu (which will eventually
replace this lp-engine direct backend selection per the lpfx
roadmap) is intended to do "basically the same thing" — i.e. it
will adopt the same target-arch-based selection pattern. So this
work establishes the model lpfx-cpu copies, not throwaway work.

**Answer.** **Adopt target-arch-based backend selection for
`lp-engine`'s `Graphics`.** No backend feature flag on `lp-engine` or
`lp-server`. RV32 → `lpvm-native`, wasm32 → `lpvm-wasm` browser,
catchall → `lpvm-wasm` wasmtime. Single unqualified
`lp_engine::Graphics` type. Defer the rt_emu-on-host escape hatch.

**Context.** The roadmap recommends renaming for a "clean break"
since the feature flag is internal infrastructure. Today five places
use it:

- `lp-engine`: defines `cranelift` (+ `cranelift-optimizer`,
  `cranelift-verifier`).
- `lp-server`: proxies all three into `lp-engine`.
- `lp-cli`: depends on `lpvm-cranelift` directly (no feature).
- `fw-emu`: optional `cranelift` (RV32 comparison build).
- `fw-esp32`: optional `cranelift` (ESP32 comparison build).
- `lpfx-cpu`: default `cranelift` (separate; M4c handles).

Renaming cascades through `lp-server`, `fw-emu`, `fw-esp32` Cargo
files and any CI scripts.

**Options.**

A. **Hard rename to `wasmtime`.** Update every `--features cranelift`
   site in workspace + CI; drop the old name entirely. Cleanest end
   state. Some short-term churn for anyone with a stale shell
   history.

B. **Rename to `wasmtime`, keep `cranelift` as a deprecated alias for
   one cycle.** `cranelift = ["wasmtime"]` in `lp-engine`/`lp-server`
   so existing scripts still compile but produce a deprecation
   warning. More forgiving; carries a stale name in the tree.

C. **Keep the name `cranelift`** (since wasmtime *is* using
   cranelift internally as its codegen). Internally points at
   `lpvm-wasm`. Least churn; surfaces a misleading name forever.

**Suggested answer.** **A (hard rename)**, mirroring the roadmap's
explicit recommendation. The infrastructure is internal; no
out-of-tree consumer is broken; carrying a deprecated alias just
delays the cleanup. Rename in the same commit as the swap so
`cargo --features cranelift` either works (old behaviour) or fails
(new behaviour) without a half-state. Update CI scripts and
`AGENTS.md` muscle-memory notes in the same plan.

`fw-emu` / `fw-esp32` "cranelift comparison" builds become a
question on their own — see Q3.

**Answer.** **A — hard rename to `wasmtime`.** No alias kept.
`lp-engine`'s `cranelift`, `cranelift-optimizer`, `cranelift-verifier`
features all disappear; `wasmtime` becomes the host-jit feature on
`lp-engine` and `lp-server`. CI / scripts / docs (`AGENTS.md`) are
updated in the same commit. Anyone with stale shell history gets a
clear "no such feature" error and re-types it once.

### Q3. `fw-emu` / `fw-esp32` `cranelift` comparison feature — keep, rename, or delete? ✅ resolved

**Context.** Today both firmware crates have an optional `cranelift`
feature for comparison builds: replace the RV32 `native-jit` shader
backend with `lpvm-cranelift` JIT compiled into the firmware itself.
Wasmtime is **not** viable for these crates — they're `no_std` RV32
binaries (or ESP32 Xtensa-ish), and wasmtime needs `std` and a real
host platform.

So the feature literally cannot rename to `wasmtime`. Three options:

A. **Delete the comparison feature** from `fw-emu` and `fw-esp32`
   entirely. Native-jit becomes the only firmware backend. Anyone
   who wants Cranelift-output inspection uses `lp-cli shader-debug`
   (still works, AOT path stays).

B. **Rename to `cranelift-jit`** to disambiguate from the host
   `cranelift` feature (which is itself going away in Q2). Keeps the
   firmware comparison capability; clear that it's the bare-metal
   JIT path. Continues exercising `lpvm-cranelift` on the RV32
   target — non-trivial regression backstop.

C. **Keep the name `cranelift`** on the firmware crates only (the
   host `cranelift` feature is gone, so no clash). Confusing but
   minimal churn.

**Suggested answer.** **B (rename to `cranelift-jit`)**. The
firmware comparison build is a useful regression backstop for the
`lpvm-cranelift` crate on RV32 — that target's behaviour is what
the workspace `lpvm-cranelift` smoke tests cannot fully cover.
Keeping it works; renaming it to `cranelift-jit` makes the intent
explicit (cranelift JIT compiled into the firmware, distinct from
the host backend) and avoids confusion now that the host
`cranelift` feature is gone.

If real-world usage of the `--features cranelift` firmware build is
*nil* and nobody on the team has reached for it in the last several
weeks, **A (delete)** is also defensible — less code to maintain.
**Confirm with the user.**

**Answer.** **A — delete the firmware `cranelift` comparison
feature.** `fw-emu` and `fw-esp32` lose the optional `cranelift`
feature; both keep `native-jit` as the only shader backend
(matching their existing default).

User rationale: "I have used cranelift compare before for fw-emu, but
it was always a hack". The compile-time Cranelift JIT in
`lpvm-cranelift` continues to exist (used by `lp-cli shader-debug`'s
AOT object generation, the in-tree `lpvm-cranelift` smoke tests, and
M4c's `lpfx-cpu` until M4c migrates it). The bare-metal-JIT
comparison build is not worth carrying as a Cargo feature.

### Q4. Type / file rename — `CraneliftGraphics` → `WasmtimeGraphics`? ✅ resolved (subsumed by Q2.5)

Type becomes unqualified `lp_engine::Graphics` (one type per target,
selected by `cfg(target_arch = …)`). File names follow:
`gfx/host.rs` (wasmtime), `gfx/native_jit.rs` (RV32, existing),
`gfx/wasm_guest.rs` (wasm32, new). No deprecation alias.

Original Q4 text below for history:

**Context.** Roadmap lists three options:

- Rename `CraneliftGraphics` → `WasmtimeGraphics` (matches the
  backend); rename file `gfx/cranelift.rs` → `gfx/wasmtime.rs`.
- Rename to `HostGraphics` (backend-neutral; survives a future
  swap).
- Keep `CraneliftGraphics` as a `#[deprecated]` alias for one
  release cycle.

Five-six external callers to update (see "Cargo features and
consumers" above).

**Suggested answer.** **`WasmtimeGraphics` + filename `gfx/wasmtime.rs`,
no alias.** Specific is better than vague. Same rationale as the
feature flag rename — internal infra, no out-of-tree breakage,
deprecation alias adds tree noise without buying anything. Update
the call sites in the same commit.

If "this might swap again" is a real concern, `HostGraphics` is the
safer long-term name. But that bet has a cost: every reader has to
remember which "host" backend is wired in *today*. Concrete name
wins until we have evidence we'll swap again.

**Answer.** **`HostGraphics` (backend-neutral name) — no
deprecation alias.** Rename `CraneliftGraphics` → `HostGraphics` and
file `gfx/cranelift.rs` → `gfx/host.rs`. `lp-server`'s public
re-export becomes `pub use lp_engine::HostGraphics;` (gated on the
`wasmtime` feature). All call sites in `lp-cli`, `lp-server` tests,
firmware mains, and `lp-engine` test modules update to
`HostGraphics::new()` in the same commit.

Rationale: backend-neutral name spares the next swap. The
`backend_name()` log string still reports the concrete current
backend (`"wasmtime"`, see Q5) so logs stay specific; only the type
name is generic.

### Q5. `LpGraphics::backend_name()` literal — `"wasmtime"`? ✅ resolved

**Context.** Today `CraneliftGraphics::backend_name()` returns
`"cranelift"`. Used in log messages
(`lp-engine/src/nodes/shader/runtime.rs:445` area) and possibly
surfaced in JSON state for clients. Quick grep shows no hard
dependency on the literal value beyond logs.

**Suggested answer.** Change to `"wasmtime"`. It's a debug/log
identifier; nothing is keyed off it.

**Answer.** Use `"<crate>::<rt_module>"` form for grep-friendliness
and parity with existing log strings:

| Target | `backend_name()` |
|---|---|
| `cfg(target_arch = "riscv32")` | `"lpvm-native::rt_jit"` |
| `cfg(target_arch = "wasm32")` | `"lpvm-wasm::rt_browser"` |
| catchall (host) | `"lpvm-wasm::rt_wasmtime"` |

Cleanup phase verifies nothing programmatically keys off the old
`"cranelift"` literal. `lp-cli shader-debug` uses its own separate
`"rv32c"` / `"emu"` strings (`lp-cli/src/commands/shader_debug/collect.rs:139`)
— no conflict.

### Q6. Implement `compile_with_config` on `WasmLpvmEngine`? ✅ resolved

**Context.** `LpsEngine::compile_px` calls
`engine.compile_with_config(ir, meta, &cfg)`. `WasmLpvmEngine` does
not override `compile_with_config`, so the trait's default impl
delegates to `compile()`, which uses the engine's
construction-time `WasmOptions.config`. The per-call
`ShaderCompileOptions::to_compiler_config()` set in
`gfx/cranelift.rs` is silently dropped on the wasmtime path today.

**Options.**

A. **Add an override on `WasmLpvmEngine` that merges the per-call
   `CompilerConfig` into a `WasmOptions` clone before compiling.**
   Mirror what `CraneliftEngine` and `NativeJitEngine` do.

B. **Punt — accept that `lp-engine`'s `q32_options` are ignored on
   the wasmtime path until a follow-up.** Acceptable only if
   `lp-engine` never sets non-default `q32_options` in production.
   Not the case in lpfx-cpu downstream; `lp-engine` defaults today
   but no guarantee for tomorrow.

**Suggested answer.** **A — add the override.** Trivial code
(`opts.config = config.clone()`; `opts.float_mode` stays from the
engine since float mode is not part of `CompilerConfig`); restores
parity with the other two engines; means the `q32` log line written
by `lp-engine`'s shader runtime accurately reflects what the
backend compiled with. Includes a unit test in `lpvm-wasm` mirroring
the `lpvm-native` Phase-0 regression test from M4a (different
`q32.mul_mode` produces different WASM bytes).

**Answer.** **A — add the override.** Implemented in `lpvm-wasm`'s
`WasmLpvmEngine` per the suggestion. Add a unit test in
`lpvm-wasm/tests/` (or `src/rt_wasmtime/`) asserting that compiling
the same LPIR with two different `CompilerConfig.q32.mul_mode`
values produces distinct WASM byte outputs (or at least distinct
function bodies for the affected op). Same shape as M4a Phase 0.

### Q7. `lp-engine` Cargo: keep depending on `lpvm-cranelift` after M4b? ✅ resolved (subsumed by Q2.5)

Q2.5's target-arch dep selection drops `lpvm-cranelift` from
`lp-engine` automatically. Same end state. Original Q7 below:

**Context.** With `gfx/cranelift.rs` swapped to use `lpvm-wasm`,
`lp-engine` itself no longer needs `lpvm-cranelift` as a dep —
nothing inside the crate references it. The `lp-engine`
`#[cfg(feature = "cranelift")]` test in
`nodes/shader/runtime.rs:445` constructs `crate::CraneliftGraphics`
which after the rename will be `HostGraphics` and use wasmtime
internally.

**Options.**

A. **Drop `lpvm-cranelift` from `lp-engine`'s `Cargo.toml`.** Add
   `lpvm-wasm` instead (gated on the new `wasmtime` feature). Clean
   end state.

B. **Keep `lpvm-cranelift` as an optional, off-by-default dep**
   for nothing in particular — pure caution. Defer the cleanup.
   Wastes build time.

**Suggested answer.** **A.** `lp-engine` adds `lpvm-wasm` (std-only,
under `cfg(not(target_arch = "wasm32"))` since lpvm-wasm itself is
gated that way), drops `lpvm-cranelift`. The `lpvm-cranelift` crate
remains in the workspace (used by `lp-cli` and `lpfx-cpu`).

**Answer.** **A — drop `lpvm-cranelift` from `lp-engine`'s
`Cargo.toml`.** Add `lpvm-wasm` as the new optional dep behind the
`wasmtime` feature. `lpvm-cranelift` continues to exist in the
workspace for `lp-cli shader-debug`'s AOT path and (until M4c) for
`lpfx-cpu`. The `cranelift-optimizer` / `cranelift-verifier`
sub-features go away — they were forwards into `lpvm-cranelift`'s
own optional cranelift internals; now that `lp-engine` doesn't pull
in `lpvm-cranelift`, those forwards are pointless.

### Q8. `lp-engine` `wasmtime` feature gating + std requirement ✅ resolved (subsumed by Q2.5)

Dissolved by Q2.5: the `wasmtime` feature doesn't exist;
target-arch-gated `[target.cfg(...).dependencies]` handles
inclusion. `lpvm-wasm` lives in the catchall branch where `std` is
always available. Original Q8 below:

**Context.** `lpvm-wasm` requires `std` (uses `std::collections::HashMap`,
`std::sync::Mutex`, wasmtime which is itself `std`). `lp-engine` today
has a `std` feature that's enabled by default but can in principle be
disabled (transitively for fw-emu's `no_std` build).

After Q7, the `wasmtime` feature must imply `std` (or the build
breaks on non-`std` targets). On `fw-emu` / `fw-esp32`, the
`wasmtime` feature is unreachable — those crates don't enable it
(default is `native-jit`).

**Suggested answer.** Make `wasmtime = ["std", "dep:lpvm-wasm"]` in
`lp-engine`. Default features become `["std", "wasmtime"]`. `fw-emu`
/ `fw-esp32`'s `lp-server` dep already uses `default-features =
false, features = ["panic-recovery"]`, so they won't accidentally
pull in wasmtime. Confirm with a `cargo build -p fw-emu --features
native-jit --no-default-features` style check in CI.

**Answer.** Yes — `wasmtime = ["std", "dep:lpvm-wasm"]`, default
feature set becomes `["std", "wasmtime"]`. Cleanup phase verifies
that `cargo build -p fw-emu` (which uses `lp-server` with
`default-features = false`) does not accidentally pull in `lpvm-wasm`
by inspecting `cargo tree --no-default-features` output for fw-emu /
fw-esp32 builds, or just by the build succeeding on the firmware
target where `lpvm-wasm` cannot compile.

### Q9. Wasmtime-specific config knobs — defaults pinned where? ✅ resolved

**Context.** Wasmtime has a real config surface (fuel limits, epoch
interruption, memory limits, parallel compilation, etc.). Today
`WasmLpvmEngine::new` enables `consume_fuel(true)` and stops there.
The roadmap explicitly says perf tuning is a follow-up.

**Suggested answer.** Keep wasmtime defaults plus `consume_fuel`
(today's behaviour) for M4b. Pin the `WasmOptions` budget chosen in
Q1 (memory pages). Add a brief comment in `WasmLpvmEngine::new`
calling out the deferred knobs (epoch interruption, parallel compile)
so future readers don't have to rediscover them. Track perf tuning
as a separate plan.

**Answer.** Keep today's wasmtime defaults plus `consume_fuel(true)`.
Add the `host_memory_pages` budget from Q1. Document deferred knobs
in a short comment block at `WasmLpvmEngine::new`.

### Q10. Performance / size delta — how is the report produced? ✅ resolved

**Context.** Roadmap calls for documenting size / cold-start /
multi-shader behaviour deltas in
`docs/design/native/perf-report/`. There is no automation today; the
M2 baseline (~600 ms cold-start on hardware) was hand-measured.

**Options.**

A. **Hand-measured deltas captured in a new perf-report file** for
   M4b: workspace `cargo check` time, fw-emu host-build size (the
   roadmap's stated metric — though note Q3: fw-emu doesn't use
   wasmtime, so this is really `lp-cli` or `lp-server`-host binary
   size), single-shader host cold-start, multi-shader run with
   the previously-flaky pattern.

B. **Skip the perf report**, treat M4b as correctness-only, defer
   perf to the planned tracing milestone.

**Suggested answer.** **A, but scope it down.** Capture a small
report (one Markdown file under `docs/design/native/perf-report/`)
with workspace `cargo check` time before/after, host binary size of
`lp-cli` (or `lp-server`-using-host-binary, whichever is the natural
"production" host artefact in this tree) before/after, and a
multi-shader test asserting no flaky failures.

The roadmap mentions `fw-emu` size — likely a planning slip given
fw-emu can't use wasmtime. Confirm with the user; substitute the
correct artefact.

**Answer.** **A — small hand-measured report.** Captures: workspace
`cargo check` time, host binary size for the native CLI artefact
(`lp-cli` and/or `lp-server` test binary, whichever exposes the
host-server entry today — verify in design phase), and a
multi-shader stress run. Single Markdown file under
`docs/design/native/perf-report/`. Substitute the correct artefact
for the roadmap's `fw-emu` reference (firmware doesn't use
wasmtime; the relevant artefact is the desktop server / CLI binary).

### Q11. Plan name and location ✅ resolved

**Context.** M4a went into `docs/plans-old/` as a flat file
(`2026-04-19-m4a-pixel-loop-migration.md`). The plan-command
convention uses a directory, since this M4b plan will have multiple
phase files.

**Suggested answer.** Use directory `docs/plans/2026-04-19-m4b-host-backend-swap/`
to mirror M4a's filename pattern (single milestone per plan, dated
the day work starts). Move to `docs/plans-done/` when complete (this
project uses both `plans-old/` and `plans-done/`; pick whichever is
the current convention — M4a went to `plans-old/`).

**Answer.** Directory `docs/plans/2026-04-19-m4b-host-backend-swap/`,
move to `docs/plans-done/` on completion (current convention for
finished M4-era plans).

# Notes (raw / unresolved)

(none yet)
