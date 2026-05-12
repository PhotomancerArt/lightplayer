# M2.3 Authored Slot Bindings Notes

## Scope Of Work

M2.3 should make authored node bindings tell the truth before the runtime node
truth pass in M2.4. The milestone is about the shared domain model used by
source artifacts, wire, and tooling: how a node artifact declares that one of
its slots consumes from or produces to another endpoint.

In scope:

- Define the authored binding model for node defs in the shared model layer.
- Replace ad hoc node-reference fields that are really bindings in disguise.
- Establish the baseline TOML form for consumed and produced slot bindings.
- Make bus-first authored wiring the idiomatic path.
- Keep direct node-to-node slot wiring available as an explicit source/target
  form.
- Update source defs, parsing, and validation to use the new authored binding
  model.
- Update `examples/basic` and source-level tests to use the new binding shape.
- Delete as much obsolete `lpc-source/src/prop` surface as possible once the
  durable binding concepts live in `lpc-model`.
- Leave runtime loader follow-through only as far as needed to keep source
  artifacts coherent for M2.4.

Out of scope:

- Full runtime node truth pass (`ShaderNode`/`TextureNode`/`FixtureNode`
  refactor) beyond what is minimally needed to keep the source model buildable.
- Canonical wire/project sync redesign.
- Runtime slot roots and view/UI work.
- Invocation-site binding overrides as a finished feature.
- Bus-resolution policy beyond the authored shape and basic directionality
  validation.

## User Notes

- The idiomatic authored path should not be direct node-to-node wiring.
- The intended common UX is decoupled bus wiring:
  - a shader can publish `output` to something like `bus/visual_out`
  - a texture or fixture can consume `input` from that same bus channel
  - this allows library artifacts to be reused independently and “just work”
    when dropped into a project.
- Direct node-to-node slot bindings remain important, but they should be the
  explicit, local-wiring path rather than the default idiom.
- `input` is not a special concept in the node model; it is merely a slot name.
  A node should not need a dedicated `inputs` field shape.
- There are two directional binding roles:
  - produced slots bind outward to a target
  - consumed slots bind inward from a source
- Putting bindings only on `NodeInvocation` is not the right baseline.
  Invocation-site overrides are a future feature, but the primary home for
  bindings should be the node defs themselves.
- The user expects TOML that reads like:

  ```toml
  [bindings.output]
  target = "bus/visual_out"

  [bindings.input]
  source = "bus/visual_out"
  ```

  and direct node-to-node forms like:

  ```toml
  [bindings.input]
  source = "..shader#output"
  ```

- Directionality should be validated rather than encoded as separate “input”
  types.
- `lpc-source/src/prop` should shrink aggressively. Most of it reflects the old
  source/prop model and should not remain a home for new binding work.
- `toml_color` is an exception to be careful with. It contains useful authored
  color parsing decisions and should be preserved or deliberately moved, not
  casually deleted with the rest of the prop cleanup.

## Current Code State

### Source Node Definitions

Current source defs still encode connectivity in node-specific fields:

- `ShaderDef`:
  - `glsl_path`
  - `texture_loc`
  - `render_order`
  - `glsl_opts`
  - `param_defs`
- `FixtureDef`:
  - `output_loc`
  - `texture_loc`
  - mapping / color / transform / brightness / gamma config
- `TextureDef`:
  - `size`
- `OutputDef`:
  - pin / options config

This means authored connectivity is currently split across bespoke fields rather
than one uniform binding concept.

### `NodeInvocation`

`lpc-source/src/node/node_invocation.rs` currently has:

- `artifact: ArtifactPathSlot`
- `overrides: Vec<(ValuePath, SrcBinding)>`

This still reflects the earlier idea of use-site overrides, not a settled
source-binding model. It also uses `ValuePath`, which is old vocabulary for
what is now a slot-level concern.

### Existing Binding Types

`lpc-source/src/prop/src_binding.rs` currently defines `SrcBinding` as:

- `Bus(ChannelName)`
- `Literal(SrcValueSpec)`
- `NodeProp(NodePropSpec)`

This type is asymmetric: it describes where a value comes from, which fits a
consumed binding, but not a produced binding that writes to some target.

Runtime-side `lpc-engine/src/binding/binding_entry.rs` already has a more
truthful split:

- `BindingSource::{Literal, ProducedSlot, BusChannel}`
- `BindingTarget::{ConsumedSlot, BusChannel}`

That suggests the source-side model should become symmetric too.

### Examples Today

`examples/basic` still uses bespoke node-ref fields:

- `shader.toml` has `texture_loc = "..texture"`
- `fixture.toml` has `texture_loc = "..texture"` and `output_loc = "..output"`
- `texture.toml` has only size and no declared binding

