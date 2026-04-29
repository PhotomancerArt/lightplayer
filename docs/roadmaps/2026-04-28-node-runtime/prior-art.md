# Prior-art synthesis — node-runtime roadmap

Distilled from a five-reference survey
(**Godot 4**, **Bevy**, **VCV Rack**, **LX Studio**, **Three.js**),
focused specifically on the design surface for Lightplayer's
node / artifact runtime spine. This is **judgment-laden** —
"what to copy" + "what to avoid" *for a system like ours*
(embedded, `no_std + alloc`, client / server, LED show) — but
deliberately *not* judgement against the strawman in
`notes.md`. Strawman synthesis is M3's job.

Per-reference raw material lives in `m1-prior-art/pass1/answers-*.md`;
cross-comparison in `m1-prior-art/pass1/notes.md`. This document
distils both into the form that M3 actually reads.

## Methodology

For each of 10 design surfaces we asked all 5 references the
same set of focused questions. Each section below presents:

1. A short narrative on the spread.
2. **What to copy** — concrete patterns to adopt, with
   citations.
3. **What to avoid** — anti-patterns with citations.
4. (Where applicable) **Lightplayer-distinctive** territory —
   places where prior art is silent, and the silence itself is
   informative.

Citations follow the form `(<ref>:<path>:L<line>)`.

## How M3 should consume this document

- **Headline findings** below distil the load-bearing claims.
  Read once for the whole roadmap.
- **Per-section detail** is for design-time reference. When M3
  is writing the `Node` trait, jump to §1; when writing
  `ArtifactManager`, jump to §3; etc.
- **Per-reference summary table** at the bottom is a quick
  "which reference is best for what" pointer.

## Headline findings

These are the cross-cutting patterns that emerged from the
survey. Each has a corresponding per-section detail below.

| #  | Finding | See |
|----|---------|-----|
| F-1 | **Three Lightplayer features have no prior art and should be treated as load-bearing novelty:** client / server architecture (§5), per-node panic-recovery isolation (§1), unified `NodeStatus` enum (§9.1). | §1, §5, §9 |
| F-2 | **Param-promoted-to-child has no prior art either, but is *under-designed*.** M3 must explicitly design it. | §7.3 |
| F-3 | **Bevy `Handle<T>` + `Asset<T>` is the directly-portable refcount design** — adopt closely, with one adaptation: drop semantics. Bevy's channel-based drop doesn't fit `no_std + alloc`; use Godot `Ref<T>`-style synchronous refcount instead. | §3 |
| F-4 | **LX is the closest *domain* analog; Godot is the closest *engine* analog.** Use LX as the vocabulary baseline; Godot as the engine-machinery baseline. | §1, §2, §7, §10 |
| F-5 | **Tree-shaped composition + bus modulation is the validated model.** Three of four tree-shaped systems (Godot, LX, Three.js) handle composition cleanly; LX's bus model handles cross-tree refs without straining the tree. | §7, §8 |
| F-6 | **Path grammar: Godot's `NodePath` shape with strict sibling uniqueness.** None of the references enforce sibling uniqueness; we should. | §2 |
| F-7 | **Versioning: LX's `addLegacyParameter` + per-type migration handler chained through versions.** Cleanest in the survey. | §10 |

---

## §1 — Node lifecycle

Every reference defines lifecycle hooks; the spread is in
delivery mechanism (notifications vs methods vs events) and
ordering rules. Eager dispatch is universal — none of them
defer hooks themselves. Deferred *destruction* is half-and-half:
Godot's `queue_free` and Bevy's `Commands::despawn` flush at
frame boundary; VCV / LX / Three.js destroy immediately.

### What to copy

- **Godot's enter / ready / exit ordering.** Parent enters
  first, children's `_ready` fires bottom-up, parent's
  `_ready` fires last. On exit, children leave first
  (reverse iteration). This is the cleanest conventional
  shape and well-tested.
  `(godot:scene/main/node.cpp:L341-L389)`,
  `(godot:scene/main/node.cpp:L325-L338)`,
  `(godot:scene/main/node.cpp:L410-L457)`.
  - Specifically verified: in `_propagate_ready`, children's
    `_propagate_ready()` is called *before* the parent's own
    `NOTIFICATION_READY` fires. A parent in `_ready` can
    safely assume all descendants are ready.
