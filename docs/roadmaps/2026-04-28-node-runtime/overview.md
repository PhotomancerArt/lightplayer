# node-runtime roadmap

## Motivation / rationale

Two streams of work have diverged. Both are alive; neither alone is
the right shape for what Lightplayer needs to be.

**`lp-core` (existing runtime)** ships the working
client / server architecture, frame-versioned change events,
filesystem-driven node reloading, panic recovery, and
shed-on-recompile — all currently running on ESP32-C6, in the
emulator, and through `lp-cli`. **What it lacks:** a node *tree*,
a slot grammar, an artifact / class abstraction, and any
domain-aware kinds beyond the four hardcoded
`Texture | Shader | Output | Fixture` types from before
`lp-domain` existed. It was built to make the client / server
model real, and at that it succeeds; the **node model is its
weak point**.

**`lp-domain` (new model)** ships the typed domain vocabulary —
`Slot`, `Kind`, `Constraint`, `ValueSpec`, `Binding`,
`Presentation`, the six Visual artifact types
(`Pattern` / `Effect` / `Stack` / `Transition` / `Live` /
`Playlist`) with TOML serde. **What it lacks:** any runtime
spine. `lp-domain::node::Node` is a property-access trait only
— no tree, no lifecycle, no sync. The artifact loader is a
one-shot `std`-only function. The model has *only* been
serialized; it has never driven a render.

The original plan (lp-render-mvp / lpfx M1) was to build a
parallel runtime in `lpfx`, validate the model end-to-end, and
later port `lp-engine` to it. That plan would re-implement
client / server / fs-watch / sync from scratch and treat
`lp-engine`'s hardest-won machinery as something to port "later"
— almost certainly hammering it back in shape after the fact.

This roadmap takes the opposite path: **refactor `lp-core` in
place** to absorb the new domain ideas. The client / server
architecture stays load-bearing throughout. The new spine
(node tree, artifact manager, slot views, lifecycle) lands
inside `lpc-runtime`. Legacy `Texture` / `Shader` / `Output` /
`Fixture` nodes are re-shaped into the new model — porting them,
not bridging them — and that porting is what *validates* the
spine. The next roadmap (lpfx + lp-vis) builds the visual
subsystem on top of an already-tested foundation.

## Architecture / design

### End-state crate map (after this roadmap)

```
lp-core/                          # foundation; no domain knowledge
  lpc-model/                      # NEW (was: lp-model + lp-domain foundation)
                                  # Uid, Name, NodePath, PropPath, NodePropSpec,
                                  # ArtifactSpec, ChannelName, Slot, Shape, Kind,
                                  # Constraint, ValueSpec, Binding, Presentation,
                                  # Artifact + Migration traits, NodeProperties
                                  # (renamed from lp-domain::node::Node).
  lpc-runtime/                    # NEW (was: lp-engine spine code)
                                  # Node trait (tree + lifecycle + slot views),
                                  # NodeTree, ArtifactManager,
                                  # NodeStatus + frame versioning,
                                  # change events, fs-watch routing,
                                  # panic recovery, shed,
                                  # client / server protocol surface.

lp-legacy/                        # NEW container (existing legacy nodes)
  lpl-model/                      # NEW (was: lp-model::nodes/*)
                                  # Texture / Shader / Output / Fixture configs.
  lpl-runtime/                    # NEW (was: lp-engine::nodes/*)
                                  # TextureRuntime / ShaderRuntime / OutputRuntime /
                                  # FixtureRuntime, each impl lpc-runtime::Node.

lp-vis/                           # NEW container (visual subsystem)
  lpv-model/                      # RENAMED from lp-domain after foundation moves out.
                                  # Pattern, Effect, Stack, Transition, Live,
                                  # Playlist, VisualInput, EffectRef, ParamsTable.
                                  # The next roadmap adds lpv-runtime here.

# Hosts / clients (mostly renames)
lp-server                         # consumes lpc-runtime + lpl-* impls.
lp-client / lp-engine-client      # consumes lpc-runtime protocol; generic.
lp-cli                            # consumes lp-client; unchanged in shape.

# Untouched in this roadmap
lpfx/                             # rendering abstraction (rename + split next roadmap).
lp-shader/lps-*                   # shader pipeline, untouched.
```

