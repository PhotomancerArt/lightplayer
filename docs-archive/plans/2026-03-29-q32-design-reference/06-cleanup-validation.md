# Phase 6: Cleanup & validation

## Cleanup

Grep the git diff for:

- TODO comments added during this plan
- Debug `println!` statements
- Temporary code
- Leftover `@unimplemented(backend=jit)` annotations on tests this plan fixed

Remove them.

## Validation

Run the full validation suite:

```bash
# Q32 struct and builtin tests
cargo test -p lps-builtins -- q32
cargo test -p lps-builtins -- fdiv_q32

# Filetests
cargo test -p lps-filetests

# ESP32 build (compiler included)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# Emulator build
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Host builds
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

Fix all warnings, errors, and formatting issues:

```bash
cargo +nightly fmt
```

## Plan cleanup

Add a summary of the completed work to
`docs/plans/2026-03-29-q32-design-reference/summary.md`.

Move the plan files to `docs/plans-done/2026-03-29-q32-design-reference/`.

## Commit

```
docs(q32): add Q32 design doc and align reference implementation

- Add docs/design/q32.md as single source of truth for Q16.16 semantics
- Make Q32 struct operators saturating (add, sub, mul, div, mul_int)
- Fix Q32 div-by-zero: 0/0→0, pos/0→MAX, neg/0→MIN
- Fix JIT fdiv builtin 0/0 case to return 0
- Fix mismatched constant comments (E, PHI)
- Add comprehensive Q32 edge-case tests
- Update filetests: isinf/isnan Q32 annotations, div-by-zero coverage
```