- **Deferred destruction at frame boundary.** Godot's
  `queue_free` queues delete onto `delete_queue`, flushed in
  `_flush_delete_queue()` at end of frame.
  `(godot:scene/main/scene_tree.cpp:L1625-L1642)`,
  `(godot:scene/main/scene_tree.cpp:L666, L735)`. Bevy
  achieves the same with `Commands::despawn` flushed at
  stage boundary. lp-engine's "shed on next frame" already
  matches this; keep it.
- **Per-frame hook with `delta` argument.** Godot's
  `_process(delta)`, LX's `loop(deltaMs)`. Standard. Pass
  delta as a milliseconds (or microseconds) integer to keep
  it `no_std + alloc`-friendly.
- **Hook style: explicit named methods on a trait.** LX's
  `onActive` / `onEnable` / `onLoop` / `onInactive` /
  `onDisable` / `dispose`
  `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L741-L749)`.
  Easier to discover and document than Godot's
  notification-int catch-all `_notification(what)` or
  Bevy's `ComponentHooks` indirection.

### What to avoid

- **Universal per-tick callback on all instances** (Bevy
  systems iterate components every schedule run; Godot's
  `_process` fires on every node with processing enabled).
  Heavy for our domain — most nodes don't need per-frame
  work. Three.js's "render hook only on renderable
  objects" is closer to right
  `(threejs:src/core/Object3D.js:L428-L440)`. Make per-frame
  opt-in.
- **Bubble-up panics** (Bevy, VCV, Three.js). On embedded a
  panic in one Pattern would kill the whole binary. Wrap
  hook calls and route panics into node-level error state.
- **Notification-int catch-all** (Godot's
  `_notification(int what)` `(godot:scene/main/node.cpp:L68-L321)`).
  Loses type safety and discoverability. Named methods are
  better.

### Lightplayer-distinctive — keep

**Per-node panic-recovery isolation.** None of the references
have it. Godot prints + continues; Bevy panics propagate; VCV
crashes the process; LX has an error queue but no per-node
isolation; Three.js bubbles to the browser event loop.
lp-engine's existing wrapping of hook calls into a
`Result`-like and storing errors in `NodeStatus` is a
*deliberate Lightplayer design* and should be preserved
through the M5 cutover.

---

## §2 — Node identity and addressing

Generational indices are the dominant pattern for runtime
ids. Persistence stories diverge: Godot uses NodePath strings
in saved scenes (round-trip via path, not id), Bevy says don't
persist Entity ids, VCV round-trips its random 53-bit id, LX
remaps on load. Path grammar richness is highly variable.

### What to copy

- **Generational indexing for runtime ids — *if* we want
  use-after-free detection.** Godot: ObjectID =
  `(validator << SLOT_BITS) | slot | ref_bit`
  `(godot:core/object/object_id.h:L41-L63)`. Bevy:
  `Entity = EntityIndex(32) + EntityGeneration(32)`
  `(bevy:crates/bevy_ecs/src/entity/mod.rs:L424-L433)`.
  - **For Lightplayer, probably not necessary.** Embedded
    targets don't have the use-after-free frequency to
    justify it. Stick with `Uid(u32)` (M3 should re-confirm).
- **`NodePath` grammar — Godot's shape.** Absolute (`/`),
  relative, parent (`..`), unique-name (`%Name`)
  `(godot:core/string/node_path.h:L38-L99)`,
  `(godot:scene/main/node.cpp:L568-L570, L3478-L3480)`.
  Excellent design, well-tested. Adopt as the basis for our
  grammar; tweak segment-naming to LX's pattern (e.g.,
  `effects[0]` or `effect_0`, both human-readable).
- **Persistence model: paths, not ids.** Godot's saved scenes
  reference children by NodePath, not ObjectID. Runtime
  resolves path → id at load. Matches our existing
  TOML-with-paths approach.
- **Specifier-based queries alongside id-based.** Godot has
  `find_child(pattern, ...)` `(godot:scene/main/node.cpp:L2704-L2714)`,
  `find_children(pattern, type, ...)`
  `(godot:scene/main/node.cpp:L2716-L2746)`. We probably
  want similar: id-based for fast runtime; specifier-based
  for editor / debugging.
- **O(1) HashMap for child-by-name lookup.** Godot:
  `(godot:scene/main/node.h:L206)`. Bevy: O(1) Entity
  lookup
  `(bevy:crates/bevy_ecs/src/entity/mod.rs:L846-L862)`. LX:
  HashMap by id
  `(lx:src/main/java/heronarts/lx/LXComponent.java:L248)`.

