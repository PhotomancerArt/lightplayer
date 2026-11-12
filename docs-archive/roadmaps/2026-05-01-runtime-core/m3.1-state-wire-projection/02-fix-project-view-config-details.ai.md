# Phase 2: Fix ProjectView Config Details

## Scope of Phase

Make `ProjectView` apply real config details from legacy `GetChanges` responses
instead of replacing them with placeholder configs.

In scope:

- Update `lpc-view` so `NodeEntryView` can faithfully store concrete legacy
  config data received through `NodeDetail`.
- Update `ProjectView::apply_changes` so `node_details` refresh the stored
  config and merge state as before.
- Add focused tests in `lpc-view` for config detail application and config
  updates.

Out of scope:

- Wire serialization changes in `lpc-wire`; that is Phase 3.
- Runtime response generation changes in `lpc-engine`.
- New product/buffer storage or sync protocol redesign.
- Replacing `ProjectResponse::GetChanges`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file if a helper type grows.
- Place public types / entry points first, then direct support, helpers near the
  bottom, tests at the end.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by an unexpected public API choice, stop and report rather than
  improvising.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

Primary files:

- `lp-core/lpc-view/src/project/project_view.rs`
- `lp-core/lpc-view/tests/client_view.rs`

Useful source config types:

- `lpc_source::legacy::nodes::texture::TextureConfig`
- `lpc_source::legacy::nodes::shader::ShaderConfig`
- `lpc_source::legacy::nodes::output::OutputConfig`
- `lpc_source::legacy::nodes::fixture::FixtureConfig`
- `lpc_source::legacy::nodes::NodeConfig`

Current problem:

- `NodeEntryView.config` is `Box<dyn NodeConfig>`.
- `ProjectView::apply_changes` creates placeholder configs for `Created`.
- When `node_details` are present, it still replaces `entry.config` with a new
  placeholder based on `entry.kind`, instead of copying `detail.config`.

Preferred implementation:

1. Add a helper in `project_view.rs` that clones a `Box<dyn NodeConfig>` by
   downcasting based on `config.kind()`.

   Example shape:

   ```rust
   fn clone_node_config(config: &dyn NodeConfig) -> Result<Box<dyn NodeConfig>, String> {
       match config.kind() {
           NodeKind::Texture => {
               let config = config
                   .as_any()
                   .downcast_ref::<TextureConfig>()
                   .ok_or_else(|| String::from("failed to downcast TextureConfig"))?;
               Ok(Box::new(config.clone()))
           }
           // same for Shader, Output, Fixture
       }
   }
   ```

   Keep the helper near the bottom of the file, above tests if any.

2. Use this helper when applying an existing node detail:

   - Replace placeholder config construction with `clone_node_config(&*detail.config)?`.
   - Then merge or set `entry.state` as the current code does.
   - Keep version updates from `ConfigUpdated` and `StateUpdated` unchanged unless
     a test exposes a clear bug.

3. Use the same helper when creating an entry from detail only.

   - Infer `kind` from `detail.state` as the current code does.
   - Store the cloned real detail config.
   - Preserve the current pending-status behavior.

4. It is acceptable for `Created` without details to keep placeholder configs;
   no real config is available in that branch.

Tests to add in `lp-core/lpc-view/tests/client_view.rs`:

- Applying a texture detail stores the real `TextureConfig { width, height }`.
- Applying an output detail stores the real `OutputConfig::GpioStrip { pin, options }`.
- Applying a later detail after `ConfigUpdated` replaces the old stored config.
- A detail-only entry stores the real config instead of a placeholder.

Use `entry.config.as_any().downcast_ref::<...>()` in assertions.

## Validate

Run:

```bash
cargo test -p lpc-view
```
