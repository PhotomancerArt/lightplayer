# M2 Playlist Node Notes

## Scope Of Work

Add an authored playlist/switching node and an `examples/button-playlist` project that extends the
current button example into the first fyeah-sign behavior:

- idle visual runs indefinitely by default;
- a button/control-message trigger on the active entry enters a bright fast visual;
- the active visual plays for a configured duration, then returns to idle;
- if more active entries are authored, a trigger on any entry starts that entry and the playlist
  advances through later timed entries before returning to idle.

In scope:

- Add a core `Playlist` node definition in `lpc-model`.
- Add a runtime `PlaylistNode` in `lpc-engine`.
- Consume entry-local `MapSlot<u32, ControlMessage>` trigger messages from the existing bus/event
  slice.
- Select among multiple visual inputs without requiring realtime compilation of the next shader.
- Add an example that combines `Button`, `Playlist`, two shader nodes, fixture, output, and clock.
- Include a simplified idle noise shader and a fast bright active shader.

Out of scope for the first implementation:

- Host precompilation or disabling the on-device compiler.
- Realtime background compilation, shader eviction, or memory-pressure policy for playlist entries.
- Radio nodes or wireless transport.
- Full OSC address/argument messages.
- A generalized show-control transport vocabulary such as next/prev/pause.
- Transition types beyond crossfade, such as wipes or shader-authored transition effects.

## Current Codebase State

- `ControlMessage { id: u32, seq: u32 }` and `TriggerEvent` already exist in
  `lp-core/lpc-model/src/control/control_message.rs`.
- `examples/events` proves `bus#trigger` can carry sentinel maps of
  `lp::control::Message` into shader inputs.
- The current button work has added or is adding:
  - `ButtonDef` / `ButtonState` under `lp-core/lpc-model/src/nodes/button/`;
  - `ButtonNode` under `lp-core/lpc-engine/src/nodes/button/`;
  - `down`, `held`, and `up` outputs as `MapSlot<u32, ControlMessage>`.
- The button example plan currently binds `held -> bus#trigger` so a shader can show state while
  the button is pressed. The playlist example should use the `down` edge for restart semantics.
- Visual-producing nodes publish `VisualProduct` values through produced state:
  - `ShaderState::output` and `FluidState::output` are `VisualProductSlot`s.
  - Fixture nodes consume a visual product and render it on demand.
- A minimal playlist could publish an `output: VisualProductSlot` whose value is the currently
  selected child visual product. The final M2 plan instead makes the playlist publish its own
  visual product so it can direct-render the active child or render two child products during a
  crossfade.
- This still lets all shader nodes remain loaded/compiled as normal runtime nodes; the playlist
  adds render delegation rather than a custom shader compilation path.
- True crossfade is heavier in the current engine because `RenderContext` does not expose
  render-services for one visual node to render other visual products during its own render call.
  `ControlRenderContext` has that shape, but `RenderContext` currently only exposes graphics, time,
  and time-provider services.
- The older `lp-vis/lpv-model/src/visual/playlist.rs` artifact model has useful vocabulary:
  entries have visuals and optional durations, behavior can loop, and a transition can be
  crossfade. It is not wired into the current `lpc-model` runtime node graph.
- `NodeInvocation` already supports the standard child-node invocation syntax:
  - `node = { def = { path = "./active.toml" } }`
  - `[node.def] kind = "Shader"` for inline definitions.
- The current `ProjectLoader` uses `NodeInvocation` for `ProjectDef.nodes`, supports path and inline
  definitions, and adds each project child to `NodeTree`.
- The current loader is not yet recursive: it only discovers project-root child invocations. Playlist
  entries need a generalization so any node definition can expose owned child invocations.
- Slot paths support map keys such as `entries[2]`, but if playlist entries own child nodes directly,
  the playlist does not need authored `entries[2].input` bindings for the common child-output case.
- `RelativeNodeRef` already exists as a slot value and has `ValueEditorHint::NodeRef`.
  It parses relative node references such as `..idle` and `..active`.
- `NodeSlotRef` / `BindingRef` can already express `..idle#output`, but requiring that inside every
  playlist entry is more verbose than the common case needs.
- `project_loader.rs` already resolves simple sibling `RelativeNodeRef`s through
  `resolve_relative_node_ref`. The helper currently supports current node and one-hop siblings,
  which is enough for the planned flat `examples/button-playlist` shape.
- The intended "visual output defaults to `bus#visual.out`" behavior is not currently implemented
  for shader/fluid outputs. Existing examples still explicitly author
  `[bindings.output] target = "bus#visual.out"`.
