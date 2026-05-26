# Clock Node, Time Bus, Inline Invocations, And Mutation Notes

## Scope

Build a normal LightPlayer clock node that produces time as dataflow, then use it to drive shader/compute time inputs through bus bindings.

The desired feature is not just "make fluid move." It should establish a useful domain pattern:

- `ClockNode` is a regular node.
- Time is exposed as produced slots and conventionally bound to the bus.
- Shaders consume time through normal bindings.
- Projects can declare small utility nodes inline so every project does not need `clock.toml`.
- Debug UI can pause and scrub time by mutating the clock node's transient controls.

Full mutation may be substantial, but the clock controls are a valuable first real slice because they exercise authored node defs, slot versions, server mutation, client pending state, and resolver behavior.

## User Notes

- Do not treat time specially in compute shader fallback.
- Prefer a `ClockNode` that supplies time as an output bound by default to a bus slot such as `bus#time.seconds`.
- Inline invocations are desirable so projects do not need one `clock.toml` file each.
- The clock should give shader debugging controls:
  - pause/resume time,
  - scrub through time values,
  - offset around the current time by roughly `0 ± 10s`,
  - include `rate` as a useful clock control.
- `running`, `rate`, and scrub/offset controls are transient user-editable values, not durable authored config.
- Put transient values together in a dedicated shape, likely `controls`.
- Add metadata or semantics indicating changes should not be persisted by default.
- This may foreshadow a later config/param/controls split, but for now grouping transient controls and marking persistence is enough.
- The fluid example currently pushes too hard and appears not to move because compute time is not wired as a normal input.

## Current Code Context

### Node Definitions

- `lp-core/lpc-model/src/nodes/node_def.rs`
  - `NodeDef` is the closed enum of authored node definitions.
  - It currently includes `Project`, `Texture`, `Shader`, `ComputeShader`, `Fluid`, `Output`, and `Fixture`.
  - Adding `Clock` starts here and in `lp-core/lpc-model/src/node/kind.rs`.
  - `NodeDef` already implements `SlotAccess`, so authored defs are slot roots.

### Node Invocations

- `lp-core/lpc-model/src/node/node_invocation.rs`
  - `NodeInvocation` is artifact-only today:
    - `artifact: ArtifactPathSlot`
  - Its docs explicitly reserve inline node definitions for richer invocation forms.
  - This is the likely place to add an enum-like invocation shape:
    - artifact-backed invocation,
    - inline node definition.

### Project Loading

- `lp-core/lpc-engine/src/engine/project_loader.rs`
  - `ProjectLoader` loads `/project.toml`, then iterates `project_def.nodes.entries`.
  - Each child invocation is resolved through `invocation.artifact_specifier()`.
  - Each child node def is loaded from disk by `load_node_def`.
  - Runtime nodes are attached in kind-specific passes.
  - Bindings are registered from each node def's `bindings`.
  - Bus endpoints currently collapse `bus#time.seconds` into `ChannelName("time.seconds")`.
  - This is sufficient for a first `ClockNode -> bus -> shader` flow.

### Binding Priority And Defaults

- `lp-core/lpc-engine/src/dataflow/binding/binding_entry.rs`
  - `BindingPriority` is an `i32`; higher wins for bus providers.
  - Equal top bus provider priority is rejected as ambiguous.
- Current project loading uses `BindingPriority::new(0)` for authored bindings.
- Default bus conventions should not be serialized into `bindings`, because they must disappear when an explicit binding for the same slot exists.
- The right first model is likely a lower priority default/fallback binding, such as `BindingPriority::default_fallback()` below authored priority.

### Existing Time Handling

- Engine tick computes `frame_time.total_ms` and passes `time_seconds` through `TickContext`.
- Visual shader render/sample requests receive `time_seconds` directly.
- Compute shader consumed slots are resolved through bindings; if unresolved, the shader slot default is used.
- `examples/fluid/compute.toml` defines `[consumed.time]`, but does not bind it.
- `examples/fluid/compute.glsl` currently increments a global `phase`; this depends on shader runtime global persistence and is not a clear dataflow example.

