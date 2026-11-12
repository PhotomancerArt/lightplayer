# M3 Plan - Example Language Core

## Goal

Grow the `lps-glsl` proof from the M2 single-return subset into the language core needed by the
small example shaders, with `examples/basic2/shader.glsl` as the concrete end-to-end target.

M3 is still not full filetest compatibility. It should prove the architecture for local state,
function calls, scalar/vector arithmetic, swizzles, conditionals, and simple builtin dispatch while
leaving LPFN out/inout and loop-heavy examples to later milestones.

## Scope

In scope:

- Function calls before and after definitions.
- Local declarations, including `const` locals.
- Simple assignment to whole local variables.
- `if` / `else` blocks and early returns.
- Comparisons needed by palette selection helpers.
- Scalar/vector constructors, scalar casts, and vector-scalar arithmetic.
- Component reads and swizzles.
- Builtins used by palette/basic2 paths: `abs`, `clamp`, `cos`, `exp`, `floor`, `fract`, `max`,
  `min`, `mix`, `mod`, `sin`, and `smoothstep`.
- Simple LPFN value calls without out/inout, enough for `lpfn_worley(vec2, uint)` and
  `lpfn_hsv2rgb(vec3)`.
- Differential filetest fixtures that can run `rv32n.q32` and `rv32lpn.q32` side by side.

Out of scope:

- Loops.
- Structs, arrays, matrices, textures.
- Component assignment and compound assignment.
- LPFN out/inout semantics such as `lpfn_psrdnoise(..., gradient, ...)`.
- Full overload resolution.

## Phases

1. Parser expansion:
   - Add block statements, local declarations, assignments, `if` / `else`, calls, swizzles, casts,
     and comparisons.
2. Typed HIR:
   - Add locals, global constants, swizzle typing, function-call resolution, numeric coercions, and
     builtin/LPFN call forms.
3. LPIR lowering:
   - Lower locals to scalarized vregs, local calls to `CalleeRef::Local`, builtin calls to inline LPIR
     or imports, LPFN value calls to `@lpfn::*`, and `if` blocks to LPIR control markers.
4. Filetest proof:
   - Add focused `lps-glsl` fixtures and a `basic2`-derived render fixture.
   - Validate both `rv32n.q32` and `rv32lpn.q32` where practical.
5. Cleanup:
   - Format, run focused host/RV32 checks, summarize decisions, and commit.

## Validation

Run:

```bash
cargo test -p lps-glsl
cargo test -p lps-filetests targets
cargo check -p lps-glsl --target riscv32imac-unknown-none-elf
cargo check -p lps-filetests
cargo check -p lps-filetests-app
cargo run -p lps-filetests-app -- test --target rv32n.q32,rv32lpn.q32 --concise lps-glsl/m3-core.glsl
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl/basic2-render.glsl
```

## Completion Criteria

- `examples/basic2/shader.glsl` compiles through `lps-glsl`.
- A `basic2`-derived filetest passes on `rv32lpn.q32`.
- Focused feature fixtures cover locals, conditionals, swizzles, calls, and builtin math.
- `lps-glsl` still checks for `riscv32imac-unknown-none-elf`.
