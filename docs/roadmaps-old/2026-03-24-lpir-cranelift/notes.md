# Notes

## Scope

Build an LPIR → Cranelift lowering path that validates the LPIR approach
on real hardware (ESP32). Produce machine code from LPIR, run it through
filetests and on firmware, and compare binary size / memory / speed against
the existing direct GLSL→Cranelift path.

## Purpose

The LPIR roadmap (2026-03-21) established a shared middle-end between Naga
and target backends. The WASM backend is already consuming LPIR. This effort
builds the second consumer: Cranelift. The primary goal is **validation** —
proving the LPIR approach doesn't regress on embedded targets — not full
feature parity with the existing Cranelift backend.

## Current state

### What exists

- **lpir crate**: Complete — Op enum, IrFunction, IrModule, builder,
  text printer, parser, interpreter, validator.
- **Naga → LPIR lowering** (`lps-naga/src/lower.rs`): Handles scalar
  expressions, control flow, calls, LPFX, std.math. Enough to run the
  example shader.
- **LPIR → WASM emission** (`lps-wasm`): Working, consumes LPIR modules.
- **Filetests**: `cranelift.q32` and `wasm.q32` targets. WASM target uses
  the Naga→LPIR→WASM path. Cranelift target uses the direct AST→CLIF path.

### What the existing Cranelift backend looks like

- **`lps-cranelift`**: ~6000+ lines. Parses GLSL via `lps-frontend`,
  walks the typed AST, emits CLIF directly. Handles Q32 inline via
  `NumericMode` dispatch. Manages `GlModule<M: Module>` with JIT and
  object/emulator backends.
- **Backend infrastructure** (reusable): `GlModule`, `GlFunc`,
  `GlslJitModule`, `GlslEmulatorModule`, `GlslExecutable` trait,
  `SignatureBuilder`, target/ISA creation, builtins declaration/linking,
  host functions.
- **Frontend/codegen** (to be replaced): `CodegenContext`, `expr/`, `stmt/`,
  `lvalue/`, `builtins/` — the AST→CLIF translation, ~3500 lines.

### Builtin system

- **`BuiltinId`** enum (generated): stable C symbol names for Q32 math
  and LPFX. Used by Cranelift for `declare_builtins` + JIT symbol lookup.
- **LPIR imports**: `std.math` module (GLSL names: "sin", "cos", etc.) and
  `lpfx` module (mangled names with Naga handle indices). String-based,
  resolved at emission time.
- **WASM path**: LPIR `std.math` → `glsl_q32_math_builtin_id` → `BuiltinId`
  → WASM import with `BuiltinId::name()`.
- **Cranelift path**: Q32 math → `BuiltinId` directly. Float math → testcase
  names (`sinf`, `atan2f`). LPFX → `BuiltinId` via registry.
- **Interpreter**: `ImportHandler` trait, string-based dispatch. `StdMathHandler`
  uses libm. No built-in LPFX dispatch.

### How lp-engine uses the compiler

- `ShaderRuntime` calls `glsl_jit_streaming` → `Box<dyn GlslExecutable>`.
- Fast render path uses `get_direct_call_info` → raw function pointer +
  `CallConv` + `pointer_type` (from `cranelift_codegen`).
- Engine imports `cranelift_codegen::isa::CallConv` and
  `cranelift_codegen::ir::Type` directly.
- ESP32 firmware reaches this through `lp-server` → `lp-engine`.

## Questions

### 1. Crate structure: new crate or integrate into `lps-cranelift`?

**Answer**: New standalone crate. Clean slate, no compatibility constraints
with the old compiler. The old `lps-cranelift` is effectively abandoned
on this branch. We copy what we need (builtin declaration patterns, ISA
creation), but design fresh types and APIs.

Rationale: the duplication is temporary and bounded. Shared crates
(`lps-builtin-ids`, `lps-builtins`, `lps-jit-util`, `lp-model`)
are already separate. The backend infra that looks big in the old crate
(GlModule, SignatureBuilder, etc.) is mostly managing AST-specific
complexity that LPIR doesn't have. The new crate will be simpler.

Migration plan: filetests first (correctness validation), then lp-core /
firmware, then delete the old crate. A/B comparison against main via
git worktree.

