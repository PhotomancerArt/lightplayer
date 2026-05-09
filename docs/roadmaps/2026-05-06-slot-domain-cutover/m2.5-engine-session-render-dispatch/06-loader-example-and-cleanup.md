# Phase 6: Loader, Example, Cleanup, And Validation

## Scope Of Phase

Finish cleanup after the session/render-product refactor and validate the milestone end-to-end.

In scope:

- Update project loader and canonical `examples/basic` as needed.
- Remove dead render-product registry code if no longer used.
- Remove transitional aliases for old session names if possible.
- Clean up rustdocs and comments so they describe current concepts.
- Run final validation.

Out of scope:

- New UI work.
- New wire protocol work.
- Texture registry.
- Fixture-to-output products.
- Server mutation.

## Code Organization Reminders

- Delete dead files rather than leaving vestigial modules.
- Keep docs free of milestone/process language such as “for this plan.”
- Keep filenames search-friendly and concept-oriented.
- Remove temporary TODOs unless they point to explicit future work captured in `future.md`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `examples/basic/project.toml`
- `examples/basic/fixture.toml`
- `lp-core/lpc-source/tests/basic_example_parse.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/render_product/mod.rs`
- `lp-core/lpc-engine/src/lib.rs`
- `lp-core/lpc-engine/src/resolver/mod.rs`
- `lp-core/lpc-engine/src/node/mod.rs`

Expected changes:

- Ensure `examples/basic` still represents the intended flow:

```text
shader.output -> bus#visual.out -> fixture.input -> output sink
```

- Ensure no canonical path depends on `RenderProductStore`.
- Remove stale imports and exports for deleted render-product store/id concepts.
- Confirm docs for `EngineSession`, `RenderProduct`, `RenderNode`, and `NodeEntryState::Executing` explain semantic roles clearly.
- Add `summary.md` with completed work and validation commands.

## Validate

```bash
cargo fmt -p lpc-engine -p lpc-model -p lpc-source -p lpc-shared -p lpc-slot-mockup
cargo check -p lpc-engine
cargo test -p lpc-engine
cargo test -p lpc-model
cargo test -p lpc-source --test basic_example_parse
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --features schema-gen
cargo clippy -p lpc-engine -p lpc-model -p lpc-source -p lpc-shared -p lpc-slot-mockup --all-targets -- -D warnings
```

