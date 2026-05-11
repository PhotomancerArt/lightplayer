## Scope of Phase

Introduce the binding registry foundation that replaces the current value-owning
`Bus` shape for the new engine path.

The registry owns binding identity, binding metadata, channel/provider lookup,
priority validation, and versioning for future UI/wire sync. It does not own
resolved runtime values. Resolved values belong to the engine-owned resolver
cache in later phases.

Out of scope:

- Do not implement `Engine`, `Resolver`, or `ResolveSession`.
- Do not remove the existing `bus` module unless a tiny compatibility re-export
  is needed. Existing code that imports `Bus` should keep compiling if possible.
- Do not add wire protocol types for binding sync yet.
- Do not port legacy runtime bindings.

Suggested sub-agent model: `kimi-k2.5`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public items and entry points near the top, helpers below them, and
  `#[cfg(test)] mod tests` at the bottom of Rust files.
- Keep related functionality grouped together.
- Any temporary code must have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Create a new module:

```text
lp-core/lpc-engine/src/binding/
├── mod.rs
├── binding_id.rs
├── binding_entry.rs
├── binding_registry.rs
└── binding_error.rs
```

Export it from `lp-core/lpc-engine/src/lib.rs` as `pub mod binding;` and re-export
the main public types:

- `BindingId`
- `BindingEntry`
- `BindingSource`
- `BindingTarget`
- `BindingPriority`
- `BindingRegistry`
- `BindingError`

Use existing model/source types:

- `lpc_model::{ChannelName, FrameId, Kind, NodeId, PropPath}`
- `lpc_source::SrcValueSpec`

Suggested first shape:

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingId(u32);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct BindingPriority(i32);

pub struct BindingEntry {
    pub id: BindingId,
    pub source: BindingSource,
    pub target: BindingTarget,
    pub priority: BindingPriority,
    pub kind: Kind,
    pub version: FrameId,
    pub owner: NodeId,
}

pub enum BindingSource {
    Literal(SrcValueSpec),
    NodeOutput { node: NodeId, output: PropPath },
    BusChannel(ChannelName),
}

pub enum BindingTarget {
    NodeInput { node: NodeId, input: PropPath },
    NodeOutput { node: NodeId, output: PropPath },
    BusChannel(ChannelName),
}
```

Implement `BindingRegistry` with:

- `new()`
- `register(entry_without_id, frame) -> Result<BindingId, BindingError>` or a
  similarly clean API that allocates ids internally
- `unregister(id, frame) -> Result<BindingEntry, BindingError>`
- `get(id) -> Option<&BindingEntry>`
- `iter()`
- `providers_for_bus(channel: &ChannelName) -> impl Iterator<Item = &BindingEntry>`
- validation that active providers for the same bus channel cannot have the same
  priority if they are both candidates for the same target kind
- validation that all bindings for a bus channel use the same `Kind`

Keep the first implementation no_std-compatible: use `alloc` collections such as
`BTreeMap`/`Vec`, not `std`.

Tests to add:

- registers a binding and assigns a stable non-zero `BindingId`
- unregister removes the binding and increments/removes indexes correctly
- `providers_for_bus` returns entries targeting a `BusChannel`
- kind mismatch on the same bus channel returns an error
- equal-priority providers on the same bus channel return an error
- binding `version` is set/bumped from the provided frame

## Validate

Run:

```bash
cargo test -p lpc-engine binding
```

If public exports break unrelated lpc-engine tests, run and fix:

```bash
cargo test -p lpc-engine --lib
```
