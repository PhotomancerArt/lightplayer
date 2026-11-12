# Clock Node, Time Bus, Inline Invocations, And Mutation Design

## Scope

Implement a normal dataflow clock node and enough mutation infrastructure for debug UI clock controls.

In scope:

- Slot persistence metadata for transient user controls.
- Inline child node invocations in `project.toml`.
- `ClockDef`, `ClockControls`, `ClockState`, and `ClockNode`.
- Default binding conventions that lose to explicit authored bindings.
- Binding clock output to `bus#time.seconds`.
- Binding visual and compute shader `time` inputs to that bus.
- A narrow real mutation path for value leaves on node def roots.
- Debug UI controls for clock pause/rate/scrub.
- Fluid example tuning so motion is visible and less chaotic.

Out of scope:

- Persisting mutations back to TOML.
- Generic container mutation.
- Runtime-state mutation.
- Implicit project-level clock insertion.
- Full config/param/controls taxonomy beyond the `controls` convention.

## File Structure

```text
lp-core/lpc-model/src/
  node/
    kind.rs
    node_invocation.rs
  nodes/
    clock/
      mod.rs
      clock_controls.rs
      clock_def.rs
      clock_state.rs
    node_def.rs
  slot/
    slot_meta.rs

lp-core/lpc-engine/src/
  nodes/
    clock/
      mod.rs
      clock_node.rs
  engine/
    project_loader.rs
    slot_mutation.rs

lp-core/lpc-wire/src/
  project/
    wire_project_request.rs
  slot/
    mutation.rs

lp-app/lpa-server/src/
  server.rs
  handlers.rs

lp-cli/src/debug_ui/
  clock_controls.rs
  ui.rs
  node_cards.rs

examples/
  basic/project.toml
  basic/shader.toml
  fluid/project.toml
  fluid/compute.toml
  fluid/compute.glsl
  fluid/fluid.toml
```

## Architecture Summary

`ClockNode` is a normal runtime node. It exposes produced runtime slots through `ClockState`. Its `seconds` slot is conventionally bound to `bus#time.seconds` by a default binding rule, not by hidden engine behavior.

Shader and compute nodes consume time through their existing consumed-slot resolution path:

```toml
[bindings.time]
source = "bus#time.seconds"
```

No shader path special-cases `time`.

Clock controls are authored def fields under a dedicated transient `controls` record, but default controls do not need to appear in project TOML:

```toml
[nodes.clock]
kind = "clock"
```

`ClockDef::default()` supplies `controls.running = true`, `controls.rate = 1.0`, and `controls.scrub_offset_seconds = 0.0`.

The clock's default binding supplies `seconds -> bus#time.seconds` only when the clock def does not explicitly bind `seconds`.

`controls` values are writable and marked transient through slot metadata. The loaded in-memory project can mutate them, but future TOML save/writeback should skip transient fields by default.

Inline invocations let small utility nodes live directly inside `project.toml`. Artifact-backed invocation remains supported:

```toml
[nodes.shader]
artifact = "./shader.toml"
```

Real mutation is added narrowly. The client sends project-scoped slot mutation requests. The server applies `SetValue` only to value leaves on `node.<id>.def` roots, using expected shape/data revisions. Accepted mutation updates the in-memory authored def and emits normal project-read slot changes on the next poll.

## Main Components

### Slot Persistence Metadata

Add a small persistence hint to `SlotMeta`:

```rust
pub enum SlotPersistence {
    Persisted,
    Transient,
}
```

Default is `Persisted`. `ClockControls` uses `Transient`. This is tool/persistence metadata, not resolver semantics.

### Inline Node Invocation

Change `NodeInvocation` from artifact-only to an invocation that can carry either:

- `Artifact { artifact: ArtifactPathSlot }`
- `Inline { def: NodeDef }`

Serde should preserve the clean TOML forms:

- `artifact = "./shader.toml"` for artifact invocations.
- `kind = "clock"` plus normal node-def fields for inline invocations.

### Clock Node

`ClockDef` owns:

- `bindings: BindingDefs`
- `controls: ClockControls`

`ClockControls` owns:

- `running: BoolSlot`
- `rate: ValueSlot<f32>` or a small semantic f32 slot with slider metadata
- `scrub_offset_seconds: ValueSlot<f32>` or semantic signed f32 slot with `-10..10` slider metadata

`ClockState` owns:

- `seconds: ValueSlot<f32>`
- `delta_seconds: ValueSlot<f32>`

`ClockNode` stores accumulated seconds internally and updates state every tick according to controls.

### Default Bindings

Default bindings are fallback route conventions. They are not ordinary serialized authored bindings.

For clock:

```text
seconds -> bus#time.seconds
```

Rules:

- If `bindings.seconds` exists, use it at authored priority.
- If `bindings.seconds` does not exist, register the default binding at fallback priority.
- Authored bindings must win over default bindings.
- Equal-priority ambiguity should still be rejected for authored bindings.

This should start as a small loader helper and a `BindingPriority` convenience, such as:

```rust
BindingPriority::default_fallback()
BindingPriority::authored()
```

Existing explicit bindings using `BindingPriority::new(0)` should remain functionally authored priority, or be moved to `authored()` in this plan.

### Mutation

Use existing wire mutation types inside `ProjectReadRequest`:

```rust
pub struct ProjectReadRequest {
    // ...
    pub mutations: Vec<WireSlotMutationRequest>,
}
```

Response returns mutation results alongside the read results:

```rust
pub struct ProjectReadResponse {
    // ...
    pub mutations: Vec<WireSlotMutationResponse>,
}
```

Mutations are applied before read queries and probes are collected, so one
round trip can say "make this edit, then give me the current state."

The first implementation supports:

- root names shaped like `node.<id>.def`,
- value leaf paths only,
- `SetValue`,
- optimistic conflict checks,
- type mismatch rejection,
- unsupported target rejection for everything outside scope.

### Debug UI

The debug UI finds clock nodes from synced node/slot data and renders controls:

- running checkbox,
- rate slider,
- scrub offset slider,
- pending/error status from `SlotMirrorView`.

Controls send mutation requests and wait for server confirmation; no optimistic local write.

## Example Shape

`examples/fluid/project.toml` should inline clock:

```toml
[nodes.clock]
kind = "clock"
```

`examples/fluid/compute.toml` should bind time:

```toml
[bindings.time]
source = "bus#time.seconds"
```

`examples/fluid/compute.glsl` should use `time` directly rather than persistent global `phase`.
