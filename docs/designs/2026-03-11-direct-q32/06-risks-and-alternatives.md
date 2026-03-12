# Risks and Alternatives

## Risks

### Correctness parity

The Q32 transform has been validated against real shader output on the ESP32.
Direct emission must produce identical results. The primary mitigation is
cross-validation: run both paths on the same inputs, compare bit-for-bit.

The risk is highest for edge cases in the Q32 math (overflow handling in
multiply, rounding behavior in division, comparison semantics for NaN-like
sentinel values). These are all already handled in the transform code —
the risk is in the re-expression, not the math.

### Scope creep in the trait surface

The initial trait covers ~15 methods. As more operations are added (e.g.
fused multiply-add, specialized vector operations), the trait may grow.
Mitigation: keep the trait focused on scalar operations. Vector/matrix
operations compose from scalars and don't need trait methods.

If the trait grows beyond ~20 methods, consider splitting into sub-traits
(NumericArithmetic, NumericComparison, etc.) or using a single
`emit_operation(op: NumericOp, args: &[Value])` dispatch method.

### Generic parameter propagation

If using a generic parameter for the strategy (`CodegenContext<'a, M, N>`),
the `N` parameter propagates through every function that takes a
CodegenContext. This is ~50 functions in the codegen layer.

Mitigation: use the enum approach (`NumericMode { Float, Q32 }`) instead
of generics. The branch prediction cost is negligible. Or use `&dyn
NumericStrategy` for trait-object dispatch.

### Testing without the transform

The Q32 transform provides a convenient testing mechanism: feed it known
float IR, check the output. Without the transform, testing is at two
levels:

1. Unit tests for each Q32Strategy method (given inputs, check emitted
   instructions)
2. Integration tests (compile shader, run, check output values)

Both are sufficient, but the unit tests require more setup (creating a
FunctionBuilder context to emit into). This is manageable.

## Alternatives considered

### A. Keep the transform, optimize it

Instead of direct emission, optimize the Q32 transform to use less memory:
- In-place instruction rewriting (avoid cloning the IR)
- Lightweight declaration-only float module (see streaming-glsl-improvements2)
- Fuse the transform into the compilation step

This addresses the memory problem without changing the architecture. The
downside is that the transform's inherent costs remain — it still walks the
entire IR graph, still needs a module for function resolution, and still
adds pipeline complexity.

Verdict: viable for moderate improvements, but has a ceiling. Direct
emission removes the ceiling.

### B. Compile directly to Q32 without a strategy trait

Instead of a pluggable strategy, fork the codegen for Q32: a separate
set of emission functions that emit Q32 instructions directly.

This is faster to implement but creates code duplication. Every codegen
change would need to be made in two places. The strategy trait avoids this.

Verdict: rejected. The trait is a small upfront cost that pays for itself
in maintainability.

### C. CLIF-level peephole optimizer instead of transform

Instead of rewriting the entire IR, add a peephole pass that converts
individual float instructions to their Q32 equivalents in-place.

This would be lighter than the current transform (no second module, no
IR copy) but still requires a post-codegen pass. It's an intermediate
step between the current transform and direct emission.

Verdict: possible, but doesn't simplify the pipeline the way direct
emission does. If we're going to touch the IR post-codegen, the
transform is already doing that.

### D. Do nothing, live with the memory constraints

The compiler works. The memory is tight but sufficient for the current
test cases. Larger shaders might not compile, but that's a future problem.

Verdict: not acceptable long-term. The ESP32 use case is the primary
deployment target. Memory headroom determines what programs can run.
Every KB saved expands the practical program space.

## Decision factors

| Factor                    | Transform (status quo) | Direct emission |
|---------------------------|----------------------|-----------------|
| Peak memory (streaming)   | ~244 KB              | ~210 KB est.    |
| Peak memory (batch)       | ~228 KB              | ~210 KB est.    |
| Compilation speed         | Baseline             | ~15-20% faster  |
| Code complexity           | Two-pass pipeline    | Single-pass     |
| Maintenance burden        | Transform + codegen  | Strategy + codegen |
| Extensibility             | New transform per fmt | New strategy impl |
| Risk                      | Known, working       | Migration risk  |
| Streaming viability       | Net negative         | Net positive    |

The estimated ~210 KB peak for direct emission assumes: single module
(~17 KB saved), no transform overhead (~5 KB saved), reduced bookkeeping.
Actual numbers need measurement after implementation.
