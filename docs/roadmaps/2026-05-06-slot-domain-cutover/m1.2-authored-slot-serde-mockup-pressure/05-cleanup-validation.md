# Phase 5: Cleanup And Validation

## Scope Of Phase

Clean up M1.2 and run final validation.

In scope:

- Remove temporary debugging artifacts.
- Ensure generated evidence files are not tracked.
- Ensure rustdocs explain authored serde behavior where it matters.
- Update plan summary.
- Run focused validation and fix warnings.

Out of scope:

- Real `lpc-source` conversion.
- Engine project loader changes.
- Full workspace validation.
- CI push/watch unless requested separately.

## Code Organization Reminders

- Keep docs close to the types they explain.
- Keep generated evidence under gitignored paths.
- Keep tests at the bottom of Rust files.
- Preserve filesystem-oriented concept-per-file organization.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Audit:

```bash
git status --short
git ls-files target/slot-mockup-evidence
rg "TODO|dbg!" lp-core/lpc-model lp-core/lpc-slot-mockup
rg "Versioned" lp-core/lpc-slot-mockup/src/tests/authored_serde.rs lp-core/lpc-model/src/slot/slots lp-core/lpc-model/src/slot/value_slot.rs
```

Expected cleanup:

- No generated evidence files are staged.
- No serde output includes `Versioned` internals.
- Plan summary exists at:
  `docs/roadmaps/2026-05-06-slot-domain-cutover/m1.2-authored-slot-serde-mockup-pressure/summary.md`
- M2 notes may be lightly updated to say M1.2 proved authored serde and mockup
  pressure before real source conversion.

Final validation:

```bash
cargo fmt --package lpc-model --package lpc-slot-mockup
cargo test -p lpc-model --lib --tests
cargo test -p lpc-model --features derive --test slot_record_derive
cargo test -p lpc-slot-mockup -- --nocapture
cargo check -p lpc-model --features schema-gen
cargo clippy -p lpc-model --all-targets -- -D warnings
cargo clippy -p lpc-slot-mockup --all-targets -- -D warnings
git diff --check
```