### What to avoid

- **Sibling-name collisions allowed.** Godot lets siblings
  share names; path lookup returns first match
  `(godot:scene/main/node.cpp:L2686-L2702)`. Bevy's `Name`
  doesn't enforce. Three.js doesn't enforce. **We should
  enforce strictly** — collision = error at add-child time.
  LX's parent-scoped uniqueness check
  `(lx:src/main/java/heronarts/lx/LXComponent.java:L534-L547)`
  is the right shape.
- **Id-only addressing** (VCV — modules referenced by
  `int64_t` only `(vcv:include/engine/Module.hpp:L40)`).
  Loses human-readability and editor friendliness.
- **No id index at all** (Three.js — `getObjectById` is O(N)
  depth-first search
  `(threejs:src/core/Object3D.js:L917-L920)`). Embedded
  targets need O(1) for sync.
- **1-indexing in OSC paths** (LX's
  `pattern/1` not `pattern/0`
  `(lx:src/main/java/heronarts/lx/LXComponent.java:L987-L1000)`).
  UX choice for OSC users; we don't have OSC, and
  0-indexing matches the rest of our stack.

### Lightplayer-distinctive — design

**Generational id is optional.** Both options have prior art.
M3 should confirm: keep `Uid(u32)` flat (cheaper, simpler,
fits embedded scale) vs adopt generational (catches
use-after-free at handle-deref time). Recommendation: keep
flat.

---

## §3 — Resource refcount / asset management

The cleanest, most directly-portable design surface in the
survey. **Bevy's `Handle<T>` + `Asset<T>` is the gold standard**
— refcounted via Arc, generational index, hot-reload via
`AssetEvent`, separation of strong (keeps alive) vs weak
(`Uuid`) handles.

### What to copy

- **`Handle<T>` enum: `Strong(Arc<StrongHandle>)` vs
  `Uuid(Uuid)`.** Strong handle keeps asset alive; weak
  handle is a stable id reference that doesn't.
  `(bevy:crates/bevy_asset/src/handle.rs:L117-L211)`,
  `(bevy:crates/bevy_asset/src/handle.rs:L132-L141)`. Adapt:
  for us, weak handle is the persisted `ArtifactSpec`
  string; strong handle is `ArtifactRef<T>` with refcount.
- **Hot reload via event broadcast + handle stays valid.**
  Asset content is replaced behind the handle; consumers
  receive an event and re-read.
  `AssetEvent::{Added, Modified, Removed}`
  `(bevy:crates/bevy_asset/src/event.rs:L47-L89)`. Godot's
  `Resource::emit_changed()` does the same shape
  `(godot:core/io/resource.h:L137)`. Don't replace handles
  on reload — only the content behind them.
- **RAII handle semantics.** Constructor increments
  refcount; destructor decrements. Godot's `Ref<T>`:
  `ref_pointer()` calls `reference()` (increment),
  destructor calls `unreference()` which `memdelete`s if
  refcount hits zero
  `(godot:core/object/ref_counted.h:L199-L213)`.
- **Refcount-zero triggers eviction.** Godot's pattern:
  zero refcount → immediate `memdelete`. **For
  Lightplayer, prefer this over Bevy's channel-based
  drop** (see "What to avoid / adapt" below). Direct
  synchronous behaviour fits single-thread embedded.

### What to avoid / adapt

- **Bevy's channel-based Drop.** `StrongHandle::drop` sends
  a `DropEvent` via crossbeam channel; the receiving side
  in `Assets::track_assets` processes drops later
  `(bevy:crates/bevy_asset/src/handle.rs:L96-L103)`. Decouples
  drop from eviction, fits Bevy's async-friendly architecture
  — but **doesn't fit our `no_std + alloc` single-thread
  model**. Use Godot's direct decrement-and-evict instead.
- **No refcount at all** (Three.js manual `dispose()`,
  fire-`disposeEvent`, hope-renderer-cleans-up
  `(threejs:src/textures/Texture.js:L647-L657)`; VCV
  manual `delete` `(vcv:src/engine/Engine.cpp:L545-L548)`).
  Works for systems where assets aren't shared at scale.
  We have shared assets (multiple Patterns of the same
  type, multiple Effects of the same type).
- **Plugin-as-asset-boundary** (VCV — Models live for the
  entire plugin's lifetime
  `(vcv:include/plugin/Plugin.hpp:L21)`). Coarse;
  doesn't allow shedding a single artifact under memory
  pressure.

---

