# M5 filetests migration — all phases

Single reference for the eight phases. Per-phase files: `01-phase-…md` through `08-phase-…md` in this directory. Design overview: [`00-design.md`](./00-design.md).

**Prerequisite:** [`lps-value-q32-restructure`](../2026-04-07-lps-value-q32-restructure/00-design.md) (done) — `LpsValueQ32`, `lpvm_abi`.

---

## Phase 1 — `LpvmInstance`: `call_q32` + `debug_state`

**File:** [`01-phase-lpvm-instance-call-q32.md`](./01-phase-lpvm-instance-call-q32.md)

**Scope**

- In `lp-shader/lpvm/src/instance.rs`, extend `LpvmInstance` with:
  - **`call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error>`**  
    Arguments: one slice of **all parameter words** in order — same layout as concatenating the result of `flatten_q32_arg` for each formal parameter. Return: **flattened return value** only; `void` → empty `Vec`. (Extend later if `out` / full `GlslReturn` must surface on the trait.)
  - **`debug_state(&self) -> Option<String>`** with default **`None`**.
- Provide a **default `call_q32`** that keeps the workspace building (e.g. lossy route via `call` + `LpsValueF32`, or minimal helper), until Phases 2–4 override with exact paths.
- Update `lp-shader/lpvm/src/lib.rs` crate docs for the two call paths.

**Code organization**

- Trait stays in `instance.rs`; default methods or small private helpers in the same module; helpers at bottom.

**Validate:** `cargo check -p lpvm` && `cargo test -p lpvm`

---

## Phase 2 — `CraneliftInstance`: exact `call_q32`, `debug_state`

**File:** [`02-phase-cranelift-call-q32.md`](./02-phase-cranelift-call-q32.md)

**Scope**

- Implement **`call_q32`** on `CraneliftInstance` (`lp-shader/lpvm-cranelift/src/lpvm_instance.rs`): **exact** Q32 — same machine path as today’s Q32 call; **no** `f32` for float lanes. Split `args` per parameter using `glsl_component_count` / `flatten_q32_arg` rules; build `LpsValueQ32` or call an internal that already takes flat `Vec<i32>`; decode return and **re-flatten** to `Vec<i32>` for the trait (add small encode helper in `lpvm_abi` only if nothing reusable exists).
- **`debug_state`:** return **`None`**, or a short host string if easy.

**Validate:** `cargo check -p lpvm-cranelift --features glsl,std` && `cargo test -p lpvm-cranelift`

---

## Phase 3 — `EmuInstance`: exact `call_q32`, rich `debug_state`

**File:** [`03-phase-emu-call-q32.md`](./03-phase-emu-call-q32.md)

**Scope**

- **`call_q32`:** same ABI contract as Phase 2; delegate to **`emu_run` / `glsl_q32_call_emulated`** (or equivalent) when it already uses flat `i32`.
- **`debug_state`:** move **emulator diagnostics** here (registers, PC, trap) — replace reliance on `GlslExecutable::format_emulator_state()` from filetests. Optionally `None` on success; capture state on trap/error on `EmuInstance`.

**Validate:** `cargo check -p lpvm-emu` && `cargo test -p lpvm-emu`

---

## Phase 4 — WASM `LpvmInstance`: `call_q32` + `debug_state`

**File:** [`04-phase-wasm-call-q32.md`](./04-phase-wasm-call-q32.md)

**Scope**

- **wasmtime** and **browser** instances: implement extended trait.
- **`call_q32`:** start from **trait default**; if **`wasm.q32` filetests fail**, implement the same `LpsValueQ32` + flatten path as native `call`.
- **`debug_state`:** **`None`**.

**Validate:** `cargo check -p lpvm-wasm` && `cargo test -p lpvm-wasm`

---

## Phase 5 — Filetests: one engine + one module per **file**

**File:** [`05-phase-filetests-engine-module.md`](./05-phase-filetests-engine-module.md)

**Scope**

