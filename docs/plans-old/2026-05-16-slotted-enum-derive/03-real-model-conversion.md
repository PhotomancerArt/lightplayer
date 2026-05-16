# Phase 3: Real Model Conversion

## Scope Of Phase

Convert real model structured slot enums to `#[derive(Slotted)]` and remove the
manual slot enum machinery they currently carry.

In scope:

- `NodeDef`
- `MappingConfig`
- `PathSpec`
- tests around artifact root loading and fixture mapping

Out of scope:

- unrelated atomic value enums such as `RingOrder`
- broad serde removal
- changing public authored TOML syntax

## Code Organization Reminders

- Keep `NodeArtifact(pub EnumSlot<NodeDef>)` as the artifact/root wrapper.
- Keep `NodeDef` as a raw enum payload, not a direct `SlotAccess` root.
- Keep constructors and semantic helpers where useful.
- Remove generated-equivalent manual code aggressively.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Primary files:

- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/nodes/fixture/mapping.rs`
- `lp-core/lpc-model/src/nodes/mod.rs`
- `lp-core/lpc-model/src/node/mod.rs`

Convert `NodeDef`:

```rust
#[derive(Clone, Debug, PartialEq, Slotted)]
pub enum NodeDef {
    #[default]
    Project(ProjectDef),
    Texture(TextureDef),
    Shader(ShaderDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}
```

Remove manual impls if derive fully replaces them:

- `Default`
- `SlotEnumShape`
- `SlottedEnum`
- `SlottedEnumMut`

Keep `SlotAccess for NodeDef` only if existing code still relies on treating a
loaded `NodeDef` as a delegating runtime object. If it can be removed cleanly,
prefer removing it; otherwise leave it as an explicit compatibility shim.

Convert `MappingConfig` and `PathSpec`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Slotted)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MappingConfig {
    #[default]
    Unset,
    PathPoints {
        pub paths: MapSlot<u32, EnumSlot<PathSpec>>,
        pub sample_diameter: PositiveF32Slot,
    },
}
```

Add the neutral `Unset` variant to `MappingConfig` as the default nop state.
Existing authored examples/tests that actually configure mapping data should
use `PathPoints` explicitly.

For `PathSpec`, use `#[default]` on `RingArray`. The slot discriminator should be `RingArray`, even while serde may temporarily still accept/emit `ring_array` until serde is removed from this path.

Remove manual mapping/path slot impls and shape helper functions once replaced.
Keep `RingOrder` as a `SlotValue`; it is an atomic enum-like value, not a
structured slot enum.

## Validate

```bash
cargo fmt -p lpc-model
cargo test -p lpc-model nodes::node_def
cargo test -p lpc-model nodes::fixture::mapping
cargo test -p lpc-model slot_codec
cargo test -p lpc-engine project_loader
```
