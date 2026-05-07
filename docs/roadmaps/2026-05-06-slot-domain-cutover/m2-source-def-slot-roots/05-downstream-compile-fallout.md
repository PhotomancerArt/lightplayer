# Phase 5: Downstream Compile Fallout

## Scope Of Phase

Update downstream crates that consume `lpc-source` defs so they compile and keep
their current behavior after source fields become slot-aware.

In scope:

- Fix compile errors in:
  - `lpc-engine`
  - `lpc-wire`
  - `lpc-view`
  - `lpa-client`
  - `lpa-server`
  - `lp-cli`
- Update source field reads to use `.value()` or small helper methods.
- Update `examples/basic` assumptions in tests and CLI scaffolding.
- Keep behavior changes limited to the source model shape changes required by
  M2.

Out of scope:

- Replacing project sync.
- Runtime node slot roots.
- Large engine API cleanup.
- Client-driven production mutation.

## Code Organization Reminders

- Prefer small helper methods on source defs when repeated `.value()` chains
  become noisy.
- Do not hide version semantics behind broad implicit conversions.
- Keep compatibility projection names explicitly legacy where they remain.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Likely relevant files:

- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/resolver/*`
- `lp-core/lpc-wire/src/project/*`
- `lp-app/lpa-client/src/*`
- `lp-app/lpa-server/src/*`
- `lp-cli/src/commands/create/project.rs`

Expected changes:

- Replace direct field access on slot-aware source fields with `.value()` or
  local source helper methods.
- Update `TextureDef` consumers from `width`/`height` to `size.value().width`
  / `size.value().height` if `TextureDef.size` lands.
- Update `OutputDef` consumers from enum matching to direct struct fields.
- Update fixture mapping consumers for keyed map data.
- Keep project loading by TOML artifact references intact.

Be careful with `no_std`: do not add host-only conveniences to source/runtime
paths.

## Validate

```bash
cargo fmt --package lpc-source --package lpc-engine --package lpc-wire --package lpc-view --package lpa-client --package lpa-server --package lp-cli
cargo check -p lpc-engine
cargo check -p lpa-client
cargo check -p lpa-server
cargo check -p lp-cli
cargo test -p lpc-engine --lib --tests
```