## §4 — Scene / patch / instance instantiation

Standard pattern across all references except VCV (flat by
design): disk → resolve → instantiate → tree. Multiple
instances share immutable parts (asset references), copy
mutable parts (transform, per-instance state). No surprises.
The interesting nuance is Bevy's separate
"resolve dependencies first, then spawn" split.

### What to copy

- **Instantiation pipeline structure.** Godot's
  `ResourceLoader::load(path)` → `PackedScene` →
  `instantiate(edit_state)` → `SceneState::instantiate()` →
  recursive child build → `add_child()` →
  `_propagate_enter_tree()` → `_propagate_ready()`
  `(godot:scene/resources/packed_scene.h:L268-L273)`,
  `(godot:scene/resources/packed_scene.cpp:L168-L180, L215-L234)`.
  Our analog: `ArtifactSpec` → `ArtifactManager::load` →
  `Artifact` → `Node::instantiate(artifact, parent)` →
  enter-tree-and-ready cascade.
- **Bevy's resolve-before-spawn split.** `Scene::resolve()`
  produces a `ResolvedScene` *before* any entity is spawned;
  failures here surface before any partial-tree exists
  `(bevy:crates/bevy_scene/src/scene.rs:L50-L68)`,
  `(bevy:crates/bevy_scene/src/spawn.rs:L55-L191)`. **Worth
  adopting for our error model:** validate that all
  `ArtifactSpec` references resolve before spawning, so
  partial-tree-with-broken-children is harder to produce.
- **Shared / per-instance split via handle vs direct field.**
  Godot: `Ref<Resource>` = shared (mesh, texture, material);
  direct fields = per-instance (transform, script vars)
  `(godot:core/io/resource.h:L158-L159)`. Bevy: `Handle<T>`
  fields = shared; component fields = per-instance.
  Lightplayer: `ArtifactRef<T>` = shared; node-owned slot
  state = per-instance.
- **`local_to_scene` for forced-per-instance copies.** Godot's
  `bool local_to_scene` on `Resource`: when true, the
  resource is duplicated per scene instance instead of shared
  `(godot:core/io/resource.h:L158-L159)`. Useful for cases
  where a Pattern wants its own copy of state that's normally
  shared across instances.

### What to avoid

- **No nested instantiation** (VCV — flat by design
  `(vcv:src/engine/Engine.cpp:L210)`). Loses hierarchical
  composition; we need this for `Stack { Effect { Pattern { ... } } }`.
- **Loaders that fail-and-leave-partial-tree without
  rollback.** None of the references have a clean rollback
  story; LX's `Placeholder` (see §9) sidesteps the problem
  by making missing-class a node type rather than a load
  failure. Our story should be similar (see §9).

---

## §5 — Change tracking and editor / wire sync

**Striking N/A from every reference.** None of them have a
built-in client / server architecture. All assume the editor
and runtime are co-resident (same process for Godot's editor,
LX's UI, VCV's GUI; same browser thread for Three.js; not
relevant for Bevy game runtime). Multi-process editors (Godot
Editor running the game out-of-process) are a desktop-debugger
shape, not a network-separated client / server.

### What to copy

— *No external prior art applies.* See "Lightplayer-distinctive"
below.

### What to avoid

— *No external anti-pattern to flag.* The surveyed systems
aren't doing this poorly; they aren't doing it at all.

### Lightplayer-distinctive — load-bearing

This is the most important "no prior art" finding (F-1).

**lp-engine's existing client / server architecture is
distinctive and validated by the absence of prior art.** Frame-
versioned change events, fs-watch routing, server runtime
distinct from the editor, generational protocol versioning —
this entire stack is something we built because we needed it
and others didn't. M3's design pass for sync should derive from
lp-engine's existing implementation, not from external prior
art. The "what works in lp-core, keep it" instinct from the
M2 / M5 plan is fully validated here.

Bevy's `Changed<T>` query filter
`(bevy:crates/bevy_ecs/src/lifecycle.rs:L396-L616)` and
LX's `LXListenableParameter` listener pattern
`(lx:src/main/java/heronarts/lx/parameter/LXListenableParameter.java)`
are *primitives* we could draw on for the in-process side of
the spine, but the wire-protocol / cross-process shape is
ours to design.

---

## §6 — Property reflection

String-based access by-path is standard in editor-driven
systems. Static + dynamic hybrid is the dominant pattern
(compile-time bindings + runtime overrides). VCV is
indexed-only; Three.js is ad-hoc.

