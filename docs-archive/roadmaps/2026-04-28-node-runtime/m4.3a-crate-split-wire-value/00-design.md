# M4.3a — Crate Split + WireValue Design

# Scope of work

Establish clear crate roles for the LightPlayer core model, source,
wire, engine, and client layers before M4.3 runtime spine work grows
further into the current mixed `lpc-model` shape.

This milestone splits authored source and wire concerns out of
`lpc-model`, introduces shared `WireValue` / `WireType` model types,
and moves GLSL/runtime conversions into `lpc-engine`.

The intended dependency boundary:

- `lpc-model`: shared concepts between source, wire, engine, and client;
  must not depend on `lps-shared`.
- `lpc-source`: authored/on-disk source model, using `Src*` names where
  a prefix is needed; must not depend on `lps-shared`.
- `lpc-wire`: view wire model, using `Wire*` names; must not
  depend on `lps-shared`.
- `lpc-engine`: engine runtime model and execution support; may depend
  on `lps-shared`, and owns conversion between shader/runtime values and
  model/wire values.
- `lp-view`: client-side engine view/cache; should depend on
  `lpc-model + lpc-wire`, not `lps-shared`.

Out of scope:

- Broad rename of existing non-core `lp-*` crates. The user may do this
  later, but M4.3a should keep scope focused on core role separation.
- Full aesthetic cleanup of all names and modules. This milestone should
  create the crate boundaries; a follow-up cleanup pass is expected.
- New sync behavior beyond the minimal type/view shapes needed to compile
  after the split.

# File structure

```text
lp-core/
├── lpc-model/                         # UPDATE: shared concepts only, no lps-shared
│   └── src/
│       ├── lib.rs
│       ├── node/
│       │   ├── mod.rs
│       │   ├── node_id.rs
│       │   ├── node_name.rs
│       │   ├── node_prop_spec.rs
│       │   └── node_spec.rs
│       ├── prop/
│       │   ├── mod.rs
│       │   ├── constraint.rs
│       │   ├── kind.rs
│       │   ├── prop_namespace.rs
│       │   ├── prop_path.rs
│       │   ├── prop_value.rs
│       │   ├── wire_type.rs          # NEW: model-side storage/type projection
│       │   └── wire_value.rs         # NEW: public value shape promoted from LpsValueWire
│       ├── bus/
│       │   ├── mod.rs
│       │   └── channel_name.rs
│       ├── tree/
│       │   ├── mod.rs
│       │   └── tree_path.rs
│       ├── project/
│       │   ├── mod.rs
│       │   └── frame_id.rs
│       ├── error.rs
│       ├── lp_config.rs              # KEEP for now unless proven engine-only
│       ├── lp_path.rs
│       └── serial.rs
│
├── lpc-source/                        # NEW: authored source / on-disk format, Src*
│   └── src/
│       ├── lib.rs
│       ├── artifact/
│       │   ├── mod.rs
│       │   ├── artifact.rs
│       │   ├── artifact_spec.rs
│       │   └── load_artifact.rs
│       ├── node/
│       │   ├── mod.rs
│       │   └── src_node_config.rs
│       ├── prop/
│       │   ├── mod.rs
│       │   ├── src_binding.rs
│       │   ├── src_shape.rs
│       │   ├── src_slot.rs
│       │   ├── src_value_spec.rs
│       │   ├── src_value_spec_wire.rs
│       │   ├── src_texture_spec.rs
│       │   └── toml_parse.rs
│       ├── presentation.rs
│       └── schema/
│           ├── mod.rs
│           ├── migration.rs
│           └── registry.rs
│
├── lpc-wire/                          # NEW: engine↔client wire model, Wire*
│   └── src/
│       ├── lib.rs
│       ├── message/
│       │   ├── mod.rs
│       │   ├── client_message.rs
│       │   ├── server_message.rs
│       │   └── message.rs
│       ├── project/
│       │   ├── mod.rs
│       │   ├── wire_project_handle.rs
│       │   ├── wire_project_request.rs
│       │   ├── wire_project_status.rs
│       │   └── wire_project_view.rs
│       ├── tree/
│       │   ├── mod.rs
│       │   ├── wire_child_kind.rs
│       │   ├── wire_entry_state.rs
│       │   └── wire_tree_delta.rs
│       ├── state/
│       │   ├── mod.rs
│       │   ├── macros.rs
│       │   └── test_state.rs
│       ├── json.rs
│       ├── server.rs
│       └── transport_error.rs
│
├── lpc-engine/                        # UPDATE: engine/runtime, depends on lps-shared
│   └── src/
│       ├── lib.rs
│       ├── bus/
│       ├── resolver/
│       ├── tree/
│       ├── prop/
│       │   ├── mod.rs
│       │   └── runtime_prop_access.rs
│       └── wire_bridge/
│           ├── mod.rs
│           ├── lps_value_to_wire_value.rs
│           └── wire_type_to_lps_type.rs
│
└── lp-view/                  # UPDATE: client cache/view, no lps-shared
    └── src/
        ├── lib.rs
        └── prop/
            ├── mod.rs
            └── wire_prop_access.rs
```

