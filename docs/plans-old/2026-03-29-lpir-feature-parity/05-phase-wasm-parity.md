# Phase 5: WASM backend parity check

## Scope of phase

Verify that the same LPIR programs that pass on **jit.q32** after phases 1–4 also **compile and
run** on **wasm.q32** where the corpus expects it. Fix **`lps-wasm`** only if the emitter or
link step is wrong — do not paper over frontend bugs with file-level `@unimplemented` unless the
gap is genuinely WASM-only and documented.

## Code organization reminders

- Prefer fixes in shared LPIR validation over WASM-only hacks.
- WASM-specific work belongs under `lps-wasm/src/emit/`.

## Implementation details

1. Run a **slice** of the matrix and bvec directories on wasm:

```bash
./scripts/filetests.sh --target wasm.q32 vec/bvec2/
./scripts/filetests.sh --target wasm.q32 matrix/mat2/
```

2. Compare failures against **jit.q32** for the same files; classify as:
   - shared bug (fix in naga/LPIR)
   - WASM emitter bug (fix in `lps-wasm`)
   - intentional platform limit (rare; document with `@unsupported(backend=wasm)` and `reason=`)

3. **Do not** remove file-level `@unimplemented(backend=wasm)` wholesale without proving the
   backend passes; triage incrementally.

## Validate

```bash
cargo test -p lps-wasm
./scripts/filetests.sh --target wasm.q32 vec/bvec2/fn-all.glsl matrix/mat2/op-add.glsl
```

(Host Cranelift parity:)

```bash
./scripts/filetests.sh vec/bvec2/fn-all.glsl matrix/mat2/op-add.glsl
```

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