### What to copy

- **`set_by_path(path, value) -> Result` API for editor
  integration.** Godot's `Object::set(name, value)` /
  `Object::get(name)` `(godot:core/object/object.cpp:L292-L377)`,
  Bevy's `GetPath` trait
  `(bevy:crates/bevy_reflect/src/path/mod.rs:L86-L200)`,
  LX's `getParameter(path)`
  `(lx:src/main/java/heronarts/lx/LXComponent.java:L1368-L1373)`.
  All editor-driven systems have this. Type the value as
  `Variant` / `dyn Reflect` / our own `SlotValue`.
- **Indexed access for nested / aggregate properties.**
  Godot's `set_indexed(NodePath, value)`
  `(godot:core/object/object.cpp:L1693-L1697)`. Our
  `PropPath` grammar (`field.sub[0]`) maps to this.
- **Slot grammar as the reflection schema.** Our existing
  `Slot { Shape, Kind, Constraint, ValueSpec, Binding,
  Presentation }` is *already* a richer schema than any of
  the references' reflection systems, with explicit kind /
  constraint / presentation as first-class metadata. Treat
  this as a strength; don't bolt on a separate
  `#[derive(Reflect)]`-style mechanism.

### What to avoid

- **`PROPERTY_USAGE_*` flags for read-only enforcement**
  (Godot — `PROPERTY_USAGE_READ_ONLY` only prevents editor
  edits, not runtime sets
  `(godot:core/object/property_info.h)`). Use a typed
  `Constraint` on the slot instead.
- **Derive-macro overhead** (Bevy's `#[derive(Reflect)]` plus
  `TypeRegistry` machinery
  `(bevy:crates/bevy_reflect/src/reflect.rs:L101-L200)`).
  Real cost in compile time and binary size; we don't need
  it at our scale. Slot grammar is already typed.
- **Indexed-only access** (VCV — `getParam(int index)` only
  `(vcv:include/engine/Module.hpp:L272-L292)`). Loses
  editor-friendliness.
- **Ad-hoc `setValues({key: value})` with no schema** (Three.js
  `(threejs:src/materials/Material.js:L555-L597)`). No type
  enforcement, no read-only handling.

---

## §7 — Composition: dynamic children

Dynamic add / remove is universal. Bidirectional parent ↔
children is common (Bevy explicit, Godot HashMap + parent ptr,
LX parent ptr + arrays). Type constraints vary widely. The
"param-promoted-to-child" pattern has no prior art.

### What to copy

- **Typed-add validation at the API.** LX's
  `LXEffect.Container.addEffect()` requires `LXEffect`,
  `LXPatternEngine.addPattern()` requires `LXPattern`
  `(lx:src/main/java/heronarts/lx/effect/LXEffect.java:L64-L88)`,
  `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L505-L567)`.
  Strongest and cleanest type enforcement in the survey.
  Match this in Rust with separate `add_effect(Effect) ->
  Result` / `add_pattern(Pattern) -> Result` methods, not
  a generic `add_child(Node)`.
- **Bidirectional parent ↔ children.** Bevy: `ChildOf`
  component on child + `Children` on parent, kept in sync
  by relationship hooks
  `(bevy:crates/bevy_ecs/src/relationship/mod.rs:L96-L200)`.
  Godot: parent pointer + children HashMap on parent
  `(godot:scene/main/node.h:L206)`. LX: parent pointer +
  child arrays. Fast upward traversal matters for `NodePath`
  resolution.
- **Auto-removal of invalid relationships.** Bevy's
  self-parenting and missing-target auto-removal
  `(bevy:crates/bevy_ecs/src/relationship/mod.rs:L154-L199)`.
  Cleaner than throwing on invalid input.
- **Internal vs external children distinction.** Godot's
  `INTERNAL_MODE_FRONT` / `INTERNAL_MODE_BACK`
  `(godot:scene/main/node.h:L124-L128)` lets a node have
  framework-owned children (not visible in editor) plus
  user-composed children. Useful for: param-promoted
  Pattern children, where the param owns the child
  structurally but the user doesn't manipulate it directly
  in the tree view.

### What to avoid

- **No type constraints at all** (Godot `add_child(Node)`
  `(godot:scene/main/node.h:L525-L527)`, Three.js
  `add(Object3D)` `(threejs:src/core/Object3D.js:L767-L783)`).
  Footgun for our API.
