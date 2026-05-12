# Phase 6: Cleanup And Validation

## Scope Of Phase

Clean up M2, document the source slot-root model, and run final validation.

In scope:

- Remove temporary debug code, commented-out experiments, and stale TODOs.
- Ensure docs and rustdocs explain source slot-root semantics.
- Update M2 summary after implementation.
- Run final validation.
- Review `examples/basic` TOML for consistency with the new source model.

Out of scope:

- Runtime node slot roots.
- Legacy detail removal.
- Production UI work.
- Push/CI unless separately requested.

## Code Organization Reminders

- Prefer durable docs near the code they explain.
- Keep examples concise and canonical.
- Do not leave generated `OUT_DIR` files in the repo.
- Keep tests at the bottom of Rust files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Audit commands:

```bash
git status --short
rg "TODO|dbg!|println!" lp-core/lpc-source lp-core/lpc-model lp-core/lpc-engine lp-core/lpc-wire lp-core/lpc-view lp-app lp-cli
rg "uid" examples/basic lp-core/lpc-source
rg "width|height|GpioStrip|ring_lamp_counts|\\[\\[" examples/basic lp-core/lpc-source lp-core/lpc-engine lp-cli
```

Expected cleanup:

- Keep intentional test `println!` evidence only in source test files where
  `--nocapture` output is useful.
- Make sure source static shape registration is generated and no manual shape
  registration lists were added.
- Write
  `docs/roadmaps/2026-05-06-slot-domain-cutover/m2-source-def-slot-roots/summary.md`
  after implementation.

Final validation:

```bash
cargo fmt --check --package lpc-model --package lpc-source --package lpc-slot-codegen --package lpc-slot-macros --package lpc-wire --package lpc-view --package lpc-engine --package lpa-client --package lpa-server --package lp-cli
cargo test -p lpc-model --lib --tests
cargo test -p lpc-source --lib --tests -- --nocapture
cargo test -p lpc-wire --lib --tests
cargo test -p lpc-view --lib --tests
cargo test -p lpc-engine --lib --tests
cargo check -p lpc-source --features schema-gen
cargo check -p lpc-engine
cargo check -p lpa-client
cargo check -p lpa-server
cargo check -p lp-cli
cargo clippy -p lpc-source --all-targets -- -D warnings
cargo clippy -p lpc-engine --all-targets -- -D warnings
cargo clippy -p lpc-wire --all-targets -- -D warnings
cargo clippy -p lpc-view --all-targets -- -D warnings
cargo clippy -p lpa-client --all-targets -- -D warnings
cargo clippy -p lpa-server --all-targets -- -D warnings
cargo clippy -p lp-cli --all-targets -- -D warnings
git diff --check
```
