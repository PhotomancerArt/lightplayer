# lps-glsl Frontend Roadmap

## Motivation and Rationale

LightPlayer's on-device GLSL JIT compiler is the product. The current Naga-based frontend preserves
correctness and breadth, but its parse/lower cost is too large and too monolithic for playlist
hot-loading on a single-core ESP32-C6.

This roadmap builds `lp-shader/lps-glsl`, a smaller LightPlayer-shaped GLSL frontend that compiles
the existing examples on
device, with selected filetests used as validity evidence for the features those examples need. It
does not attempt full filetest or full GLSL compatibility.

The central product bet is schedulability: a compiler that can yield between small units is more
useful for hot-loading than a faster compiler that still blocks rendering for one large interval.

## Target Compatibility

The first compatibility target is the current example shader set:

```text
examples/fast/shader.glsl
examples/basic2/shader.glsl
examples/basic/shader.glsl
examples/noise.fx/main.glsl
examples/perf/baseline/shader.glsl
examples/perf/fastmath/shader.glsl
examples/rocaille/shader.glsl
```

That means supporting uniforms, global and local constants, scalar/vector expressions, functions,
early-return conditionals, nested `for` loops, swizzles, component assignment, casts, example-used
builtins, and LPFN calls including the `lpfn_psrdnoise(..., gradient, ...)` out/inout pattern.

It does not initially require structs, arrays, textures, matrices, preprocessor support, switch
statements, GPU stage metadata, or full GLSL layout semantics.

## Architecture

The frontend should be a resumable compiler pipeline from the start:

```text
source
  -> lps-glsl lexer / token tape
  -> top-level index
  -> per-function syntax parse
  -> semantic analysis
  -> typed HIR
  -> LPIR lowering
  -> existing lpvm-native backend
```

The top-level index records uniforms, globals, constants, function signatures, and function body
spans without eagerly building a whole-module AST. Reachability starts from `render` and filetest
entry functions, so unused helper functions can be skipped in fast mode while strict host modes can
still validate more of the source.

HIR means High-level Intermediate Representation. In this roadmap it is a compact, typed,
per-function representation with source spans. HIR is the place where GLSL syntax becomes semantic
compiler vocabulary: lvalues, rvalues, resolved calls, typed constructors, swizzles, blocks,
branches, loops, returns, and out/inout copy semantics.

## Resumability

The first yield resolution should be coarse and reliable:

```text
lex chunk
scan one top-level declaration
parse one function body
analyze one function
lower one function to LPIR
finalize and validate module
```

If profiles show large functions causing frame disruption, later milestones can split work at
statement or block granularity. The API should still look like a job from day one:

```rust
pub struct CompileJob { ... }

impl CompileJob {
    pub fn step(&mut self, budget: CompileBudget) -> CompileStepResult;
}
```

The synchronous compile API can be a thin loop over `lps_glsl::CompileJob`, so tests and startup
paths do not need a separate compiler. If a wider engine-facing type needs the LightPlayer prefix,
it can alias or wrap this as `LpCompileJob`.

## Filetest Integration

Expose lps-glsl through a normal filetest target rather than a separate CLI flag:

```text
rv32n.q32    Naga frontend + lpvm-native RV32 backend
rv32lpn.q32  lps-glsl frontend + lpvm-native RV32 backend
```

This lets local runs and CI summaries show both frontends side by side:

```bash
cargo run -p lps-filetests-app -- test --target rv32n.q32,rv32lpn.q32 examples/fast/shader.glsl
```

Internally, `Target` may gain a `frontend` field so compile routing stays clean. User-facing target
names should stay short and explicit. Later, if useful, the same pattern can add `wasmlp.q32` or
`jitlp.q32`, but the first product proof should be `rv32lpn.q32`.

## Diagnostics

Diagnostics should be span-first and best-effort:

```text
Span { start, end }
SourceMap { line_starts }
Diagnostic { severity, code, primary_span, message, notes }
```

Halt-on-first-error is acceptable for the initial product path. The lexer, parser, AST/HIR nodes,
and semantic tables should carry enough span information that better recovery and multi-error
reporting can be added later without changing every layer.

## WGSL Boundary

Do not force GLSL and future WGSL to share a syntax layer. The reusable boundary is semantic:

```text
GLSL parser -> typed HIR builder -> LPIR lowering
WGSL parser -> typed HIR builder -> LPIR lowering
```

WGSL can reuse diagnostics, type representation, builtin registry, HIR, resumable job scheduling,
and LPIR lowering. Its lexer/parser can be separate or copied when the need is real.

## Alternatives Considered

Full AST-first compiler: best for broad language support and diagnostics, but too allocation-heavy
and too eager for the embedded hot-load path.

Direct LPIR emission: fastest for tiny straight-line code, but brittle for nested control flow,
swizzles, lvalues, casts, and out/inout calls. It is useful inside lowering, not as the whole
frontend architecture.

Reusing the old `glsl-parser` fork: useful as a grammar reference or host comparison tool, but
likely too broad and AST-shaped for the embedded endpoint.

Naga fallback on device: attractive for compatibility, but it keeps the monolithic cost and risks
making the custom frontend optional in the product path.

## Risks

The example set may hide syntax that appears quickly in real playlists, especially structs,
textures, or arrays. The roadmap should keep unsupported diagnostics clear and use filetests to add
nearby coverage as features become needed.

Resumability can complicate parser state. Start with coarse units and refine only where measured
latency requires it.

Typed HIR can grow into a second general shader IR. Keep it per-function, compact, and explicitly
shaped around lowering to LPIR.

Builtin and LPFN overload resolution can become a correctness trap. Keep Naga differential tests on
host for every supported builtin signature.

## Scope Estimate

This is a multi-milestone compiler effort. The first useful proof should be small: compile
`examples/fast/shader.glsl` through `lps-glsl` and run it with `rv32lpn.q32`. The first product
decision point is after all current examples compile and run through emulator validation with
measured frontend latency.
