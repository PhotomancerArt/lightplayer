# 00 — Overview

## Where we are

After M2, the workspace looks like this:

```
lp-core/
  lpc-model/      foundation types: NodeId, NodePath, ArtifactSpec,
                  Slot/Shape/Kind/Constraint/ValueSpec, Binding,
                  Artifact trait, ChannelName, FrameId, …
  lpc-runtime/    spine in progress: ProjectRuntime, flat NodeEntry map,
                  fs-watch, frame versioning, panic recovery — but no
                  tree, no artifacts, no slot resolution.
lp-legacy/
  lpl-model/      legacy node configs (TextureConfig, ShaderConfig,
                  OutputConfig, FixtureConfig) + the legacy
                  NodeConfig trait, ProjectResponse, NodeKind enum.
  lpl-runtime/    legacy node impls (TextureRuntime, ShaderRuntime,
                  OutputRuntime, FixtureRuntime) + a temporary
                  ProjectHooks trait object the runtime calls into.
lp-vis/
  lpv-model/      visual artifact types (Pattern, Effect, Stack, Live,
                  Playlist, Transition) + ParamsTable, VisualInput,
                  EffectRef, LiveCandidate. No runtime.
```

The **legacy world** has nodes-as-flat-map, one bespoke `*Config`
per kind, and per-`NodeRuntime` impls that hand-roll lifecycle. It
works, ships on ESP32, but has no tree, no slot grammar, no artifact
abstraction.

The **visual world** has rich on-disk types with slot grammar, but
there's no runtime — `lpv-model` types have only ever been
serialised, never instantiated.

This roadmap **bridges them** by evolving `lpc-runtime` into a
domain-agnostic spine that both worlds plug into. Legacy nodes port
to the new shape (M5); visual nodes get their runtime in the next
roadmap.

## Conceptual model

```
   Authored model                       Runtime model
   ─────────────────                    ────────────────────────
   Artifact                             Node           ── per-instance impl
     ├─ slots (params, etc.)              ↑ owned by
     ├─ defaults                        NodeEntry      ── per-instance metadata
     ├─ structural children               ├─ EntryState (Pending/Alive/Failed)
     └─ embedded code (GLSL etc.)         ├─ status, frame versions, child list
        ↓ instantiate                     └─ NodeConfig (authored overrides)
                                            └─ ArtifactRef ─► ArtifactManager
   ArtifactSpec ─── load ──────────►    ArtifactManager
       │                                 ├─ refcount per spec
       │                                 ├─ state machine (Resolved/Loaded/…)
       ▼                                 └─ hot reload via fs-watch
   NodeConfig                                ↓
   ├─ artifact: ArtifactSpec            NodeTree
   └─ overrides: Binding map            ├─ NodeId → NodeEntry
                                        ├─ NodePath ↔ NodeId index
                                        ├─ parent / children
                                        └─ ProjectRuntime<D>
                                             ├─ tick / sync
                                             ├─ fs-watch routing
                                             ├─ frame versioning
                                             └─ panic recovery + memory pressure
```

The four namespaces every node has:

```
Node {
   params:  named   slots, consumed (authored)   — bindable
   inputs:  indexed slots, consumed (composition) — Input children (§01)
   outputs: indexed slots, produced (primary)     — render product
   state:   named   slots, produced (debug)       — node-recorded, introspectable
}
```

`params` and `inputs` are **consumed** values that flow *into* the
node from outside (literal, bus, sibling output, child output) and
are bindable. `outputs` and `state` are **produced** values the node
writes during `tick`. (Detail in [05](05-slots-and-props.md).)

## End-state crate map (after this roadmap)

```
lp-core/
  lpc-model/        foundation, no domain knowledge.
  lpc-runtime/      generic spine: ProjectRuntime<D: ProjectDomain>,
                    NodeTree, ArtifactManager, sync, panic recovery.

lp-legacy/
  lpl-model/        legacy configs (no longer carry NodeConfig trait).
  lpl-runtime/      legacy nodes impl lpc_runtime::Node.
                    LegacyDomain: ProjectDomain.

lp-vis/
  lpv-model/        visual artifacts (Pattern, Effect, Stack, …) —
                    already shippable; runtime in next roadmap.
```

Shape per crate is `model` (no_std, structures, serde) /
`runtime` (no_std, the live behaviour). Three subsystems
(`lpc` / `lpl` / `lpv`) follow the two-letter convention already used
by `lp-shader/lps-*`.

## What's load-bearing-novel

Most of the spine is paved by prior art (Godot lifecycle / paths,
Bevy `Handle<T>` for refcounts, LX placeholders for missing
artifacts, LX `addLegacyParameter` for migrations). What's
genuinely novel:

1. **Client / server architecture with frame-versioned wire sync.**
   Embedded engine + remote editor as a first-class shape. Most
   prior art is desktop-app, ui-and-engine-in-one-process.
2. **Per-node panic-recovery isolation.** Honoured by
   `panic_node::catch_node_panic`; surfaces as
   `NodeStatus::Error(panic_msg)` rather than process exit. Lets
   one bad shader take itself out without taking down the firmware.
3. **Unified `NodeStatus` on the container, not on `Node`.** Single
   source of truth for per-node lifecycle state, and the wire
   diff target.
4. **Three-kind child model — `Input` / `Sidecar` / `Inline` —**
   combining structural composition with author-friendly inline
   children, all under a single tree-uniform mechanism. The
   `Inline` kind in particular (binding-owned children spawned by
   a slot override) is, as far as we can tell, ours alone (§01).
