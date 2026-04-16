# LPIR Inliner — Notes

## Scope

Add a function inlining pass to LPIR that eliminates call overhead for
user-defined functions in shaders compiled via the rv32n native backend.

Requires a new `Block`/`EndBlock`/`ExitBlock` structured forward-jump construct
in LPIR to handle multi-return callees without fake-loop overhead.

## Milestones

| # | Name | Doc | Dependency |
|---|------|-----|------------|
| M0 | Stable CalleeRef refactor | [m0](m0-stable-callee-ref.md) | — |
| M1 | OptPass enum + filetest annotations | [m1](m1-optpass-filetests.md) | — |
| M2 | Block/EndBlock/ExitBlock LPIR ops | [m2](m2-block-ops.md) | — |
| M3 | LPIR inlining pass | [m3](m3-inlining-pass.md) | M2 |
| M4 | Wire into native + validation | [m4](m4-wire-and-validate.md) | M1, M3 |
| M5 | Dead function elimination | [m5](m5-dead-func-elim.md) | M0, M4 |

M0, M1, M2 are independent and can be done in parallel. M3 requires M2
(Block ops for multi-return) but NOT M0 (inliner never deletes functions).
M4 requires M1 (OptPass gating) and M3 (the inliner). M5 requires M0
(stable ids for safe deletion) and M4 (inliner wired in so there are dead
functions to eliminate).

## Current state

### Call overhead (rv32n)
- ~18-24 instructions per call for vec3-returning functions (sret path)
- Prologue: ~5 instructions (addi sp, sw fp, sw ra, mv s1, addi fp)
- Epilogue: ~4 instructions (lw ra, lw fp, addi sp, ret)
- sret stores: 3 sw (vec3) / 4 sw (vec4)
- Call site: auipc+jalr + sret setup + result loads
- Plus regalloc clobber saves/restores for caller-saved t-regs

### LPIR structure
- `IrFunction::body` is a flat `Vec<LpirOp>` with structured control flow
- `IfStart`/`LoopStart`/`SwitchStart` carry absolute body indices as offsets
- `CalleeRef(u32)` indexes imports (low) then local functions (high)
- VRegs are per-function namespace; `vreg_types` tracks types
- `vreg_pool` is append-only storage for Call arg/result VReg lists
- `slots` are per-function stack allocations (SlotAddr/Load/Store)
- Offsets are redundant — computed by the builder, can be recomputed from
  structure alone (walk forward with a stack, match End to openers)

### Filetests
- 53 tests under `filetests/function/` covering call semantics
- `call-simple`, `call-nested`, `call-multiple`, `call-order`, `call-return-value`
  are the direct call-graph tests
- `debug/rainbow.glsl` is a real shader with many small helper calls
- One compile per file per target; no per-test compile flag mechanism today
- `NativeCompileOptions` has float_mode, debug_info, emu_trace, alloc_trace
- Env var pattern exists: `LPVM_ALLOC_TRACE=1` → option field
- No general "compile option annotation" in the test file format

### What can't be inlined
- Entry points (`is_entry`)
- Imports / builtins (no body)
- Functions called indirectly (not applicable for GLSL)

## Questions

### Q1: Should inlining be LPIR-level or native-backend-specific?

**Answer:** LPIR level. The pass lives in the `lpir` crate, operates on
`IrFunction`/`LpirModule`. Only wired into `lpvm-native`'s compile_module
initially. Other backends can opt in later.

### Q2: How to handle filetests?

**Answer:** Add a `// @config(key, value)` annotation to the filetest
format. All optimizations are always in the pipeline — they control
themselves via their own config structs.

- `// @config(inline.mode, never)` — call-semantics tests
- `// @config(inline.mode, always)` — inliner correctness tests
- No annotation — defaults (Auto mode with heuristic)

No `@disable` annotation. No `OptPass` enum. Each pass has a config
struct (e.g. `InlineConfig`) with a mode field. The `CompilerConfig`
aggregates all pass configs.

### Q3: Should Block/EndBlock/ExitBlock be added to all LPIR consumers?

**Answer:** Yes. All consumers: printer, parser, validator, interpreter,
builder, and all backend lowerers (native, cranelift, wasm). Block is
structurally simpler than Loop — no back-edge, no continuing block.

### Q4: Dead function elimination and CalleeRef refactor?

**Answer:** Two separate concerns:

1. **CalleeRef refactor** (M0): Replace the flat `CalleeRef(u32)` index with
   a typed enum (`Import(ImportId)` / `Local(FuncId)`). Stable ids make
   function deletion safe without renumbering.

2. **DeadFuncElim** is a separate pass from inlining. The inliner never
   deletes functions — it just replaces Call ops with inlined bodies.
   DeadFuncElim runs afterward, removes functions with zero remaining local
   call sites that aren't in the provided root set.

   - Filetests: prune OFF (all functions callable by harness).
   - Production: prune ON with roots = shader entry point(s).

### Q5: Inlining heuristic — what to inline?

**Answer:** The inliner doesn't need to know about entry points or roots.
It works bottom-up from leaf functions:

1. Find leaves (no local calls in body).
2. Inline each leaf into all callers.
3. Callers may become new leaves. Repeat.

Inline everything. No size threshold for V1. Can add one later.
The inliner never deletes functions — pruning is separate.

### Q6: vmctx handling during inlining?

**Answer:** Map callee vmctx vreg to caller vmctx vreg. Remap all other
callee vregs to fresh indices in the caller's namespace.

## Notes

- User noted that offsets are redundant and can be recomputed after inlining.
  This simplifies the implementation significantly — no need to track offset
  shifts during splicing.
- User strongly prefers Block/EndBlock/ExitBlock over fake-loop wrapper for
  multi-return handling, to avoid loop-related regalloc overhead.
- User's mental model: functions in shaders are "for author sanity" — inlining
  is the expected compilation strategy.
