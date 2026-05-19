# M1 Control Message And Trigger Event Notes

## Scope Of Work

M1 creates the minimal slotted control-message/event vocabulary needed for the next button and
playlist work.

In scope:

- Add a slotted `ControlMessage` / `TriggerEvent` model shape.
- Keep the first message envelope intentionally small:
  - `id: u32`
  - `seq: u32`
- Add typed conversion to/from `LpValue` so messages can travel through slots and bus bindings.
- Add shared shader input support for sentinel-mapped maps, used by both compute and visual shaders.
- Add a checked-in example project that demonstrates the end-to-end event flow.
- Add tests proving repeated events with different sequence IDs remain distinct.
- Document that address and args are deferred/commented out for later OSC-compatible expansion.

Out of scope:

- Button node implementation.
- Playlist node implementation.
- Radio node implementation.
- OSC network I/O.
- OSC typed args, bundles, timetags, or pattern matching.
- A generic persistent event log or multi-consumer event queue.

## Current Codebase State

- `LpValue` and `LpType` already include `U32`.
- Primitive `SlotValue` support already exists for `u32`.
- Slotted node definitions use `ValueSlot<T>` where `T: SlotValue`.
- The bus/binding model can route arbitrary `LpValue` through `BindingSource::ProducedSlot`,
  `BindingTarget::BusChannel`, and `QueryKey::Bus`.
- Current graph products are `VisualProduct` and `ControlProduct`.
- `ControlProduct` is not a naming blocker. It is sample/render output data in the broader control
  domain. `ControlMessage` can be the discrete addressable message/event side of that same domain.
- `ButtonEvent` exists in `lpc-shared::hardware` but should stay hardware-specific. The graph-level
  event vocabulary should be generic.

## User Decisions

- First graph-level event family should be called something like `TriggerEvent`.
- Other event families are expected later.
- OSC is good prior art, but the deadline version should omit address and args.
- Address is implied by the bus channel/binding path for now.
- Omit `source` from M1 entirely so the event shape is directly GLSL-compatible.
- `id` and `seq` should stay `u32` for M1; no new numeric model scalar type yet.
- Use `id`, not `uid`, to match the existing `FluidEmitter` sentinel map convention.
- Desired first fields:
  - `id: u32`, unique enough for local deduplication, probably random by convention.
  - `seq: u32`, source-local sequencing/retrigger identity.
- LightPlayer should grow toward brokering control data across OSC, MIDI, DMX, radio, UI, and
  internal graph behavior.

## Open Questions

### Q1. Should M1 add new numeric model scalar types?

Decision: no. Use the existing `u32` model value path.

Context: `id` and `seq` can be `u32` for the sign/radio/button timeline. This avoids changing
the core value grammar and codecs before the control-message shape has proven itself. Widening to
`u64` can be revisited when OSC/MIDI/DMX bridge semantics need stronger dedupe guarantees.

### Q2. Should the first type be named `ControlMessage`, `TriggerEvent`, or both?

Suggested answer: both, with `TriggerEvent` as a thin semantic alias/wrapper around the same minimal
envelope if that keeps future naming clear.

Context: `ControlMessage` is the long-term broker-domain name. `TriggerEvent` is the first
one-shot no-args use case. Implementation can choose either:

- one struct named `ControlMessage` with tests and comments calling the no-args edge use a trigger,
  or
- `ControlMessage` plus `pub type TriggerEvent = ControlMessage`.

Avoid duplicating storage shapes in M1.

### Q3. How should messages encode as `LpValue`?

Suggested answer: as `LpValue::Struct { name: Some("ControlMessage"), fields: ... }`.

Context: This avoids adding a new top-level `LpValue::Message` before the shape is proven, while
still giving slot/bus code a typed value.

Expected fields:

```text
id: U32
seq: U32
```

### Q4. Where should files live?

Suggested answer: `lpc-model`, not `lpc-shared`, because this is authored/slotted graph vocabulary,
not hardware-driver vocabulary.

Suggested files:

- `lp-core/lpc-model/src/control/control_message.rs`
- `lp-core/lpc-model/src/control/mod.rs`

If the repo strongly prefers existing product/control placement, use `lp-core/lpc-model/src/products/control/control_message.rs`, but keep the file concept-separated.

### Q5. Does M1 need runtime bus behavior changes?

Suggested answer: only tests and small resolver fixes if the existing bus rejects non-shader values.

Context: Current bus code has older `LpsValueF32` paths, while resolver bindings can carry
`LpValue`. M1 should find and update the narrowest path needed so produced
`MapSlot<u32, ControlMessage>` values can be bound to `bus#trigger`, merged by key, and consumed by
compute or visual shaders as fixed sentinel array uniforms. If that turns into a larger bus refactor,
stop and split a follow-up phase rather than bloating M1.

### Q6. What proves the end-to-end flow?

Decision: add a checked-in example project, not only unit tests.

Expected shape:

- `examples/trigger-events/project.toml`
- two compute shader nodes that each publish a sentinel-mapped `events` output to `bus#trigger`
- one visual shader node that consumes `bus#trigger` as an 8-slot sentinel-mapped event array
- the visual shader renders one colored circle per active event slot, using `id` and `seq` to vary
  placement/color/intensity enough that test output is inspectable
- the usual fixture/output files so it behaves like the other examples
