# M4.3b — Name alignment notes

# Scope of work

Do a naming and module-organization pass across the `lpc-*` crates after
the M4.3a crate split:

- `lpc-model`
- `lpc-source`
- `lpc-wire`
- `lpc-view`
- `lpc-engine`

The goal is to align type names, module names, re-exports, and
documentation around each crate's purpose before M4.3 runtime-spine
work continues.

This is mostly API naming and organization. It should avoid behavior
changes.

# Current state

## Crate roles

- `lpc-model`: shared concepts used by source/wire/view/engine. Should
  not imply wire-only or runtime-only ownership.
- `lpc-source`: authored/on-disk source model. Primary ambiguous names
  mostly use `Src*`, but short aliases remain.
- `lpc-wire`: engine-client wire contract. Some types use `Wire*`,
  but many public names still say `Client*`, `Server*`, or `Api*`.
- `lpc-view`: client-side view/cache for one engine. It still uses
  `Client*` names inherited from `lp-engine-client`.
- `lpc-engine`: runtime engine internals. It uses a mix of `Runtime*`,
  `NodeRuntime`, `ProjectRuntime`, and unprefixed engine-owned types.

## Public type inventory

### `lpc-model`

Shared identity/addressing:

- `NodeId`, `NodeName`, `NodeNameError`, `NodeSpec`,
  `NodePropSpec`, `TreePath`, `NodePathSegment`, `PropPath`,
  `Segment`, `PathError`, `PathParseError`, `FrameId`,
  `ChannelName`, `LpPath`, `LpPathBuf`.

Shared quantity/value model:

- `Kind`, `Dimension`, `Unit`, `Colorspace`, `InterpMethod`,
  `Constraint`, `ConstraintRange`, `ConstraintChoice`,
  `ConstraintFree`, `PropNamespace`, `PropValue<T>`,
  `WireValue`, `WireType`, `WireStructMember`.

Still-questionable/shared leftovers:

- `NodeProps` — legacy property-get/set trait, currently in model.
- `NodeSpecifier` / `nodes` module alias — legacy compatibility name.
- `LightplayerConfig`, `ProjectConfig` — shared config-ish types.
- `DomainError` — cross-cutting error.

Naming tension:

- `WireValue` and `WireType` live in `lpc-model`, even though `Wire*`
  reads like `lpc-wire` ownership. User instinct: `ModelValue` or
  similar.
- `PropValue<T>` is already a runtime-ish change-tracking wrapper name,
  so simply renaming `WireValue` to `Value` may be too ambiguous.

### `lpc-source`

Primary source names:

- `SrcNodeConfig`, `SrcBinding`, `SrcShape`, `SrcSlot`,
  `SrcTextureSpec`, `SrcValueSpec`.

Short compatibility aliases:

- `NodeConfig = SrcNodeConfig`
- `Binding = SrcBinding`
- `Shape = SrcShape`
- `Slot = SrcSlot`
- `TextureSpec = SrcTextureSpec`
- `ValueSpec = SrcValueSpec`

Unprefixed public names:

- `Artifact`, `ArtifactSpec`, `ArtifactReadRoot`, `LoadError`,
  `Migration`, `Registry`, `Presentation`, `BindingResolver`,
  `FromTomlError`, `LoadCtx`.

Naming tension:

- `Src*` is partially applied. `Artifact`, `ArtifactSpec`,
  `Presentation`, `Migration`, and `Registry` are still source-owned
  but unprefixed.
- The public aliases preserve old call sites but dilute the new naming.
- `LoadCtx` sounds engine/runtime-ish but lives with source value spec
  default materialization.

### `lpc-wire`

Wire-prefixed names:

- `WireProjectHandle`, `WireProjectRequest`, `WireNodeStatus`,
  `WireChildKind`, `WireEntryState`, `WireTreeDelta`.

Unprefixed / non-wire-prefixed names:

