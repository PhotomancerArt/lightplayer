# Stage IV: JitModule API, GlslMetadata, and Compiler Orchestration — Notes

## Scope of work

- Public API: `jit(source, CompileOptions)`, `jit_from_ir(ir, CompileOptions)` →
  `Result<JitModule>` (or a richer wrapper type holding JIT + metadata + call
  helpers).
- `compile.rs`: GLSL → Naga → LPIR → Cranelift JIT, optional per-function
  draining for peak memory.
- `GlslMetadata`: per-function GLSL names, param types, **in/out/inout**
  qualifiers, return type — produced alongside LPIR during lowering.
- `values.rs`: `GlslQ32`, `GlslF32`, `GlslReturn`, `CallError`, Level 1 `call()`.
- Level 3: `direct_call()` with flat pointer ABI and struct-return handling.
- Tests: end-to-end GLSL → JIT → typed `call()`.

**Out of scope:** filetest runner (Stage V2), object/emulator (Stage V1),
lp-engine (Stage VI).

## Current state

### `lpir-cranelift`

- `jit_from_ir(ir: &IrModule, mode: FloatMode) -> Result<(JITModule, Vec<FuncId>)>`
  — low-level; callers transmute raw pointers.
- No `CompileOptions`, no `jit()` from source, no `JitModule` struct wrapping
  results.
- No `GlslQ32` / `call()` / `DirectCall`.

### `lps-frontend`

- `compile(source) -> NagaModule` — Naga `Module` + `functions: Vec<(Handle, FunctionInfo)>`.
- `FunctionInfo { name, params: Vec<(String, GlslType)>, return_type }` — **no
  param qualifiers** (in/out/inout). `function_info()` unwraps pointer types to
  pointee for display type only.
- `lower(naga_module) -> IrModule` — single return value; no metadata bundle.
- Lowering already tracks `pointer_args` in `LowerCtx` for out/inout codegen.

### `lpir`

- `IrModule { imports, functions }` — no metadata field.
- `IrFunction` has scalar `param_count`, `vreg_types`, `return_types` only.

### Roadmap additions (emulator)

- Stage V1 owns RV32 object + emulator in-crate; Stage V2 owns `rv32.q32`
  filetests. Stage IV stays host JIT only.

## Questions

### Q1: Where should `GlslMetadata` (and related types) live?

**Context:** Needed by `lpir-cranelift` for `call()`, possibly filetests (Stage
V2), and conceptually describes how to interpret an `IrModule` / `IrFunction`
at the GLSL ABI level.

**Answer:** **`lpir` crate** — main home for IR model and companion metadata.
Add `glsl_metadata.rs` (or similar): `GlslParamQualifier`, `GlslParamMeta`,
`GlslFunctionMeta`, `GlslModuleMeta`. `lps-frontend` produces it during `lower`;
`lpir-cranelift` consumes it for `call()`.

### Q2: How to expose the memory-conscious lowering path vs borrowed IR?

**Context:** Roadmap wants sort-by-size, define, drop each `IrFunction`. Current
`jit_from_ir(&IrModule)` cannot remove functions from a shared reference.

**Answer:** Two entry points: `jit_from_ir(&IrModule)` for tests / borrowed IR
(no drain). `jit_from_ir_owned(IrModule, …)` (or equivalent) **consumes** the
module and produces the JIT executable — sort by size, define each function,
drop each `IrFunction` as you go. `jit(source)` uses the owned path after
lowering.

### Q3: `DirectCall` shape — trampoline vs raw ABI?

**Context:** Roadmap wants `call(args: *const u32, results: *mut u32)`. Cranelift
may use struct-return, different reg counts per platform.

**Answer:** **Rust-side trampoline** (or `lps-jit-util`-style helpers) per
function signature — handles struct-return / ABI inside; callers see flat
`u32` buffers. No JIT-generated trampoline in Stage IV unless needed later.

### Q4: Source locations in errors?

**Context:** Naga parse errors have locations; lowering could attach span info
later. User asked for at least **function-level** context so errors are not
orphaned.

**Answer:** **Function-scoped errors in Stage IV** — modest effort, no LPIR
span redesign.

**LPIR today:** No per-op source spans. LPIR text `ParseError` has line/column
for parse failures only.

**Without IR changes:**

- **Lowering:** `lower_function` already has the GLSL name. Wrap errors at that
  boundary (one place in `lower.rs`), e.g. prefix or `LowerError::InFunction {
  name, inner }`, so messages read `in function 'foo': …`.
- **Cranelift:** Ensure every per-function failure includes `IrFunction.name`
  (some paths already do in `declare`/`define`).
- **Naga parse:** Keep `emit_to_string(source)` — already line-oriented text.

**Harder (later):** Map Naga handles to GLSL line/column — separate investigation.

Full expression-level sourcelocs through LPIR remain **out of scope** for Stage IV.

**User note:** Ship **function name** on lowering + emission errors now; richer
lowering diagnostics (handles → span) can wait. Most failures are expected in the
frontend (Naga parse); emission errors should be rare once the pipeline is stable.

## Notes

- `FunctionInfo` must gain **per-parameter qualifier** (In / Out / InOut) from
  Naga `Function` arguments (binding / pointer address space).
- `lower()` should return `(IrModule, GlslModuleMeta)` or attach metadata into
  `IrModule` via a new optional field — prefer separate return to avoid growing
  `IrModule` for hand-written IR that has no GLSL metadata.
