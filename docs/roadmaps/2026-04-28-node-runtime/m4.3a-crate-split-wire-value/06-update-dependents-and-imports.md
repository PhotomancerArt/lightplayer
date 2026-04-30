# Phase 6 — Update Dependents + Imports

## Scope of phase

Update workspace dependencies and import paths across crates after the
`lpc-model` / `lpc-source` / `lpc-wire` / `lpc-engine` split.

Out of scope:

- Do not introduce new behavior.
- Do not do broad naming cleanup beyond what is required for compilation.
- Do not rename existing non-core `lp-*` crates.
- Do not weaken tests or skip compile failures.
- Do not commit.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by a design mismatch in the crate split, stop and report.
- Report back: files changed, validation run, validation result, and any
  deviations from this phase.

## Implementation details

Update `Cargo.toml` dependencies and Rust imports across known consumers.

Known crates to inspect:

- `lp-core/lpc-engine`
- `lp-core/lp-engine-client`
- `lp-core/lp-server`
- `lp-core/lp-client`
- `lp-legacy/lpl-model`
- `lp-legacy/lpl-runtime`
- `lpfx/lpfx`
- `lpfx/lpfx-cpu`
- `lp-fw/fw-core`
- `lp-fw/fw-tests`
- `lp-cli`
- any tests/filetest helpers that import moved `lpc-model` types

Use these dependency rules:

- Shared concepts (`NodeId`, `TreePath`, `PropPath`, `FrameId`, `Kind`,
  `WireValue`, `WireType`, `ChannelName`) come from `lpc-model`.
- Authored source/on-disk concepts (`SrcArtifact`, `ArtifactSpec`,
  `SrcBinding`, `SrcShape`, `SrcSlot`, `SrcValueSpec`, `SrcNodeConfig`,
  schema/migration/load helpers) come from `lpc-source`.
- Engine-client wire concepts (`WireMessage`, `WireTreeDelta`,
  `WireEntryState`, `WireProjectHandle`, transport/json helpers, legacy state
  serialization helpers) come from `lpc-wire`.
- Runtime/engine concepts (`Bus`, `ResolverCache`, `RuntimePropAccess`,
  conversion helpers) come from `lpc-engine`.
- Client-side view/cache helpers (`WirePropAccess`) come from
  `lp-engine-client`.

Use `rg` rather than shell `grep` if searching. Look specifically for stale
references such as:

- `lpc_model::LpsValue`
- `lpc_model::LpsType`
- `lpc_model::Artifact`
- `lpc_model::Binding`
- `lpc_model::Shape`
- `lpc_model::Slot`
- `lpc_model::ValueSpec`
- `lpc_model::TreeDelta` / `WireTreeDelta` (after split: prefer
  `lpc_wire::WireTreeDelta`; drop old `tree_delta` imports)
- `lpc_model::EntryStateView`
- `lpc_model::ClientMessage`
- `lpc_model::ServerMessage`
- `lpc_protocol`
- `lpc_artifact`
- `lpc_runtime`

Do not leave broad compatibility re-exports in `lpc-model` just to avoid
updating imports. This milestone is explicitly about clear roles.

## Tests to preserve/add

No new behavior tests are required in this phase. Preserve existing tests and
update imports/types as needed.

## Validate

Run targeted checks first:

```bash
cargo check -p lpc-model -p lpc-source -p lpc-wire -p lpc-engine -p lpc-view
cargo check -p lpl-model -p lpl-runtime
cargo check -p lpa-server -p lpa-client
cargo check -p lpfx -p lpfx-cpu
```

Then run host build for default members:

```bash
just build-host
```

If formatting changed, run:

```bash
cargo +nightly fmt
```
