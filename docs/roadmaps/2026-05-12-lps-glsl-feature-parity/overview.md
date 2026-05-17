# lps-glsl Feature Parity Roadmap

## Goal

Bring `lp-shader/lps-glsl` close enough to the existing Naga-backed frontend that LightPlayer examples and shader filetests can move to the native GLSL frontend with confidence.

The working definition of parity is:

- existing examples compile and render correctly through the default firmware path
- the existing success-path filetests pass on `rv32lpn.q32`, except for explicitly documented out-of-scope cases
- diagnostics have source spans and useful line indicators
- the firmware can still build without Naga in the lps-glsl path
- the architecture leaves a plausible path for a later WGSL frontend

This is an implementation roadmap, not a research project. Naga and the existing filetests are the oracle for language behavior; `lps-glsl` is the smaller, faster, on-device implementation.

## Architecture Shape

The useful separation is:

```text
GLSL source
  -> lexer/tokens/spans
  -> syntax parser
  -> semantic analysis and HIR
  -> LPIR lowering
  -> native RV32 backend
```

The parser can remain GLSL-specific. The semantic/HIR boundary should stay language-neutral enough that a WGSL frontend could later lower into the same typed HIR or into a nearby sibling HIR.

The most important architecture addition is a real place model. Many parity features become much simpler once assignment targets, readable projections, and writable call arguments are represented uniformly:

```text
local
uniform/global
field/member
array index
vector swizzle
nested combinations
```

The place model should be paired with one canonical aggregate shape/layout view:

```text
LpsType
  -> existing lps_shared::layout + lpvm byte/path helpers
  -> lps-glsl TypeShape / LayoutView adaptor
  -> scalar lane layout for value-like aggregates
  -> byte layout for slot-backed aggregates, uniforms, globals, and pointer ABI
```

The default implementation strategy should be hybrid:

- keep simple scalar/vector/matrix/struct expressions lane-flat when that is cheapest
- use slot-backed aggregate storage at ABI boundaries, for globals/uniforms, and for dynamic aggregate indexing/writeback cases
- keep the semantic API above both choices so `p.items[i].color.xy` is typed as one place path even if lowering chooses lanes or memory

After that, loops, `inout`, structs, arrays, matrix operations, and future WGSL lowering are mostly extensions of the same semantic and lowering machinery rather than one-off parser tricks.

## File Organization

Keep the crate small-file oriented as it grows. A likely shape is:

```text
lp-shader/lps-glsl/src/
  lexer.rs
  token.rs
  source.rs
  diagnostic.rs
  job.rs
  compile.rs
  hir/
    mod.rs
    expr.rs
    stmt.rs
    ty.rs
    function.rs
  syntax/
    mod.rs
    expr.rs
    stmt.rs
    decl.rs
    ty.rs
  sem/
    mod.rs
    scope.rs
    lvalue.rs
    calls.rs
    builtins.rs
    convert.rs
  lower/
    mod.rs
    expr.rs
    stmt.rs
    aggregate.rs
```

This does not need to land as one refactor. Split files when a feature makes the existing files hard to navigate.

## Milestones

0. M0: Prep, incremental contracts, and scaffolding
1. M1: Control flow, operators, and lvalues
2. M2: Functions, overloads, and parameter qualifiers
3. M3: Aggregate foundations, arrays, structs, and globals
4. M4: Matrices, textures, and builtin coverage
5. M5: Parity closure, diagnostics, and cleanup

The ordering is intentionally foundation-first. Some structs and arrays can work with flattened lanes, but feature parity needs the foundations before chasing every filetest:

- nested place paths for `local.member[index].field.xy`
- aggregate metadata backed by existing layout logic: field offsets, array strides, and matrix column layout
- a clear rule for when values stay lane-flat versus become slot-backed
- function ABI handling for `in`, `out`, `inout`, and aggregate returns

Each milestone should end with filetests that are meaningful enough to prevent regressions before the next hardware demo.

## Validation Pattern

Use filetests as the primary gate:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise scalar vec control operators
```

As coverage grows, run broader slices:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise function array struct matrix builtins texture global const uniform
```

Before hardware claims:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
just demo-esp32c6-host
```

Before landing a major milestone:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

## Autonomy Protocol

Work milestone-by-milestone. Within a milestone:

- implement the smallest semantic slice that unlocks a filetest group
- add or move filetests before hardware testing
- use the existing Naga path as behavior reference
- keep diagnostic formatting improving opportunistically
- record intentionally unsupported cases explicitly

Stop for user input only at the stop conditions in `notes.md`.