- Binding priority already has the right shape for defaults:
  - authored bindings use priority `0`;
  - fallback defaults use priority `-1000`;
  - bus resolution selects the single highest-priority provider and errors if the highest priority is
    ambiguous.

## User Notes

- The immediate deliverable should extend the button example with a playlist.
- The first item is an idle noise entry, like the current basic example but probably simpler; the
  changing palettes are worth preserving if they do not bloat the shader.
- The active item is a fast moving bright colored visual; details can come later.
- The first item should play forever.
- The active item should have its own trigger input. An event/message on that entry starts it,
  respecting the outgoing crossfade from the previous entry, then it plays through.
- Repeated button presses should reset the active animation rather than toggling or cycling back to
  idle.
- A future authoring shape may include more playlist items so the button triggers item 2, or another
  event triggers a later item, then the playlist goes through the remaining timed items before
  returning to the top.
- For now, assume enough memory to keep all compiled shaders resident. Do not design a compile-ahead
  or eviction system into this slice.
- Cross fading should be included in the playlist plan, with the implementation kept honest about
  the small render-delegation plumbing it requires.

## Suggested First-Slice Semantics

Use a playlist state machine with three logical states:

- `Idle`: selected index is the configured idle entry, default `1`, and it has no timeout.
- `ActiveSequence`: selected index starts at the entry whose trigger received a new message and
  advances by duration through later entries.
- `ReturnToIdle`: when the current active entry duration expires and there is no next timed active
  entry, select idle again.

Trigger behavior:

- Each playlist entry may consume a trigger map from its own `trigger` slot.
- Any new `ControlMessage` not seen before on entry `N` starts or restarts entry `N`.
- Repeated press while the active entry is running restarts that same entry and resets the duration
  timer.
- A trigger for a different entry switches to that entry, respecting the outgoing fade from the
  previous entry.
- The first example should bind `button.down -> bus#trigger -> entries[2].trigger`, not `held`, so a
  press is a discrete restart event.

Visual behavior:

- The playlist has `entries: MapSlot<u32, PlaylistEntry>`.
- Each entry owns a standard child `NodeInvocation` in a `node` field.
- Each entry owns local `bindings`, including the common `[entries.N.bindings.trigger]` binding.
- Each entry has optional `duration` in seconds; an omitted duration means indefinite.
- The loader instantiates each entry node as a child of the playlist node and records the mapping
  from entry index to child `NodeId`.
- The runtime resolves the active entry child's `output` produced slot by default, then renders
  through the playlist's own visual product.
- `playlist.output` is bound to `bus#visual.out`, replacing the direct shader-output binding used by
  simple examples.

## Open Questions

### Q1. Should the first implementation use pass-through selection and defer crossfade?

Suggested answer: no. Keep direct pass-through rendering as the non-transition fast path, but
include crossfade in M2 by adding the small render-delegation plumbing that lets a playlist render
two child visual products and combine them into its own target.

Context: pass-through selection is a useful fallback shape, but it is not enough for the desired
visual product. Crossfade requires rendering two child visuals in the playlist's render path, and
`RenderContext` currently needs the same style of callback services that `ControlRenderContext`
already has.

User answer: crossfade would make the result nicer and the conceptual model is right: the playlist
exports its own render product and delegates to two child render products. It is acceptable to skip
if it becomes too hard, but the plan should think through the small plumbing needed rather than
assuming it is impossible.

Updated direction: include crossfade as a planned implementation slice by adding the small render
service plumbing. The core addition is to give `RenderContext` the same style of child visual render
callbacks that `ControlRenderContext` already has. `PlaylistNode` can then implement `RenderNode`,
render active/previous child products into reusable scratch textures, blend them into the caller
target, and fall back to direct pass-through outside transition windows.

### Q2. Should the playlist consume `down` or `held` from the button node?

Suggested answer: use `down`.

Context: `held` is useful for shader-visible state in the button example, but a playlist trigger
should restart once per debounced press. Binding `held` would retrigger every frame while the button
is pressed unless the playlist filtered a repeated `(id, seq)` every tick.

User answer: trigger on button down is fine.

Updated direction: bind `button.down` to `bus#trigger`, then bind the active entry's local
`trigger` slot from that bus:

```toml
[entries.2.bindings.trigger]
source = "bus#trigger"
```

### Q3. Do we need per-entry local shader time in the first playlist example?

Suggested answer: expose playlist-local time in the first slice because active-entry animations
commonly need to restart from phase zero.

Context: a pass-through selector can switch visual products easily, but resetting the selected
shader's time is more subtle. If a child shader consumes a playlist-produced time bus, resolving the
child visual during playlist tick can create a dependency cycle back into the executing playlist
node.

