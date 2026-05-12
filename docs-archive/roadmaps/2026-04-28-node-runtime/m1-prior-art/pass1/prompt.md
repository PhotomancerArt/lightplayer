# Pass 1 — Prior-art survey prompt

This is the **shared prompt** for all five sub-agents
surveying prior art for a node/artifact runtime spine. The
dispatching message tells you which reference codebase you
are assigned (one of Godot 4, Bevy, VCV Rack, LX Studio,
Three.js); everything else lives in this file.

## What we're researching and why

We're designing a runtime spine for **Lightplayer**, an
embedded GLSL JIT system for controlling LED installations.
Specifically we need to commit to trait shapes,
lifecycle semantics, path grammars, and asset / refcount
behaviour for a node tree. Before we lock those in, we want
to know how analogous systems handle the same design
surface — what works, what doesn't, what edge cases bite.

Your output (`answers-<ref>.md`) becomes raw material that the
*main* agent later synthesises into a single
`prior-art.md`. Your job is **factual extraction with
citations**. Judgment ("we should copy X / avoid Y") is
explicitly *not* your job — that happens in synthesis.

## About Lightplayer

Lightplayer is an embedded GLSL JIT shader execution system.
GLSL shaders are compiled to native RISC-V machine code
**on-device** (ESP32-C6) at runtime, then executed directly
from RAM. The product is a **client / server LED-show
controller** — think "TouchDesigner for LEDs" but running on
$5 microcontrollers, with the editor / authoring UI living in
a browser elsewhere talking to the server over the network.

Constraints that bite:

- **`no_std + alloc` Rust on the device.** No `libstd`, no
  thread spawning, no `tokio`. Allocation works but is
  precious (~512KB RAM, ~4MB flash on ESP32-C6).
- **Single render thread on-device.** Frame-driven, no
  preemption.
- **Client / server split.** The editor never lives in the
  same process as the runtime. State changes propagate via
  a frame-versioned change protocol.
- **Filesystem-driven content.** Authored artifacts (TOML
  files for now) are loaded from a flash filesystem; changes
  on disk reload nodes in place.
- **Recursive / composed visuals.** A `Stack` contains
  `Effect`s contains `Pattern`s; a `Pattern`'s `gradient`
  parameter can itself reference another `Pattern` (which
  becomes a child node).

## About the spine we're designing

The spine is split into "model" (data, including
serialisation / TOML schema) and "runtime" (instantiation,
lifecycle, scheduling). Key concepts:

- **Artifact** — class-like prototype loaded from disk.
  Identified by `ArtifactSpec` (a path-like string).
  Refcounted; the same artifact backs many node instances.
- **Node** — runtime instance in a tree. Has a stable `Uid`,
  a hierarchical `NodePath` (e.g. `/show/stack/effect_0`),
  and four namespaces of properties (see below).
