# LPIR → Cranelift: Hardware Validation

## Motivation

The LPIR roadmap (2026-03-21) established a shared middle-end IR between
the Naga GLSL frontend and target backends. The WASM backend already
consumes LPIR. The Cranelift backend still uses the original AST→CLIF
path — a ~6000-line crate that grew organically as a first compiler, first
Rust project, and first Cranelift integration.

This effort builds the second LPIR consumer: a new Cranelift backend from
scratch. The primary goal is **hardware validation** — proving the LPIR
approach doesn't regress binary size, memory, or execution speed on ESP32.

Secondary goals:

1. **Clean compiler API**: Replace the sprawling `GlslCompiler` /
   `GlslExecutable` / `GlslOptions` surface with a minimal, considered
   API designed around two execution levels (typed test calls and
   convention-abstracted direct calls).

2. **Unified builtin naming**: Establish `__lp_<module>_<fn>_<mode>` as the
   single naming convention across all builtins, replacing the current
   three-system alignment-by-convention.

3. **Host JIT filetests**: Add a fast `jit.q32` filetest target that runs
   on the host CPU, complementing the slower RISC-V emulator path.

4. **Idiomatic Cranelift usage**: Auto struct-return for multi-return,
   proper variable-based SSA construction, no manual calling convention
   hacks.

## Architecture

### Pipeline

```
GLSL source
  │
  ▼
Naga frontend (parse + type check)            ── existing, unchanged
  │
  ▼
naga::Module                                   ── existing, unchanged
  │
  ▼  lp-glsl-naga  ── Naga → LPIR lowering    ── existing, shared with WASM
  │   • scalarizes vectors
  │   • decomposes builtins (smoothstep → scalar math)
  │   • handles LPFX calls and out-pointer ABI
  │   • resolves to glsl:: and lpfx:: imports
  │   • extracts GlslMetadata (typed params, qualifiers, return types)
  │
  ▼
IrModule + GlslMetadata                        ── LPIR (float-agnostic)
  │
  │  (Naga Module dropped — no longer needed)
  │
  ┌──────────┴──────────┐
  ▼                     ▼
WASM emitter            NEW: LPIR → CLIF emitter
(lp-glsl-wasm)          (lpir-cranelift)
  │ Q32: inline i64       │ Q32: builtin calls (__lp_lpir_*_q32)
  │ glsl/lpfx: WASM       │ glsl/lpfx: Cranelift func refs
  │   imports              │   via BuiltinId
  ▼                     ▼
.wasm bytes             machine code
                        (host JIT or RV32 object)
```

### Memory-conscious compilation

On ESP32, memory during compilation is a critical constraint. The pipeline
frees intermediate representations as early as possible:

```
Step 1:  GLSL → Naga Module
Step 2:  Naga Module → IrModule + GlslMetadata
Step 3:  Drop Naga Module
Step 4:  Lower IrFunctions → CLIF one at a time, biggest first
         After each function: define in Cranelift module, drop IrFunction
Step 5:  Finalize Cranelift module → JitModule + GlslMetadata
```

Peak memory is LPIR-of-biggest-function + CLIF-of-biggest-function. By
processing the biggest function first, this peak occurs when the Cranelift
module has accumulated the least defined code.

`GlslMetadata` — function names, GLSL-typed parameters with in/out/inout
qualifiers, return types — is extracted during Naga→LPIR lowering while
Naga type information is available, and survives into the final JitModule.
This metadata powers the Level 1 (typed) call interface. It must be part of
the design from the start.

### Crate structure

