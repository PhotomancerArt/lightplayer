# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up the M1.1 implementation and run final validation.

In scope:

- Remove stray debug output, commented experiments, and accidental TODOs.
- Ensure rustdocs explain inferred slot fields and root shape id behavior.
- Run final targeted validation.
- Write `summary.md`.

Out of scope:

- Real source def conversion.
- Full workspace validation.
- CI push/watch workflow unless requested separately.

## Code Organization Reminders

- Keep documentation close to the traits/macros it explains.
- Avoid suppressing warnings.
- Leave unrelated dirty files alone.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report what changed, what was validated, and deviations.

## Implementation Details

Check:

- `rg "TODO|dbg!|println!" lp-core/lpc-model lp-core/lpc-slot-macros lp-core/lpc-slot-mockup`
  - Existing intentional test `println!` output in `lpc-slot-mockup/src/tests` is allowed.
- `rg "#\\[slot\\((value|leaf|record|map|option_ref|enum)|shape_id =" lp-core/lpc-slot-mockup/src lp-core/lpc-model/tests`
  - Remaining explicit attrs should be justified compatibility overrides.

Write:

- `docs/roadmaps/2026-05-06-slot-domain-cutover/m1.1-slot-derive-inference/summary.md`

## Validate

```bash
cargo fmt
cargo test -p lpc-model -p lpc-slot-mockup --lib --tests
cargo check -p lpc-engine -p lpa-client -p lpa-server -p lp-cli
```

Optional clippy if time permits:

```bash
cargo clippy -p lpc-model --all-targets -- -D warnings
cargo clippy -p lpc-slot-mockup --all-targets -- -D warnings
```