- **Slot grammar** — every property has a `Slot` describing
  its shape, kind, constraints, default, binding, and
  presentation. One `Slot` type, four namespaces:
  - `params` — named, typed, bus-bindable.
  - `inputs` — indexed, structural composition (the children
    that *are* the node's input).
  - `outputs` — indexed, usually one (e.g. an RGBA frame).
  - `state` — named, sidecar runtime state (not authored).
- **NodeTree** — the runtime container. Owns `Uid → Node`,
  `NodePath ↔ Uid` indices, parent / children, lifecycle.
- **ArtifactManager** — loads / caches / refcounts / sheds
  artifacts. Responds to filesystem changes.

We are *not* asking you to evaluate this design. It's
context so you understand which questions matter.

## What makes an answer useful

Each question gets a focused answer that **a developer
designing the Lightplayer spine** can act on. Useful answers:

- **Cite a specific function, struct, or file location.** Not
  "the system has a way to..." but
  `Node::_notification` at `(godot:scene/main/node.cpp:L1421)`.
- **Stay narrowly factual.** Describe what's there, not
  whether it's good. Synthesis is someone else's job.
- **Acknowledge missing concepts.** "**N/A** — Three.js
  doesn't refcount; you call `dispose()` manually" is a
  *useful* answer. Don't invent a refcount system that
  isn't there.
- **Length: short.** Target one paragraph + 1–3 citations
  per question. If the reference defines 30 lifecycle
  hooks, list the top ~5 most-used and cite their
  definition site. Avoid exhaustive surveys.
- **Filter by relevance.** If the reference does something
  in a way that's irrelevant to a `no_std + alloc`
  embedded runtime (e.g. a thread-pool-backed loader, a
  Tokio task system), say it's there but don't dwell —
  one sentence + citation is enough for "they do X with
  threads, which doesn't apply to us, but the
  refcount / handle protocol works like Y." We mainly
  care about the protocol shapes, not the OS-bound
  implementation details.

## Format requirements

- **Citation form:**
  `(<ref>:<path/from/repo/root>:L<line>)`, e.g.
  `(godot:scene/main/node.cpp:L1421)`. Ranges:
  `L1421-L1450`. The `<ref>` token is one of:
  `godot`, `bevy`, `vcv`, `lx`, `threejs`.
- **Structure:** mirror the section / question numbering
  below verbatim in your answers file. Cross-comparison is
  mechanical only if every answers file has the same
  structure.
- **Output location:** the dispatching message will tell
  you the exact path
  (`docs/roadmaps/2026-04-28-node-runtime/m1-prior-art/pass1/answers-<ref>.md`).
- **Single Markdown file output.** No directories, no
  helper files, no code edits to the reference codebase
  (it's external and read-only anyway).

---

## Section 1: Node lifecycle

Hooks called by the framework across all three phases of a
node's existence: **enter** (added to tree), **live**
(per-frame / per-tick updates), and **leave** (removed from
tree, destroyed).

- **1.1** What named lifecycle hooks does this system define
  for nodes / components / modules? List the ~5 most-used
  hooks across all three phases (enter / live / leave), each
  with its purpose and a citation for the definition site.
- **1.2** When a new node is added to the running tree, in
  what order are enter-phase hooks invoked (e.g.,
  `_init` → `_enter_tree` → `_ready`)? Cite the dispatcher.
- **1.3** What hooks fire each frame / tick / step on a
  *live* node, and in what order across the tree
  (root-first, leaf-first, sibling-ordered, schedule-driven)?
  Cite the per-frame dispatcher.
- **1.4** During teardown: parent-first or child-first
  traversal? Are there explicit "about to be destroyed" vs
  "being destroyed now" hooks? Cite the recursion site and
  the hook names.
- **1.5** Are lifecycle calls eager (synchronous, during the
  triggering call) or deferred (queued, run on a later
  frame / tick / message)? Same question for teardown
  (`queue_free`-style?). Cite the deferred mechanism if any.
- **1.6** What does the **dispatcher** do when a hook panics /
  throws / returns an error — isolate the offending node,
  abort the frame, propagate up, kill the process? (How the
  *node* represents and logs the error is Section 9.)

---

## Section 2: Node identity and addressing

How nodes are identified and referenced — both for runtime
lookup and for persisted references in saved files / over
the wire. Distinct from but related to path grammar.

- **2.1** What is the **runtime** node identifier (numeric
  handle, interned name, hash, generational index)? How is
  it minted? Stable for the node's lifetime? Cite the
  definition.
- **2.2** Are runtime ids **stable across save / load** (do
  they round-trip), or are they regenerated each session?
  If they round-trip, cite the load path that handles this;
  if they don't, what plays the role of "persisted id"?
- **2.3** What's the **path syntax** for one node to address
  another (e.g., `$Player/Camera`, `audio.bus[0].level`)?
  Show 2–3 examples and cite the parser / grammar location.
- **2.4** Does the path grammar support: relative paths?
  Absolute paths? Wildcards? Indexed segments?
  Multi-element / glob / range segments?
- **2.5** What's the difference between an **id-based**
  reference (resolves to one specific node) and a
  **specifier / query-based** reference (resolves to
  "first match" or "all matches")? Does the system have
  both? When is each used?
- **2.6** How is name uniqueness enforced (collision rules)?
  Can two siblings share a name? What disambiguates them
  if so?
- **2.7** Lookup complexity for each id type the system
  has — O(1) for runtime ids? O(depth) for paths?
  O(N) for queries? Cite the lookup function for each.

---

## Section 3: Resource refcount / asset management

How the system loads, caches, refcounts, and sheds shared
"asset / resource / artifact" data.

- **3.1** What's the refcounted "asset" or "resource" type?
  Cite its definition.
- **3.2** What's the handle type that consumers hold? How
  does refcount work — automatic via Drop / destructor, or
  manual (explicit `addref` / `release`)?
- **3.3** Is loading synchronous, async, or lazy on first
  use? Cite the loader entry point.
- **3.4** How is unloading / eviction triggered —
  refcount-zero, explicit shed call, GC sweep?
- **3.5** Is hot reload supported? How does it propagate to
  consumers (event bus, handle dereference, replacement)?

---

## Section 4: Scene / patch / instance instantiation

How an authored thing on disk becomes a runtime instance in
the tree.

- **4.1** How does an authored "thing" (scene file / patch /
  preset / module) become a runtime instance? Cite the
  instantiation entry point and trace from
  "load file from disk" to "ready in tree".
- **4.2** Can the same authored thing be instantiated
  multiple times concurrently? How are the instances kept
  independent (cloning, fresh state, shared immutable
  parts)?
- **4.3** What pieces are shared between instances vs
  per-instance (e.g., shared mesh data + per-instance
  transform)? Cite a representative split.
- **4.4** Does the framework support nested instantiation
  (a scene contains another scene that contains another)?
  Any depth / recursion limits?

---

## Section 5: Change tracking and editor / wire sync

How property changes are tracked, serialised for save / sync,
and reconciled across processes. (Operational *node* state —
status enum, error states — is Section 9, not here.)
**Mostly N/A** for systems that don't have a separate editor /
client process; if so, say so once at the top of the section
and answer only what applies.

- **5.1** How does the system track property changes for
  editor or remote sync? Per-property dirty flags, central
  event bus, frame-versioned snapshots, ECS-style
  `Changed<T>` filters, something else? Cite.
- **5.2** What's sent over the wire — full state diffs,
  per-property events, full snapshots? Cite the
  serialisation point if applicable.
- **5.3** Is change tracking opt-in (some properties marked
  for sync) or automatic for all properties?
- **5.4** What's the relationship between **save format**
  (project on disk) and **sync format** (over the wire)?
  Same schema? Different?
- **5.5** How does the system handle changes that originate
  *during* render / process (e.g., a node mutating a
  sibling)?
- **5.6** Reconciliation when client and server diverge
  (network blip, conflicting edits) — last-writer-wins,
  operational transform, CRDT, explicit conflict resolution?
  Cite if applicable. **N/A** if single-process.

---

## Section 6: Property reflection

How the system enumerates and accesses properties at
runtime (used for editors, scripting, save-load).

- **6.1** How does the system enumerate a node's properties
  at runtime? Cite the reflection API.
- **6.2** Are property types statically known (codegen /
  derive macros) or dynamically queried (runtime type
  info)? Cite.
- **6.3** Can properties be set / get by string name? Cite
  the dispatch site.
- **6.4** Are property edits authority-checked (e.g.
  read-only properties, type-checked sets) and how?

---

## Section 7: Composition — dynamic children

How children are added / removed during play, and how
"a property's value can become a child node" works (if at
all). This section is about **tree structure**;
Section 8 covers **data flow** between nodes.

- **7.1** Can a node's children change at runtime
  (added / removed during normal play, not just at load)?
  Cite a representative add / remove call site.
- **7.2** Are there node types that own their children
  *structurally* (a Stack of N effects) vs types where
  children are *composed from external configuration*?
- **7.3** How is "this property's value is itself a node"
  expressed (if at all)? Specifically: a parameter that
  becomes a child node when bound to a complex artifact
  (e.g., a `gradient` parameter sourcing a `Pattern`).
- **7.4** What's the constraint model — can any node have
  any child, or are there type restrictions enforced at
  add-child time?

---

## Section 8: Inter-node dependencies and execution ordering

How the system orchestrates execution when one node consumes
another's output (e.g., effect reads pattern's output
buffer; DSP module reads upstream module's signal). This
section is about **data flow**; Section 7 covers **tree
structure**.

- **8.1** How is "node A reads node B's output" expressed —
  direct pointer, id reference, slot binding, cable
  connection between ports? Cite a representative example.
- **8.2** How does the runtime determine execution order —
  topological sort of the dataflow graph, fixed tree
  traversal, dynamic pull-on-demand, schedule-system?
  Cite.
- **8.3** **Push** or **pull** evaluation? Does B compute
  and push to A, or does A request from B at evaluation
  time? Or hybrid?
- **8.4** How are cycles detected or prevented —
  compile-time, runtime check at insert, just allowed and
  resolved with feedback / one-frame delay?
- **8.5** Is the dependency graph cached and rebuilt only
  when topology changes, or recomputed every frame?
- **8.6** What kinds of cross-tree dependencies are allowed
  (siblings, ancestors, distant cousins, anywhere via a
  bus / message system)? Cite a representative
  cross-reference.

---

## Section 9: Node state, errors, and logging

How nodes track their operational state, how errors and
warnings are categorised + represented + stored + surfaced,
and how the system handles things going wrong at load
time (missing / malformed / wrong-type artifact) or at
runtime (OOM, panic, hook failure, division by zero). Section
1.6 covers what the *dispatcher* does on a hook panic; this
section covers the *node's* observable state and the
error-storage / logging side. Schema *evolution*
(versioning, migration) is Section 10.

- **9.1** Does each node carry an explicit **operational
  state** value (e.g. an enum like `Loading`, `Ready`,
  `Disabled`, `InitError`, `Warn`, `Error`)? List the values
  and cite the type. If there's no explicit enum, what plays
  the role (a flag, a presence check, the existence of an
  error in a sidecar table)?
- **9.2** What error **categories** does the system
  distinguish — parse error, type mismatch, missing
  dependency, runtime exception, allocation failure, panic?
  Typed enum / error-trait hierarchy / stringly-represented?
  Cite the type.
- **9.3** **Load-time errors:** what happens when a
  referenced asset is missing, malformed (parse error), or
  the wrong type (expected Scene, got Material)? Does the
  node still exist (with error state, placeholder), or is it
  dropped from the tree? Cite the load-error path.
- **9.4** **Runtime errors:** what happens during normal
  play when an allocation fails (OOM), a hook panics, an
  exception escapes, an arithmetic op faults? Persistent
  error state, removal from tree, log + continue? Cite
  the runtime-error path.
- **9.5** Where are errors **stored** — attached to the
  offending node, in a global log, both? What's the entry
  format — message-only string, structured
  `{code, message, node_ref, frame, cause}`, rich with
  cause chain?
- **9.6** What's the error **logging mechanism** —
  in-memory ring buffer, file on disk, sent to client /
  editor over the wire, a generic logging facade
  (`tracing`, `slf4j`, etc.)? How is the log queried
  (filter by severity, filter by node, time-windowed)?
- **9.7** Are non-error **warnings** (deprecated asset,
  fallback in use, configuration nag) tracked separately,
  or folded into the error stream with severity levels?
  Cite the severity type if any.
- **9.8** Behaviour while a node is in error state — still
  rendered (with a placeholder?), skipped, removed from the
  tree, frozen at last good state? Can the node **recover**
  (e.g. by reloading from a fixed file)? Cite the recovery
  path if any.

---

## Section 10: Schema versioning and evolution

How the system handles schema evolution as the codebase
grows and on-disk files outlive the version that wrote
them.

- **10.1** Is a schema version embedded in saved files? At
  what granularity (per-file, per-section, per-property)?
  Cite a representative file header / parser.
- **10.2** What's the migration path from an older schema
  to the current one — handler functions, declarative
  rules, code-generated migrators? Cite a representative
  migration handler.
- **10.3** **Forward** compatibility: can older code load
  files written by newer code? (Ignore unknown fields,
  fail fast, partial load?)
- **10.4** **Backward** compatibility: can newer code load
  files written by older code without losing data?
- **10.5** How are deprecated / removed fields handled —
  silently dropped, warning logged, blocking error?
