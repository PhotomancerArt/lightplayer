# Phase 1: Move Legacy Source/Config Types Into `lpc-source`

Tag: `sub-agent: yes`
Parallel: `-`

## Scope of phase

Move the authored legacy source/config surface currently in `lp-core/lpl-model`
into `lp-core/lpc-source/src/legacy`.

In scope:

- Create `lpc_source::legacy` modules for legacy authored node configuration and
  source-side types.
- Move these `lpl-model` modules into `lpc-source::legacy`:
  - `glsl_opts.rs`
  - `nodes/kind.rs`
  - `nodes/mod.rs`
  - `nodes/texture/config.rs`
  - `nodes/texture/format.rs`
  - `nodes/shader/config.rs`
  - `nodes/fixture/config.rs`
  - `nodes/fixture/mapping.rs`
  - `nodes/output/config.rs`
- Keep the workspace compiling after this phase, even if `lpl-model` still
  exists temporarily as a re-export shim. This temporary shim is removed in a
  later phase.

Out of scope:

- Do not move runtime state types in this phase.
- Do not move `ProjectResponse`, `NodeChange`, `NodeDetail`, or legacy message
  aliases in this phase.
- Do not move concrete runtime implementations from `lpl-runtime`.
- Do not remove `lpl-model` from the workspace yet.
- Do not rename `LegacyProjectRuntime`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary compatibility code must be explicitly documented and should only
  point to the later cleanup phase.

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

Source files to move from:

- `lp-core/lpl-model/src/glsl_opts.rs`
- `lp-core/lpl-model/src/nodes/mod.rs`
- `lp-core/lpl-model/src/nodes/kind.rs`
- `lp-core/lpl-model/src/nodes/texture/config.rs`
- `lp-core/lpl-model/src/nodes/texture/format.rs`
- `lp-core/lpl-model/src/nodes/shader/config.rs`
- `lp-core/lpl-model/src/nodes/fixture/config.rs`
- `lp-core/lpl-model/src/nodes/fixture/mapping.rs`
- `lp-core/lpl-model/src/nodes/output/config.rs`

Target shape:

```text
lp-core/lpc-source/src/legacy/
├── mod.rs
├── glsl_opts.rs
└── nodes/
    ├── mod.rs
    ├── kind.rs
    ├── texture/
    │   ├── mod.rs
    │   ├── config.rs
    │   └── format.rs
    ├── shader/
    │   ├── mod.rs
    │   └── config.rs
    ├── fixture/
    │   ├── mod.rs
    │   ├── config.rs
    │   └── mapping.rs
    └── output/
        ├── mod.rs
        └── config.rs
```

Update `lp-core/lpc-source/src/lib.rs` to expose:

```rust
pub mod legacy;
```

Update `lp-core/lpc-source/Cargo.toml` only if needed. It already depends on
`lpc-model`, `serde`, and `hashbrown`, which should cover most moved config
types.

Keep `lp-core/lpl-model` compiling temporarily by changing its config/type
definitions into re-exports from `lpc_source::legacy` where practical. For
example:

```rust
pub use lpc_source::legacy::nodes::{NodeConfig, NodeKind};
pub use lpc_source::legacy::glsl_opts;
```

If a full re-export shim is easier than updating every caller in this phase, use
that. The important result is that the source of truth for authored legacy
configs is `lpc-source`, not `lpl-model`.

Update any obvious direct imports in files touched by this phase only. Do not
perform a workspace-wide import rewrite yet unless needed for validation.

## Validate

Run from workspace root:

```bash
cargo check -p lpc-source
cargo check -p lpl-model
cargo test -p lpc-engine
```

If validation fails because downstream crates need a small import/dependency
adjustment caused by this move, make the minimal fix in scope. If it becomes a
large workspace-wide rewrite, stop and report.
