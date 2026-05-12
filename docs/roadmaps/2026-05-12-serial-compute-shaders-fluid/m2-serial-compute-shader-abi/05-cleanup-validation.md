# Phase 5: Cleanup And Validation

## Scope Of Phase

Clean up temporary code, verify docs, and run final validation for M2.

In scope:

- Remove debugging artifacts and commented experiments.
- Ensure rustdocs explain the new compute ABI and global lifecycle.
- Ensure tests are organized at the bottom of files.
- Run final validation.
- Update roadmap notes/todo if implementation discovers follow-up work.

Out of scope:

- New feature work beyond fixing issues found by validation.
- M3 planning or implementation.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Tests go at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Checklist:

- New public types have clear rustdocs:
  - `CompileComputeDesc`
  - `LpsComputeShader`
  - compute ABI validation types/errors
  - `LpvmInstance` global access methods
- Docs do not mention "this plan" or implementation roadmap context inside
  Rust source.
- No temporary `println!`, `dbg!`, or commented-out experiments.
- Unsupported future work is captured in plan `future.md` only if there is
  specific context worth saving.
- `summary.md` describes what landed and what M3 should build on.

## Validate

```bash
cargo fmt --check
cargo test -p lpvm
cargo test -p lp-shader
cargo test -p lpc-model
cargo check -p lpc-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If frontend lowering changed:

```bash
cargo test -p lps-frontend
```

