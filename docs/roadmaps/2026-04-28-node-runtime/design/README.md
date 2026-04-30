# Node-runtime spine — design

This directory is the **binding shape decision** for the node-runtime
spine. M4 and M5 implement against it; if implementation reveals a
mistake, edit the relevant file (with a brief erratum entry at the
bottom) rather than letting M4 / M5 silently diverge.

**Not** an implementation plan. There are no phases, no commit plans,
no checklists. Where a section sketches a Rust signature, it's to pin
down a decision, not to dictate the final code.

## Reading order

These files build on each other; read in order.

| #  | File                                                            | What it covers                                                                                          |
|----|-----------------------------------------------------------------|---------------------------------------------------------------------------------------------------------|
| 00 | [overview](00-overview.md)                                      | Motivation, crate map, conceptual diagram, glossary, **what's load-bearing-novel**                       |
| 01 | [tree](01-tree.md)                                              | `NodeTree`, `NodeEntry`, `NodeId` / `NodePath`, `EntryState` lazy lifecycle, `ChildKind`                |
| 02 | [node](02-node.md)                                              | `Node` trait surface, `tick`, panic isolation, contexts, what's *not* on the trait                      |
| 03 | [artifact](03-artifact.md)                                      | `Artifact` trait, `ArtifactSpec`, `ArtifactManager` state machine, hot reload, refcount                 |
| 04 | [config](04-config.md)                                          | `NodeConfig` — per-instance authored data, override map, structural-vs-overrides split, legacy bridge   |
| 05 | [slots-and-props](05-slots-and-props.md)                        | `Slot` (schema) vs `Prop<T>` (runtime), four namespaces, `PropAccess` reflection                        |
| 06 | [bindings-and-resolution](06-bindings-and-resolution.md)        | `Binding` enum (`Bus`/`Literal`/`NodeProp`), pull-based resolution, cascade `[bindings]`                |
| 07 | [sync](07-sync.md)                                              | Client/server mirror, `FrameId`, `NodeView` snapshots, delta computation                                |
| 08 | [domain](08-domain.md)                                          | `ProjectDomain` trait, legacy / visual / future-domain mapping, M2 flag resolution                      |

## M4.3a crate split update

M4.3a moved the design vocabulary onto clearer crate roles:

- `lpc-model`: shared concepts only (`NodeId`, `TreePath`,
  `PropPath`, `FrameId`, `Kind`, `WireType`, `WireValue`).
- `lpc-source`: authored/on-disk source model (`SrcArtifact`,
  `SrcBinding`, `SrcShape`, `SrcValueSpec`).
- `lpc-wire`: engine-client wire model (`WireMessage`,
  `WireTreeDelta`, `WireProjectHandle`, state serialization helpers).
- `lpc-engine`: runtime spine and shader/runtime conversion boundary.
- `lp-engine-client`: client-side engine view/cache.

Older design files may still use pre-M4.3a names such as
`lpc-runtime`, `TreeDelta`, or `ValueSpec` in explanatory text. Read
those as **`lpc-engine`**, **`lpc-wire::WireTreeDelta`**, and
**`SrcValueSpec`/`WireValue` payloads** respectively, unless the text is explicitly describing
runtime-only **`LpsValueF32`** behavior.

## Cross-references

- Strawman + decisions log: [`../notes.md`](../notes.md). Files here
  don't re-decide anything resolved there; they build on it.
- Prior-art synthesis: [`../m1-prior-art/synthesis.md`](../m1-prior-art/synthesis.md).
  Cited inline as "(prior-art §N)" where decisions trace to specific
  findings.
- M2 as-built: `lp-core/lpc-model/`, `lp-core/lpc-engine/`,
  `lp-core/lpc-source/`, `lp-core/lpc-wire/`,
  `lp-legacy/lpl-model/`, `lp-legacy/lpl-runtime/`. The current
  `NodeRuntime` trait is the kernel of `Node` (02); the current
  `ProjectRuntime` is the kernel of `ProjectRuntime<D>` (08).

## Errata

Empty at write time. As M4 / M5 surface mistakes in any of these
files, errata land here with the file, the date, and the discovery
context.