### Slot Mutation Infrastructure

- `lp-core/lpc-wire/src/slot/mutation.rs`
  - Wire request/response exists:
    - `WireSlotMutationRequest`
    - `WireSlotMutationResponse`
    - optimistic shape/data revisions
    - `SetValue(LpValue)`
  - It should be carried by `ProjectReadRequest` so mutation and follow-up read
    share one response envelope.

- `lp-core/lpc-view/src/slot/mirror.rs`
  - Client-side pending mutation model exists.
  - It validates against local shape/data and tracks pending requests without optimistic local writes.
  - This is close to the desired client behavior for clock controls.

- `lp-core/lpc-slot-mockup`
  - Contains a server-side mutation harness and tests.
  - Useful reference, but real engine mutation has not been ported.

### Wire/Server

- `lp-core/lpc-wire/src/messages/project_read/project_read_request.rs`
  - `ProjectReadRequest` is the canonical project-scoped request.
  - Mutations belong here, not in a parallel project request variant.

- `lp-app/lpa-server/src/server.rs`
  - Streaming project read is handled specially in `tick_and_send`.
  - Other project requests fall through to `handlers::handle_client_message`.
  - Mutation can initially use normal materialized response payloads because mutation responses are small.

### Debug UI

- `lp-cli/src/debug_ui`
  - Already reads slot roots and runtime status.
  - It does not yet send mutation requests.
  - `ProjectView.slots` has pending mutation support through `SlotMirrorView`, but the debug UI does not expose it.

## Suggested Domain Shape

### Clock Def

Initial authored definition could live at:

- `lp-core/lpc-model/src/nodes/clock/clock_def.rs`
- `lp-core/lpc-model/src/nodes/clock/clock_state.rs`

Potential fields:

```rust
pub struct ClockDef {
    pub bindings: BindingDefs,
    pub controls: ClockControls,
}

pub struct ClockControls {
    pub running: BoolSlot,
    pub rate: ValueSlot<f32>,
    pub scrub_offset_seconds: ValueSlot<f32>,
}
```

Possible additions:

- `base_seconds`
- `mode`
- `wrap_seconds`

Suggested first pass: keep it to `controls.running`, `controls.rate`, and
`controls.scrub_offset_seconds`.

`controls` or each field within it should carry explicit slot metadata saying:

- writable by clients,
- transient / do not persist by default.

The likely model addition is a small persistence enum on `SlotMeta`, such as:

```rust
pub enum SlotPersistence {
    Persisted,
    Transient,
}
```

This is presentation/tooling metadata rather than resolver dataflow semantics.
`SlotSemantics` should remain focused on direction and merge policy.

### Clock State

Clock state should expose at least:

- `seconds: ValueSlot<f32>`
- maybe `delta_seconds: ValueSlot<f32>`
- maybe `frame: ValueSlot<u32/u64>` later

The primary default binding should be from `seconds` to `bus#time.seconds`.

### Runtime Behavior

If `running = true`:

- `seconds = accumulated_clock_seconds + scrub_offset_seconds`
- accumulated time advances by `delta * rate`.

If `running = false`:

- accumulated time is frozen.
- `scrub_offset_seconds` still changes the produced `seconds`.

This makes pause/scrub deterministic and lets a user inspect a glitch around a stable time anchor.

## Inline Invocation Design Pressure

Target TOML shape:

```toml
[nodes.clock]
kind = "clock"
```

`ClockDef::default()` should provide default `controls` and a default binding convention for `seconds -> bus#time.seconds`, but that binding should not serialize as an authored binding and should not apply if the user has an explicit binding for `seconds`.

Artifact form remains:

```toml
[nodes.clock]
artifact = "./clock.toml"
```

This implies `NodeInvocation` should no longer be artifact-only. Possible names:

