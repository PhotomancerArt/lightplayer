# Phase 7: Cleanup And Final Validation

## Scope Of Phase

Clean up, document, and run final validation.

In scope:
- Remove temporary code and debug artifacts.
- Update rsdocs where public APIs changed.
- Update `summary.md`.
- Run final validation.

Out of scope:
- New UI work.
- Follow-up memory watchdog/resource recovery work.

## Code Organization Reminders

- Keep one concept per file.
- Put tests at the bottom.
- Do not leave commented-out experiments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and deviations.

## Implementation Details

Search for:
- `TODO`
- `dbg!`
- `println!`
- commented-out experiments
- stale `render_size` direct-sampling assumptions

Write `summary.md` with what was built and key decisions.

## Validate

```bash
cargo fmt --check
git diff --check
cargo test -p lp-shader render_samples -- --nocapture
cargo test -p lpvm validate_render_samples -- --nocapture
cargo test -p lpc-model fixture -- --nocapture
cargo test -p lpc-engine fixture -- --nocapture
cargo test -p lpc-engine output_ -- --nocapture
cargo check -p lp-cli
```

