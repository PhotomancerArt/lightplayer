# Phase 4: Corpus unmark and three-target filetests

## Scope of phase

- Strip **`@unimplemented(backend=jit)`** / **`wasm`** from files in [**`expected-passing-tests.md`**](./expected-passing-tests.md) **Tier A** as they go green.
- Run **`jit.q32`**, **`wasm.q32`**, **`rv32.q32`** on the **Tier A** glob set.
- **Tier B** (bounds / **EXPECT_TRAP**): only unmark if traps match; otherwise document in file + [`summary.md`](./summary.md).

## Code organization reminders

- Filetest edits: remove only annotations that are **proven** unnecessary; prefer **`@unsupported`** with **`reason=`** over deleting tests.

## Implementation details

1. Use `scripts/glsl-filetests.sh --target <t> <patterns…>` with globs from the expected-passing list.

2. WASM-specific issues (e.g. float traps) get **`@unsupported(backend=wasm, …)`** only when normatively required, mirroring stage I policy.

3. Update **[`summary.md`](./summary.md)** with file counts per target and any remaining skips.

### Tests

- Full Tier A three-target sweep.

## Validate

```bash
for t in jit.q32 wasm.q32 rv32.q32; do
  bash scripts/glsl-filetests.sh --target "$t" \
    matrix/mat2/incdec-matrix-element.glsl matrix/mat3/incdec-matrix-element.glsl matrix/mat4/incdec-matrix-element.glsl \
    matrix/mat2/incdec-matrix-column.glsl matrix/mat3/incdec-matrix-column.glsl matrix/mat4/incdec-matrix-column.glsl \
    operators/incdec-matrix-element.glsl operators/incdec-matrix-column.glsl operators/incdec-matrix.glsl \
    vec/bvec2/assign-element.glsl vec/bvec2/index-variable-valid.glsl \
    vec/bvec2/access-array.glsl vec/bvec3/access-array.glsl vec/bvec4/access-array.glsl
done
```

Adjust operator file list if some are redundant or out of scope.