- **Direct buffer passing for composition** (LX's
  `effect.setBuffer(getBuffer())`
  `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L682-L691)`).
  Couples effect to pattern's render mechanism. We have
  `Slot { Kind: TextureRgba8 }` to express this typed.

### Lightplayer-distinctive — design

**Param-promoted-to-child is novel; M3 must design it.** No
reference does "this property's value, when set to a complex
artifact, becomes a child node":

- Godot's NodePath property is cross-reference, not
  promotion `(godot:scene/main/node.cpp:L545)`.
- Bevy's `Handle<T>` is asset, not entity.
- LX's `ObjectParameter` is selection from a list, not
  promotion
  `(lx:src/main/java/heronarts/lx/parameter/ObjectParameter.java)`.
- VCV's cables are sibling-routing, not parent-owns-child.

The closest mechanism is **Godot's internal-mode children**,
which lets a node own framework-private children outside
the user-visible tree. Likely shape for our design:

- A slot whose `ValueSpec` resolves to an artifact spawns
  an *anonymous* child node in a slot-owned subtree.
- Slot rebinding (param value changes from artifact-A to
  artifact-B, or to a literal value) destroys the old
  subtree and instantiates a new one.
- Subtree is internal — visible to the renderer, not
  surfaced as a normal child to editor consumers.

This needs explicit treatment in M3's `design.md`.

---

## §8 — Inter-node dependencies and execution ordering

Highly divergent; each system picks an ordering model that
fits its domain. Tree-shaped systems (Godot, LX, Three.js)
prevent cycles by construction. Push (LX) vs pull (Godot,
Bevy, Three.js) split tracks dataflow vs state-read shapes.
VCV's parallel work-stealing is highly DSP-specific.

### What to copy

- **Tree-shape prevents cycles by construction** (LX, Three.js,
  Godot conventionally). Lightplayer is tree-shaped (`Stack
  { Effect { Pattern { ... } } }`); we don't need cycle
  detection in structural composition.
- **Push from child to parent for render data.** LX: parent
  channel calls `pattern.loop(buffer)`, pattern writes to
  buffer, parent passes buffer down to effects
  `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L952-L1078)`.
  Match this: child produces, parent consumes.
- **Pull for occasional state reads.** Godot's `get_node(path)`
  + read property pattern
  `(godot:scene/main/node.cpp:L2686-L2720)`. For "this
  Effect needs to know what's happening on a sibling
  Pattern" use cases.
- **Tree traversal: parent before children for setup,
  child before parent for render.** Godot's pattern.
  Setup: parent's `_enter_tree`, then children's
  `_enter_tree`, children's `_ready`, then parent's
  `_ready`. Render: children produce frames, parent
  composites them. Codify in M3.

### What to avoid

- **DAG topological sort + cycle detection at schedule build
  time** (Bevy
  `(bevy:crates/bevy_ecs/src/schedule/schedule.rs:L280-L500)`).
  Necessary for ECS where any system can read any
  component; overkill for our tree-shape.
- **Multi-threaded work-stealing** (VCV — spin barriers,
  hybrid barriers, FCFS module scheduling
  `(vcv:src/engine/Engine.cpp:L339-L348)`). Highly
  DSP-specific; doesn't apply to single-thread embedded.
- **One-sample delay for cycles** (VCV — feedback resolved
  by reading previous frame's output
  `(vcv:src/engine/Engine.cpp:L376-L383)`). Standard for
  DSP, alien for visuals.
- **No execution-order guarantees** (VCV — modules
  processed in undefined parallel order). We need
  deterministic order for reproducibility.

### Lightplayer-distinctive — design caveat

**Bus-binding cycles need a design decision.** Tree composition
is cycle-free, but bus-binding (param ← bus channel ←
upstream producer) could form a cycle (param's value depends
on a modulator that depends on the param). LX's modulation
engine doesn't detect cycles; it just evaluates list-order
`(lx:src/main/java/heronarts/lx/modulation/LXModulationEngine.java:L82-L85)`.

M3 should decide: detect-and-error at bind-time, or allow
and accept one-frame-delay semantics. Recommendation: detect
at bind-time. We're not a DSP system; cycles are user error.

---

## §9 — Node state, errors, and logging

Wide divergence. **No reference has a unified per-node
operational state enum** (`Loading` / `Ready` / `InitError` /
`Error`). LX's `Placeholder` pattern (verified) is the closest
analog to "missing artifact preserves data". Typed error enums
(Bevy / LX) > untyped print (Godot, VCV, Three.js).