```
lp-glsl/
├── lpir/                      # LPIR core (existing, unchanged)
├── lp-glsl-naga/              # Naga → LPIR lowering (existing)
│                              #   UPDATE: extract GlslMetadata during lowering
├── lp-glsl-wasm/              # LPIR → WASM (existing, unchanged)
├── lp-glsl-cranelift/         # OLD: AST → CLIF (abandoned, deleted at end)
├── lpir-cranelift/            # NEW: LPIR → CLIF → JIT/object
│   └── src/
│       ├── lib.rs             #   Public API: jit(), jit_from_ir(), CompileOptions
│       ├── compile.rs         #   GLSL → Naga → LPIR → CLIF orchestration
│       ├── emit.rs            #   LPIR → CLIF translation (block stack, ops)
│       ├── builtins.rs        #   BuiltinId declaration + import resolution
│       ├── module.rs          #   JitModule, DirectCall
│       ├── values.rs          #   GlslQ32, GlslF32, GlslReturn, CallResult
│       └── error.rs           #   CompileError, CallError
├── lp-glsl-builtin-ids/       # UPDATE: new naming, self-describing BuiltinId
├── lp-glsl-builtins/          # UPDATE: rename symbols to __lp_<module>_<fn>_<mode>
└── lp-glsl-filetests/         # UPDATE: Stage V2 — jit.q32 + rv32.q32 targets
```

### Builtin naming convention

```
Symbol:    __lp_<module>_<fn>_<mode>
LPIR:      <module>::<fn>           (mode added by emitter)
File:      lp-builtins/src/lp/<module>/<fn>_<mode>.rs
GLSL code: <fn>                     (module implied)

Modules:
  lpir  — IR ops needing library impl (fdiv, sqrt, ftoi_sat, itof)
  glsl  — GLSL built-in functions (sin, cos, smoothstep, mix, pow)
  lpfx  — LightPlayer effects (fbm, snoise, hash, psrdnoise)

Mode suffix:
  _q32  — Q32 fixed-point representation
  _f32  — native IEEE 754 float
  (none) — mode-independent (integer-only, e.g. hash)

Examples:
  __lp_lpir_fdiv_q32        LPIR fdiv op, Q32 implementation
  __lp_glsl_sin_q32         GLSL sin(), Q32 implementation
  __lp_lpfx_fbm2_q32        LPFX fbm2, Q32 implementation
  __lp_lpfx_hash11          LPFX hash11, mode-independent
```

BuiltinId becomes self-describing: given (module, name, mode) it derives
symbol name, LPIR import path, file path, and GLSL name. Replaces the
current generated flat enum where `LpQ32Sin` → `"__lp_q32_sin"` is a
convention-only mapping.

### Compiler API

```
Compilation:
  jit(source, options)       → Result<JitModule>     GLSL string in
  jit_from_ir(ir, options)   → Result<JitModule>     LPIR module in

CompileOptions:
  float_mode: FloatMode      Q32 (primary) or F32 (future)

Execution:

  Level 1 — typed, mode-aware (tests):
    module.call("main", &[GlslQ32::Vec2(0.5, 0.3), ...])
      → CallResult<GlslQ32>
      → GlslReturn { value: Option<GlslQ32>, outs: Vec<GlslQ32> }

    All params passed flat; module knows in/out/inout from GlslMetadata.
    Returns: .value is the return value, .outs are out/inout params positional.
    Module handles scalarization, Q32 encoding/decoding, calling convention.

  Level 3 — direct, convention-abstracted (engine hot path):
    module.direct_call("main") → Option<DirectCall>
    DirectCall.call(args: *const u32, results: *mut u32)

    Caller handles scalarization and Q32 encoding.
    Calling convention (struct-return, registers) is abstracted.

GlslQ32 variants:
  Float(f64), Vec2(f64, f64), Vec3(..), Vec4(..)   f64 for Q32 precision
  Int(i32), IVec2(i32, i32), IVec3(..), IVec4(..)
  UInt(u32), ...
```

JitModule is a concrete struct, not `dyn Trait`. Object/emulator path
(future) is a separate type.

### LPIR → CLIF translation

Single-pass block stack, same approach as wasmtime's WASM→CLIF translation:

- `IfStart` → branch to then/else blocks, push merge block
- `LoopStart` → push header/exit blocks, jump to header
- `Break` → jump to exit block
- `Continue` → jump to header block
- `BrIfNot` → conditional branch to exit block
- `End` → pop stack, jump to merge/header, switch to next block