Additional decisions:

- **Host JIT first** for filetests (much faster than emulator path).
  Add host-jit as a filetest target — this is independently useful.
  ESP32/emulator path comes later for firmware integration.
- **Multi-return**: Use idiomatic Cranelift auto struct-return instead of
  the manual StructReturn hack in the old compiler. LPIR already models
  multi-return cleanly.
- **Executable interface**: Design new trait for the new crate. Don't try
  to match `GlslExecutable` exactly — shape it for the LPIR pipeline's
  needs. Migrate consumers when they switch crates.

### 2. Structured control flow → Cranelift CFG translation strategy

**Answer**: Single-pass block stack. LPIR was designed with this in mind.
IfStart/Else/End → then/else/merge blocks. LoopStart/End → header/exit
blocks. Break/Continue → jumps to exit/header. BrIfNot → conditional
branch. VRegs map 1:1 to Cranelift Variables, `def_var`/`use_var` handles
SSA construction automatically.

### 3 & 4. Builtin naming, modules, and import resolution

**Answer**: Unified naming convention for all builtins:

    __lp_<module>_<fn>_<mode>

Three modules:

- `lpir` — ops the IR has opcodes for but need library impl for some
  modes (fdiv, sqrt, ftoi_sat, itof). NOT LPIR imports — emitter-internal.
- `glsl` — GLSL built-in functions (SPIR-V GLSL.std.450: sin, cos,
  smoothstep, mix, pow, etc.). LPIR imports as `glsl::sin`, `glsl::cos`.
- `lpfx` — LightPlayer effects (fbm, snoise, hash, etc.). LPIR imports
  as `lpfx::fbm2`, `lpfx::psrdnoise`.

Mode suffix: `_q32` / `_f32` for functions that operate on float values.
No suffix for mode-independent functions (integer-only, like hash).

The convention renders consistently across all contexts:

- **ELF symbol**: `__lps_sin_q32`
- **LPIR import**: `glsl::sin` (mode is emitter's concern, not IR's)
- **File path**: `lp-builtins/src/lp/glsl/sin_q32.rs`
- **GLSL code**: `sin` (module implied by language)

Hierarchy: mode is below function. `sin_q32` and `sin_f32` are neighbors
(same math, different representation). `cos_q32` is a different function.

`BuiltinId` becomes self-describing: given (module, name, mode) it derives
all four forms. This replaces the current generated flat enum where
`LpQ32Sin` → `"__lp_q32_sin"` is a convention-only mapping.

The rename is mechanical — a dedicated (short) stage of this roadmap.

Import resolution: LPIR `ImportDecl` with `module_name = "glsl"`,
`func_name = "sin"` → `BuiltinId { module: Glsl, name: "sin" }` →
emitter adds mode → `__lps_sin_q32`. This resolution logic lives in
`lps-builtin-ids` (shared by WASM and Cranelift emitters). Each
emitter maps `BuiltinId` → target-specific linkage (WASM import string
vs Cranelift func ref).

### 5. Float mode handling in the LPIR→Cranelift emitter

**Answer**: Q32 saturating is the primary target — it's what the device
runs and what host JIT filetests should emulate. Native f32 support can
be stubbed / held space for if easy, but is not a deliverable. Q32
wrapping is out of scope.

Q32 saturating: LPIR float ops (`fadd`, `fmul`, etc.) → calls to
`__lp_lpir_<op>_q32` builtins. LPIR `glsl::sin` import → call to
`__lps_sin_q32`. Structurally straightforward — the emitter maps
LPIR ops to function calls, not inline instruction sequences.

### 6. Feature gating strategy for lp-engine

**Answer**: No feature flags. Clean switch. On this branch, lp-engine
swaps its dependency from `lps-cranelift` to the new crate and uses
the new API directly. A/B comparison against the old compiler is done
via git worktree on main. Feature flags would create messy conditional
compilation for a temporary migration state.

### 7. What to do about `DirectCallInfo` and `cranelift_codegen` types in lp-engine

**Answer**: Leave it. The new crate still produces Cranelift JIT output —
same `CallConv`, same `pointer_type`. The coupling is real but not blocking.
Abstract later if we ever have a non-Cranelift native backend.

### 8. Filetest target naming

