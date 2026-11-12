# M2 Playlist Node Design

## Scope Of Work

Add a core playlist node that can own multiple visual child nodes, switch from an idle entry into a
triggered entry sequence, expose `entry_time` for active-entry shaders, and render crossfades
between entries. The first user-facing artifact is `examples/button-playlist`, an extension of the
button example for the fyeah sign.

In scope:

- `Playlist` model node with standard `NodeInvocation` children per entry.
- Recursive project loading for playlist-owned child nodes.
- Default visual output binding for simple top-level visual nodes, with suppression for playlist
  entry children.
- Resolver support for early-published produced slots so `..#entry_time` works without recursive
  playlist ticking.
- Playlist runtime selection, duration handling, entry-level restart triggers, and crossfade render.
- Example project with `idle` and `active` entry names.

Out of scope:

- Radio nodes or wireless transport.
- OSC address/args.
- Dynamic shader compilation ahead of playlist transitions or shader eviction.
- Transition types beyond crossfade.
- Multiple public playlist-local time slots. The public slot is `entry_time`; internal absolute
  switch time remains an implementation detail.

## File Structure

```text
lp-core/lpc-model/src/
  nodes/
    playlist/
      mod.rs
      playlist_def.rs
      playlist_entry.rs
      playlist_state.rs
    mod.rs
    node_def.rs
  node/kind.rs

lp-core/lpc-engine/src/
  dataflow/resolver/
    resolve_session.rs
    tick_resolver.rs
  engine/
    engine.rs
    project_loader.rs
  node/
    contexts.rs
  nodes/
    playlist/
      mod.rs
      playlist_node.rs
    mod.rs

examples/button-playlist/
  project.toml
  button.toml
  playlist.toml
  idle.toml
  idle.glsl
  active.toml
  active.glsl
  clock.toml
  fixture.toml
  output.toml
```

## Architecture Summary

The playlist is a real visual node. It owns entry child nodes using the same invocation syntax as
project children:

```toml
[entries.1]
name = "idle"
fade_after = 0.12
node = { def = { path = "./idle.toml" } }

[entries.2]
name = "active"
duration = 4.0
fade_after = 0.8

[entries.2.bindings.trigger]
source = "bus#trigger"

[entries.2.node.def]
kind = "Shader"
source = { path = "active.glsl" }

[entries.2.node.def.bindings.time]
source = "..#entry_time"
```

The runtime graph shape is:

```text
Button.down -> bus#trigger -> Playlist.entries[2].trigger
Clock.seconds -> bus#time.seconds

Playlist
  consumes time <- bus#time.seconds
  consumes entry trigger <- bus#trigger
  owns child entry nodes:
    idle.output
    active.output
  produces output -> bus#visual.out
  produces entry_time -> active shader time

Fixture input <- bus#visual.out
Output input <- fixture.output
```

Playlist entry child nodes do not receive automatic `output -> bus#visual.out` fallback bindings.
Their outputs are consumed structurally by the playlist. The playlist itself can receive the default
visual output binding when no explicit output target exists.

## Main Components And Interactions

### Playlist Model

`PlaylistDef` should be an ordinary slotted node definition:

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

`PlaylistEntry` owns a `NodeInvocation`:

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

`PlaylistState` exposes only the runtime values other nodes need:

```rust
pub struct PlaylistState {
    #[slot(produced)]
    pub output: VisualProductSlot,
    #[slot(produced)]
    pub entry_time: ValueSlot<f32>,
    #[slot(produced)]
    pub active_entry: ValueSlot<u32>,
}
```

### Recursive Loading

`ProjectLoader` should stop treating `ProjectDef.nodes` as the only child source. Refactor loading
around a helper that can load any `NodeInvocation` under any parent:

```text
load_child_invocation(parent_id, child_name, invocation, source_base, ownership)
```

`ownership` carries policy such as:

- project child: visual default output allowed;
- playlist entry child: visual default output suppressed and child id recorded for the owning
  playlist entry.

