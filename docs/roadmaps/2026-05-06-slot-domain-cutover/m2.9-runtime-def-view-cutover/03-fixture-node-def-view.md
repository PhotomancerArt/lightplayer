# Phase 3: Fixture Node Def View

## Scope Of Phase

Move fixture scalar config reads to `FixtureDefView` while keeping aggregate
mapping and output sink setup loader-owned.

In scope:

- Add `FixtureDefView` cache to `FixtureNode`.
- Remove scalar config parameters from `FixtureNode::new`.
- Resolve `render_size`, `color_order`, `brightness`, and `gamma_correction`
  through `TickContext` during tick.
- Keep `mapping`, `mapping_version`, and `output_sink` constructor-provided.
- Preserve existing fixture render/output behavior.

Out of scope:

- Resolver-backed `mapping`.
- Dynamic lamp-color buffer resizing for mapping changes.
- Output flow redesign.

## Code Organization Reminders

- Keep conversion helpers for optional config values close to fixture node code.
- Avoid adding a generic optional-slot reader unless multiple concrete uses
  need it immediately.
- Tests stay at the bottom of the file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`

Expected changes:

- Change `FixtureNode::new` to take only:
  - mapping;
  - mapping revision;
  - output sink buffer id;
  - any truly runtime-only setup values.
- Add `def_view: Option<FixtureDefView>`.
- In `tick()`, resolve scalar config before rendering:
  - `render_size` as `Dim2u`;
  - `color_order` as `ColorOrder`;
  - `brightness` with the same default behavior as today;
  - `gamma_correction` with the same default behavior as today.
- Update precompute cache key to use the resolved render size and existing
  mapping revision.
- Add/update tests proving a binding can override at least one scalar fixture
  config value.

Constraints:

- If optional slot values cannot be resolved ergonomically through the current
  value helper, keep the narrowest local conversion needed and document the
  limitation in `future.md`.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine fixture
cargo test -p lpc-engine project_runtime::core_project_runtime
```
