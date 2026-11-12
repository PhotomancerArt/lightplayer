# M4.3b — Name Alignment Design

# Scope of work

Align public type names, module names, crate-root exports, and README
guidance across the post-M4.3a `lpc-*` crate split:

- `lpc-model`
- `lpc-source`
- `lpc-wire`
- `lpc-view`
- `lpc-engine`

This plan is an API naming and organization pass. It should avoid
behavior changes.

Out of scope:

- Broad app crate renames.
- New runtime/source/wire behavior.
- Keeping compatibility aliases in shared crate roots. M4.3b should do
  proper renames and update call sites.

# File structure

```text
lp-core/
├── lpc-model/
│   └── src/
│       ├── lib.rs                         # UPDATE: no aliases / no NodeProps
│       ├── prop/
│       │   ├── mod.rs
│       │   ├── model_value.rs             # RENAME: wire_value.rs
│       │   ├── model_type.rs              # RENAME: wire_type.rs
│       │   ├── prop_path.rs
│       │   ├── prop_namespace.rs
│       │   ├── prop_value.rs
│       │   ├── kind.rs
│       │   └── constraint.rs
│       └── node/
│           ├── mod.rs                     # UPDATE: remove node_props
│           ├── node_id.rs
│           ├── node_name.rs
│           ├── node_spec.rs               # canonical replacement for NodeSpecifier
│           └── node_prop_spec.rs
│
├── lpc-source/
│   └── src/
│       ├── lib.rs                         # UPDATE: no short aliases
│       ├── artifact/
│       │   ├── src_artifact.rs            # RENAME: artifact.rs
│       │   ├── src_artifact_spec.rs       # RENAME: artifact_spec.rs
│       │   └── load_artifact.rs
│       ├── presentation.rs                # maybe SrcPresentation if exported broadly
│       ├── node/src_node_config.rs
│       └── prop/
│           ├── src_binding.rs
│           ├── src_shape.rs
│           ├── src_texture_spec.rs
│           ├── src_value_spec.rs
│           ├── src_value_spec_wire.rs
│           ├── toml_parse.rs
│           └── toml_color.rs
│
├── lpc-wire/
│   └── src/
│       ├── lib.rs
│       ├── message/
│       │   ├── client.rs                  # KEEP: ClientMessage / ClientRequest
│       │   └── envelope.rs                # KEEP: Message / ServerMessage
│       ├── project/
│       │   ├── wire_node_specifier.rs     # RENAME: ApiNodeSpecifier
│       │   ├── wire_project_handle.rs
│       │   └── wire_project_request.rs
│       └── tree/
│           ├── wire_slot_index.rs         # RENAME: SlotIdx
│           ├── wire_child_kind.rs
│           ├── wire_entry_state.rs
│           └── wire_tree_delta.rs
│
├── lpc-view/
│   └── src/
│       ├── lib.rs
│       ├── api/client.rs                  # inspect ClientApi; rename only if it is view-owned
│       ├── project/project_view.rs        # RENAME: view.rs
│       ├── prop/prop_access_view.rs       # RENAME: wire_prop_access.rs
│       └── tree/
│           ├── node_tree_view.rs          # RENAME: client_node_tree.rs
│           ├── tree_entry_view.rs         # RENAME: client_tree_entry.rs
│           └── apply.rs
│
└── lpc-engine/
    └── src/
        ├── prop/runtime_prop_access.rs
        ├── wire_bridge/
        │   ├── lps_value_to_model_value.rs
        │   └── model_type_to_lps_type.rs
        └── ...
```

# Conceptual architecture summary

```text
lpc-model
  shared nouns: NodeId, TreePath, PropPath, FrameId, Kind
  model representation: ModelValue, ModelType, ModelStructMember

lpc-source
  authored/source-specific exported nouns: Src*
  examples: SrcArtifact, SrcArtifactSpec, SrcBinding, SrcShape, SrcSlot, SrcValueSpec

lpc-wire
  message/request/response names imply wire
  Wire* only disambiguates parallel nouns:
    WireTreeDelta, WireNodeStatus, WireNodeSpecifier, WireSlotIndex

lpc-view
  local cache/view data uses *View suffix where natural:
    ProjectView, NodeEntryView, NodeTreeView, TreeEntryView, PropAccessView

lpc-engine
  engine runtime nouns stay natural unless ambiguous:
    ProjectRuntime, NodeTree, ResolverCache, Bus
  conversion names should say Model* when they consume lpc-model representations
```

# Naming guidelines

The implementation should update crate READMEs to include these rules.

## `lpc-model`

Use unprefixed names for foundational shared concepts:

- `NodeId`
- `TreePath`
- `PropPath`
- `FrameId`
- `Kind`
- `ChannelName`

Use `Model*` for portable structural representations owned by
`lpc-model`:

- `ModelValue`
- `ModelType`
- `ModelStructMember`

Do not use `Wire*` in `lpc-model` for these shared representations.

## `lpc-source`

Use `Src*` for exported authored/source-specific concepts:

- `SrcArtifact`
- `SrcArtifactSpec`
- `SrcBinding`
- `SrcShape`
- `SrcSlot`
- `SrcValueSpec`
- `SrcTextureSpec`

Do not keep root aliases like `ValueSpec = SrcValueSpec`. Call sites
should use the real names.

## `lpc-wire`

Message/request/response/envelope names already imply wire and do not
need `Wire*`:

- `Message`
- `ClientMessage`
- `ClientRequest`
- `ServerMessage`
- `FsRequest`
- `FsResponse`

Use `Wire*` for nouns that have model/source/view/engine siblings:

- `WireTreeDelta`
- `WireNodeStatus`
- `WireNodeSpecifier`
- `WireSlotIndex`

## `lpc-view`

Use natural `*View` suffixes for local cache/view data:

- `ProjectView`
- `NodeEntryView`
- `NodeTreeView`
- `TreeEntryView`
- `PropAccessView`
- `PropsMapView`
- `StatusChangeView`

Avoid stale `Client*` names for view/cache structures. Keep `Client*`
only for genuine app/client abstractions.

## `lpc-engine`

Keep natural engine runtime nouns when crate ownership is already clear:

- `ProjectRuntime`
- `NodeTree`
- `ResolverCache`
- `Bus`

Use `Engine*` only when ambiguity is high. Conversion helpers should name
the boundary precisely, for example `lps_value_f32_to_model_value` and
`model_type_to_lps_type`.

# Main changes

- Rename `WireValue` / `WireType` / `WireStructMember` to
  `ModelValue` / `ModelType` / `ModelStructMember`.
- Rename model modules `wire_value.rs` / `wire_type.rs` to
  `model_value.rs` / `model_type.rs`.
- Remove `NodeProps` from `lpc-model`; current inventory shows it is
  only re-exported by `lpv-model` and otherwise self-tested.
- Remove `lpc_model::NodeSpecifier` and `lpc_model::nodes`; update call
  sites to `NodeSpec` / `NodeId`.
- Remove `lpc-source` compatibility aliases (`Binding`, `Shape`, `Slot`,
  `TextureSpec`, `ValueSpec`) and update all call sites to `Src*`.
- Rename source artifact concepts to `SrcArtifact` and `SrcArtifactSpec`
  if call-site churn is reasonable.
- Rename `lpc-wire::ApiNodeSpecifier` to `WireNodeSpecifier`.
- Rename `lpc-wire::SlotIdx` to `WireSlotIndex`.
- Rename `lpc-view` cache/view objects from `Client*` / `Wire*` to the
  `*View` suffix names.
- Update README naming guidelines and roadmap/design references.
