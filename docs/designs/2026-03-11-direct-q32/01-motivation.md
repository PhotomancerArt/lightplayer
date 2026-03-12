# Motivation: Direct Numeric Emission

## Where we are

The compiler works. Shaders run on the ESP32. The Q32 fixed-point format
produces correct visuals. But we're operating at the edge of what the hardware
can handle — 228 KB peak of a 320 KB heap, leaving ~28% headroom. That's
tight enough that slightly larger programs or recompilation scenarios may
simply not fit.

## The two-module tax

The current architecture parses GLSL, generates float CLIF IR, then runs a
separate Q32 transform pass that rewrites the entire IR into fixed-point.
This requires two cranelift Modules (float + Q32), each costing ~17 KB in
declarations alone. The transform itself clones the full IR graph, remaps
function signatures, pattern-matches every instruction, and produces a second
copy of every function body.

For the batch path, the float module is dropped before compilation, so the
tax is paid in time but not peak memory. For streaming (where the real
savings should come from), both modules must coexist at peak, making the
approach a net regression.

## The transform complexity

The Q32 transform in `backend/transform/q32/` handles:

- 20+ CLIF opcodes explicitly (arithmetic, comparisons, rounding, sqrt,
  conversions, calls, memory ops, boolean ops)
- A fallback path for all other opcodes with type remapping
- Call conversion with builtin function lookup and argument rewriting
- Signature transformation (F32 params/returns → I32)
- Stack slot type remapping
- FuncId remapping across modules
- A CallConversionState for tracking FuncRef/SigRef mappings

This is substantial machinery for what is conceptually: "when the frontend
says float, use fixed-point instead."

## The opportunity

Every float operation the compiler emits originates from a known, small set of
codegen call sites:

- `expr/binary.rs`: `fadd`, `fsub`, `fmul`, `fdiv`, `fcmp`
- `expr/unary.rs`: `fneg`, `fabs`
- `expr/literal.rs`: `f32const`
- `expr/coercion.rs`: `fcvt_from_sint`, `fcvt_to_sint`
- `builtins/common.rs`: `sqrt`, `floor`, `ceil`, `fmin`, `fmax`, plus libcalls
- `builtins/trigonometric.rs`: libcalls for sin, cos, tan, etc.
- `builtins/geometric.rs`: dot, cross, length (composed from the above)
- `codegen/signature.rs`: `types::F32` in signatures

At each of these points, the intent is clear — "add two floats", "compute
sine of an angle." A pluggable numeric strategy could intercept at these
points and emit the appropriate instructions directly, rather than emitting
float IR and rewriting it after the fact.

## What we'd gain

1. **Single module.** No float module, no module duplication. ~17 KB saved
   in streaming, significant simplification in batch.

2. **No IR cloning.** The transform currently copies the entire function
   body instruction by instruction. Direct emission produces the final IR
   on the first pass.

3. **Simpler pipeline.** Parse → codegen → compile. One step fewer. The
   streaming pipeline becomes trivially beneficial.

4. **Extensible.** New numeric formats (f16, different Q formats, software
   float) are new trait implementations, not new IR transform passes.

5. **Faster compilation.** The transform pass is not free — it walks the
   entire IR graph, allocates a new function body, and remaps all
   references. Eliminating it saves both time and memory.

## What we'd lose

1. **Separation of concerns.** The frontend currently knows nothing about
   Q32. The semantic analysis is purely float. With direct emission, the
   codegen layer becomes numeric-format-aware (though through an
   abstraction).

2. **Transform testability.** The Q32 transform can be tested in isolation:
   feed it float IR, check the output. With direct emission, testing is
   at the strategy level (unit-test each operation) and integration level
   (compile a shader, check output).

3. **The existing battle-tested transform.** The current code works. It
   produces correct results. Replacing it is a risk.

## Assessment

The tradeoffs favor direct emission. The "separation of concerns" loss is
mitigated by a clean trait abstraction — the frontend semantic analysis
stays float-only, the codegen routes through a strategy, and the strategy
is independently testable. The existing transform provides a reference
implementation to validate against during migration.

The effort is substantial but incremental. Each phase produces a working
compiler. The transform can coexist with direct emission during the
transition, with a config flag selecting between them.
