# Phase 4: Cleanup and Final Validation

## Scope of phase

In scope:

- remove temporary debug artifacts and experiments
- tighten docs/comments around the new resumable compile path
- run final validation
- summarize measured behavior and any remaining limits

Out of scope:

- new feature additions beyond the planned incremental compile work

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep cleanup changes tightly scoped to artifacts introduced during this plan.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

- Search the diff for:
  - debug prints
  - stray TODOs
  - commented-out experiments
  - temporary profiling shims that should not stay
- Ensure one-shot compile paths still route through the resumable implementation cleanly.
- Add or refresh docs where needed so future warm-up work can discover the compile-job APIs.
- Write a short plan summary capturing:
  - what was built
  - measured tick-time behavior
  - measured memory behavior
  - any remaining hotspots or known limits

## Validate

Run:

```bash
rustup update nightly
cargo fmt --all
just check
just build-ci
just test
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```