- `NodeInvocation::Artifact { artifact }`
- `NodeInvocation::Inline { def }`

Because TOML externally uses an untagged form, the Rust representation may use serde untagged helper structs and normalize into an enum.

## Mutation Design Pressure

Clock debug controls require mutating fields on an authored node def:

- root: probably `node.<id>.def`
- path: `controls.running`, `controls.rate`, `controls.scrub_offset_seconds`

The server must:

- locate the live node,
- find its authored `NodeDef`,
- check expected shape and data revisions,
- type-check the incoming `LpValue`,
- update the def field,
- bump revisions,
- update affected bindings/runtime behavior on next tick.

Known complication:

- Current loaded defs are held in the artifact store and node tree handles.
- We need a clean mutable authored-def path, not a parallel "source store."

Suggested first real slice:

- Put `Vec<WireSlotMutationRequest>` directly on `ProjectReadRequest`.
- Support only `node.<id>.def` roots initially.
- Support `SetValue` for value leaves only.
- Reject runtime state roots and non-value/container paths for now.
- Use existing `WireSlotMutationRejection` variants where possible.

## Open Questions

### Q1. Should inline invocation be in this plan?

Context: The clock node is the strongest current motivation for inline invocations. Without inline defs, every project needs a `clock.toml`, which is noisy.

Suggested answer: Yes. Include inline invocations before clock example migration.

### Q2. Should clock controls mutate authored def fields, or runtime state?

Context: `running`, `rate`, and `scrub_offset_seconds` feel like controls/config. Mutating runtime state would be easier to make ephemeral, but would create a second mutable control surface.

Decision: Mutate authored def fields through `node.<id>.def`, but keep them under a dedicated transient `controls` record. Treat the file-backed/on-disk persistence question as future work; runtime mutation first updates the in-memory authored def. Add metadata so future save/writeback can skip transient controls by default.

### Q3. Should mutation persist to TOML immediately?

Context: User wants debug UI controls. Persisting to files implies artifact writeback, formatting, and maybe source reload semantics.

Suggested answer: No for this plan. Mutations affect the loaded project in memory. Future work can add persistence/save.

### Q4. What produced clock slots are in scope?

Context: Compute shaders need seconds now. Debugging likely wants delta/frame soon.

Suggested answer: Start with `seconds` and `delta_seconds`; optionally expose `frame` if easy. Bind only `seconds` in examples.

### Q5. What should the conventional bus name be?

Context: Existing bus syntax is `bus#visual.out` and `bus#control.out`. Slot/value separators are not fully solved, but bus channel currently treats everything after `#` as a channel string.

Suggested answer: Use `bus#time.seconds` now. It matches existing examples and avoids reopening path syntax.

### Q8. Should default bindings be ordinary authored bindings?

Context: The clock should bind to `bus#time.seconds` by default so users do not think about wiring. But that default must not apply when there is an explicit binding elsewhere for the same slot.

Decision: No. Default bindings are fallback binding rules, not ordinary serialized authored bindings. Register them at lower priority and only when the node does not already have an explicit binding for that slot. Start with clock `seconds -> bus#time.seconds`, then generalize later.

### Q6. Should clock be implicitly added to every project?

Context: A default clock is convenient, but implicit nodes make project/debug views less honest.

Suggested answer: No. Add it explicitly in examples, preferably inline.

### Q7. How much mutation should this plan implement?

Context: Full generic mutation is large. Clock only needs scalar value leaves on def roots.

Suggested answer: Implement the minimum real mutation path, not a debug-only special case:

- project-scoped mutation request/response,
- server applies value leaf changes to authored def roots,
- debug UI uses pending mutation machinery,
- no persistence and no container mutation yet.

## Initial Validation Targets

- `cargo test -p lpc-model`
- `cargo test -p lpc-engine`
- `cargo test -p lpc-wire`
- `cargo check -p lp-cli`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`

Run narrower checks while developing, then the full list before completion.
