# Phase 5: Cleanup And Validation

## Scope Of Phase

Clean up the implementation and run final validation appropriate for fixture pipeline changes.

In scope:

- Remove temporary debugging artifacts.
- Remove commented-out experiments.
- Check parser errors for useful context.
- Ensure docs/comments clearly state this is a constrained SVG subset.
- Run final validation commands.

Out of scope:

- Adding permanent SVG authoring UX.
- Expanding parser support beyond the planned subset.
- Hardware manual validation unless explicitly requested.

## Code Organization Reminders

- Keep parser module names search-friendly.
- Keep tests at the bottom of source files.
- Avoid broad refactors unrelated to SVG path mapping.
- Do not leave stale TODOs unless they point to real future work and are useful.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Review these files after implementation:

- `lp-core/lpc-model/src/nodes/fixture/mapping.rs`
- `lp-core/lpc-engine/src/nodes/fixture/mapping/points.rs`
- `lp-core/lpc-engine/src/nodes/fixture/mapping/svg_path/*`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `examples/*/fixture.toml`
- `examples/*/fyeah-mapping.svg`

Checklist:

- `SvgPath` authored mapping never reaches runtime precompute unresolved.
- Parser ignores irrelevant SVG content without treating it as an error.
- Parser rejects ungrouped text starting with `path:`.
- Parser rejects curve commands inside mapping groups.
- Parser errors are specific when a group appears to be a mapping group but is malformed.
- Aspect-fit behavior has a direct test.
- Channel assignment is deterministic and end-to-end by sorted `path:N`.
- No new dependency pulls `std` into embedded-only code paths.
- Firmware compiler path remains included.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model nodes::node_def
cargo test -p lpc-engine nodes::fixture::mapping
cargo test -p lpc-engine engine::project_loader
cargo test -p lp-cli --test examples_valid
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If touching only the plan files, do not run this phase. These commands are for implementation.
