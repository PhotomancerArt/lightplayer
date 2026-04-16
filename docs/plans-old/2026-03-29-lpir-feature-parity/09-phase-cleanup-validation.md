# Phase 9: Cleanup, validation, plan closure

## Scope of phase

Remove temporary debug code, run the full validation matrix, write `summary.md`, move this plan
to `docs/plans-done/`, and commit with Conventional Commits.

## Cleanup & validation

1. **Grep the diff** for `TODO` left as placeholders, `dbg!`, `println!`, commented-out blocks,
   and temporary files (`08-harness-findings.md` scratch). Remove or convert to tracked docs.

2. **Format**

```bash
cargo +nightly fmt
```

3. **Tests and checks**

```bash
cargo test -p lps-frontend -p lpir -p lpvm-cranelift -p lps-filetests -p lps-wasm
./scripts/glsl-filetests.sh
just test-filetests
```

(if `justfile` defines a stricter matrix — use it)

4. **Embedded gate** (required for shader pipeline changes per workspace rules)

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

5. **Clippy** (if the repo expects it for touched crates)

```bash
cargo clippy -p lps-frontend -p lpir -p lpvm-cranelift -p lps-filetests -p lps-wasm -- -D warnings
```

6. Fix **all** new warnings introduced by this plan’s work.

## Plan cleanup

- Add **`summary.md`** to this directory with:
    - What shipped (phases completed)
    - Known follow-ups (arrays/structs, any deferred harness work)
    - Final filetest pass rates (jit / wasm / rv32) if measured
- **Move** the entire `docs/plans/2026-03-29-lpir-feature-parity/` directory to
  `docs/plans-done/2026-03-29-lpir-feature-parity/` (preserve date + name).

## Commit

Example message (adjust body to match actual changes):

```
feat(lps): LPIR filetest parity (matrix, relational, invoke)

- Lower Relational (all/any/not, vector isnan/isinf) in lps-frontend
- Matrix metadata, stores, builtins; Cranelift invoke sret for large returns
- WASM parity checks; type_errors diagnostics; Q32 edge @unsupported
- Filetest harness fixes; rename stats field unsupported
```

## Code organization reminders

- Final commit should contain **no** stray temporary code.
- Prefer one logical commit for the plan if history allows; otherwise a small ordered series with
  each commit building and testing clean.