### What to copy

- **LX's `Placeholder` pattern for missing artifacts.**
  Verified on inspection: when a class is missing on load,
  `Placeholder.load()` stashes the *full* JsonObject; on
  save, it iterates `patternObj.entrySet()` and re-emits
  every entry untouched
  `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L90-L107)`.
  The placeholder also tracks the attempted class name
  separately for error messaging
  `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L82, L105)`.
  - **Adopt this directly:** when an `ArtifactSpec` doesn't
    resolve, the node enters `InitError` state, the original
    config blob is preserved on the node, and re-save
    round-trips it untouched. Fixing the missing artifact
    on disk → reload → real Pattern replaces the
    placeholder, no data loss.
- **Typed error enum / hierarchy.** Bevy uses `thiserror`
  per subsystem (`AssetLoadError`, `InvalidEntityError`,
  `EntityNotSpawnedError`, `SpawnSceneError`, `ApplyError`)
  `(bevy:crates/bevy_ecs/src/entity/mod.rs:L1111-L1178)`,
  `(bevy:crates/bevy_scene/src/scene.rs:L157-L170)`. LX has
  `LX.InstantiationException.Type` (`EXCEPTION` / `LICENSE`
  / `PLUGIN`)
  `(lx:src/main/java/heronarts/lx/LX.java:L79-L97)`.
- **Per-node attachment + global log split.** LX does both:
  per-component `Placeholder` for load errors plus a global
  `errorQueue`
  `(lx:src/main/java/heronarts/lx/LX.java:L377)` queryable
  via `getError()` / `popError()`. Per-node = "which
  artifacts are broken right now"; global = "what failures
  have occurred recently".
- **Severity-tagged log entries.** VCV's four levels
  (`DEBUG`, `INFO`, `WARN`, `FATAL`)
  `(vcv:include/logger.hpp:L13-L16)` are minimal but
  sufficient.

### What to avoid

