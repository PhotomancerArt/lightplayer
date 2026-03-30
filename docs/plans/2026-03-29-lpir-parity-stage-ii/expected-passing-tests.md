# Stage II — expected passing filetests

All paths are relative to `lp-glsl/lp-glsl-filetests/filetests/`.

After implementation, each listed file should pass on **`jit.q32`**, **`wasm.q32`**, and **`rv32.q32`** unless a `// run:` (or file) carries a justified **`@unsupported(...)`** (document reason in the annotation).

## Tier A — core Milestone II (pointer / Access)

### Matrix element inc/dec

- `matrix/mat2/incdec-matrix-element.glsl`
- `matrix/mat3/incdec-matrix-element.glsl`
- `matrix/mat4/incdec-matrix-element.glsl`
- `operators/incdec-matrix-element.glsl` (if present in tree and covered by same lowering)

### Matrix column inc/dec

- `matrix/mat2/incdec-matrix-column.glsl`
- `matrix/mat3/incdec-matrix-column.glsl`
- `matrix/mat4/incdec-matrix-column.glsl`
- `operators/incdec-matrix-column.glsl`

### Deferred (not stage II Access work)

- `operators/incdec-matrix.glsl` — whole-matrix postfix `m++` / `m--` expectations vs lowering are inconsistent (old value vs mutated matrix); track under a separate unary/matrix milestone, not pointer `Access`.

### Bvec / vec indexing and assign

- `vec/bvec2/assign-element.glsl`
- `vec/bvec2/index-variable-valid.glsl`
- `vec/bvec2/access-array.glsl`
- `vec/bvec3/access-array.glsl`
- `vec/bvec4/access-array.glsl`

## Tier B — bounds / traps (triage during implementation)

`vec/bvec2/index-variable-bounds.glsl` (and similar) reference **`EXPECT_TRAP`** / out-of-bounds behavior. If the LPIR / emulator / wasm path does not yet implement matching trap semantics, keep targeted **`@unimplemented`** or **`@unsupported`** with a short reason until a dedicated trap milestone; do not weaken tests to hide bugs.

## Not in stage II corpus

- `array/assign-element.glsl` and other **array** tests → Milestone IV unless they only touch vectors with no array `Access`.
- Filetests whose failures are **relational**, **mix**, **casts**, or **parse** issues — track under other milestones.

## Summary line

Record final pass counts and any remaining annotations in [`summary.md`](./summary.md) when the plan completes.
