# Phase 3: Cleanup And Validation

## Scope Of Phase

Clean up the M1.1 manual slice and record what it teaches before M2 codegen.

In scope:

- Remove scratch helpers, debug prints, and commented-out experiments.
- Ensure the manual codec test names clearly describe the covered concepts.
- Add `summary.md` for the M1.1 plan directory.
- Update the M1 summary or roadmap notes if M1.1 changes the M2 entry criteria.
- Run focused final validation.

Out of scope:

- Codegen.
- Production adoption.
- Broad workspace validation.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Expected `summary.md` sections:

- What was proven
- What still feels rough
- Reader/writer API changes needed before M2, if any
- Domain-shape deviations or recommended model changes
- Final validation commands and results

Checks to run:

- Search for scratch output:
  `rg -n "dbg!|eprintln!|println!|TODO" lp-core/lpc-slot-mockup/src/tests/manual_shape_codec.rs`
- Search for stale M1.1 references if files moved:
  `rg -n "manual_shape_codec|M1.1" docs/roadmaps/2026-05-13-slot-native-streaming-serialization lp-core/lpc-slot-mockup/src`

## Validate

```bash
cargo fmt
cargo test -p lpc-slot-mockup manual_shape_codec
cargo test -p lpc-slot-mockup native_stream
cargo test -p lpc-model slot_codec
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
```
