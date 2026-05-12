# Phase 4: Remove `lpl-*` Crates, Hooks, and Install Call Sites

Tag: `sub-agent: yes`
Parallel: `-`

## Scope of phase

Remove the temporary legacy split artifacts after the source, wire, and runtime
types have moved into `lpc-*` crates.

In scope:

- Delete `lp-core/lpl-model`.
- Delete `lp-core/lpl-runtime`.
- Remove the crates from workspace members and all `Cargo.toml` dependencies.
- Remove hook registration artifacts:
  - `LegacyProjectHooks`
  - `set_project_hooks`
  - `with_hooks`
  - `project_hooks::install`
  - every `lpl_runtime::install()` call site
- Update imports across the workspace from `lpl_model` and `lpl_runtime` to:
  - `lpc_source::legacy` for authored configs/source types;
  - `lpc_wire::legacy` for state/protocol/message types;
  - `lpc_engine::legacy` or `lpc_engine` re-exports for runtime/provider types.
- Update Cargo feature wiring so host and firmware builds still include the
  on-device shader compiler path.

Out of scope:

- Do not rename `LegacyProjectRuntime`.
- Do not remove legacy behavior or weaken tests.
- Do not introduce compatibility shim crates.
- Do not design the final `Engine` API.

## Code Organization Reminders

- Prefer targeted import rewrites over broad unrelated refactors.
- Keep related module exports grouped together.
- Remove dead modules and dependencies completely.
- Do not leave commented-out code.
- Keep tests at the bottom of Rust source files.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by an unexpected dependency or public API issue, stop and report.
- Report back: files changed, validation run, result, and any deviations.

## Implementation Details

Read shared context first:

- `docs/roadmaps/2026-05-01-runtime-core/m1-update-in-place/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m1-update-in-place/00-design.md`

Search for all remaining references before editing:

```bash
rg "lpl_model|lpl-model|lpl_runtime|lpl-runtime|LegacyProjectHooks|set_project_hooks|with_hooks|project_hooks::install|\\.install\\(" .
```

Do not use `grep`; use `rg`.

Expected import replacements:

- `lpl_model::nodes::texture::TextureConfig` ->
  `lpc_source::legacy::nodes::texture::TextureConfig`
- `lpl_model::nodes::texture::TextureFormat` ->
  `lpc_source::legacy::nodes::texture::TextureFormat`
- `lpl_model::nodes::fixture::MappingConfig` ->
  `lpc_source::legacy::nodes::fixture::MappingConfig`
- `lpl_model::NodeConfig` / `NodeKind` ->
  `lpc_source::legacy::nodes::NodeConfig` / `NodeKind` unless earlier phases
  placed them elsewhere by design.
- `lpl_model::NodeState` / `ProjectResponse` / `NodeChange` / `NodeDetail` ->
  `lpc_wire::legacy::...`
- `lpl_model::LegacyMessage` / `LegacyServerMessage` ->
  `lpc_wire::legacy::LegacyMessage` / `LegacyServerMessage`
- `lpl_runtime::MemoryOutputProvider` and output provider types ->
  `lpc_engine::MemoryOutputProvider` or `lpc_engine::legacy::output::...`,
  depending on final exports from Phase 3.

Update these likely areas:

- root `Cargo.toml`
- `Cargo.lock` via Cargo commands, not by hand if possible
- `lp-core/lpc-engine/Cargo.toml`
- `lp-core/lpc-view/Cargo.toml`
- `lp-core/lpc-shared/Cargo.toml`
- `lp-app/lpa-server/Cargo.toml`
- `lp-app/lpa-client/Cargo.toml`
- firmware crate `Cargo.toml` files that mention `lpl-*`
- `lp-cli` imports and `Cargo.toml`
- `fw-tests` imports and `Cargo.toml`

After dependency updates, run `cargo check` commands so `Cargo.lock` is updated
by Cargo as needed.

Remove obsolete hook files/modules from `lpc-engine`, including
`legacy_project/hooks.rs` if it still exists.

## Validate

Run from workspace root:

```bash
cargo test -p lpc-engine
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p lpa-client
cargo check -p lp-cli
```

Because this phase removes crates and touches runtime/compile paths, also run:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If validation fails from a small import/dependency omission, fix it. If a
non-obvious runtime behavior bug appears, stop and report.
