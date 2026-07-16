# ADR: Declarative Default Bindings — a Slot Declares Its Own Default

- **Status:** Accepted
- **Date:** 2026-07-09
- **Deciders:** Photomancer
- **Supersedes:** the five hardcoded default-binding loader helpers and
  the "any unbound f32 `time` input auto-binds" node-kind convention;
  executes the direction of the (now superseded) planning effort
  `2026-06-26-default-binding-metadata-explain-slot`
- **Superseded by:** None

## Context

Most real projects "work by magic": five hardcoded helpers in
`lpc-engine/src/engine/project_loader.rs` inject runtime bindings at
load — clock `seconds` → `bus:time`, shader/fluid `time` inputs ←
`bus:time`, visual outputs → `bus:visual.out` — each with its own
bespoke conditions (`shader_needs_default_time_binding` sniffs the slot
shape; fluid gates on `has_default_time_bus`; visual output checks
`ownership.suppress_visual_default_output()`). These defaults exist
only in engine memory: not in authored data, not in schema, invisible
to clients until the binding-graph probe (ADR 2026-07-06) started
reporting them by priority.

Meanwhile two implicit consumed slots — `fixture.input`,
`output.input` — are registered by bare name with no declaration
anywhere: no def field, no shape entry, nothing for indicators,
pickers, or type checking to hang on to. (Fluid's inputs and radio's
`input` are already declared def fields; these two are the last
holdouts.) And two produced product slots carry wrong metadata:
`ShaderState.output` and `FixtureState.output` are direction `Local` —
it "works" only because the loader binds them explicitly, but any
direction-driven policy would generate the wrong default from the lie.

The product goal (user, 2026-07-09): *"if you start a blank project and
add the basic nodes, they just work"* — zero-config wiring, customize
from there — with the wiring honest and visible, not magic.

## Decision

**A slot declares its own default binding.** One concept, two
encodings matching how slots are defined:

### 1. Static slots: a field attribute → the static descriptor

```rust
#[slot(produced, default_bind = "bus:time")]
pub seconds: ValueSlot<f32>,            // ClockState

#[slot(consumed, default_bind = "bus:visual.out")]
pub input: VisualProductSlot,           // FixtureDef (new, D8)
```

The attribute surfaces into the interned static slot shape: every
`ClockState` shares the default; zero project JSON.

### 2. Dynamic slots: a data field on the slot-def

Shader `consumed` map entries are themselves Slotted data, so the
default lives in the data:

```rust
pub struct ShaderSlotDef {
    // …
    pub default_bind: OptionSlot<ValueSlot<BindingRef>>,
}
```

```json
"consumed": { "time": { "kind": "value", "value": "f32",
                        "default_bind": "bus:time" } }
```

The create-shader templates write this explicitly — the shader `time`
default becomes visible, overridable config instead of a global
convention. **Auto-binding by node-kind convention is retired**; there
is no node-kind policy method (nothing needs node-level logic — the
audit found every existing default maps to a field attribute or the
shader data field).

### Metadata home: a first-class field on the field shape

`default_bind` lands beside `semantics` and `policy`:

```rust
pub struct StaticSlotFieldShape {           // const, read-only memory
    pub name: &'static str,
    pub shape: &'static StaticSlotShapeDescriptor,
    pub semantics: SlotSemantics,
    pub policy: SlotPolicy,
    pub default_bind: Option<&'static str>, // NEW
}
pub struct SlotFieldShape {                 // owned/serde mirror
    // …
    pub default_bind: Option<String>,       // NEW, skip-if-none
}
```

Rejected homes:

- **Inside `SlotSemantics`** — conceptually attractive (it *is*
  dataflow semantics) but `SlotSemantics` is `Copy`,
  const-constructible, and the *same type* is shared by the static and
  owned worlds; a string payload breaks all three properties or forces
  a closed well-known-channel enum. The field-shape level is where
  static/runtime representations already diverge on string ownership.
- **`SlotMeta`** — the display grab-bag; its own docs require dataflow
  behavior to live elsewhere.

### Attribute semantics

- **Endpoint only, direction derived**: a produced slot's default
  *publishes* (`target: bus:…`); a consumed slot's default *sources*
  (`source: bus:…`). Same derivation rule the binding editor (M4)
  uses. No source/target mini-grammar in the attribute.