### Conceptual model

```
   Authored model                       Runtime model
   ─────────────────                    ────────────────────────
   Artifact                             Node
     ↓ instantiate                        ↑ owns
   ArtifactSpec ─── load ──────────►   ArtifactManager
     │                                    ↓ refcount
     │                                  Artifact (parsed TOML)
     ▼                                    │
   Slot grammar                           ▼
   { Shape, Kind,                       NodeTree
     Constraint,                          ├── Uid → Node
     Default, Bind,                       ├── NodePath ↔ Uid
     Present }                            └── parent / children
                                          │
                                          ▼ render loop
                                        ProjectEngine
                                          ├── tick() / sync()
                                          ├── fs-watch routing
                                          ├── frame versioning
                                          └── panic recovery / shed
```

### Per-node anatomy (the four namespaces)

```
Node
├── meta:    NodeMeta { schema_version, kind, title, ... }
├── params:  named slots, kind-typed, bus-bindable          (namespace 1)
├── inputs:  indexed slots, structural composition          (namespace 2)
├── outputs: indexed slots, primary output(s)               (namespace 3)
├── state:   named slots, sidecar runtime state             (namespace 4)
├── children: ordered Vec<Uid>
└── lifecycle: init / render / destroy / shed_optional_buffers /
               update_config / handle_fs_change
```

### Migration story (in this roadmap)

```
Today                      M1            M2                       M3 → M5                  M6
─────                      ──            ──                       ──────                   ──
lp-domain {foundation}     untouched     → lpc-model              spine concepts in        cleanup
lp-domain {visual types}   untouched     → lpv-model (renamed)    lpc-runtime              validation
                                                                                           summary.md
lp-model {generic}         untouched     → lpc-model              impl in lpc-runtime
lp-model {nodes/*}         untouched     → lpl-model

lp-engine {spine}          untouched     → lpc-runtime            Node trait, NodeTree,
                                                                  ArtifactManager,
                                                                  ProjectRuntime cutover.

lp-engine {nodes/*}        untouched     → lpl-runtime            legacy nodes ported
                                                                  to new Node shape.

prior art research         M1 (parallel with M2)
spine design pass          M3 (post-M1, post-M2)
```

### Three known consumers (this roadmap and beyond)

1. **`lp-server`** — already exists; uses the new spine
   transparently after M5 cutover. ESP32 + emulator + lp-cli
   stay green throughout.
2. **`lpfx + lp-vis` roadmap (next)** — builds the visual
   subsystem on the new spine. Pattern / Effect / Stack
   instances. Renames `lpfx` to the rendering abstraction;
   adds `lpfx-gpu`. Filetest harness for CPU↔GPU comparison
   lands here.
3. **`lp-engine` port (later)** — once `lp-vis` ships, the
   real Visual nodes replace the legacy `Shader` / `Texture`
   nodes in firmware / server contexts. `lpl-*` retires
   gradually.

## Alternatives considered

- **Path S — build the spine separately, migrate `lp-engine`
  later.** Rejected: the client / server architecture is
  `lp-core`'s most novel and load-bearing piece; rebuilding
  it from scratch (or porting it as an afterthought) wastes
  the existing investment. Filetest doesn't actually require
  a parallel spine — `lp-engine` was already designed to be
  runnable outside `lp-server`.
- **Bridge legacy nodes (run old + new runtimes in parallel
  for some milestones).** Rejected: validating the new shape
  is the *point* of porting legacy. A bridge intermediate
  state defers the validation and adds parallel-runtime risk.
  We port directly; the cutover is what proves the spine.
- **`lpfx` keeps doing both rendering abstraction and visual
  subsystem.** Rejected: those are different concepts with
  different consumers and different release cycles. A wgpu
  backend doesn't care that `Pattern` exists; a `Pattern`
  doesn't care which backend runs it. Splitting clears the
  conceptual confusion.
- **Module-level split inside `lp-engine` / `lp-model` instead
  of new crates.** Rejected: an embedded codebase targeting
  ESP32 + browser + host + emulator needs crate-level
  boundaries for build hygiene, dep restriction, and
  per-target feature gating. Modules force every consumer to
  compile every domain.
