# LPIR / Naga compiler — feature parity and old-stack removal — Notes

## Scope

Bring the **Naga → LPIR → (Cranelift / WASM / RV32 emu)** pipeline to **feature parity** with the
legacy **TypedShader → `lp-glsl-cranelift`** stack, then **retire the old compiler** from product
and validation paths.

In scope:

- Closing GLSL/language gaps surfaced by filetests (especially `jit.q32` vs what already passes on
  `rv32.q32` / `wasm.q32` where the issue is lowering/metadata, not ABI).
- Wiring remaining **callers** that still depend on `lp-glsl-cranelift` onto `lpir-cranelift` (or an
  agreed subset).
- Host metadata / `JitModule::call` / invoke paths for aggregates (matrices, eventually
  arrays/structs) as needed for parity.
- Correctness fixes (e.g. postfix inc/dec on vector components) and harness stability (filetest
  threading flake).

Out of scope until decided otherwise (see questions):

- SPIR-V / WGSL frontends (Naga enables them later; not required for “replace old GLSL compiler”).
- Optional tools that exist only for the old IR (`lp-glsl-q32-metrics-app` today).

## Current state of the codebase

### New stack (Naga / LPIR)

- **`lp-glsl-naga`**: GLSL-in → `naga::Module` → LPIR + `GlslModuleMeta`.
- **`lpir-cranelift`**: LPIR → CLIF → host JIT (`jit()`), optional RV32 object / emu (`riscv32-emu`
  feature). Uses `lp-glsl-naga` behind `std` for the full GLSL entry.
- **`lp-glsl-wasm`**: Naga → LPIR → WASM (Stage V path).
- **`lp-glsl-filetests`**: `compile_for_target` routes **Jit**, **Rv32**, and **Wasm** through the
  LPIR executables (`LpirJitExecutable`, `LpirRv32Executable`, `WasmExecutable`) — not
  `lp-glsl-cranelift`.
- **`lp-engine`** (`ShaderRuntime`, `std`): compiles shaders via `lpir_cranelift::jit` and
  `CompileOptions` / `DirectCall` (in progress per stage VI plans).

### Legacy stack (still in tree)

- **`lp-glsl-cranelift`**: `TypedShader` frontend path, `glsl_jit` / `glsl_jit_streaming`, large
  CLIF emitter.
- **`lp-glsl-frontend`** + **`glsl-parser`**: feed the legacy Cranelift crate.
- **Still depending on `lp-glsl-cranelift`** (workspace `Cargo.toml` edges):
    - `lp-shader/esp32-glsl-jit`
    - `lp-fw/fw-esp32` (optional feature)
    - `lp-shader/lp-glsl-q32-metrics-app`

### Feature gaps (backlog detail)

Captured in [`todo.md`](todo.md): matrices, arrays, structs in Naga→LPIR lowering; vector relational
builtins (`isnan`, `isinf`); `GlslType` / invoke limits for multi-word returns; postfix component
inc/dec semantics; possible filetest harness concurrency issue.

### Related roadmaps

- [`2026-03-20-naga`](../2026-03-20-naga/overview.md) — motivation for Naga + WASM rewrite.
- [`2026-03-24-lpir-cranelift`](../2026-03-24-lpir-cranelift/overview.md) — LPIR→Cranelift backend,
  `jit.q32` goal.

## Questions

### Q1 — Definition of “feature parity”

**Context:** Filetests already exercise the LPIR path on three backends. Many failures are explicit
“unsupported” in lowering rather than silent miscompilation. Legacy `cranelift.q32` may no longer be
the default filetest target; parity might mean “`jit.q32` + `rv32.q32` + `wasm.q32` match
expectations” rather than “bit-identical to old JIT.”

**Suggested answer:** Treat **filetest pass parity** on the three LPIR targets (same expectations
files) as the bar for language/builtins coverage, plus **explicit checklist** for any behavior not
covered by filetests (e.g. engine-only LPFX hooks, ESP32 object layout).

**Answer:** _(pending)_

---

### Q2 — Order: language gaps vs. caller migration

**Context:** `lp-engine` and filetests already use the new path. `fw-esp32` / `esp32-glsl-jit` /
metrics app still use `lp-glsl-cranelift`.

**Suggested answer:** Prioritize **language + metadata + invoke** gaps that block filetests (matches
`todo.md` order: postfix fix → matrices + ABI → vector builtins → arrays → structs), then **migrate
embedded callers** to `lpir-cranelift` object/JIT API, then delete legacy crates.

**Answer:** _(pending)_

---

### Q3 — When to remove `lp-glsl-cranelift` (and frontend/parser)

**Context:** Removing the old crate reduces maintenance but drops a comparison baseline. Some tests
and examples live only under `lp-glsl-cranelift`.

**Suggested answer:** Remove when no workspace crate depends on it **and** filetest parity (Q1) is
met; port or drop examples/tests; keep a **git tag** or doc pointer to the last revision with the
old stack if regression comparison is needed.

**Answer:** _(pending)_

---

### Q4 — `lp-glsl-q32-metrics-app` and other dev-only tools

**Context:** The app imports `GlslCompiler` / `GlModule` from the legacy crate.

**Suggested answer:** Either **rewrite** the app on `lpir-cranelift` + LPIR introspection for
metrics, or **archive** the tool until needed; avoid indefinite dual maintenance.

**Answer:** _(pending)_

---

### Q5 — Filetest harness concurrency

**Context:** Report that `LP_FILETESTS_THREADS=1` sometimes shows widespread false failures; needs
repro.

**Suggested answer:** Schedule a **small dedicated stage** or bugfix track: reproduce, fix
isolation/accounting, document the env var in roadmap validation stage.

**Answer:** _(pending)_

---

## Notes

_(Iteration notes from user answers.)_