For playlist entries:

- use `entry.name` as the child `NodeName` when present;
- otherwise use `entry_<index>`;
- reject invalid or duplicate names with a clear loader error;
- resolve path-backed nodes relative to the playlist file, or the file containing an inline playlist
  definition.

Relative node references must support nested parent refs such as `..#entry_time`, because active
child shaders bind their `time` input to the playlist's produced `entry_time`.

Playlist entry bindings are authored locally on the entry, but registered against the owning
playlist node. For example:

```toml
[entries.2.bindings.trigger]
source = "bus#trigger"
```

registers a consumed binding whose target slot is `entries[2].trigger` on the playlist node.

### Default Visual Output Binding

Add a default output binding for visual producers:

```text
node.output -> bus#visual.out
priority = default_fallback()
```

Apply this to top-level `Shader`, `Fluid`, `Playlist`, and future visual nodes only when:

- the node has no explicit `output` target binding;
- the node is not structurally owned as a playlist entry child;
- a future ownership policy has not suppressed visual output defaults.

Explicit authored output bindings should always be honored.

### Early Produced Slot Publication

`PlaylistNode` needs to update `entry_time` and let child shaders consume it while the playlist is
still ticking. Add a general resolver capability:

```text
TickContext::publish_runtime_slot(state_root, slot_path)
  -> snapshot SlotData through SlotShapeRegistry
  -> TickResolver::publish_produced_slot(node_id, slot_path, Production)
  -> Resolver cache stores QueryKey::ProducedSlot { node, slot }
```

The resolver already checks its cache before asking the host to produce a slot. Once playlist
publishes `entry_time`, a child shader resolving `..#entry_time` gets the cached value instead of
recursively ticking the playlist.

### Render Delegation And Crossfade

`PlaylistNode` implements `RenderNode`. The engine should give `RenderContext` child-visual render
callbacks similar to `ControlRenderContext`:

```text
RenderContext::render_texture_into(child_product, request, target)
RenderContext::sample_visual_into(child_product, request, target)
```

The playlist render path:

- outside a transition: render the active child product directly into the caller target;
- during a transition: render previous and active child products into reusable scratch textures,
  blend into the caller target using the outgoing entry's `fade_after` or `default_fade`;
- support `Rgba16Unorm` first, which is the fixture path used by current examples.

`entry_time` is the time for the current entry. If an outgoing child depends on `entry_time`, the
first implementation may render it with its last ticked uniforms during fade-out. A future
render-local override system can support distinct outgoing and incoming local times, but the first
priority is that incoming active entries restart cleanly.

### Playlist State Machine

The state machine tracks:

- `current_entry`;
- `previous_entry` during crossfade;
- `switch_time`;
- `transition_start_time`;
- `transition_duration`;
- last seen trigger `(entry, id, seq)` values.

Behavior:

- start at `idle_entry`, default `1`;
- any new trigger on entry `N` starts or restarts entry `N`;
- repeated triggers for the current entry reset `switch_time` and replay that entry from phase zero;
- a trigger for a different entry switches to that entry, using the outgoing entry fade;
- triggered entries in the first example require `duration`;
- when a timed entry's duration expires, advance to the next entry key greater than current;
- if no next active entry exists, return to `idle_entry`;
- omitted duration means indefinite and is intended for idle.

The outgoing fade is chosen from the entry being left:

```text
fade_duration = leaving_entry.fade_after.unwrap_or(default_fade)
```

### Example

`examples/button-playlist` should use these names:

- entry 1: `idle`;
- entry 2: `active`.

The button binds `down -> bus#trigger`. The idle shader can consume global time. The active entry
binds `trigger <- bus#trigger`, and the active shader binds `time <- ..#entry_time` so it
restarts on activation.

The idle shader should be a simplified noise/palette visual. The active shader should be bright,
fast, and visibly different. Exact art direction can be refined later without changing the node
architecture.
