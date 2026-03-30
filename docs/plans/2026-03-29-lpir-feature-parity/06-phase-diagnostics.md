# Phase 6: Diagnostic codes (`type_errors/`)

## Scope of phase

Fix **expected diagnostic** mismatches in `filetests/type_errors/` (e.g. `++` on `bool` should
report `E0112` / post-increment rules, not a generic lowering error). Implement **early**
validation in `lp-glsl-naga` (or diagnostics layer) so errors are attributed before opaque
`unsupported expression` paths.

## Code organization reminders

- Keep error codes centralized (`lp-glsl-diagnostics`) if codes are defined there.
- Small, explicit checks beat large pattern matches scattered through lowering.

## Implementation details

1. Run the failing files and list actual vs expected:

```bash
./scripts/glsl-filetests.sh type_errors/incdec-bool.glsl
./scripts/glsl-filetests.sh type_errors/incdec-non-lvalue.glsl
./scripts/glsl-filetests.sh type_errors/incdec-nested.glsl
./scripts/glsl-filetests.sh type_errors/expected-error-line-offset.glsl
```

2. Add validation passes or earlier Naga hooks so invalid inc/dec and lvalues fail with the
   **documented** codes and line numbers expected by the filetest format.

3. **Tests** — filetests are the primary oracle; add unit tests only if a helper is non-trivial.

## Validate

```bash
cargo test -p lp-glsl-naga
cargo test -p lp-glsl-filetests
./scripts/glsl-filetests.sh type_errors/
```

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