Each LPIR VReg maps to one Cranelift `Variable`. `def_var`/`use_var` handles
SSA construction (block params / phi nodes) automatically.

Q32 float ops (`fadd`, `fmul`, etc.) → calls to `__lp_lpir_<op>_q32`
builtins. LPIR `glsl::sin` import → call to `__lp_glsl_sin_q32`. All
resolved via BuiltinId → Cranelift func ref.

### Filetest targets

```
wasm.q32     LPIR → WASM → wasmtime           existing
jit.q32      LPIR → CLIF → host CPU            Stage V2 (default local target)
rv32.q32     LPIR → CLIF → RV32 → emulator     Stage V2 (+ CI)
```

**Stage V2** removes the legacy **`cranelift.q32`** (old AST → RV32) from
filetests; **`DEFAULT_TARGETS`** is **`jit.q32` only** for speed — **CI** runs
**`wasm.q32`** and **`rv32.q32`** as well.

Future (not this roadmap): `lpir.q32` (LPIR interpreter), `clif.q32`
(Cranelift interpreter).

### Migration path

1. **Stage V1:** RV32 object + builtins link + emulator **inside
   `lpir-cranelift`** (in-crate tests)
2. **Stage V2:** Filetests — `jit.q32` (host) and `rv32.q32` (emulator) both use
   `lpir-cranelift`
3. Switch lp-engine from `lp-glsl-cranelift` to new crate (clean swap,
   no feature flags)
4. Validate on ESP32 via fw-esp32
5. A/B compare against old compiler on main via git worktree
6. Delete old `lp-glsl-cranelift` and `lp-glsl-frontend`

## Alternatives considered

**Integrate LPIR path into existing `lp-glsl-cranelift`**: The crate's
backend infrastructure (GlModule, GlslExecutable) is tightly coupled to
the AST pipeline. Adding LPIR alongside would grow the crate without
improving it. Since the AST path will be deleted, the shared-crate
complexity is all cost and no lasting benefit.

**Feature-flag A/B switching in lp-engine**: Conditional compilation for
a temporary migration state creates ongoing tax. Git worktree comparison
against main is cleaner.

**Shared executable trait between old and new crates**: Premature. The old
`GlslExecutable` was shaped by the AST pipeline. The new crate should
design its interface for LPIR's needs. Forced compatibility constrains
both without benefiting either.

## Risks

- **Backend infrastructure reimplementation**: ~500 lines of module
  management, builtin declaration, JIT finalization, and executable
  interface. Straightforward Cranelift API usage, not algorithmically
  complex.

- **Builtin rename scope**: The `__lp_<module>_<fn>_<mode>` rename touches
  `lp-glsl-builtin-ids`, `lp-glsl-builtins`, WASM import resolution, the
  builtins generator, and test code. Mechanical but wide. Mitigated by
  doing it as a dedicated stage.

- **LPIR coverage gaps**: The Naga→LPIR lowering may not cover all
  constructs the target shaders use. Gaps discovered during Cranelift
  integration need fixes in `lp-glsl-naga`, not workarounds.

- **Performance validation ambiguity**: If LPIR-path binaries are larger
  or slower, attributing the cause (LPIR overhead? different CLIF shape?
  missing optimizations?) may be hard. Mitigation: compare CLIF output
  between old and new paths for the same shader.

- **Memory during compilation**: The whole IrModule must exist
  simultaneously (cross-function references during lowering). Per-function
  CLIF lowering + drop mitigates this, but a very large shader could still
  pressure ESP32 memory. Monitoring compilation memory is important during
  hardware validation.

- **GlslMetadata extraction**: The Naga→LPIR lowering must extract
  GLSL-level type information (vec2, out vec3, etc.) before the Naga module
  is dropped. If this metadata is incomplete or wrong, the Level 1 call
  interface breaks. Must be designed into the lowering from the start.