- **Filetest harness in this roadmap.** Rejected: filetest's
  real value is CPU↔GPU correctness/perf comparison, which
  this roadmap can't deliver because lpfx isn't split yet.
  Defer to the lpfx + lp-vis roadmap where the comparison is
  the point.
- **Keep `lp-domain` as-is (only foundation moves out, visual
  types stay under the old name).** Rejected: once foundation
  moves out, `lp-domain` *is* the visual model — calling it
  `lp-domain` is a misleading name, and leaving it as the lone
  outlier violates the per-domain `lp{x}-` prefix convention
  (D-2 / D-5). The "no `lpv-runtime` companion yet" objection
  doesn't survive contact with M2: `lpc-model` and `lpl-model`
  also exist standalone for parts of M2, and there's no
  visual *runtime* code anywhere yet to be torn between
  locations. M2 finishes the rename; the next roadmap adds
  `lpv-runtime` alongside.

## Risks

- **The cutover in M5 breaks ESP32 / emulator / lp-cli.**
  Mitigation: conformance tests preserve behaviour vs the old
  runtime *before* `ProjectRuntime` is replaced. Each piece
  of legacy machinery (frame versioning, fs-watch, status
  enum, shed, panic recovery) gets a regression test.
- **The new trait shape turns out wrong for one of legacy's
  edge cases.** Specifically: lazy demand-driven render,
  shed-before-recompile, panic-recovery wrapping. Mitigation:
  M3 design pass explicitly walks legacy node behaviours
  through the proposed trait surface; M5 design phase walks
  again with concrete impls.
- **M2 crate restructure surfaces unexpected coupling.** Some
  type that "should" move into `lpc-model` turns out to depend
  on something that should stay in `lpl-model`. Mitigation:
  M2 has explicit checkpoints (C1–C4); after each, agent
  verifies + cleans up before moving on.
- **`lpv-model`'s visual types reference foundation types that
  moved.** Mitigation: M2's checkpoints C3 + C4 handle this in
  one pass — foundation extraction (C3) and the
  `lp-domain → lpv-model` rename (C4) are sequential, so the
  visual types just `use lpc_model::{Slot, Kind, ...}` from
  their new home post-rename. No transitional shell.
- **Prior art investigation (M1) takes longer than expected.**
  Mitigation: M1 is parallel with M2 (the longest single
  chunk of work); it doesn't gate M2. M3 (design pass) does
  consume M1, but M2's mechanical move buys time.
- **Two-letter prefix convention bikeshed.** Mitigation: the
  convention is already in use (`lp-shader/lps-*`); we're
  generalising, not inventing.
- **The domain's "tree of nodes" ends up too restrictive for
  some Visual the lp-vis roadmap surfaces** (e.g., dynamic
  Mixer arity, Live's selection runtime). Mitigation: prior
  art research includes Godot's scene tree (which handles
  comparable dynamism); the trait surface is designed for
  open-ended children.
- **`Uid(u32)` runs out** — at >4B node creations, which
  doesn't happen but should be acknowledged. Mitigation:
  document the bound; if it ever bites, monotonic counter
  becomes a generational id (no API break needed).

## Scope estimate

Six milestones; M1 + M2 parallelisable, rest serial.

| #  | Milestone                                  | Strategy                  | Depends on |
|----|--------------------------------------------|---------------------------|------------|
| M1 | Prior-art investigation                    | B — small plan            | —          |
| M2 | Crate restructure (mechanical, manual)     | A — direct, w/ checkpoints | —          |
| M3 | Spine design pass                          | A — direct                | M1 + M2    |
| M4 | Artifact spine — manager + slots + loader  | C — full plan             | M3         |
| M5 | Node spine + sync cutover (legacy ported)  | C — full plan             | M4         |
| M6 | Cleanup + validation + summary             | B — small plan            | M5         |

The biggest milestone by code volume is **M2** (mechanical
relocation of effectively all the runtime types). The biggest
by *risk* is **M5** (ProjectRuntime cutover; ESP32 has to
stay green). The biggest by *design surface* is **M3 + M4 + M5
combined**.

After this roadmap: the lpfx + lp-vis roadmap (rework of
`docs/roadmaps/2026-04-23-lp-render-mvp/`) builds the visual
subsystem on the spine.