- **Untyped print-and-continue** (Godot's `ERR_PRINT` /
  `ERR_FAIL_COND` `(godot:core/error/error_macros.h:L38-L82)`,
  VCV's `WARN()`, Three.js `console.error`). Fine for human
  debugging; useless for programmatic recovery or for
  surfacing structured errors to the editor.
- **No node attachment** (Godot — errors are global log
  events `(godot:core/error/error_macros.h:L60-L80)`; Bevy
  — errors emitted as events, not attached to entities;
  Three.js — `console.error`). Hard for editor to display
  "this Effect is broken" inline.
- **No structured query interface** (Godot — text search
  only; VCV — file log only `(vcv:include/logger.hpp:L24-L41)`).
  Editor needs filter-by-severity, filter-by-node, etc.

### Lightplayer-distinctive — load-bearing

**`NodeStatus` enum on container is novel.** lp-engine's
existing `Created` / `InitError` / `Ok` / `Warn` / `Error`
states have no prior art. Together with frame-versioned
change events (§5), this gives editors a typed view of
runtime health that none of the surveyed systems offer. M3
should treat this as a load-bearing Lightplayer feature.

**Filesystem-change-triggered automatic retry** (lp-engine's
`handle_fs_changes` recreating broken nodes when their
artifact reappears) is also unique. Keep.

---

## §10 — Schema versioning and evolution

Format / version field at file root is universal except Bevy.
"Ignore unknown fields" is the dominant forward-compat
strategy. Migration approaches range from ad-hoc inline (VCV,
Godot) to declarative-but-manual (LX) to nothing (Bevy,
Three.js).

### What to copy

- **LX's `addLegacyParameter("oldPath", new_param)`.**
  Cleanest pattern in the survey. At parameter registration
  time, declare which old paths map to the new parameter;
  load path checks legacy mappings before the primary map
  `(lx:src/main/java/heronarts/lx/LXComponent.java:L1247-L1253, L1484-L1496)`.
  - **Adapt to per-type migration handler chained through
    versions:** for each artifact type, declare a
    `migrate(old_toml: Toml, from_version: u32) -> Toml`
    function that returns the toml in the next version's
    shape. Chain through versions to migrate from any old
    version to current.
- **Version + class fields at component level.** LX writes
  per-component `id`, `class`, plus a top-level project
  `version`
  `(lx:src/main/java/heronarts/lx/LXComponent.java:L1466-L1467)`,
  `(lx:src/main/java/heronarts/lx/LX.java:L983)`. Granular
  enough for per-type migrations.
- **Forward compatibility: ignore unknown fields, log
  warnings.** Godot's pattern
  `(godot:scene/resources/packed_scene.cpp)`. JSON / TOML
  parsers naturally ignore unknown fields; we just need to
  emit a warning when this happens.

### What to avoid

- **Ad-hoc inline migrations scattered through load paths.**
  VCV is the worst offender — handlers for `"wires"` →
  `"cables"`, pre-1.0 IDs, `"disabled"` → `"bypass"` are
  scattered across multiple files
  `(vcv:src/engine/Engine.cpp:L1309-L1311, L1332-L1333)`,
  `(vcv:src/engine/Module.cpp:L186-L188, L228-L240)`. Hard
  to audit, hard to remove.
- **No migration system** (Bevy: explicit `#[reflect(ignore)]`
  is too lossy
  `(bevy:crates/bevy_scene/src/scene.rs)`; Three.js:
  metadata version is informational only
  `(threejs:src/core/Object3D.js:L1287-L1291)`). Fine for
  game engines that ship as static binaries; not fine for
  long-lived authored projects.
- **`_bind_compatibility_methods`-style API rename
  shims** (Godot
  `(godot:core/object/object.h:L448-L452)`). Method
  renames aren't our problem; field renames in TOML are.

---

## Per-reference summary table

When designing a specific surface, jump to the reference
that does it best.

| Surface | Best reference | Why |
|---------|---------------|-----|
| Lifecycle hooks (enter / ready / exit ordering) | **Godot** | Cleanest conventional shape; bottom-up `_ready`. |
| Path grammar | **Godot** | Richest (`..`, `/`, `%Name`); well-tested. |
| Generational id (if needed) | **Godot** or **Bevy** | Equivalent designs. |
| Refcount + asset management | **Bevy** | Directly portable; `Handle<T>` + `Asset<T>` is the gold standard. |
| Refcount drop semantics | **Godot** (`Ref<T>` direct sync) | Bevy's channel-based drop doesn't fit `no_std + alloc`. |
| Hot reload propagation | **Godot** or **Bevy** | Both: event broadcast + handle stays valid. |
| Resolve-deps-then-spawn split | **Bevy** | Good error model precedent. |
| Typed-add child API | **LX** | `addEffect(LXEffect)`, `addPattern(LXPattern)`. |
| Vocabulary (Pattern / Effect / Stack / Channel) | **LX** | Closest domain analog; we already share it. |
| Bus / modulation routing | **LX** | `LXModulationEngine` is the closest analog to our bus. |
| Internal-vs-external children | **Godot** | `INTERNAL_MODE_FRONT` / `INTERNAL_MODE_BACK`. |
| Property reflection (set-by-path) | **Godot** | `Object::set_indexed(NodePath, value)`. |
| Slot / parameter metadata | (already best-in-class) | Our slot grammar exceeds all references. |
| Placeholder for missing artifact | **LX** | Verified full-JSON round-trip. |
| Typed error enum | **Bevy** (per-subsystem) or **LX** (`InstantiationException.Type`) | Either works; no clear winner. |
| Per-node + global error split | **LX** | `Placeholder` + `errorQueue`. |
| Schema migration | **LX** | `addLegacyParameter` is the cleanest declarative shape. |
| Client / server architecture | (none — Lightplayer territory) | F-1; lp-engine is the reference. |
| Per-node panic isolation | (none — Lightplayer territory) | F-1; lp-engine wraps. |
| Unified `NodeStatus` | (none — Lightplayer territory) | F-1; lp-engine has it. |
| Param-promoted-to-child | (none — design from scratch) | F-2; M3 designs. |

## Closing note for M3

Three load-bearing Lightplayer-distinctive designs (F-1) and
one under-designed Lightplayer-distinctive design (F-2) come
out of this survey. The remaining 7 design surfaces have
strong prior art that we should track closely.

The strawman in
`docs/roadmaps/2026-04-28-node-runtime/notes.md` is *broadly*
consistent with the prior art on the 7 well-supported
surfaces. The novel surfaces (F-1, F-2) need explicit
treatment in `design.md`:

- Document why no prior art applies (so future-us doesn't
  retro-search).
- Be deliberate about the design choices — these are the
  riskiest parts of the spine.
- Walk lp-engine's existing implementation through the new
  trait surface for the F-1 features (preservation is the
  goal).
- Sketch the param-promoted-to-child mechanism in M3 itself,
  not deferred to M4.