- `Message<R>`, `ClientMessage`, `ClientRequest`,
  `ServerMessage<R>`, `NoDomain`, `WireNodeSpecifier`,
  `ClientMsgBody`, `ServerMsgBody<R>`, `ServerConfig`,
  `AvailableProject`, `LoadedProject`, `MemoryStats`,
  `SampleStats`, `FsRequest`, `FsResponse`, `TransportError`,
  `TestState`, `json::Error`, `WireSlotIndex`.

Naming tension:

- `Wire*` is only used on some types.
- `Client*` / `Server*` may be fine because they describe direction,
  but they do not align with the crate prefix convention.
- `WireNodeSpecifier` aligns wire-facing node selection with other
  disambiguating `Wire*` payload nouns.
- `WireSlotIndex` pairs with `WireChildKind`; it is slot-list index
  on the wire contract.

### `lpc-view`

Current public names:

- `ClientApi`, `ClientProjectView`, `ClientNodeEntry`,
  `ClientNodeTree`, `ClientTreeEntry`, `StatusChange`,
  `ApplyError`, `WirePropAccess`, `WirePropsMap`.

Naming tension:

- The crate is now `lpc-view`, but most public types still use
  `Client*`.
- `WirePropAccess` exposes wire values, but the owner is client/view
  state. Possible aligned names: `ViewPropAccess`, `ViewPropsMap`.
- `StatusChange` and `ApplyError` are view-owned but unprefixed.

### `lpc-engine`

Current public names:

- Runtime/project: `ProjectRuntime`, `NodeRuntime`, `ProjectHooks`,
  `NodeInitContext`, `RenderContext`, `FrameTime`.
- Tree/runtime state: `NodeTree`, `NodeEntry`, `EntryState`,
  `TreeError`, `tree_deltas_since`.
- Resolver/bus: `ResolverCache`, `ResolvedSlot`, `ResolveSource`,
  `BindingKind`, `Bus`, `BusError`, `ChannelEntry`.
- Rendering/output: `Graphics`, `LpGraphics`, `LpShader`,
  `ShaderCompileOptions`, `MemoryOutputProvider`,
  `OutputChannelHandle`, `OutputFormat`, `OutputProvider`.
- Conversion: `RuntimePropAccess`, `lps_value_f32_to_wire_value`,
  `wire_type_to_lps_type`.

Naming tension:

- `Runtime*` survives in some type names while crate is `lpc-engine`.
- `NodeRuntime` / `ProjectRuntime` may be okay as concepts, but
  `RuntimePropAccess` may want `EnginePropAccess` if we want crate
  prefix alignment.
- `Graphics`, `Bus`, `NodeTree`, etc. are engine-owned but not
  prefixed. Prefixing everything may be noisy.

# Questions

## Confirmation-style questions

| #   | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Plan directory is `docs/roadmaps/2026-04-28-node-runtime/m4.3b-name-alignment/`? | User invoked `/plan m4.3b-name-alignment` in this roadmap context. | Yes |
| Q2 | Treat this as API naming/organization only, with no behavior changes? | Goal is alignment before moving on. | Yes |
| Q3 | Keep compatibility aliases only when needed for migration, and document them as temporary? | Existing `lpc-source` aliases dilute naming but reduce churn. | Yes |
| Q4 | Do not broadly rename app crates in this plan? | User already did app/core moves; this plan targets model-related `lpc-*` naming. | Yes |
| Q5 | Prefer one concept per file while reorganizing modules? | Matches prior user preference. | Yes |

## Discussion-style questions

### Q-A — Value/type names in `lpc-model`

Should `WireValue` / `WireType` be renamed now that they live in
`lpc-model`?

Options:

1. Keep `WireValue` / `WireType`: emphasizes crossing disk/wire
   boundaries, but conflicts with `lpc-wire` ownership.
2. Rename to `ModelValue` / `ModelType`: aligns with crate and user
   instinct, but `ModelType` may be confused with Rust/domain model
   types.
