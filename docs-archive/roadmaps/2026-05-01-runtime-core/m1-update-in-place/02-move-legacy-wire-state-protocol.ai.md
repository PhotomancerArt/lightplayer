# Phase 2: Move Legacy State/Protocol Types Into `lpc-wire`

Tag: `sub-agent: yes`
Parallel: `-`

## Scope of phase

Move legacy runtime state and protocol payload types from `lp-core/lpl-model`
into `lp-core/lpc-wire/src/legacy`.

In scope:

- Create `lpc_wire::legacy` modules for legacy runtime state and client/server
  payloads.
- Move these `lpl-model` modules into `lpc-wire::legacy`:
  - `nodes/texture/state.rs`
  - `nodes/shader/state.rs`
  - `nodes/fixture/state.rs`
  - `nodes/output/state.rs`
  - `project/api.rs`
  - `project/mod.rs`
- Move legacy message aliases into `lpc_wire::legacy`:
  - `LegacyMessage`
  - `LegacyServerMessage`
  - `LegacyServerMsgBody`
- Update `lpc-wire` dependencies so it may depend on `lpc-source`.
- Keep the workspace compiling after this phase, even if `lpl-model` still
  exists temporarily as a re-export shim. This shim is removed later.

Out of scope:

- Do not move authored config/source types; Phase 1 owns those.
- Do not move concrete runtime implementations.
- Do not remove `lpl-model` from the workspace yet.
- Do not remove hook registration in this phase.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary compatibility code must be explicit and short-lived.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by an unexpected dependency cycle or design issue, stop and report.
- Report back: files changed, validation run, result, and any deviations.

## Implementation Details

Read shared context first:

- `docs/roadmaps/2026-05-01-runtime-core/m1-update-in-place/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m1-update-in-place/00-design.md`

Expected dependency direction:

```text
lpc-model <- lpc-source <- lpc-wire
```

`lpc-wire` may depend on `lpc-source` because source/config payloads can be sent
over the wire. `lpc-model` must not depend on `lpc-source` or `lpc-wire`.

Target shape:

```text
lp-core/lpc-wire/src/legacy/
├── mod.rs
├── project/
│   ├── mod.rs
│   └── api.rs
└── nodes/
    ├── mod.rs
    ├── texture/
    │   ├── mod.rs
    │   └── state.rs
    ├── shader/
    │   ├── mod.rs
    │   └── state.rs
    ├── fixture/
    │   ├── mod.rs
    │   └── state.rs
    └── output/
        ├── mod.rs
        └── state.rs
```

Update `lp-core/lpc-wire/src/lib.rs` to expose:

```rust
pub mod legacy;
```

Update `lp-core/lpc-wire/Cargo.toml` to depend on `lpc-source` with
`default-features = false`. Update the `std` feature to include
`lpc-source/std` if needed.

When moving `project/api.rs`, update imports:

- config/source types should come from `lpc_source::legacy`;
- status/envelope types should come from existing `lpc_wire` modules using
  `crate::...`;
- foundation types should come from `lpc_model`.

Keep `lp-core/lpl-model` compiling temporarily by re-exporting moved state and
protocol types from `lpc_wire::legacy` where practical. Do not add new behavior
to `lpl-model`; it is only a temporary compatibility layer until Phase 4.

Update imports in crates touched by this phase only when needed for validation.
Avoid broad formatting churn.

## Validate

Run from workspace root:

```bash
cargo check -p lpc-wire
cargo check -p lpl-model
cargo test -p lpc-engine
cargo check -p lpa-server
```

If validation fails because a small import path now needs
`lpc_wire::legacy` or `lpc_source::legacy`, fix it. If it turns into a large
workspace-wide migration, stop and report.
