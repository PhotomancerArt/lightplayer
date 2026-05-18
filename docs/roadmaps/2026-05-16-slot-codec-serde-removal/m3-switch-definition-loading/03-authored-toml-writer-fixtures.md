# Phase 3: Switch Authored TOML Fixture Writes

## Scope Of Phase

In scope:

- update `ProjectBuilder` node artifact writes to use SlotCodec TOML for
  slotted model payloads
- keep explicit `kind = "..."` wrapper metadata in generated authored files
- keep or improve the existing project-builder test
- add coverage that generated artifacts can be loaded by `ProjectLoader`

Out of scope:

- creating a polished public authored writer API unless it naturally falls out
  small
- changing authored TOML syntax for aesthetic reasons
- removing serde derives

## Code Organization Reminders

- A local helper in `builder.rs` is fine if the helper is only test-project
  fixture infrastructure.
- If a reusable helper is added to `NodeDef`, make it clearly about authored
  TOML wrapper writing.
- Avoid hidden hard-coded shape lists outside the `NodeDef` wrapper machinery.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant file:

- `lp-core/lpc-shared/src/project/builder.rs`

Current writer pattern:

```rust
let toml = prepend_kind(
    "texture",
    toml::to_string(&config).expect("Failed to serialize texture def to TOML"),
);
```

Desired pattern:

1. Create/register a `SlotShapeRegistry`.
2. Call `registry.write_slot_toml(&config)` or
   `registry.write_slot_toml_data(config.shape_id(), config.data())`.
3. Serialize the returned `toml::Value` to text.
4. Prepend or inject `kind`.

If `toml::Value` display formatting is awkward, keep the helper small and
document any formatting difference in `summary.md`.

## Validate

```bash
cargo test -p lpc-shared project::builder
cargo test -p lpc-engine project_loader
```
