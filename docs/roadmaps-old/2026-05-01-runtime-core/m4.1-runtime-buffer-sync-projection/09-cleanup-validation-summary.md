# Phase 9: Cleanup, validation, and summary

## Scope of phase

Clean up M4.1 implementation, run validation, and write the milestone summary.

In scope:

- Remove stray temporary code, debug prints, commented-out code, and accidental
  TODOs.
- Run formatting and validation.
- Update roadmap notes/future docs if implementation revealed follow-ups.
- Add `summary.md` for M4.1.

Out of scope:

- New feature work.
- M4.2/M4.3/M4.5 implementation.
- Manual `just demo` validation beyond documenting that user must run it.

## Code organization reminders

- Keep summary concise.
- Decisions should capture only non-obvious future-relevant choices.
- Do not archive roadmap plan directories.

## Sub-agent reminders

- Do not commit.
- Do not suppress warnings.
- Do not make broad refactors.
- If validation fails with a non-trivial bug, stop and report.

## Implementation details

Search the diff for:

- `TODO`
- `todo!`
- `unimplemented!`
- `dbg!`
- `println!`
- `#[allow`
- commented-out code that looks temporary

Do not remove legitimate pre-existing docs/TODOs outside this plan unless they
were touched by M4.1 and are now stale.

Write `docs/roadmaps/2026-05-01-runtime-core/m4.1-runtime-buffer-sync-projection/summary.md`
with:

- "What was built"
- "Decisions for future reference"

Mention manual validation status clearly: `just demo` should be run by the user
because the current UI is visual and temporary.

## Validate

Run:

```bash
cargo +nightly fmt
cargo test -p lpc-model
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-engine --test scene_render --test scene_update --test partial_state_updates
cargo test -p lpa-server
cargo check -p lp-cli
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
git diff --check
```
