# naga (lp2025 vendored fork)

Vendored copy of [naga 29.0.0](https://crates.io/crates/naga) (crates.io
sources; the `Cargo.toml` is the registry-normalized one). Wired into the
workspace via `[patch.crates-io]` in the root `Cargo.toml`, following the
`pp-rs` precedent.

## Local changes

Two changes, both in glsl-in and both marked `[lp2025 fork]` in the source.

### 1. `&&` / `||` short-circuit (`src/front/glsl/context.rs`)

In the `HirExprKind::Binary` lowering:

GLSL requires `&&` and `||` to short-circuit (GLSL ES 3.0 §5.9: "the second
operand is evaluated only if necessary"). Upstream glsl-in lowers both
operands eagerly, hoisting side-effecting calls/assignments in the right
operand into unconditionally-executed statements before the IR is even built —
so every consumer of the module (any backend, or external lowerings like
lp2025's `lps-frontend`) inherits the spec violation, and the information
needed to undo it is gone by then.

The fork lowers the right operand into its own body first:

- If that body is pure (only `Emit` statements), it is spliced into the
  current body and the plain `Binary` expression is kept — identical output
  to upstream for the common pure case.
- Otherwise it lowers the operator the same way upstream already lowers the
  ternary (`?:`): a temporary local written in both arms of an
  `Statement::If`, with the right operand evaluated only in the arm the spec
  says evaluates it, and a `Load` of the local as the result.

Const contexts (`self.is_const`) are excluded and take the upstream path
unchanged.

### 2. `continue` inside `do … while` (`src/front/glsl/parser/functions.rs`)

In the `TokenValue::Do` arm of `parse_statement`:

Upstream lowers `do { body } while (cond);` as
`Loop { body: [body, if (!cond) { break; }], continuing: [], break_if: None }`.
A `continue` in the body jumps to the (empty) continuing block and back to
the top, so the condition is never tested on that iteration and the loop
re-enters unconditionally — GLSL ES 3.0 §6.3 says `continue` in a do-while
proceeds to the condition test. Every consumer of the IR (any backend, and
lp2025's `lps-frontend`) inherits the bug; `lps-frontend` used to carry a
shape-matching workaround (move a trailing `if (…) { break; }` into the
continuing section) that was itself unsound for `while`/`for` bodies that
happen to end in `if (x) break;` after a `continue`. That workaround is gone.

The fork lowers the condition in the `continuing` block and sets
`break_if: Some(!cond)`, which is the shape wgsl-in produces for
`loop { … continuing { break if !cond; } }` and the one every backend
already emits correctly. The condition is evaluated in `continuing`, not
hoisted into the body, because it may read variables the body writes.
The body's dead-code cull after a terminator (`return`/`break`/`continue`)
is unchanged.

Found by the adapter-gated `wgpu.f32` filetest target
(`scripts/filetests.sh --target wgpu.f32`, not in the default set) on the
`control/torture/brknest_*` corpus, 2026-09-07.

## Upstreaming

Both are upstream bugs (still present on wgpu `trunk` as of 2026-09-07) and
the patches are written to be upstreamable:

- naga's **wgsl-in** already lowers runtime `&&`/`||` to exactly this
  temp-local + `If` shape ("To simulate short-circuiting behavior…",
  `src/front/wgsl/lower/mod.rs`, `binary_short_circuit` path) — glsl-in
  never got the same treatment.
- glsl-in's own ternary lowering (directly below the patched arm) already
  implements lazy operand evaluation for `?:`.

- For the do-while fix, wgsl-in's `ast::StatementKind::Loop` lowering
  already emits `break_if` from the continuing block, and glsl-in's own
  `for` arm already builds a `continuing` block for the increment
  expression — the do-while arm was the odd one out.

`upstream-glsl-short-circuit.patch` and `upstream-glsl-do-while-continue.patch`
are the fork hunks rebased onto the wgpu repo layout
(`naga/src/front/glsl/…`), ready for PRs when we want to send them; drop
the `[lp2025 fork]` comment tags and add glsl-in snapshot tests in the wgpu
repo when doing so. If/when both land upstream and a naga release containing
them is adopted, delete this fork and the `[patch.crates-io]` entry.

## Updating

To move to a newer naga: re-vendor the new crates.io sources, re-apply the
`[lp2025 fork]` hunks in `src/front/glsl/context.rs` and
`src/front/glsl/parser/functions.rs` (or drop whichever upstream has fixed),
restore this file and the fork header comment plus `[workspace]` footer in
`Cargo.toml`, and re-run the control-flow torture corpus on the default
targets *and* the GPU probe target (`scripts/filetests.sh control/torture`
and `scripts/filetests.sh --target wgpu.f32 control/torture`).
