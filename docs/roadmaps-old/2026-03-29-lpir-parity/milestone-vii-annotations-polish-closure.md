# Milestone VII: Annotations, polish, and closure

## Goal

Zero unexpected failures on all targets. All deferred items annotated. Plan and roadmap closed.

## Suggested plan name

`lpir-parity-milestone-vii`

## Scope

**In scope:**

### Annotation sweep

- **Struct files** (`struct/define-simple.glsl`, `struct/define-vector.glsl`): verify marked
  `@unimplemented()`.
- **Naga frontend limitations** (`matrixCompMult`, `mix(bvec)`, `while (bool j = expr)`): verify
  marked `@unimplemented(reason="Naga frontend limitation")`.
- **Q32 edge tests**: verify `@unsupported(float_mode=q32, reason="…")` on all IEEE-dependent
  cases.
- **Remove stale annotations**: run `LP_FIX_XFAIL=1 ./scripts/glsl-filetests.sh` (or `--fix`)
  to strip `@unimplemented` from tests that now pass.
- **Baseline tooling** (introduced for this roadmap): `--mark-unimplemented` /
  `LP_MARK_UNIMPLEMENTED=1` plus optional `--assume-yes`; document in README if the workflow
  changes again.

### Code quality

- `cargo +nightly fmt` on all touched crates.
- `cargo clippy -D warnings` on `lps-frontend`, `lpir`, `lpvm-cranelift`, `lps-filetests`,
  `lps-wasm`.
- Grep uncommitted diff for `TODO`, `dbg!`, `println!`, `HACK`, temporary code. Remove or
  convert to tracked issues.
- Fix all new warnings.

### Validation

- `./scripts/glsl-filetests.sh` → exit 0 (jit.q32).
- `./scripts/glsl-filetests.sh --target wasm.q32` → exit 0.
- `./scripts/glsl-filetests.sh --target rv32.q32` → exit 0.
- `cargo test -p lps-frontend -p lpir -p lpvm-cranelift -p lps-filetests -p lps-wasm`.
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`.
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`.

### Documentation

- Update `docs/reports/2026-03-29-lpir-feature-parity-audit.md` with final numbers.
- Update `lp-shader/lps-filetests/README.md` if annotation semantics or tooling changed.
- Write final parity report using the Milestone VI comparison tooling.

### Closure

- Move `docs/plans/2026-03-29-lpir-feature-parity/` to `docs/plans-done/`.
- Move `docs/roadmaps/2026-03-29-lpir-parity/` stays (roadmaps are not moved).
- Commit with Conventional Commits format.

**Out of scope:**

- New features beyond parity.
- Struct support.

## Key decisions

- One commit per milestone (or per logical unit if a milestone was implemented in stages), not one
  giant commit for the entire roadmap.

## Deliverables

- Clean `git status` — all work committed.
- All three targets at 0 unexpected failures.
- Updated reports and documentation.
- Plan directory moved to `plans-done`.

## Dependencies

All prior milestones complete.

## Estimated scope

Small. Primarily annotation review, formatting, and validation commands.
