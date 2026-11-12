# Phase 4: Playlist Runtime

- parallel: -
- sub-agent: supervised

## Scope Of Phase

Implement `PlaylistNode` selection and rendering. This phase should make authored playlists load,
publish `entry_time`, restart an entry sequence from the entry whose trigger fired, render through
the active child visual, and crossfade between previous and current entries using the Phase 3
render-delegation primitive.

In scope:

- `lp-core/lpc-engine/src/nodes/playlist/`
- `NodeDef::Playlist` runtime attachment
- entry child id table passed from loader to runtime
- entry-local trigger detection
- duration/advance/return-to-idle behavior
- `entry_time` publication before child resolution
- visual product output and render delegation

Out of scope:

- Final example art polish.
- Radio integration.
- New transition types beyond crossfade.

## Code Organization Reminders

- Keep state-machine helpers below the headline `PlaylistNode` impl.
- Keep texture blending helpers near render code, with tests at the bottom.
- Avoid generic playlist abstractions beyond what the example and tests need.
- If render delegation proves blocked, stop and report the blocker instead of silently reducing the
  playlist to a selector-only node.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add files:

```text
lp-core/lpc-engine/src/nodes/playlist/
  mod.rs
  playlist_node.rs
```

Wire into:

```text
lp-core/lpc-engine/src/nodes/mod.rs
lp-core/lpc-engine/src/engine/project_loader.rs
```

Runtime constructor should receive child metadata from the loader:

```rust
pub struct PlaylistRuntimeEntry {
    pub index: u32,
    pub child: NodeId,
    pub output_slot: SlotPath, // default "output"
}
```

`PlaylistNode` owns:

- `PlaylistState`
- optional `PlaylistDefView`
- runtime entries map
- `current_entry`
- optional `previous_entry`
- `switch_time`
- optional transition start/duration
- last seen trigger seqs by entry and id
- reusable scratch textures for crossfade

Tick behavior:

1. Read playlist config through generated views.
2. Resolve consumed `time` as `f32`.
3. Resolve each entry's consumed `trigger` as a `MapSlot<u32, ControlMessage>` or inspect
   `SlotData::Map` at `entries[index].trigger`.
4. Detect any new `(entry, id, seq)` not seen before.
5. On a new entry trigger, switch or restart to that entry and reset `switch_time`.
6. If no trigger and current timed entry duration expired, advance to the next entry or
   return to `idle_entry`.
7. Compute `entry_time = time - switch_time`, clamped at `0.0`.
8. Set `PlaylistState.output = VisualProduct::new(ctx.node_id(), 0)`.
9. Set `PlaylistState.entry_time` and `active_entry`.
10. Call `ctx.publish_runtime_slot` for at least `entry_time`, `active_entry`, and `output`.
11. Resolve active child output so the child ticks after `entry_time` is cached.
12. During transitions, also resolve previous child output if needed for rendering.

Trigger semantics:

- Bind button `down` to `bus#trigger`, then bind the active entry's `trigger` from `bus#trigger`.
- Repeated triggers for the current entry restart that entry.
- A trigger for another entry switches to that entry, using the outgoing entry fade.
- `held` should not be used for playlist trigger in the example.

Duration semantics:

- `duration` is seconds.
- Idle can omit duration.
- Entries reached from a trigger should have duration in this first example. If a triggered
  non-idle entry lacks duration, return a clear node error rather than silently sticking forever.

Render behavior:

- Outside transition: delegate active child visual directly to `ctx.render_texture_into`.
- During transition: use the fade duration from the entry being left:

```text
duration = leaving_entry.fade_after.unwrap_or(default_fade)
```

- If duration is zero, switch immediately.
- For `Rgba16Unorm`, render previous and current child products into scratch textures and blend.
- For unsupported formats, return a clear `NodeError` or direct-render current child if that is
  consistent with existing engine behavior.

Known first-slice limitation:

- `entry_time` belongs to the current entry. If an outgoing active child also depends on
  `entry_time`, it may render fade-out using its last ticked uniforms. Do not add a second public
  time slot in this phase.

Tests:

- Playlist starts on idle.
- Trigger on entry 2 switches to active and resets `entry_time`.
- Repeated trigger while active restarts entry 2 rather than returning to idle.
- Trigger on a later entry starts that entry and then plays through later timed entries.
- Active duration returns to idle.
- Multiple active entries advance in key order before returning idle.
- `fade_after` overrides default fade when leaving an entry.
- Entry children do not publish default `visual.out`.
- Active child shader can bind `time` from `..#entry_time` without recursive playlist tick.

## Validate

Run:

```bash
cargo test -p lpc-engine playlist
cargo test -p lpc-engine project_loader
cargo check -p lpc-engine
```