# Conceptual architecture

```text
          authored files
               │
               ▼
        lpc-source (Src*)
        SrcArtifact / SrcShape / SrcBinding / SrcValueSpec
               │
               ▼
        lpc-engine (Engine*)
        NodeTree / ResolverCache / Bus / RuntimePropAccess
               │
       LpsValueF32 -> WireValue
               ▼
          lpc-wire (Wire*)
        WireMessage / WireTreeDelta / WireProjectHandle
               │
               ▼
      lp-view (Client*)
        client cache / WirePropAccess / UI views

lpc-model sits underneath all of them:
NodeId, TreePath, PropPath, FrameId, Kind, WireType, WireValue, etc.
```

# Main components

## `lpc-model`

`lpc-model` is the shared vocabulary crate. It owns identity,
addressing, semantic kinds, model-side value/type shapes, and small
cross-layer primitives.

It should not re-export `LpsValue`, `LpsType`, `TextureBuffer`, or other
`lps-shared` runtime/compiler types. `WireValue` replaces the current
private `LpsValueWire` concept from `value_spec.rs`, and `WireType`
replaces `Kind::storage() -> LpsType`.

## `lpc-source`

`lpc-source` owns persisted authored LightPlayer source. This includes
artifacts, slots, shapes, bindings, source node config, TOML parsing,
schema migration, and source-side value specs.

It should reuse the current `ValueSpec` serde behavior instead of
duplicating it. The large current `value_spec.rs` should be split into
single-concept files during the move: value spec, value spec wire form,
texture spec, TOML parsing, and materialization support.

## `lpc-wire`

`lpc-wire` owns the view wire contract. This includes messages,
tree deltas, project request/view types, transport errors, JSON helpers,
and legacy partial state serialization helpers.

Wire types should be named with `Wire*` where the name would otherwise
be ambiguous, for example `WireProjectHandle`, `WireTreeDelta`, and
`WireEntryState`.

## `lpc-engine`

`lpc-engine` owns runtime/engine-only behavior and is allowed to depend
on `lps-shared`. It owns the conversion boundary between shader/runtime
types and model/wire-safe types:

- `LpsValueF32 -> WireValue`
- `WireType -> LpsType`

The existing `PropAccess` semantics move here as `RuntimePropAccess` and
continue to expose `LpsValueF32` (shader/runtime union type from
`lps-shared`).

## `lp-view`

`lp-view` owns the client-side cache and UI view helpers for
engine state. It should not depend on `lps-shared`.

Client-side property iteration is separate from runtime property
reflection. It should expose `WireValue` via `WirePropAccess`, backed by
the client's local wire/cache state.
