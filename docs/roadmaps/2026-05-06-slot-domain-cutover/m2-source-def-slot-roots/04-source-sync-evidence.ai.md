# Phase 4: Source Sync Evidence

## Scope Of Phase

Build the source-side evidence harness for full source slot roots using real
TOML files.

In scope:

- Add source tests that load `examples/basic/project.toml` and referenced child
  node TOML files.
- Register generated source shapes.
- Build a list of source roots and snapshot them through `lpc-wire`.
- Print readable server/client tree evidence.
- Assert important paths and values across project, shader, texture, output,
  and fixture.
- Add focused shader `param_defs` test data if `examples/basic` remains empty.

Out of scope:

- Production UI work.
- Runtime node roots.
- Project sync replacement.
- Client mutation.

## Code Organization Reminders

- Prefer `src/tests/fixture.rs` or a similarly named helper file if test setup
  becomes non-trivial.
- Keep test output readable; evidence logs should help a human see the model.
- Keep assertions shape-aware rather than relying only on string contains.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-source/src/tests/mod.rs`
- `lp-core/lpc-source/src/tests/source_slot_roots.rs`
- `lp-core/lpc-source/src/tests/source_slot_fixture.rs`
- `lp-core/lpc-wire/src/slot/access_sync.rs`
- `lp-core/lpc-view/src/slot/mirror.rs` if a client mirror is useful
- `examples/basic/*.toml`

Expected test flow:

1. Set a known `current_state_version`.
2. Load `examples/basic/project.toml`.
3. Resolve each `[nodes.<name>] artifact` path relative to the project file.
4. Deserialize the child defs by known node name or loaded `kind`.
5. Register source shapes with `lpc_source::slot_shapes`.
6. Build source roots such as:
   - `source.project`
   - `source.shader`
   - `source.texture`
   - `source.output`
   - `source.fixture`
7. Snapshot with `lpc_wire::build_slot_full_sync`.
8. Optionally apply to `lpc_view::SlotMirrorView`.
9. Print tree walks.
10. Assert representative paths and values.

Keep this source-side. Do not route through `lpc-engine` unless unavoidable.

## Validate

```bash
cargo fmt --package lpc-source
cargo test -p lpc-source --lib --tests -- --nocapture
cargo test -p lpc-wire --lib --tests
cargo test -p lpc-view --lib --tests
```
