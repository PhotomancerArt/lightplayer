# Phase 3: Move Legacy Runtime Implementation Into `lpc-engine`

Tag: `sub-agent: yes`
Parallel: `-`

## Scope of phase

Move concrete legacy runtime implementations from `lp-core/lpl-runtime` into
`lp-core/lpc-engine/src/legacy`, and make `lpc-engine` the direct owner of the
legacy shader -> texture -> fixture -> output runtime behavior.

In scope:

- Move concrete node runtime modules:
  - `nodes/texture`
  - `nodes/shader`
  - `nodes/fixture`
  - `nodes/output`
- Move `lpl-runtime/src/output` into `lpc-engine::legacy::output` or another
  clearly named engine module.
- Move `lpl-runtime/src/legacy_hooks.rs` behavior into `lpc-engine`, preferably
  as `lpc_engine::legacy::project` support called directly by
  `LegacyProjectRuntime`.
- Update imports to use `lpc_source::legacy` for configs and
  `lpc_wire::legacy` for state/protocol types.
- Keep behavior equivalent to existing tests.

Out of scope:

- Do not remove `lpl-runtime` crate from the workspace yet; Phase 4 owns final
  deletion and call-site cleanup.
- Do not keep or redesign the hook registry.
- Do not rename `LegacyProjectRuntime`.
- Do not port legacy nodes to the new `Node` trait.
- Do not design the final `Engine` API.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public entry points and important runtime flow before helpers.
- Place helper utility functions at the bottom of files.
- Keep tests at the bottom of Rust source files.
- Keep related runtime code grouped together.
- Avoid broad mechanical formatting churn outside moved files.

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

Move from:

```text
lp-core/lpl-runtime/src/
├── legacy_hooks.rs
├── nodes/
│   ├── texture/
│   ├── shader/
│   ├── fixture/
│   └── output/
└── output/
```

Move to a shape like:

```text
lp-core/lpc-engine/src/legacy/
├── mod.rs
├── project.rs
├── nodes/
│   ├── mod.rs
│   ├── texture/
│   ├── shader/
│   ├── fixture/
│   └── output/
└── output/
```

Update `lp-core/lpc-engine/src/lib.rs` to expose the moved output/provider and
legacy runtime types currently re-exported through `lpl-runtime` where callers
expect them via `lpc_engine`:

```rust
pub mod legacy;
pub use legacy::nodes::{FixtureRuntime, OutputRuntime, ShaderRuntime, TextureRuntime};
pub use legacy::output::{MemoryOutputProvider, OutputChannelHandle, OutputFormat, OutputProvider};
```

Use the actual local organization if a cleaner name already exists, but keep
exports stable for existing callers where possible.

Update `lp-core/lpc-engine/Cargo.toml` so `lpc-engine` has every dependency
needed by the moved runtime modules. Many shader/backend dependencies are
already present. Add `lpc-source` and `lpc-wire` feature wiring if needed.

`LegacyProjectRuntime` direct methods should no longer call hook registration
after this phase. They should call local engine implementation directly, for
example:

```rust
pub fn init_nodes(&mut self) -> Result<(), Error> {
    crate::legacy::project::init_nodes(self)
}
```

Equivalent direct calls are needed for:

- `init_nodes`
- `tick`
- `handle_fs_changes`
- `get_changes`

Keep lazy texture rendering behavior intact:

```text
FixtureRuntime::render
  -> RenderContext::get_texture
  -> ensure_texture_rendered
  -> ShaderRuntime renders into TextureRuntime
  -> FixtureRuntime writes output buffer
```

Do not introduce new public runtime contracts or a new `Engine` type.

## Validate

Run from workspace root:

```bash
cargo test -p lpc-engine
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

This phase touches runtime/compile paths. If the focused validation passes and
time allows, also run:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
```

If validation fails due to a small import or dependency fix, make the minimal
fix. If behavior changes or a non-obvious runtime bug appears, stop and report.
