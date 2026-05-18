# Phase 3: Convert Semantic Leaves To `ValueSlot<T>`

## Scope Of Phase

Delete duplicated semantic slot containers and replace them with semantic values plus type aliases.

In scope:

- Convert representative leaves first, then continue aggressively through `lpc-model/src/slots`.
- Replace hand-written `FooSlot { inner: WithRevision<T> }` structs with `pub type FooSlot = ValueSlot<Foo>`.
- Use `#[derive(SlotValue)]` where it fits.
- Keep manual `SlotValue` impls only where derive is not yet expressive enough.
- Update call sites broken by semantic wrapper values.

Out of scope:

- Preserving old raw value APIs exactly.
- Building final custom TOML/JSON codecs.
- Full domain model redesign.

## Code Organization Reminders

- Keep one semantic value concept per file.
- Prefer short files.
- Do not bury semantic metadata in generic codec modules.
- Put tests at the bottom of each file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Primary files:

- `lp-core/lpc-model/src/slots/ratio.rs`
- `lp-core/lpc-model/src/slots/positive_f32.rs`
- `lp-core/lpc-model/src/slots/render_order.rs`
- `lp-core/lpc-model/src/slots/xy.rs`
- `lp-core/lpc-model/src/slots/source_path.rs`
- `lp-core/lpc-model/src/slots/artifact_path.rs`
- `lp-core/lpc-model/src/slots/dim2u.rs`
- `lp-core/lpc-model/src/slots/affine2d.rs`
- `lp-core/lpc-model/src/slots/color_order.rs`
- `lp-core/lpc-model/src/slots/relative_node_ref.rs`
- `lp-core/lpc-model/src/slots/resource_ref.rs`
- `lp-core/lpc-model/src/slots/mod.rs`
- `lp-core/lpc-model/src/slot/mod.rs`

Preferred style:

```rust
#[derive(Clone, Copy, Debug, PartialEq, SlotValue)]
#[slot_value(editor = slider(min = 0.0, max = 1.0, step = 0.01))]
pub struct Ratio(pub f32);

pub type RatioSlot = ValueSlot<Ratio>;
```

Call site consequence:

Old:

```rust
RatioSlot::new(0.75)
assert_eq!(slot.value(), &0.75);
```

New:

```rust
RatioSlot::new(Ratio(0.75))
assert_eq!(slot.value(), &Ratio(0.75));
```

Use helper constructors only if the call-site churn becomes noisy enough to hide intent:

```rust
impl Ratio {
    pub fn new(value: f32) -> Self {
        Self(value)
    }
}
```

Do not reintroduce a custom `RatioSlot` struct just for constructor convenience.

For path-like values:

```rust
#[derive(Clone, Debug, PartialEq, Eq, SlotValue)]
#[slot_value(editor = path)]
pub struct SourcePath(pub String);

pub type SourcePathSlot = ValueSlot<SourcePath>;
```

Use direct aliases only when no semantic editor/shape distinction exists.

Update exports so downstream code imports semantic values and aliases normally.

## Validate

```bash
cargo fmt
cargo test -p lpc-model slots
cargo check -p lpc-model --no-default-features
```