User answer: instinctively yes, but it should be modeled as an activation/switch timestamp snapshot,
not as a replacement for the clock. The playlist should have a time input bound to the same time bus
as everything else, track the last switch/activation time, and expose time since last switch for
child shaders that want local phase. The idle shader can keep using global time.

Updated direction: add a playlist `time` consumed slot and one shader-facing runtime output,
`entry_time`, meaning seconds since the current entry was activated. Internally the node can store
an absolute `switch_time` snapshot, but authored child shaders should bind to `entry_time`.

Resolver follow-up: direct binding from a child shader back to `playlist.entry_time` is not
trivially safe in the current engine. `EngineResolveHost` marks a node as `Executing` while it ticks
or renders. If the playlist is executing and asks the engine to prepare/render a child shader, and
that child shader resolves a produced slot on the playlist, the engine can hit its re-entry guard or
try to read runtime state from a node that is not currently `Alive`.

Safer first-step options:

- Add an explicit "publish produced slot now" path to the tick resolver. After `PlaylistNode`
  updates `PlaylistState::entry_time` and `output`, it can publish those slot snapshots into the
  same-frame resolver cache before resolving child visual inputs. If a child shader then asks for
  `playlist.entry_time`, the resolver returns the cached production and does not call
  `PlaylistNode::tick()` recursively.
- Keep a sibling `ActivationClock` out of the first design unless the publish path proves too
  invasive. The user explicitly called this a common playlist case, so ergonomics favor keeping
  activation time inside the playlist.
- Later, add render-local uniform overrides only if the cache publication path is not enough.

User answer: avoid a separate clock if possible. The playlist should be able to update and publish
its time-since-last-switch state, then resolve/render children. A resolver special case is
acceptable if it means "these produced values have already been produced; if someone asks for them,
do not call our tick again."

Updated direction: implement a small general resolver capability, not a playlist-only hack:

```text
TickContext::publish_runtime_slot(state_root, slot_path)
    -> snapshot SlotData through SlotShapeRegistry
    -> TickResolver::publish_produced_slot(node_id, slot_path, Production)
    -> Resolver cache stores QueryKey::ProducedSlot { node, slot }
```

The resolver already checks its cache before host production, so child shader input resolution can
reuse the early-published values and avoid the executing-node recursion. This is useful beyond
playlists for any future node that needs to publish partial runtime state before resolving dependent
children.

### Q8. What should the playlist-local time slot be called?

Suggested answer: expose one slot named `entry_time`, a `f32` value in seconds since the current
playlist entry was activated. Do not expose separate `entry_ms`, `entry_seconds`, or
`activation_time` in the first slice.

Context: shader authors mostly want a drop-in replacement for `time`. `switch_time` sounds like an
absolute timestamp, which can remain an internal field used to compute the public elapsed value.

User answer: only one exposed time value is needed. Names considered: `switch_time`, `entry_time`,
or similar.

Updated direction: use `entry_time` publicly and keep `switch_time` internal.

### Q9. How should per-entry crossfade overrides work?

Suggested answer: each entry may define an outgoing fade override, used when transitioning away from
that entry. If absent, use the playlist default fade. Keep only one per-entry transition direction
in this slice.

Context: putting the override "after" the entry matches duration semantics: the entry plays, then
its fade describes how it leaves. This also gives the active entry direct control over how it
returns to idle. Triggering from the idle entry uses the idle entry's outgoing fade, which is still
coherent because the trigger interrupts idle and leaves it.

User answer: there should be a default crossfade, with per-entry override. We need to pick before or
after; after probably makes sense.

Updated direction: use an `after_fade` or `fade_after` field on `PlaylistEntry`. In TOML, prefer the
shorter `fade_after` name unless implementation discovers an existing naming convention to match.

### Q4. What should happen if an entry trigger arrives while another entry is active?

Suggested answer: start the entry whose trigger fired. If it is already active, restart that same
entry and reset `entry_time`. If it is different from the current entry, crossfade from the current
entry into the triggered entry.

Context: this makes repeated button presses feel immediate and avoids overloading one generic
playlist trigger with both "start active sequence" and future "next" transport semantics.

User answer: trigger needs to be on the entry for this work. A future generic `next` trigger may be
useful, but an event should cause the target entry to start playing, respecting the crossfade from
the previous entry, then play through.

### Q5. How should durations be represented?

Suggested answer: use authored `duration = <f32 seconds>`, matching the older
`lp-vis/lpv-model` playlist artifact and the public `entry_time` shader slot. Omitted duration means
indefinite. The runtime can still convert to milliseconds or compare against `f32` frame time
internally as appropriate.

