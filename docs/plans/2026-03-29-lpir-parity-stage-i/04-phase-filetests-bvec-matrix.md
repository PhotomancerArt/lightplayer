# Phase 4: Bvec relational + matrix equality filetests

## Scope of phase

Unmark and pass **bvec** tests that failed only due to relational typing/lowering, and the **six**
matrix equality / inequality files.

## Code organization reminders

- When stripping `@unimplemented`, use `LP_FIX_XFAIL=1` / `--fix` if the harness can remove stale
  markers after a pass; otherwise edit manually.
- Do not widen scope to matrix inc/dec or bvec casts (Milestone II / III).

## Implementation details

### Matrix

Paths:

- `matrix/mat2/op-equal.glsl`, `matrix/mat2/op-not-equal.glsl`
- `matrix/mat3/op-equal.glsl`, `matrix/mat3/op-not-equal.glsl`
- `matrix/mat4/op-equal.glsl`, `matrix/mat4/op-not-equal.glsl`

Remove `// @unimplemented(backend=jit)` when tests pass. If a file still fails for **non-relational
**
reasons (e.g. mat4 return ABI), re-add marker or move to Milestone V per roadmap.

### Bvec (bool vectors)

Relational-heavy tests live under **`filetests/vec/bvec2/`**, **`bvec3/`**, **`bvec4/`** (and
sometimes `uvec*/` for nested `all`/`any`). There is no top-level `bvec/` directory.

Triage failures tied to `all` / `any` / nested relational / `not` / `expr_type_inner` on
`Relational`. **Enumerate** each file you fix in [`summary.md`](./summary.md) (Tier B); each must
pass **jit, wasm, rv32**. Defer pointer/store failures to Milestone II.

**Matrix + WASM:** several matrix equality files still have `// @unimplemented(backend=wasm)` at
file or line level — remove when WASM passes, or parity is incomplete.

## Validate

Tier A matrix files — repeat for **each** of `jit.q32`, `wasm.q32`, `rv32.q32`:

```bash
cd lp2025
MATRIX="matrix/mat2/op-equal.glsl matrix/mat2/op-not-equal.glsl \
  matrix/mat3/op-equal.glsl matrix/mat3/op-not-equal.glsl \
  matrix/mat4/op-equal.glsl matrix/mat4/op-not-equal.glsl"
for t in jit.q32 wasm.q32 rv32.q32; do
  ./scripts/glsl-filetests.sh --target "$t" $MATRIX
done
```

Tier B (after triage), same pattern per file or directory:

```bash
./scripts/glsl-filetests.sh --target wasm.q32 vec/bvec2/
# … etc.
```

```bash
cd lps && cargo test -p lps-filetests
```

Corpus definition: [`expected-passing-tests.md`](./expected-passing-tests.md).
