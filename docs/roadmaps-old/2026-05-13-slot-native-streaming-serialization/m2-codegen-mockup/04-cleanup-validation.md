# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up M2 and record the path into M3.

In scope:

- Remove scratch/debug output.
- Inspect generated output size/shape manually.
- Add `summary.md`.
- Run focused validation.
- Record any code-size risks before broader adoption.

Out of scope:

- Production adoption.
- Broad CI validation unless explicitly requested.

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

Summary should include:

- What generated code now covers.
- How much control flow is shared vs generated.
- Code-size concerns or likely monomorphization risks.
- Remaining rough edges before M3.

Checks:

- `rg -n "dbg!|eprintln!|println!|TODO" lp-core/lpc-slot-codegen/src lp-core/lpc-slot-mockup/src lp-core/lpc-model/src/slot_codec`
- Inspect generated file in `target` or copy a short representative snippet
  into the summary if useful.

## Validate

```bash
cargo fmt
cargo test -p lpc-model slot_codec
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup manual_shape_codec
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
```
