# Stage V2: Filetest integration (`jit.q32`, `rv32.q32`) — notes

## Scope of work

- Extend **`lp-glsl-filetests`** for **`lpir-cranelift`**:
  - **`jit.q32`:** GLSL → `lpir_cranelift::jit` → `JitModule` → expectations via
    **`GlslExecutable`** adapter.
  - **`rv32.q32`:** GLSL → LPIR → **Stage V1** object + link + emulator → same
    trait boundary.
- Keep **`wasm.q32`** (unchanged backend).
- **Remove the legacy `cranelift.q32` target** and **`Backend::Cranelift`**:
  no `glsl_emu_riscv32` / old AST compiler in the filetest runner.
- **Drop `lp-glsl-cranelift` from `lp-glsl-filetests` dependencies** by depending
  on **new small crates** (copies; legacy crates unchanged): **`lp-glsl-diagnostics`**,
  **`lp-glsl-core`**, **`lp-glsl-values`**, **`lp-glsl-exec`**. Implement the trait
  in **`lp-glsl-wasm`** and the lpir adapters against **`lp-glsl-exec`**. Old
  **`lp-glsl-cranelift`** keeps its own copies for non-filetests callers until
  Stage VII deletes that crate.
- **`DEFAULT_TARGETS`:** **`[jit.q32]` only** for fast local runs (adjust later if
  needed).
- **CI:** run **`wasm.q32`** and **`rv32.q32`** in addition to **`jit.q32`**
  (exact mechanism: second test job, env-gated integration test, or app flag —
  document in README / CI config).
- Extend **annotations** for **`jit`** and **`rv32`**; **migrate** existing
  `backend=cranelift` filters to **`jit`** or **`rv32`** as appropriate (same
  behavioral intent: host vs RV32 emu was never distinguished in the old filter;
  prefer **`rv32`** when the old path was emulator, **`jit`** when validating
  host LPIR is enough — many tests can use **`jit`** only).
- **Triage** scalar corpus on the new matrix.

**Out of scope:** lp-engine (Stage VI), vector filetests. **Deleting the entire
`lp-glsl-cranelift` crate** remains **Stage VII**; V2 only removes it from
**filetests** after the new-crate wiring (phase 04).

## Current state of the codebase

### Targets

- `target::Target` has `backend`, `float_mode`, `isa`, `exec_mode`.
- **`Target::name()`** is `"{backend}.{float_mode}"`.
- `DEFAULT_TARGETS`: `[cranelift.q32, wasm.q32]`; cranelift uses
  **`glsl_emu_riscv32_with_metadata`**.

### Execution

- **New stack (in repo):** **`lp-glsl-exec`** (`GlslExecutable`), **`lp-glsl-values`**
  (`GlslValue`), **`lp-glsl-diagnostics`** (`GlslError`), **`lp-glsl-core`**
  (signatures for the trait). **Legacy:** **`lp-glsl-cranelift`** still holds the
  old trait/value definitions until rewired or removed.
- **V2 target:** **`WasmExecutable`** implements **`lp_glsl_exec::GlslExecutable`**;
  **`compile_for_target`** dispatches Wasm / Jit / Rv32 (no Cranelift).

### `lpir-cranelift`

- **`jit`**, **`JitModule`**, **`call`**, **`CompileOptions`** — Stage IV.
- Stage V1: object + link + emulator for **`rv32.q32`**.

## Questions

### Q1: Default target set during transition

**Context:** Speed vs coverage for local runs.

**Answer:** **`DEFAULT_TARGETS = [jit.q32]` only.** **CI** runs **`jit.q32`**,
**`wasm.q32`**, and **`rv32.q32`**. Easy to change later.

### Q2: Where do `GlslExecutable` adapters live?

**Context:** Filetests must not depend on **`lp-glsl-cranelift`** after legacy
removal.

**Answer:** **Dedicated crates:** **`lp-glsl-exec`** (trait), **`lp-glsl-values`**
(value types), plus **`lp-glsl-diagnostics`** / **`lp-glsl-core`** as needed.
Adapters stay in **`lp-glsl-filetests`** (`lpir_jit_executable`,
`lpir_rv32_executable`); **`lp-glsl-wasm`** implements the trait without the old
compiler crate. **No hoist into `lp-glsl-frontend`** for V2—frontend stays as-is
until a later deprecation pass consolidates or deletes duplicates.

### Q3: Annotation `backend=` vocabulary

**Context:** `parse_backend` has `cranelift` / `wasm` today.

**Answer:** Remove **`cranelift`**. Add **`jit`** and **`rv32`**. Keep **`wasm`**.
**Migration:** replace `backend=cranelift` in filetests with **`jit`** or
**`rv32`** per test intent; update parser and tests.

## Answers

_(Same as resolved questions above.)_

## Notes

- **`rv32.q32`** is LPIR → RV32 emulator; it replaces the old **`cranelift.q32`**
  slot in CI for ISA-level coverage, not a third parallel legacy path.
- V1 should land before **`rv32.q32`** is fully green; CI can still run **`jit`**
  + **`wasm`** if **`rv32`** is feature-gated early.
