# Phase 5 — Engine/Client Prop Access + Conversion Boundaries

## Scope of phase

Move property access reflection to the correct runtime/client owners and add
the explicit engine conversion boundary between `lps-shared` runtime types and
model/wire-safe types.

Out of scope:

- Do not move additional source or wire modules except imports needed by this
  phase.
- Do not implement new sync behavior beyond view/access traits and conversion
  helpers.
- Do not make `lpc-model`, `lpc-source`, `lpc-wire`, or `lp-view`
  depend on `lps-shared`.
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
- If blocked by conversion semantics for textures, stop and report.
- Report back: files changed, validation run, validation result, and any
  deviations from this phase.

## Implementation details

### Runtime property access

Move the current `lpc-model::PropAccess` semantics to `lpc-engine` as
`RuntimePropAccess`.

Target:

```text
lp-core/lpc-engine/src/prop/
├── mod.rs
└── runtime_prop_access.rs
```

The trait should remain runtime/in-process and should expose **`LpsValueF32`**
(the `lps-shared` runtime value union; older plans used the `LpsValue` alias).

```rust
pub trait RuntimePropAccess {
    fn get(&self, path: &PropPath) -> Option<(LpsValueF32, FrameId)>;

    fn iter_changed_since(
        &self,
        frame: FrameId,
    ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + '_>;

    fn snapshot(&self) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + '_>;
}
```

Use the project's canonical import for **`LpsValueF32`** from `lps-shared` /
`lp-shader`. Do not expose this trait from `lpc-model`.

### Client property access

Add client-side property view iteration to `lp-view` as
`WirePropAccess`.

Target:

```text
lp-core/lp-view/src/prop/
├── mod.rs
└── wire_prop_access.rs
```

This trait or small view wrapper should expose `WireValue`, not `LpsValue`:

```rust
pub trait WirePropAccess {
    fn get(&self, path: &PropPath) -> Option<(&WireValue, FrameId)>;

    fn iter_changed_since(
        &self,
        frame: FrameId,
    ) -> Box<dyn Iterator<Item = (&PropPath, &WireValue, FrameId)> + '_>;

    fn snapshot(&self) -> Box<dyn Iterator<Item = (&PropPath, &WireValue, FrameId)> + '_>;
}
```

Adjust ownership/borrowing if existing client cache shapes require owned
values. Keep the client dependency closure free of `lps-shared`.

### Engine conversion boundary

Add conversion helpers in `lpc-engine`:

```text
lp-core/lpc-engine/src/wire_bridge/
├── mod.rs
├── lps_value_to_wire_value.rs
└── wire_type_to_lps_type.rs
```

Implement:

- `LpsValueF32 -> WireValue` conversion.
- `WireType -> LpsType` conversion.

The `LpsValueF32 -> WireValue` conversion should mirror the old private
`LpsValueWire::from(&LpsValueF32)` behavior from `value_spec.rs`.

Texture handling:

- If `LpsValueF32::Texture2D` includes a stable descriptor id, preserve that id in
  a `WireValue::Texture(...)` form.
- If there is no stable id available yet, stop and report. Do not invent a fake
  id policy.

The inverse `WireValue -> LpsValueF32` is not a general direct conversion in this
milestone. Source/materialization code should be recipe-driven.

### Source materialization

If Phase 3 removed `SrcValueSpec::materialize()` because it required
`LpsValueF32`, add an engine-side conversion/materialization helper here. Keep it
in `lpc-engine`, not `lpc-source`.

### Dependency cleanup

After this phase:

- `lpc-model` must not depend on `lps-shared`.
- `lpc-source` must not depend on `lps-shared`.
- `lpc-wire` must not depend on `lps-shared`.
- `lp-view` must not depend on `lps-shared`.
- `lpc-engine` may depend on `lps-shared`.

## Tests to preserve/add

- Move or recreate the existing `PropAccess` tests under
  `lpc-engine::RuntimePropAccess`.
- Add a simple `WirePropAccess` test over a client-side cache/map.
- Add conversion tests for representative scalar/vector/array/struct values.
- Add `WireType -> LpsType` mapping tests for representative `Kind::storage()`
  results.

## Validate

Run:

```bash
cargo test -p lpc-engine
cargo test -p lpc-view
cargo test -p lpc-model
cargo check -p lpc-view --no-default-features
```

If formatting changed, run:

```bash
cargo +nightly fmt
```