5. **Always-lazy `EntryState` machine.** Children are
   parse-validated at parent-init but **constructed on demand**,
   and can be demoted from `Alive` back to `Pending` under memory
   pressure. The release valve falls out of the lazy model for
   free — important on ESP32-C6 (§01).
6. **Pull-based slot resolution with frame-stamped caching.**
   Every "did anything change?" question is a cache lookup
   inside `tick`; nothing in the spine pushes config / artifact
   changes into nodes (§02, §06).

## Glossary

This vocabulary is used consistently across all design files.

- **Artifact** — the on-disk *class* / prototype. Defines slot
  schema, defaults, structural children, embedded code (GLSL or
  builtin tag). Loaded via `ArtifactSpec`. Multiple instances can
  share one. ([03](03-artifact.md))
- **ArtifactSpec** — opaque string reference to an artifact (e.g.
  `"./fluid.pattern.toml"`).
- **ArtifactManager** — refcounted cache of loaded artifacts with a
  small state machine: `Resolved` / `Loaded` / `Prepared` / `Idle`
  + error variants. Bumps a `content_frame` on hot reload; nodes
  observe at next tick. ([03](03-artifact.md))
- **NodeConfig** — per-instance *authored use-site data*. Two
  fields: `artifact: ArtifactSpec` + `overrides: BTreeMap<PropPath,
  Binding>`. Lives in the parent's TOML at the use site.
  ([04](04-config.md))
- **Node trait** — small object-safe runtime spine implemented by
  every concrete node. Four required methods: `tick`, `destroy`,
  `handle_memory_pressure`, `props`. ([02](02-node.md))
- **NodeTree** — central container; `Vec<Option<NodeEntry>>` indexed
  by `NodeId.0`, plus path / sibling indices. ([01](01-tree.md))
- **NodeEntry** — per-instance metadata: `id`, `path`, `parent`,
  `children`, `child_kinds`, `status`, `*_ver: FrameId`,
  `EntryState`, `NodeConfig`, `ArtifactRef`. ([01](01-tree.md))
- **EntryState** — lazy lifecycle state of a `NodeEntry`:
  `Pending` (artifact resolved, node not instantiated) / `Alive`
  (instantiated and ticking) / `Failed` (instantiation failed).
  ([01](01-tree.md))
- **ChildKind** — discriminator on every child entry:
  `Input` (structural, parent-lifetime) /
  `Sidecar` (programmer-side declared, parent-lifetime) /
  `Inline` (slot-binding-owned, binding-lifetime). ([01](01-tree.md))
- **Slot** — schema-side declaration on the artifact. `Shape`
  (Scalar/Array/Struct) + `Kind` + `Constraint` + mandatory
  `default` + optional `bind` + optional `present`. Authored in
  TOML. ([05](05-slots-and-props.md))
- **Prop\<T\>** — runtime value with `FrameId` change tracking, used
  for produced fields (outputs + state) inside the node impl's
  `*Props` struct. Renamed from `StateField`. ([05](05-slots-and-props.md))
- **Binding** — `enum Binding { Bus(ChannelName), Literal(LpsValue),
  NodeProp(NodePropRef) }`. Stored on `NodeConfig`, applied at
  resolution time. ([06](06-bindings-and-resolution.md))
- **PropAccess** — derived reflection trait on `*Props` structs;
  lets the editor / sync layer read produced values generically.
  ([05](05-slots-and-props.md))
- **PropPath** — dot-and-bracket slot address, e.g.
  `params.gradient` or `outputs[0].rgb`.
- **NodePath** — slash-joined `name.type` segments, e.g.
  `/main.show/fluid.pattern`.
- **NodePropSpec** — combined `<NodePath>#<PropPath>` for addressing
  a specific property on a specific node.
- **ChannelName** — bus channel string, e.g. `audio/in/0/level`.
- **FrameId** — monotonic per-tick counter; the wire diff key.
- **NodeId** — opaque `u32` runtime id; never authored.
- **ProjectDomain** — trait that parameterises `ProjectRuntime`
  with the domain's artifact union, response payload, instantiate
  hook, etc. `LegacyDomain` and (future) `VisualDomain` impl it.
  ([08](08-domain.md))

## What this roadmap does NOT design

Out of scope here, deferred to subsequent roadmaps:

- **Bus implementation.** Channel resolution, multi-bus
  (Local/Group/Sync/Flock), cross-host sync. M5 ships a stub
  (`HashMap<ChannelName, NodeId>`) for legacy `target_textures`.
  ([06](06-bindings-and-resolution.md))
- **`lpfx` rendering abstraction.** CPU/GPU split. Next roadmap.
- **`lpv-runtime`.** Visual node impls. Next roadmap.
- **Editor UI** of any kind.
- **`lp-rig` extraction** (Fixture / Output split into their own
  subsystem).

## What this roadmap DOES design

Inside this design pass:

- The four-method `Node` trait surface ([02](02-node.md)).
- `NodeTree` / `NodeEntry` / `EntryState` / `ChildKind` ([01](01-tree.md)).
- `Artifact` trait + `ArtifactManager` state machine ([03](03-artifact.md)).
- `NodeConfig` shape ([04](04-config.md)).
- `Slot` (schema) vs `Prop<T>` (runtime) split ([05](05-slots-and-props.md)).
- `Binding` enum + pull-based resolution ([06](06-bindings-and-resolution.md)).
- Client / server sync surface ([07](07-sync.md)).
- `ProjectDomain` parameterisation + legacy mapping ([08](08-domain.md)).

The execution side (M4 = artifact spine, M5 = node spine + sync
cutover, M6 = cleanup) lives in the per-milestone plans, not here.
