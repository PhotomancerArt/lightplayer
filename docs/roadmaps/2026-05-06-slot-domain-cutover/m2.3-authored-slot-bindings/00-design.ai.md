# M2.3 Authored Slot Bindings Design

## Scope Of Work

M2.3 introduces the durable, semantic binding model used by node definitions.
Bindings are part of what a node artifact means, so the core types live in
`lpc-model`, not in `lpc-source`.

In scope:

- Add shared binding types to `lpc-model`.
- Represent binding endpoints as parsed semantic Rust types, not raw strings.
- Add a default-empty `bindings` field to node defs that participate in authored
  dataflow.
- Replace bespoke connectivity fields that are really bindings in disguise.
- Support both produced and consumed slot bindings:
  - `source` for consumed slots.
  - `target` for produced slots.
- Establish the bus-first authored pattern as the idiomatic path.
- Preserve direct node-slot references as explicit local wiring.
- Update source parsing/tests/examples enough to prove the authored model.
- Delete as much obsolete `lpc-source/src/prop` surface as possible once the
  model-level binding types replace it.
- Keep runtime follow-through minimal; M2.4 owns the runtime node truth pass.

Out of scope:

- Runtime shader/texture/fixture flow refactor.
- Full bus resolution semantics.
- Runtime slot roots.
- Canonical project sync, client/view work, or UI.
- Invocation-site binding overrides as a finished feature.
- Artifact mutation.

## File Structure

```text
lp-core/lpc-model/src/
  binding/
    mod.rs
    binding_def.rs
    binding_defs.rs
    binding_endpoint.rs
    bus_slot_ref.rs
    node_slot_ref.rs

  node/
    shader/
      shader_def.rs
    texture/
      texture_def.rs
    fixture/
      fixture_def.rs
    output/
      output_def.rs

lp-core/lpc-source/src/
  prop/
    src_binding.rs        # delete if possible
    ...                   # audit and remove obsolete prop-era code aggressively

lp-core/lpc-engine/src/project_runtime/
  project_loader.rs       # minimal interpretation follow-through

examples/basic/
  shader.toml
  texture.toml
  fixture.toml
```

## Architecture Summary

Bindings are declarations attached to slots. They do not define special node
concepts like "input"; `input` is just a common slot name.

Each node def can carry a `BindingDefs` container. `BindingDefs` is a stable-key
map from slot name to `BindingDef`:

```rust
pub struct BindingDefs {
    pub slots: MapSlot<String, BindingDef>,
}
```

Each `BindingDef` is directional. It declares exactly one of:

- `source`: this node's slot consumes from an endpoint.
- `target`: this node's slot produces to an endpoint.

The model-level representation stores parsed endpoints:

```rust
pub struct BindingDef {
    pub source: Option<BindingEndpoint>,
    pub target: Option<BindingEndpoint>,
}

pub enum BindingEndpoint {
    Bus(BusSlotRef),
    Node(NodeSlotRef),
    Literal(LpValue),
}
```

`Literal` is only valid for consumed bindings. It is included in the semantic
model because literals are real authored values, but validation should reject
literal targets.

## Endpoint Syntax

The authored string syntax follows one owner/slot pattern:

```text
<owner>#<slot>
```

For node references:

```text
..shader#output
..texture#output
```

For bus references:

```text
bus#visual.out
bus#time.phase
```

This keeps the TOML readable while allowing the model to store semantic parsed
forms:

- `NodeSlotRef { node: RelativeNodeRef, slot: SlotPath }`
- `BusSlotRef { slot: SlotPath }`

M2.3 should not over-design bus resolution. The important part is that "the bus"
is a first-class endpoint owner with slots, matching the same pattern as nodes.

## TOML Shape

The common bus-first form:

```toml
# shader.toml
kind = "shader"
glsl_path = "shader.glsl"
render_order = 0

[bindings.output]
target = "bus#visual.out"
```

```toml
# texture.toml
kind = "texture"

[size]
width = 16
height = 16

[bindings.input]
source = "bus#visual.out"
```

Direct local wiring remains available:

```toml
[bindings.input]
source = "..shader#output"
```

## Source Def Changes

`ShaderDef` should stop using `texture_loc` as the normal way to express
render-product flow. It should publish its `output` slot through `bindings`.

`TextureDef` should add `bindings` and use `input` as a normal consumed slot
name for its render-product input.

`FixtureDef` should replace `texture_loc` with a binding on `input`. `output_loc`
is also connectivity, but outputs are intentionally special IO boundaries. M2.3
should decide whether to move `output_loc` into bindings now or keep it until
M2.4 can model output sink registration cleanly.

`OutputDef` likely remains config-only for M2.3 unless a real authored slot
binding appears.

## Validation Boundaries

`lpc-model` owns structural validation:

- exactly one of `source` / `target`
- endpoint syntax parses into semantic refs
- literal targets are invalid

`lpc-source` owns artifact/source concerns:

- loading TOML
- path-relative artifact resolution
- source-only adapters while old types are retired

`lpc-engine` owns compose/runtime validation:

- binding direction matches the node's slot role
- endpoint type is compatible with the consumed/produced slot
- bus channel conflicts and priority rules
- conversion into runtime `BindingDraft`s

## Relationship To `SrcBinding`

`SrcBinding` is vestigial in its current form because it is source-shaped only.
M2.3 should replace it with the new model-level binding types rather than grow a
parallel system.

If complete deletion is too much in one pass, old references should be renamed
or isolated as compatibility leftovers. New code should use `BindingDef`,
`BindingDefs`, and `BindingEndpoint`.

## `lpc-source/src/prop` Cleanup

M2.3 should treat `lpc-source/src/prop` as suspect legacy vocabulary. Much of it
comes from the older source/prop/binding model and no longer matches the
slot-domain direction.

The cleanup target is aggressive:

- remove `SrcBinding` if all callers can move to model-level `BindingDef`
- remove source-only prop/path/spec helpers that duplicate `lpc-model` concepts
- preserve or deliberately move `toml_color`; it carries useful authored-color
  parsing behavior
- keep only genuinely source-specific loading/adaptation code
- avoid adding new code under `lpc-source/src/prop`

If any `prop` files must remain temporarily, they should be clearly documented
as transitional and should not be used by new source defs.

## Relationship To M2.4

M2.3 prepares the authored model. M2.4 consumes it.

M2.4 should be able to assume:

- node defs carry semantic authored bindings
- bus-first published/consumed dataflow is represented in examples
- direct node-slot references are represented as parsed refs
- runtime loader work can interpret bindings instead of bespoke `texture_loc`
  and `output_loc` fields

If the runtime refactor needs more binding semantics, that belongs in M2.4 only
after the authored model is stable.
