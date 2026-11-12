# Phase 7: Cleanup And Validation

## Scope Of Phase

Remove obsolete push-path code, clean docs, and run final targeted validation.

In scope:

- Remove stale output sink terminology where no longer accurate.
- Remove dead helpers and tests tied only to fixture-pushed buffers.
- Update rustdocs for products, outputs, fixtures, and runtime services.
- Update roadmap todo if this milestone closes any listed items.
- Run final validation commands.

Out of scope:

- Full workspace cargo commands that include RV32-only crates on host.
- New feature work beyond cleanup needed for this milestone.

## Code Organization Reminders

- Delete unused speculative code aggressively.
- Keep concept-per-file organization.
- Do not leave commented-out experiments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

Search for and clean stale terms as appropriate:

- `RenderProduct` when it means visual product.
- `output_sink` when output no longer acts as passive sink.
- `FixtureDef.output_loc`.
- fixture demand-root assumptions.
- comments saying fixtures push to output.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine
cargo clippy -p lpc-engine -p lpc-model --all-targets -- -D warnings
```
