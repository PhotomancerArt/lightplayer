# lps-glsl Frontend Probe

## Goal

Prove whether a LightPlayer-shaped GLSL frontend can replace the expensive Naga parse/lower step
for the current LightPlayer examples while preserving the existing on-device JIT product path:

```text
GLSL source -> lps-glsl -> LPIR -> lpvm-native -> RV32 machine code
```

This probe is deliberately small. It should answer whether the architecture is promising before
we build a broad GLSL compiler.

## Success Criteria

- A new experimental frontend can lower the current `examples/**/*.glsl` shaders, plus selected
  filetests for the same language features, to valid LPIR.
- The same lowered LPIR runs through `rv32lpn.q32` in the existing filetest harness.
- Naga remains available as the host correctness oracle for the same source.
- The production compile path remains unchanged unless the experiment is explicitly selected.
- Unsupported syntax fails with a precise diagnostic instead of silently falling back.

## Existing Validity Harness

The canonical validity corpus is `lp-shader/lps-filetests/filetests/`. It currently has broad
coverage for:

- scalar/vector operators and constructors
- functions, overloading, `in`/`out`/`inout`, returns
- globals, uniforms, structs, textures
- LPFN builtins
- control flow and const/type error cases

The current execution path runs through `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`.
All targets call `lower_glsl(...)` once, then compile the resulting LPIR through the selected
backend. That is the right seam for the experiment.

## Recommended Filetest Wiring

Use a new explicit filetest target for the lps-glsl frontend plus native RV32 backend:

```text
rv32n.q32    GLSL -> lps-frontend/Naga -> LPIR -> lpvm-native -> RV32 emulator
rv32lpn.q32  GLSL -> lps-glsl          -> LPIR -> lpvm-native -> RV32 emulator
```

The name reads as `rv32` + `lp` frontend + `n` native backend. Existing target names stay stable
and continue to imply the Naga frontend. The target model can still carry an internal frontend axis,
but the user-facing CLI should expose it as target names so summary output can show both frontends
side by side.

For the first pass:

```bash
cargo run -p lps-filetests-app -- test \
  --target rv32n.q32,rv32lpn.q32 \
  function/define-simple.glsl function/call-simple.glsl operators/
```

Do not add per-file frontend directives yet. Existing annotation machinery can work once
`rv32lpn.q32` is a normal target name.

## First Supported Subset

Start with source that proves the examples pipeline, not the whole language:

- function signatures and definitions
- scalar `float`, `int`, `uint`, `bool`
- `vec2`, `vec3`, `vec4` constructors and component-wise arithmetic
- local declarations with optional initializers
- `return`
- function calls to user functions
- direct builtin calls for `sin`, `cos`, `min`, `max`, `clamp`, `mix`, `floor`, `mod`
- uniforms with simple `layout(binding = N)` metadata
- early-return `if` chains and simple `if` / `else`
- `for` loops needed by `examples/rocaille/shader.glsl`
- component reads, swizzles, component assignment, and compound assignment
- LPFN calls used by examples, including `lpfn_psrdnoise` out/inout-style arguments

Leave these out of the first probe:

- arrays
- structs
- matrices
- textures
- overload ambiguity edge cases
- full GLSL preprocessor behavior

## Architecture Shape

Use a token tape plus spans, not an owned full AST:

```text
source
  -> token tape
  -> top-level index
       uniforms
       globals
       function signatures
       function body spans
  -> reachability from requested functions / render
  -> typed HIR for reachable function bodies
  -> LPIR lowering
```

The top-level index is the important proof. It lets us avoid parsing unreachable function bodies and
gives the future resumable compiler natural work units.

Function bodies should use recursive descent statements plus a Pratt expression parser. Type
checking should produce compact per-function HIR, keeping the semantic layer just large enough to
represent lvalues, swizzles, branches, loops, calls, casts, and out/inout behavior before LPIR
lowering.

## WGSL-Ready Boundary

Do not share the lexer or syntax parser with future WGSL. Share the semantic/lowering boundary:

```text
GLSL parser  -> typed frontend events / body lowering API -> LPIR builder
WGSL parser  -> typed frontend events / body lowering API -> LPIR builder
```

That keeps the architecture modular without designing for a language we are not building yet.

## Compiler Theory Bets

- Prefer indexed, demand-driven compilation over whole-program AST construction.
- Prefer arena/lifetime-scoped allocation over many independent `Vec`/`String` allocations.
- Keep identifiers interned and source slices borrowed by span.
- Use Pratt parsing for expressions because GLSL expression precedence is rich but local.
- Keep HIR typed, compact, and per-function; avoid growing a Naga-like general shader IR.
- Treat builtins as typed intrinsic/import table entries, not source-injected declarations.
- Track compile scheduling as a first-class constraint: each stage should become a future
  `lps_glsl::CompileJob::step(...)` unit.

## Probe Phases

### Phase 1 - Skeleton and Filetest Seam

- Add `lp-shader/lps-glsl` as `no_std + alloc`.
- Add an internal frontend selector to the filetest target model.
- Add `rv32lpn.q32` to `lps-filetests` target parsing/display.
- Route `rv32lpn.q32` through `lps-glsl` and keep `rv32n.q32` on the Naga path.

### Phase 2 - Token Tape and Top-Level Index

- Lex comments, identifiers, literals, punctuation, and GLSL keywords.
- Parse top-level declarations shallowly.
- Record function signatures and body spans.
- Add unit tests against small snippets and `examples/basic/shader.glsl`.

### Phase 3 - Minimal LPIR Lowering

- Lower scalar arithmetic, vector constructors, locals, calls, returns, and example-shaped control
  flow.
- Validate produced LPIR with `lpir::validate_module`.
- Run a curated filetest subset through `rv32n.q32,rv32lpn.q32`.

### Phase 4 - Differential Evidence

- For each supported filetest, compile with both frontends.
- Compare `LpsModuleSig`.
- Compare rendered/run outputs through the existing filetest execution path.
- Measure frontend-only time and allocation counts on host, then emulator.

## Early Corpus

Good first filetests:

- `function/define-simple.glsl`
- `function/call-simple.glsl`
- `function/call-nested.glsl`
- `function/return-scalar.glsl`
- simple scalar/vector operator files under `operators/`
- selected `builtins/common-clamp.glsl`, `builtins/common-mix.glsl`, `builtins/trig-sin.glsl`

Good realism probes:

- `examples/fast/shader.glsl`
- `examples/basic2/shader.glsl`
- `examples/basic/shader.glsl`
- `examples/noise.fx/main.glsl`
- `examples/rocaille/shader.glsl`

## Non-Goals

- Do not replace the production frontend during the probe.
- Do not gate the compiler behind `std`.
- Do not add host precompilation as a workaround.
- Do not implement a fallback from `lps-glsl` to Naga for embedded runtime compilation.
- Do not aim for complete GLSL or full filetest compatibility before proving the example path.