- Stop recompiling the same `.glsl` for every `// expect:` line.
- Per **test file** and target (`jit.q32`, …): create **one** backend engine (`CraneliftEngine` | `EmuEngine` | `WasmLpvmEngine`) and **one** `LpvmModule` from `engine.compile(&ir, &meta)` after a single GLSL → IR compile.
- Per **test case**: only **`instantiate()`** → run → drop instance.
- If **`dyn LpvmModule`** is awkward, use a **`FiletestModule` enum** (or similar) and match to instantiate.

**Files:** new `engine.rs` (or `filetest_lpvm.rs`); wire **`run_detail.rs`**, **`compile.rs`**.

**Validate:** `cargo check -p lps-filetests`; run a **small** filetest filter.

---

## Phase 6 — Filetests: `execution.rs` uses `LpvmInstance`

**File:** [`06-phase-filetests-execution.md`](./06-phase-filetests-execution.md)

**Scope**

- Remove **`dyn GlslExecutable`** from **`execution.rs`**; drive tests through **`LpvmInstance`** (or an enum wrapper over the three instance types).
- **`.f32` targets:** `instance.call(name, &[LpsValueF32])` → existing float comparison.
- **`.q32` targets:** build flat **`Vec<i32>`** args, **`instance.call_q32(name, &args)`**, decode with **`decode_q32_return`** / `LpsValueQ32` for assertions.
- Errors: append **`instance.debug_state()`** when `Some(_)`.
- Map instance **`Error`** to `anyhow` / `GlslError` like today.

**Validate:** `cargo test -p lps-filetests`; spot-check `./scripts/glsl-filetests.sh` with a filter.

---

## Phase 7 — Delete `GlslExecutable` glue from filetests

**File:** [`07-phase-remove-glsl-executable-wrappers.md`](./07-phase-remove-glsl-executable-wrappers.md)

**Scope**

- Delete or empty: **`lpir_jit_executable.rs`**, **`lpir_rv32_executable.rs`**, **`wasm_runner.rs`**, **`wasm_link.rs`** (when replaced by `lpvm-wasm` path).
- **`q32_exec_common.rs`:** keep only shared helpers (signatures, flat args, compare); drop **`Q32ShaderExecutable`** / **`GlslExecutable`** impls if unused.
- Remove **`lps-exec`** from **`lps-filetests/Cargo.toml`** if unused.
- Update **`test_run/mod.rs`**.

**Validate:** `rg "lps_exec|GlslExecutable" lp-shader/lps-filetests`; `just test-filetests` (or full `glsl-filetests.sh`); `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server` if shader crates changed.

---

## Phase 8 — Cleanup, summary, move plan to `plans-done`

**File:** [`08-phase-cleanup-validation.md`](./08-phase-cleanup-validation.md)

**Scope**

- Remove **TODO / dbg! / stray println!** from the M5 diff; **`cargo +nightly fmt`**; fix **warnings**.
- Run full **filetest matrix** (CI parity): **jit.q32**, **jit.f32**, **rv32.q32**, **rv32.f32**, **wasm.q32**, **wasm.f32** as applicable.
- Write **`summary.md`** in this plan folder.
- Move **`docs/plans/2026-04-07-lpvm2-m5-filetests/`** → **`docs/plans-done/`**.
- **Conventional commit** describing `call_q32`, `debug_state`, filetest migration, engine/module per file.

**Validate:** `just fci-glsl` or full `cargo check` / `cargo test` set listed in phase 8 file.

---

## Phase order (checklist)

| # | Deliverable |
|---|-------------|
| 1 | `LpvmInstance` + `call_q32` + `debug_state` defaults (`lpvm`) |
| 2 | `CraneliftInstance` exact `call_q32` |
| 3 | `EmuInstance` exact `call_q32` + `debug_state` |
| 4 | WASM instances satisfy trait |
| 5 | Filetests: engine + module per file |
| 6 | `execution.rs`: `call` / `call_q32`, errors |
| 7 | Remove `GlslExecutable` wrappers |
| 8 | Cleanup, CI matrix, `summary.md`, `plans-done`, commit |
