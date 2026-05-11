# Phase 1: Relational expressions (bvec / vector relational)

## Scope of phase

Implement lowering for Naga `Expression::Relational` where today the compiler errors with
`unsupported expression: Relational { fun: All | Any | Not | … }`. This unblocks:

- `all()`, `any()`, `not()` on boolean vectors
- vector `==` / aggregate comparisons that lower through `Relational::All`
- `builtins/common-isnan.glsl` and `builtins/common-isinf.glsl` (component-wise relational on
  vectors)

## Code organization reminders

- Prefer granular helpers (e.g. one small function per `RelationalFunction` group) in
  `lower_expr.rs` or a dedicated submodule if the file grows.
- Entry points and tests first; private helpers at the bottom of the module.
- Any speculative path should carry a `TODO` with a single-line reason.

## Implementation details

1. **Inspect Naga** — confirm which `naga::RelationalFunction` variants appear for GLSL 450
   (`All`, `Any`, `Not`, `IsNan`, `IsInf`, etc.).

2. **`lower_expr.rs`** — add a match arm for `Expression::Relational`:
   - Resolve the argument expression(s) to scalarized VRegs (existing infrastructure).
   - `All` / `Any` on `bvecN`: reduce component `i32` truth values with `iand` / `ior` chains (GLSL
     `bool` as `i32` 0/1).
   - `Not` on `bvecN`: `ieq` each component with 0 or `ixor`/logical not as appropriate.
   - `IsNan` / `IsInf` on float vectors: component-wise using the same strategy as scalars (Q32 may
     use integer pattern checks — follow existing scalar lowering).

3. **Tests**
   - Run: `./scripts/filetests.sh vec/bvec2/fn-all.glsl` (and siblings),
     `vec/bvec2/op-equal.glsl`,
     `builtins/common-isnan.glsl`, `builtins/common-isinf.glsl`.
   - `cargo test -p lps-frontend`

4. Do **not** change matrix or array handling in this phase.

## Validate

```bash
cargo test -p lps-frontend
cargo test -p lps-filetests
./scripts/filetests.sh vec/bvec2/fn-all.glsl vec/bvec2/op-equal.glsl builtins/common-isnan.glsl builtins/common-isinf.glsl
```

Optional broader spot-check:

```bash
./scripts/filetests.sh vec/bvec3 vec/bvec4
```

For embedded policy (if you touch `lp-core` / `lp-fw` — not expected here):

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