3. Rename to `CoreValue` / `CoreType`: indicates shared core
   vocabulary, but less aligned with crate prefix.
4. Rename to `ValueShape` / `TypeShape` or `ValueRepr` / `TypeRepr`:
   emphasizes structural representation, but is wordier.

Suggested answer: `ModelValue` and `ModelType`, plus
`ModelStructMember`. They are shared portable model representations;
`lpc-wire` can still use them in wire payloads without owning them.

### Q-B — Prefix policy by crate

Should every ambiguous public type use the crate role prefix?

Possible policy:

- `lpc-model`: `Model*` only for portable representation types where
  "model-owned" is not obvious (`ModelValue`, `ModelType`); keep
  foundational nouns unprefixed (`NodeId`, `TreePath`, `FrameId`,
  `Kind`).
- `lpc-source`: `Src*` for authored source shapes (`SrcBinding`,
  `SrcShape`, `SrcSlot`, `SrcValueSpec`, maybe `SrcArtifact`,
  `SrcArtifactSpec`, `SrcPresentation`).
- `lpc-wire`: `Wire*` for protocol payload/domain nouns
  (`WireMessage`, `WireProjectRequest`, `WireNodeStatus`,
  `WireTreeDelta`), but keep directional wrapper names if useful
  (`ClientMessage`, `ServerMessage`?) — open question.
- `lpc-view`: `View*` for client-side view/cache types
  (`ViewProject`, `ViewNodeEntry`, `ViewNodeTree`,
  `ViewPropAccess`).
- `lpc-engine`: `Engine*` only where ambiguity is high; keep core
  runtime nouns if they are the engine concept (`ProjectRuntime`,
  `NodeTree`, `ResolverCache`, `Bus`).

Suggested answer: adopt this selective-prefix policy, not a blanket
rename of every public type.

### Q-C — `Src` vs `Source`

Should source-owned type prefixes be `Src*` or `Source*`?

Current code uses `Src*`. `Source*` is clearer in prose but creates
long names (`SourceValueSpec`, `SourceNodeConfig`,
`SourceTextureSpec`). `Src*` matches Rust crate shorthand and keeps
types readable.

Suggested answer: keep `Src*`.

### Q-D — Should `lpc-wire` go all-in on `Wire*`?

Current wire types are mixed. A consistent version might use:

- `WireMessage`, `WireClientMessage`, `WireClientRequest`,
  `WireServerMessage`
- `WireClientMsgBody`, `WireServerMsgBody`
- `WireApiNodeSpecifier` or `WireNodeSpecifier`
- `WireAvailableProject`, `WireLoadedProject`, `WireMemoryStats`,
  `WireSampleStats`
- `WireFsRequest`, `WireFsResponse`, `WireTransportError`
- `WireSlotIndex`

The cost is high churn and verbose names; the benefit is very clear
ownership.

Suggested answer: apply `Wire*` to domain payload types and ambiguous
request/view/status/delta types, but keep directional envelope names
(`ClientMessage`, `ServerMessage`, `ClientRequest`) unless they cause
ambiguity. Rename `ApiNodeSpecifier` and `SlotIdx`, because those are
currently the most out-of-pattern.

### Q-E — Should `lpc-view` use `View*` instead of `Client*`?

Now that the crate is `lpc-view`, current names like `ClientProjectView`,
`ClientNodeTree`, and `ClientTreeEntry` read stale.

Possible aligned names:

- `ClientApi` -> `ViewApi`? or keep because API is client-facing?
- `ClientProjectView` -> `ProjectView` or `ViewProject`
- `ClientNodeEntry` -> `ViewNodeEntry`
- `ClientNodeTree` -> `ViewNodeTree`
- `ClientTreeEntry` -> `ViewTreeEntry`
- `WirePropAccess` -> `ViewPropAccess`
- `WirePropsMap` -> `ViewPropsMap`
- `StatusChange` -> `ViewStatusChange`