- **`bus:` endpoints only** (matches D1 bus-only authoring; node-ref
  defaults have no use case). The derive macro validates lexically
  (`lpc-slot-macros` cannot depend on `lpc-model` — cycle); a
  shape-walk test runs `BindingRef::parse` over every `default_bind`
  in every registered static shape and asserts `Bus`.

### D8: declared slots are required

`FixtureDef.input: VisualProductSlot` and `OutputDef.input:
ControlProductSlot`, `#[slot(consumed)]`, with defaults
`bus:visual.out` / `bus:control.out`. Products are lightweight handles,
so def fields represent them fine; unset slots don't serialize, so
authored JSON is unchanged. With these two, every produced/consumed
slot in the model is declared; M3's binding-derived rows become a
safety net for genuinely dynamic wiring rather than the norm.

### Materialization: one generic loader pass

After a node's authored bindings register, walk its def + state slot
shapes (and shader `consumed_slots` entries) and for each
`default_bind`:

- **Authored wins**: skip when an authored binding for the same local
  slot exists (absent entry re-enables the default — deleting a
  binding is the "reset to default" gesture).
- **Ownership suppression stays a loader-context rule**, not slot
  metadata: playlist entry children still have produced-product
  defaults suppressed so they don't fight over `bus:visual.out`.
- Register at `BindingPriority::default_fallback()` — the wire's
  `origin: authored | default` is priority-derived
  (`wire_binding_origin`), so the M2 probe contract is untouched.
- **Unconditionally.** The `has_default_time_bus` gate is deleted: a
  fluid without a clock now registers its `time` default and the
  channel shows up *unfilled* (readers, no writers) on the bus, where
  the UI warns — surfacing the problem honestly instead of hiding it by
  not wiring. (Fluid cannot work without time; helping the user see
  that is the point.)

Defaults are **never serialized** into project JSON — they are schema
policy, not authored config.

### Guardrails (so the metadata cannot rot)

- `ShaderState.output` / `FixtureState.output` fixed to
  `#[slot(produced)]` (plus the missing
  `default_policy = "read_only_transient"` container attrs).
- Structural test: every product-carrying slot in every registered
  static shape must be `Produced`.
- Load-time assertion: a binding draft whose source is a
  `ProducedSlot` (or target a `ConsumedSlot`) must reference a slot
  whose shape direction matches — any future mislabel fails the load
  with a clear reason instead of silently generating wrong wiring.
- Shape-walk validation of every `default_bind` (real parser, `Bus`
  only).

### Deleted

`register_clock_default_time_binding`, `add_visual_default_time_binding`,
`shader_needs_default_time_binding`,
`register_visual_default_output_binding`,
`add_visual_default_output_binding`,
`register_fluid_default_time_binding`, `node_provides_default_time_bus`,
`has_default_time_bus`, `is_time_seconds_bus_target`, and the
`ProjectedNode.provides_default_time_bus` bookkeeping.

## Consequences

- Zero-config projects work: shader + fixture + output with no
  `bindings` JSON wires shader → `bus:visual.out` → fixture →
  `bus:control.out` → output. Authored bindings still override
  everything; characterization tests pin both.
- Defaults are self-describing: the honest-indicator UI (M5's other
  half) can explain *why* a slot is wired ("Default binding — declared
  by the slot: `seconds` publishes `bus:time`") from the slot's own
  metadata, and the DEF badge derives from `origin: default` exactly as
  the bus pane already does.
- Two small behavior changes, both user-approved: fluid-without-clock
  registers its default (unfilled channel + warning instead of silent
  absence); fixture/output inputs gain defaults (was authored-only).
- Shape dumps and JSON schemas gain `default_bind` where set
  (regenerated; the checked-in schema drift gate covers it).
- Future work this composes with, not included: "auto-bind to next
  unused channel" (a second clock proposing/taking `bus:time/2` —
  pairs with the `/instance` naming convention and M4's channel
  proposals); shape-derived binding kinds (the stringly
  `binding_kind_for_slot` survives M5); the "make explicit / pin"
  authoring escape hatch (M4-adjacent).
