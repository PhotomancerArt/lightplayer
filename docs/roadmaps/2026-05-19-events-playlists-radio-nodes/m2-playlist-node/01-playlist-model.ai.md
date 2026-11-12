# Phase 1: Playlist Model

- parallel: -
- sub-agent: main

## Scope Of Phase

Add the authored playlist node model and runtime state model in `lpc-model`. This phase does not add
engine runtime behavior, recursive loading, or examples.

In scope:

- `PlaylistDef`
- `PlaylistEntry`
- `PlaylistState`
- `NodeKind::Playlist`
- `NodeDef::Playlist`
- model exports and generated shape/view support
- model parsing/shape tests

Out of scope:

- `PlaylistNode` runtime implementation.
- Recursive project loading.
- Crossfade rendering.
- Default visual output binding behavior.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep `mod.rs` limited to module declarations and re-exports.
- Put helper defaults below the headline type impls.
- Put tests at the bottom of each file.
- Do not add temporary model fields unless they are clearly marked with `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add files:

```text
lp-core/lpc-model/src/nodes/playlist/
  mod.rs
  playlist_def.rs
  playlist_entry.rs
  playlist_state.rs
```

Wire the node into:

```text
lp-core/lpc-model/src/nodes/mod.rs
lp-core/lpc-model/src/nodes/node_def.rs
lp-core/lpc-model/src/node/kind.rs
lp-core/lpc-model/src/lib.rs
```

Model shape:

```rust
pub struct PlaylistDef {
    pub bindings: BindingDefs,
    #[slot(consumed)]
    pub time: ValueSlot<f32>,
    pub idle_entry: ValueSlot<u32>,
    pub default_fade: PositiveF32Slot,
    pub entries: MapSlot<u32, PlaylistEntry>,
}
```

Use sensible defaults:

- `time = 0.0`
- `idle_entry = 1`
- `default_fade = PositiveF32(0.25)`
- `entries = empty map`

`PlaylistEntry`:

```rust
pub struct PlaylistEntry {
    pub bindings: BindingDefs,
    #[slot(consumed, merge = "by_key", map(key = "u32", value_ref = "lp::control::Message"))]
    pub trigger: MapSlot<u32, ControlMessage>,
    pub name: OptionSlot<ValueSlot<String>>,
    pub duration: OptionSlot<PositiveF32Slot>,
    pub fade_after: OptionSlot<PositiveF32Slot>,
    pub node: NodeInvocation,
}
```

`PlaylistState`:

```rust
#[slot(default_policy = "read_only_transient")]
pub struct PlaylistState {
    #[slot(produced)]
    pub output: VisualProductSlot,
    #[slot(produced)]
    pub entry_time: ValueSlot<f32>,
    #[slot(produced)]
    pub active_entry: ValueSlot<u32>,
}
```

Add tests that prove:

- minimal `kind = "Playlist"` parses;
- entries parse with `node = { def = { path = "./idle.toml" } }`;
- entries parse with inline `[entries.2.node.def] kind = "Shader"`;
- entries parse with `[entries.2.bindings.trigger] source = "bus#trigger"`;
- `time` is consumed latest;
- `PlaylistEntry.trigger` is consumed by-key;
- `output`, `entry_time`, and `active_entry` are produced;
- `NodeDef::kind_name`, `variant_name`, and `as_playlist` work.

## Validate

Run:

```bash
cargo test -p lpc-model playlist
cargo check -p lpc-model
```