Suggested answer: use `View*` for view/cache structures and keep
`ClientApi` only if it represents a client abstraction rather than a
view object. Prefer `ProjectView` over `ViewProject` only if we want
natural English more than prefix grouping.

### Q-F — Legacy aliases and compatibility names

Should M4.3b remove aliases like `lpc_source::ValueSpec` and
`lpc_model::NodeSpecifier`, or leave them for a later cleanup?

Removing them now improves nomenclature but increases churn. Keeping
them can undermine clarity if docs keep using aliases.

Suggested answer: remove aliases that are unused after updating call
sites; keep only aliases needed by current legacy or external-facing
callers, and mark them with short deprecation/TODO comments.

### Q-G — Does `NodeProps` belong in `lpc-model`?

`NodeProps` is a legacy property-get/set trait that survived in
`lpc-model`. It is not obviously a shared model concept.

Potential homes:

- Move to `lpl-model` if it is legacy-only.
- Move to `lpc-engine` if it is runtime behavior.
- Retire if unused.

Suggested answer: inventory call sites during the plan. If legacy-only,
move to `lpl-model`; otherwise move to `lpc-engine`. Do not keep it in
`lpc-model` unless it is genuinely shared.

# Notes

- User wants this pass before moving on from the crate split.
- User specifically called out `WireValue` and "ModelValue or
  something" as likely naming drift.
- The plan should include README/doc updates so future work uses the
  aligned names by default.
- Confirmation answers:
  - Q1 accepted: plan directory is
    `docs/roadmaps/2026-04-28-node-runtime/m4.3b-name-alignment/`.
  - Q2 accepted: this is API naming/organization only, with no
    behavior changes.
  - Q3 changed: do **not** keep compatibility aliases for migration;
    do proper renames across call sites.
  - Q4 accepted: do not broadly rename app crates in this plan.
  - Q5 accepted: prefer one concept per file, with the main concept
    first in the file.
- Q-A accepted: rename `WireValue` / `WireType` /
  `WireStructMember` to `ModelValue` / `ModelType` /
  `ModelStructMember` in `lpc-model`.
- Q-B accepted: adopt a selective-prefix policy. Keep foundational
  shared nouns unprefixed in `lpc-model`; use `Model*` for portable
  representation types, `Src*` for authored source-specific types,
  `Wire*` for wire contract domain payloads, `View*` / natural
  `*View` for view/cache types, and only use `Engine*` where
  ambiguity is high.
- Q-C accepted: keep `Src*` as the source-owned prefix.
- Q-D accepted: message/request/response/envelope names imply wire
  and do not need a `Wire*` prefix. Use `Wire*` only to
  disambiguate nouns that also exist in model/source/view/engine
  forms, e.g. `WireTreeDelta`, `WireNodeStatus`,
  `WireNodeSpecifier`, `WireSlotIndex`.
- Q-E accepted with suffix style: `lpc-view` types should generally
  use `*View` suffixes rather than `View*` prefixes where that reads
  naturally. Rename stale `Client*` view/cache names toward
  `ProjectView`, `NodeEntryView`, `NodeTreeView`, `TreeEntryView`,
  `PropAccessView`, `PropsMapView`, and `StatusChangeView` (exact
  names finalized in design).
- Q-F accepted: remove compatibility aliases such as
  `lpc_source::ValueSpec` / `Binding` / `Shape` and
  `lpc_model::NodeSpecifier` / `nodes`; update call sites to the
  real names. If legacy code needs an alias, keep it close to the
  legacy crate rather than in shared crate roots.
- Q-G accepted: investigate `NodeProps`; it does not seem like it
  belongs in `lpc-model`.
- Q-G investigation result: `NodeProps` is only referenced by
  `lpc-model` docs/tests/re-export and one `lpv-model` re-export.
  It appears removable from `lpc-model`; if `lpv-model` needs a
  similar abstraction later, define it there rather than keeping this
  shared model trait.