This is exactly the authored-model mismatch that M2.3 should clean up.

### Why This Matters For M2.4

M2.4 wants a truthful runtime flow:

- shader produces render product
- texture consumes that product and materializes a target
- fixture consumes texture output
- output remains an IO sink boundary

That runtime plan is much easier to execute if the authored source defs already
say, in one uniform way, which slots consume from or produce to what.

## Open Questions

### Q1. What Is The Shared Authored Binding Container?

Context: connectivity is currently represented by bespoke fields like
`texture_loc` and `output_loc`. The user expects a common `BindingDefs`-style
concept that can live on node defs.

Suggested answer: introduce a shared `BindingDefs` field directly on node defs,
with the binding container and endpoint types living in `lpc-model` as part of
the durable semantic node-definition shape.

### Q2. What Is The Per-Slot Binding Shape?

Context: produced and consumed bindings are both needed, and `input` should not
be a privileged concept. The TOML should read naturally and make direction
obvious.

Suggested answer: each binding entry should be keyed by slot name and allow
exactly one of:

- `source = <endpoint>` for consumed slots
- `target = <endpoint>` for produced slots

with direction validated against the slot contract.

### Q3. What Is An Authored Endpoint?

Context: the common case is bus-first decoupled wiring, but direct node-slot
references remain important. `SrcBinding` is too source-shaped to represent
both directions cleanly.

Suggested answer: retire `SrcBinding` in its current asymmetric form and replace
it with a symmetric authored binding model in `lpc-model` that can represent:

- bus channels
- relative node-slot refs
- literals only where they make sense for consumed slots

The preferred direction is new types such as `BindingDef` /
`BindingEndpointDef`, rather than stretching old `SrcBinding` semantics.

### Q4. Where Do Bindings Live Relative To `NodeInvocation`?

Context: invocation-site overrides may still be valuable later, but the baseline
should be node-def-local authored bindings.

Suggested answer: make node-def-local bindings the baseline in M2.3. Keep
`NodeInvocation.overrides` as future work or transitional leftover, but do not
expand it as the primary solution in this milestone.

### Q5. How Aggressively Should M2.3 Replace Existing Bespoke Ref Fields?

Context: `texture_loc` and `output_loc` are really bindings in disguise.
However, some fields may remain conceptually authored config rather than
bindings.

Suggested answer: replace connectivity fields that exist only to express data
flow (`texture_loc`, likely `output_loc` if represented as a slot connection)
with the new binding model. Keep true authored config such as texture size,
GLSL path, render order, fixture mapping, output pin/options, etc.

### Q6. What Minimal Runtime/Loader Follow-Through Belongs In M2.3?

Context: the milestone is source-side, but if the new source defs are never read
by loader/tests, the plan will be too abstract.

Suggested answer: include only the minimal loader/test follow-through needed to
prove that source defs parse, validate, and can be interpreted into runtime
binding drafts. Leave the full runtime node flow refactor to M2.4.


### Q7. Keep Or Replace `SrcBinding`?

Context: `SrcBinding` currently encodes only source-shaped binding ideas and
reads as vestigial now that authored bindings must support both `source` and
`target`.

User answer: replace it. The current `SrcBinding` shape should not remain the
center of the design. Preferred direction is to move to clearer types such as
`BindingDef` and `BindingEndpointDef` rather than preserving the old semantics
under the same abstraction.


### Source/Model Boundary

The separation between `lpc-source` and `lpc-model` has become blurry, and that
is not inherently bad. The user explicitly wants node defs and their referenced
binding concepts to live in the shared semantic model when those concepts are
part of the durable node-definition shape.

That implies:

- if node defs carry `bindings`, the binding data types should live in
  `lpc-model`, not remain `lpc-source`-only
- binding endpoints should be semantic parsed Rust forms, not raw strings
- `lpc-source` should keep the genuinely source-specific concerns: artifact IO,
  locator resolution, TOML loading helpers, and any source-only adapters

This milestone should therefore favor promoting durable binding types into the
shared model rather than building a new `lpc-source`-local abstraction.

### `lpc-source/src/prop` Cleanup Goal

The user wants an aggressive cleanup of `lpc-source/src/prop` at the end of
M2.3. Treat the module as old vocabulary unless a file is clearly still useful
as source-specific loading/adaptation code.

The preferred end state is:

- durable binding/slot/value semantics live in `lpc-model`
- `lpc-source` owns artifact IO and source loading mechanics
- new source defs do not depend on `lpc-source/src/prop`
- useful authored-color parsing from `toml_color` is preserved
- any remaining `prop` files are temporary and easy to search/delete later
