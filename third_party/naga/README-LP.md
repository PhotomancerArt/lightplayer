# naga (lp2025 vendored fork)

Vendored copy of [naga 29.0.0](https://crates.io/crates/naga) (crates.io
sources; the `Cargo.toml` is the registry-normalized one). Wired into the
workspace via `[patch.crates-io]` in the root `Cargo.toml`, following the
`pp-rs` precedent.

## Local changes

One change, in `src/front/glsl/context.rs` (`HirExprKind::Binary` lowering,
marked `[lp2025 fork]`):

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

## Upstreaming

This is arguably an upstream bug and the patch is written to be
upstreamable (it mirrors the neighbouring ternary lowering). If/when it lands
upstream and a naga release containing it is adopted, delete this fork and the
`[patch.crates-io]` entry.

## Updating

To move to a newer naga: re-vendor the new crates.io sources, re-apply the
`[lp2025 fork]` hunk in `src/front/glsl/context.rs` (or drop it if upstream
fixed short-circuiting), restore this file and the fork header comment plus
`[workspace]` footer in `Cargo.toml`, and re-run the control-flow torture
corpus (`scripts/filetests.sh control/torture`).