Context: existing shader time is `f32`, the previous playlist artifact uses seconds, and authors are
likely to think in seconds for visual clips. Tests should include boundary cases to avoid float edge
surprises.

### Q6. Should entries be a `MapSlot<u32, PlaylistEntry>` or a fixed set of fields?

Suggested answer: use `MapSlot<u32, PlaylistEntry>`.

Context: the user explicitly wants room for more entries. Slot paths already support map keys, and
the loader can use those stable keys to build the entry index to child-node table without adding
fixed fields for each playlist item.

### Q7. What should the example be named?

Suggested answer: `examples/button-playlist`.

Context: it is an extension of `examples/button`, but it exercises a different user-facing behavior
and should remain available as a separate checked-in example.

### Q10. How should playlist entries reference child visual nodes?

Suggested answer: entries should own standard `NodeInvocation`s, not use a playlist-specific node
reference field. The child node's default visual source is its `output` produced slot. Do not require
authors to write `[bindings."entries[1].input"]` or `visual = "..idle#output"` for the common case.

Context: the codebase already has `NodeInvocation` for exactly this kind of authored child-node
setup. Reusing it preserves path-backed and inline definitions, source-relative path behavior, and
future project tree sync expectations.

Preferred authoring:

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
```

Lifecycle/loading direction:

- `PlaylistEntry.node` has type `NodeInvocation`.
- `ProjectLoader` should be refactored from "load project children" into a recursive
  `load_child_invocation(parent, child_name, invocation, containing_source_base)` helper.
- `ProjectDef.nodes` remains the first caller of that helper.
- `PlaylistDef.entries` becomes the second caller: for each entry, add a child under the playlist
  node.
- Use the entry `name` as the child node name when it is a valid, unique `NodeName`; otherwise fail
  with a clear loader error or fall back to `entry_<index>`. Prefer requiring valid unique names in
  the first example so tree paths read as `playlist/idle` and `playlist/active`.
- Resolve path-backed entry nodes relative to the playlist definition file, or relative to the file
  containing an inline playlist definition.
- Attach/runtime initialization uses the same runtime-node attachment code as project-root children.

Binding/runtime direction:

- The playlist does not need an explicit binding for each child visual in TOML.
- Entry-local bindings are registered against nested playlist slot paths. For example,
  `[entries.2.bindings.trigger]` registers a consumed binding for `entries[2].trigger`.
- The loader passes `entry_index -> child NodeId` metadata into `PlaylistNode::new`, or stores it in
  a small runtime-side entry table.
- `PlaylistNode` resolves `QueryKey::ProducedSlot { node: child_id, slot: "output" }` for the active
  and previous entries.
- This keeps child outputs in the ordinary produced-slot model without exposing a fake authored
  `entries[index].input` slot.
- If a child shader needs playlist-local time, it can use ordinary binding syntax from child to
  parent, e.g. `[bindings.time] source = "..#entry_time"`. That requires expanding
  `resolve_relative_node_ref` beyond the current flat sibling-only logic to support parent refs and
  nested descendants.

### Q11. How should visual default output bindings interact with playlist children?

Suggested answer: implement visual default output bindings, but suppress them for structurally owned
playlist entry children.

Context: the desired system shape is that a simple top-level visual node works without explicitly
binding `output -> bus#visual.out`, and any explicit binding or higher-priority provider overrides
that default. Playlist children are different: their output is consumed by the owning playlist via
structural entry metadata, not published to the project visual bus.

Current code reality:

- There is no shader/fluid/playlist default output bus binding yet.
- Default time bindings exist and use `BindingPriority::default_fallback()`.
- If multiple playlist child shaders all registered fallback providers to `visual.out`, then a
  project with no higher-priority provider would have ambiguous bus providers at the same fallback
  priority.
- If the playlist itself binds/publishes `output -> bus#visual.out` at authored priority, that would
  win over child fallback providers, but it is still noisy and fragile to register child defaults
  that can never be the intended output.

Updated direction:

- Add `register_visual_default_output_binding` for top-level visual producers (`Shader`, `Fluid`,
  `Playlist`, and maybe future visual nodes).
- The default target is `bus#visual.out` at `BindingPriority::default_fallback()`.
- Do not register that fallback when:
  - the node already has an explicit `output` target binding;
  - the node is structurally owned as a playlist entry child;
  - a future parent/owner marks the child output as internally consumed.
- Still honor an explicit child output target if the author writes one; only suppress automatic
  fallback leakage.
- The example should omit explicit `output -> bus#visual.out` from `idle`/`active` child shaders,
  and either rely on playlist's default output binding or explicitly bind playlist output in
  `playlist.toml`.