**Answer**: Name targets by where the code runs, not how it got there.

Final state target names:

- `wasm.q32` — LPIR → WASM → wasmtime (existing)
- `jit.q32` — LPIR → CLIF → machine code → host CPU (new, primary)
- `rv32.q32` — LPIR → CLIF → RV32 object → RISC-V emulator (replaces
  `cranelift.q32`)

Future targets (not in this roadmap):

- `lpir.q32` — LPIR interpreter
- `clif.q32` — Cranelift interpreter

Short, obvious, uniform `.q32`/`.f32` suffix. Old `cranelift.q32` gets
removed when the LPIR pipeline replaces it.

### 9. Scope of language support

**Answer**: Match WASM emitter coverage exactly:

- All scalar LPIR ops (arithmetic, comparison, casts, constants, select, copy)
- Structured CF → Cranelift CFG (if/loop/break/continue/return)
- `glsl::*` import calls
- `lpfx::*` import calls
- Memory (slots, load/store, memcpy)
- Multi-return (idiomatic Cranelift auto struct-return)

Out of scope: vectors, switch (not yet used), optimizations beyond
what Cranelift provides natively.

### 10. Compiler API surface

**Answer**: Clean slate API for the new crate. Major simplification.

**Compilation**:

- `jit(source: &str, options: CompileOptions) -> Result<JitModule>`
- `jit_from_ir(ir: &IrModule, options: CompileOptions) -> Result<JitModule>`
- No `GlslCompiler` struct, no streaming, no `RunMode` enum.

**Execution — two levels**:

Level 1 — typed, mode-aware (tests/filetests):

- `module.call("main", &[GlslQ32::Vec2(0.5, 0.3), ...]) -> CallResult<GlslQ32>`
- Returns `GlslReturn<GlslQ32>` with `.value: Option<GlslQ32>` (return value)
  and `.outs: Vec<GlslQ32>` (out/inout params, positional).
- Module handles scalarization, Q32 encoding/decoding, calling convention.
- All params passed flat; module knows qualifiers from signature metadata.
- `GlslQ32` enum: `Float(f64)`, `Vec2(f64, f64)`, `Vec3(...)`, `Vec4(...)`,
  `Int(i32)`, `IVec2(...)`, etc. f64 for Q32 round-trip precision.
- `CallError` covers fuel, emulator crash, type mismatch, etc.

Level 3 — direct call, convention-abstracted (engine hot path):

- `module.direct_call("main") -> Option<DirectCall>`
- `DirectCall` provides `call(args: *const u32, results: *mut u32)`.
- Calling convention (struct-return, registers) is hidden.
- Caller handles scalarization and Q32 encoding (knows the layout).

Level 2 (mode-agnostic high-level) deferred — build when needed.

**No trait**: `JitModule` is a concrete struct, not `dyn GlslExecutable`.
Object/emulator path (future) is a separate type, not unified behind a trait
until we know the real needs.

### 11. Memory pressure (embedded architectural constraint)

**Answer**: On ESP32, memory during compilation is a critical constraint.
The pipeline must free intermediate representations as soon as possible:

1. Parse GLSL → Naga Module
2. Lower Naga → IrModule (full module, needs cross-function context)
3. **Drop Naga Module** — no longer needed
4. Lower IrFunctions → CLIF **one at a time, biggest first** — peak memory
   is LPIR-of-biggest-fn + CLIF-of-biggest-fn. After each function is
   defined in the Cranelift module, **drop that IrFunction**.
5. Finalize Cranelift module

Key implication: **GLSL-level type metadata must be extracted and stored
independently** before LPIR functions are dropped. The JitModule needs to
know "main takes (vec2, vec2, float) returns vec4" for the Level 1 call
interface, but the IrFunction only has scalar param_types [F32, F32, F32,
F32, F32] and return_types [F32, F32, F32, F32]. The GLSL grouping must
be captured during Naga→LPIR lowering and preserved as lightweight metadata
alongside the compiled module.

This metadata (function name, GLSL-typed params with qualifiers, return
type) should be part of the design from the start — not retrofitted later.
We had to go back and add it in the old compiler.

IrModule should support per-function ownership transfer or draining so
individual functions can be dropped after lowering to CLIF.
