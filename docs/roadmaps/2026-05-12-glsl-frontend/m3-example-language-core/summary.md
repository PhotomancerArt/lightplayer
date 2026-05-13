# M3 Summary - Example Language Core

## What was built

- Expanded `lps-glsl` body parsing beyond M2 return-only functions:
  - local declarations
  - whole-local assignment
  - `if` / `else`
  - block statements
  - swizzles
  - comparisons
- Added typed HIR support for:
  - local variables and scoped blocks
  - top-level const initializers
  - calls before definitions
  - numeric coercions and scalar casts
  - scalar/vector constructors with scalar broadcast
  - builtins used by the example palette path
- Added LPIR lowering for:
  - scalarized locals
  - local function calls with VMContext forwarding
  - conditionals
  - swizzle reads
  - inline builtins such as `mix`, `smoothstep`, `clamp`, `fract`, `mod`, `min`, and `max`
  - GLSL scalar imports such as `sin`, `cos`, `exp`, and `atan`
  - simple value-returning LPFN imports for `lpfn_worley` and `lpfn_hsv2rgb`
- Added M3 filetest fixtures:
  - `lps-glsl/m3-core.glsl` for locals, conditionals, swizzles, calls, and builtin math
  - `lps-glsl/basic2-render.glsl`, derived from `examples/basic2/shader.glsl`, preserving the
    forward call to `worley_demo`

## Decisions for future reference

#### Forward Calls

- **Decision:** Resolve user calls against the full top-level function signature table before body
  lowering.
- **Why:** `basic2` intentionally calls `worley_demo` before the definition; this is also a clean
  split between indexing and body typing.
- **Rejected alternatives:** Reordering source functions or requiring helper declarations.
- **Revisit when:** Recursive calls or reachability pruning become part of the compile strategy.

#### LPFN Value Calls

- **Decision:** M3 supports only value-returning LPFN calls without out/inout parameters.
- **Why:** This is enough for `basic2`; `lpfn_psrdnoise` needs out-vector handling and belongs with
  the LPFN-focused milestone.
- **Rejected alternatives:** Pulling all LPFN ABI support into M3.
- **Revisit when:** Starting M4.

#### Side-by-Side Validation

- **Decision:** Use side-by-side `rv32n.q32,rv32lpn.q32` validation for feature fixtures, and
  `rv32lpn.q32`-only validation for exact `basic2`.
- **Why:** The exact `basic2` source order is a new LP frontend capability; Naga rejects the forward
  call.
- **Rejected alternatives:** Reordering the `basic2` fixture to make Naga pass.
- **Revisit when:** A separate differential fixture for reordered `basic2` becomes useful.

## Validation

Passed:

```bash
cargo test -p lps-glsl
cargo test -p lps-filetests targets
cargo check -p lps-glsl --target riscv32imac-unknown-none-elf
cargo check -p lps-filetests
cargo check -p lps-filetests-app
cargo run -p lps-filetests-app -- test --target rv32n.q32,rv32lpn.q32 --concise lps-glsl/m3-core.glsl
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl/basic2-render.glsl
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl/fast-render.glsl lps-glsl/m3-core.glsl lps-glsl/basic2-render.glsl
```

## Remaining Edges

- `basic`, `noise.fx`, and perf palette paths still need broader LPFN and control-flow coverage.
- `lpfn_psrdnoise` out/inout lowering is deferred.
- Loops, arrays, structs, matrices, textures, and component assignment remain out of scope.
