# Pass 1 — Main-agent observations

Cross-comparison after reading the five `answers-*.md` files. The
goal here is to identify convergences (where references agree
and we should copy), divergences (where the spread is itself
informative), and gaps in the surveyed prior art (where we're
on our own). These observations feed `prior-art.md` synthesis.

## Pass-1 completion summary

All five surveys returned. File sizes:

- `answers-godot.md` — 25KB / 582 lines
- `answers-bevy.md` — 28KB / 471 lines
- `answers-vcv.md` — 21KB / 422 lines
- `answers-lx.md` — 25KB / 459 lines
- `answers-threejs.md` — 21KB / 490 lines

Citations are dense and locatable. Sections all answered (some
with N/A) per the 9-section + ~55-question structure.

## Per-section cross-comparison

### §1 — Node lifecycle

**Convergence:**

- **Eager dispatch is universal.** All five fire hooks
  synchronously during the trigger. None defer hooks themselves.
- **Deferred *destruction* is half-and-half.** Godot has
  `queue_free` (frame-end flush). Bevy has `Commands` (deferred
  command buffer flushed at stage boundary). VCV / LX / Three.js
  are eager — destruction is immediate.
- **Per-frame hook style varies but every system has one.**
  Godot `_process(delta)`. Bevy systems on a `Schedule`. VCV
  `process()` per audio sample. LX `loop(deltaMs)`. Three.js's
  is the weakest — render-time `onBeforeRender` only on
  renderable objects.

**Divergence:**

- **Hook delivery mechanism.** Notification ints + virtuals
  (Godot), component hooks + observer events (Bevy), C++ virtuals
  (VCV), explicit named methods (LX), event dispatch (Three.js).
  No clear winner; each fits the system's style.
- **Order on add.** Godot is parent-first enter, with `_ready`
  cascading bottom-up after all children entered. Bevy has no
  global "entity enter tree" — each component's hooks fire
  independently. LX is parent calls children synchronously.
  Three.js has no order — single `add()` returns immediately.
- **Order on teardown.** Godot is *child-first* in `_exit_tree`
  (reverse iteration), then *child-last (LIFO)* in destructor.
  Bevy's `LINKED_SPAWN` cascades parent → children. LX disposes
  children first by convention. VCV is flat. Three.js does
  nothing automatic.
- **Error isolation.** None of them have the lp-engine-style
  panic-recovery wrapping per-node. Godot prints + continues.
  Bevy panics propagate. VCV crashes the process. LX has an
  error queue + `pushError` (closest, but not real isolation).
  Three.js bubbles to the browser event loop.

**Lightplayer takeaway:**

- The "Godot pattern" — parent-first enter, children-ready
  before parent-ready, child-first exit — is the cleanest
  conventional shape. Worth adopting verbatim.
- Deferred destruction is a real pattern (Godot, Bevy) and
  prevents use-after-free during process. lp-engine's "shed
  on next frame" already does this; keep it.
- **Per-node panic-recovery isolation has no prior art.**
  lp-engine's existing wrapping is novel (and necessary for
  embedded — we can't crash the whole binary on a Pattern
  bug). Document this as a Lightplayer-distinctive feature.

### §2 — Node identity and addressing

**Convergence:**

- **Generational indices are dominant for runtime ids.** Godot
  ObjectID = `(validator << SLOT_BITS) | slot | ref_bit`. Bevy
  Entity = 32-bit index + 32-bit generation. They use generation
  to detect use-after-free.
- **Persistence is *separate* from runtime ids almost
  everywhere.** Godot uses NodePath for cross-references in
  saved scenes. Bevy explicitly says don't persist Entity ids.
  Three.js round-trips UUID but regenerates `id`. Only VCV
  round-trips its 53-bit random id. LX has a remap table.
- **Uniqueness rules are loose almost universally.** Godot
  doesn't enforce sibling uniqueness (path lookup gets first
  match). Bevy's `Name` doesn't enforce. LX checks parent-scoped
  uniqueness. Three.js doesn't enforce.

**Divergence:**

- **Path grammar richness.** Godot's `NodePath` has
  `..` (parent), `/` (absolute), `%Name` (owner-unique). LX
  has OSC-style 1-indexed slash paths. VCV has none (id-only).
  Bevy has reflect paths only (for component data, not entity
  trees). Three.js has none.
- **Lookup complexity per id type.** Godot offers O(1) for
  ObjectID, O(depth) for NodePath, O(1) HashMap for child-name.
  Bevy is O(1) for Entity, O(N) for Name. LX has both
  HashMap (id) and recursive descent (path). VCV is O(1) for
  id only. Three.js is O(N) for everything.

**Lightplayer takeaway:**

- **Generational indexing is worth considering for `Uid`.**
  Our `Uid(u32)` doesn't have generation. The trade-off is
  that 32 bits split into 16+16 (or 24+8) limits node count.
  Decision deferred to M3, but lean toward keeping `Uid(u32)`
  flat — embedded systems don't have the use-after-free
  frequency to justify it.
- **Path grammar should look like Godot's `NodePath`** with
  `..`, `/`, plus indexed segments (`effects[0]` or
  `effects_0` per our existing strawman). LX's 1-indexing is
  a UX choice (1-indexing in OSC) we don't need.
- **Persistence model: paths, not ids.** Like Godot. Saved
  artifacts reference children by path; runtime resolves to
  Uid. This matches what we're already doing.
- **Sibling uniqueness should be enforced** — strictly. None
  of the surveyed systems do this strongly, and the resulting
  ambiguity is a known footgun. We can do better.

### §3 — Resource refcount / asset management

**Convergence:**

- **RAII handle is the gold standard for refcounted assets.**
  Godot `Ref<T>` (auto-increment in ctor, decrement in dtor).
  Bevy `Handle<T>` (Arc-backed, drops via channel + asset
  system).
- **Hot reload is *event broadcast + handle stays valid*.**
  Godot `emit_changed()`. Bevy `AssetEvent::Modified`. Both
  preserve handles; consumers re-read content behind the
  handle, no replacement.

**Divergence:**

- **Manual disposal as alternative.** VCV and Three.js have no
  refcount; rely on manual `dispose()` or plugin lifetime.
  Three.js fires a `dispose` event; the renderer cleans up GPU
  resources. This is a *valid simpler pattern* for systems
  without sharing pressure.
- **Loading is sync (Godot default), async (Bevy default), or
  manual (VCV plugin load).**
- **Eviction strategies vary.** Godot keeps in cache until
  explicit clear. Bevy drops on refcount-zero through async
  channel.

**Lightplayer takeaway:**

- **Adopt Bevy's `Handle<T>` + `Asset<T>` design closely** —
  it's the directly-applicable Rust + refcount + hot-reload
  pattern. Specifically:
  - `ArtifactRef<T>` ≈ `Handle<T>` (Arc + generational index).
  - `ArtifactManager` ≈ `Assets<T>`.
  - Filesystem watch fires "asset modified" events; consumers
    re-read content behind the handle.
- **The async/channel-based drop in Bevy may be too much for
  embedded.** We don't have async runtime. Direct refcount
  drop (Godot-style RAII) likely fits better. **This is a
  spot-check candidate** for synthesis.
- **Lazy loading isn't critical** for embedded but a
  load-on-first-use pattern is cheap. Match Bevy's lazy.

### §4 — Scene / patch / instance instantiation

**Convergence:**

- **Disk → resolve → instantiate → tree** universally.
- **Multiple instances share immutable, copy mutable.**
  Every system distinguishes "shared resource handle" from
  "per-instance state". The exact split (e.g., shared mesh
  data + per-instance transform) is consistent.
- **Nested instantiation is supported by every tree-shaped
  system.** Godot, Bevy, LX, Three.js all allow scenes within
  scenes. VCV is the lone exception (flat by design).

**Divergence:**

- **Bevy has an explicit *resolve-dependencies-first* step**
  (`Scene::resolve()` → `ResolvedScene`) before spawning. The
  others bind dependencies during instantiation. Bevy's split
  catches missing deps before the partial-tree is realised.

**Lightplayer takeaway:**

- Standard pattern — no surprises. Our `ArtifactManager +
  NodeTree` design lines up.
- **Bevy's resolve-then-spawn split is interesting.** Worth
  considering for our error model: validate that all
  ArtifactSpec references resolve *before* starting to
  instantiate, so partial-tree-with-broken-children is harder
  to produce.

### §5 — Change tracking and editor / wire sync

**This is the most striking N/A-rich section.**

- **Godot:** N/A — single-process.
- **Bevy:** N/A — single-process. (Has `Changed<T>` filter
  internally, but no editor sync.)
- **VCV:** N/A — single-process.
- **LX:** Single-process. Has OSC for *external controllers*,
  not for internal sync.
- **Three.js:** N/A — single-process.

**None of the surveyed systems have a built-in client/server
architecture.** All assume editor and runtime are in-process.
Multi-process editors (e.g., Godot Editor + game) are
co-resident, not network-separated.

**Lightplayer takeaway (large):**

- **lp-engine's client/server architecture is a distinctive
  feature with no prior art in our survey.** Frame-versioned
  change events, fs-watch routing, server runtime separate
  from editor — we're building this. The instinct from earlier
  ("what lp-core does well, it does well — keep it") is fully
  validated.
- This means M3's design pass for sync should derive from
  lp-engine's existing implementation, not from external
  prior art. The novelty is real.
- Section 5 of `prior-art.md` should explicitly say "no
  applicable prior art; this is Lightplayer-novel territory".

### §6 — Property reflection

**Convergence:**

- **String-based access by-path is standard** in editor-driven
  systems. Godot `Object::set/get` with NodePath. Bevy
  `GetPath` trait. LX `getParameter(path)`.
- **Static + dynamic hybrid.** Godot has compile-time
  bindings (`_bind_methods`) plus dynamic overrides (`_set` /
  `_get`). Bevy has `#[derive(Reflect)]` plus `DynamicStruct`.
  LX has type-checked listenable parameters at runtime.

**Divergence:**

- VCV is indexed-only — properties addressed by integer index
  in `params[i]` arrays. Faster but no editor-friendliness.
- Three.js has no formal reflection — `setValues()` does
  key-matching, no schema enforcement.

**Lightplayer takeaway:**

- A derive-macro-driven approach (Bevy-style) is over-engineered
  at our scale, but a typed `set_by_path(path, value) -> Result`
  API for editor integration is essential.
- Our `Slot` grammar effectively *is* the property reflection
  schema — kind, constraints, presentation are all per-slot
  metadata. We're closer to LX's "parameter as object with
  metadata" than to Bevy's compile-time reflection.
- **Read-only / authority-checked edits should be explicit
  via `Constraint` on the slot**, not via `PROPERTY_USAGE_*`
  flags. Cleaner type system.

### §7 — Composition: dynamic children

**Convergence:**

- **Dynamic add/remove is universal.** All five support
  runtime structural changes.
- **Bidirectional parent ↔ children is common.** Godot
  (parent ptr + children HashMap), Bevy (`ChildOf` +
  `Children`), LX (parent ptr + arrays). VCV is flat.
  Three.js is one-way (children array, parent pointer set on
  add).

**Divergence on type constraints:**

- **None (Godot, Three.js):** any node can be any node's
  child. Validation is by convention or `get_configuration_warnings()`.
- **Auto-removal of invalid (Bevy):** self-parenting and
  missing-parent target both auto-remove the relationship.
- **Hard type check at add (LX):** `LXEffect.Container.addEffect()`
  requires `LXEffect`, `LXPatternEngine.addPattern()` requires
  `LXPattern`. Strongest enforcement.

**The "param-promoted-to-child" pattern is novel.** None of
the surveyed systems express "this property's value, when set
to a complex artifact, becomes a child node". Godot has
NodePath properties (manual lookup, not promotion). Bevy has
`Handle<T>` (asset, not entity). LX has `ObjectParameter` (a
selection from a list, not promotion). VCV models this via
*cables* (separate connection objects, not children).

**Lightplayer takeaway:**

- Adopt LX-style typed-add validation (`add_effect(LXEffect)`
  is clean). Strong type constraints at the API layer.
- **Param-promoted-to-child needs careful design in M3** —
  no off-the-shelf wisdom. Sketches for it should likely
  treat the param-resolved-to-Pattern as a *sub-tree* owned
  by the param slot, deleted when the param's value changes
  to a non-Pattern.
- Bidirectional parent ↔ children is worth doing — fast
  upward traversal matters for `NodePath` resolution.

### §8 — Inter-node dependencies and execution ordering

**Highly divergent.** Each system picks an ordering model that
fits its domain:

| System | Order determination | Push/Pull | Cycle handling |
|--------|--------------------|-----------|----------------|
| Godot | Tree + priority sort | Pull | None (user error) |
| Bevy | DAG topo sort (build time) | Pull | Detected at build |
| VCV | Parallel undefined order | Push + delayed copy | Allowed (1-sample delay) |
| LX | Fixed list iteration | Push | Tree prevents |
| Three.js | Renderer-determined | Pull | Tree prevents |

**Convergences:**

- **Tree-shaped systems don't worry about cycles.** Topology
  prevents them by construction.
- **Pull is more common than push** for systems that read
  state. Push appears in dataflow systems (VCV, LX).

**Lightplayer takeaway:**

- We're tree-shaped (a `Stack` contains `Effect`s contains
  `Pattern`s). Cycles aren't an issue in the structural
  composition.
- Bus binding (param ← bus channel ← upstream producer) is
  the only place a cycle is possible (param → modulator →
  param). LX's modulation engine doesn't detect cycles (just
  evaluates list-order); we should consider whether to do
  better.
- **Push from child to parent** matches our render model
  (Pattern outputs frame, Effect reads it). Match LX.
- **Tree traversal order: parent before children for setup,
  child before parent for render** (children produce, parent
  consumes). Codify this in M3.

### §9 — Node state, errors, and logging

**Mixed coverage.** Most systems have *some* error reporting but
none have a unified per-node operational state enum.

**Status enum:**

- **None of the references have a `Loading` / `Ready` /
  `Disabled` / `InitError` / `Error` enum on the node.**
  Closest: LX `Placeholder` pattern (a special component
  type when class is missing), LX per-component `enabled`
  flag, Godot scattered flags.

**Error categories:**

- **Typed enums:** Bevy (per-subsystem), LX
  (`InstantiationException.Type`).
- **Untyped strings:** Godot, VCV, Three.js.

**Error storage:**

- **Per-node attachment:** LX `Placeholder` preserves the
  failed JSON.
- **Global log:** Godot, VCV, LX (also has queue).
- **Logging facade only:** Bevy (tracing), Three.js (console).

**Recovery path:**

- All converge on "fix and reload". None offer automatic
  retry on file change → recreate node.

**Lightplayer takeaway (large):**

- **lp-engine's existing `NodeStatus` enum** (`Created` /
  `InitError` / `Ok` / `Warn` / `Error`) **has no prior art
  in this survey**. Like client/server, this is novel.
- **LX's `Placeholder` pattern is the right reference for
  "missing artifact"** — preserve the original JSON, don't
  silently drop, allow re-save round-trip. We should adopt
  this directly: when an `ArtifactSpec` doesn't resolve, the
  node enters `InitError` state but the slot configuration
  persists.
- **Typed error enum > stringly-represented.** Godot/VCV's
  print-and-continue is fine for logging but bad for
  programmatic recovery. Match Bevy / LX with a typed
  hierarchy.
- **Per-node attachment of errors + global log is the right
  split.** LX does both; we should too.
- Filesystem-change-triggered automatic retry is unique
  to lp-engine (none of the references do it). Keep this.

### §10 — Schema versioning and evolution

**Convergence:**

- **Format / version field at file root** is universal except
  Bevy. Godot has format-3 marker, VCV has version field, LX
  has version + class fields, Three.js has informational
  metadata.
- **"Ignore unknown fields"** is the dominant forward-compat
  strategy.

**Divergence on migration:**

- **Ad-hoc inline (Godot, VCV, LX):** if-statements in load
  paths checking version and remapping fields. VCV is the
  most ad-hoc; LX is the most disciplined with
  `addLegacyParameter()`.
- **None (Bevy, Three.js):** unknown fields fail fast or are
  silently dropped.

**Lightplayer takeaway:**

- **LX's `addLegacyParameter("oldPath", new_param)` pattern
  is the cleanest example.** Maps an old name to a new
  parameter at registration time; one-line per migration.
- We probably want **per-artifact-type version + per-type
  migration handler** — a function that takes an old TOML
  blob and returns a new TOML blob. Compose into a chain
  for multi-version migrations.
- **Forward compatibility:** ignore unknown fields, log
  warnings. Match Godot.
- **Backward compatibility:** explicit migration handlers
  per version step.
- This work is M3 / M4 territory; pass 1 has enough material.

## Cross-cutting findings (the real outputs)

### F-1: Three Lightplayer-distinctive features have no prior art

1. **Client/server architecture with frame-versioned change
   events** (§5) — none of the references separate editor
   from runtime as different processes.
2. **Per-node panic-recovery isolation** (§1.6) — every
   surveyed system either crashes, bubbles up, or
   prints+continues.
3. **Unified `NodeStatus` enum on container** (§9.1) — none
   have a node-level operational state with values like
   `InitError` / `Warn`.

These are **deliberate Lightplayer designs that are right**.
M3 should treat them as load-bearing and document why no
prior art applies.

### F-2: One Lightplayer-distinctive feature is *under-designed* — param-promoted-to-child

(§7.3) The pattern "a `gradient` parameter, when set to a
complex artifact like `Pattern`, becomes a child node" has
no off-the-shelf wisdom. Closest references:

- Godot's NodePath property + manual `get_node()` (cross-ref,
  not promotion).
- Bevy's `Handle<T>` (asset, not entity).
- LX's `ObjectParameter` (selection from a list, not
  promotion).
- VCV's cables (sibling-to-sibling routing, not
  parent-owns-child).

**M3 must design this carefully.** Likely shape: when a
slot's binding resolves to an artifact-spec, an anonymous
child node is created in a special subtree owned by the
slot. When the binding changes, the subtree is destroyed.
This needs explicit treatment in `design.md`.

### F-3: Bevy `Handle<T>` is the most directly portable design

(§3, §4) Adopt closely. The only adaptation: drop semantics.
Bevy uses an async channel to forward Drop events to the
asset system; we don't have async runtime on-device. Use
direct synchronous refcount drop (Godot-style) instead.

### F-4: LX is the closest *domain* analog; Godot is the closest *engine* analog

(All sections) LX shares vocabulary (Pattern / Effect /
Channel / Modulation), domain (LED shows), and conceptual
shape. Godot has the most complete engine machinery
(NodePath, deferred destruction, lifecycle staging,
Resource/Ref). Synthesis should treat:

- **LX as the vocabulary baseline.** When LX has a name for
  something, use it (or document why not).
- **Godot as the engine baseline.** When Godot has a
  mechanism that fits, copy it (or document why we differ).

### F-5: Tree-shaped composition + bus modulation is the right model

(§7, §8) Three of the four tree-shaped systems (Godot, LX,
Three.js) handle composition cleanly. The bus / modulation
metaphor (LX, our existing) handles cross-tree references
without straining the tree shape. VCV's flat-with-cables
is simpler but loses hierarchy; Bevy's ECS is overkill for
our scale.

### F-6: Path grammar should follow Godot's `NodePath` with LX-style segments

(§2) Godot has the most complete grammar (`..`, `/`,
`%Name`, indexed). LX is OSC-style (1-indexed slash paths).
Our grammar should be Godot-style + LX-segment-naming +
strict sibling uniqueness.

### F-7: Versioning: LX's `addLegacyParameter` pattern, plus per-type migration handlers

(§10) The cleanest migration mechanism in the survey. Adapt
to: per-type `migrate(old_toml, from_version) -> new_toml`
function. Chain through versions.

## Spot-check candidates (to verify during synthesis)

These are claims I want to spot-check by re-reading source
during synthesis (not enough to warrant a pass 2, but worth
a citation tightening):

- **LX `Placeholder` JSON round-trip.** Does it really
  preserve the full original JSON, or just the class name?
  Need to verify before claiming as a copy-this pattern.
  `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L64-L113)`.
- **Bevy `Handle<T>` Drop semantics.** How does the channel-
  based drop work exactly? Is it usable without async
  runtime? Worth eyeballing
  `(bevy:crates/bevy_asset/src/handle.rs:L96-L103)`.
- **Godot `_propagate_ready()` recursion order.** Are
  children's `_ready` fired before or after parent's? The
  surveys say "after all children entered tree", which I
  read as bottom-up. Verify.
  `(godot:scene/main/node.cpp:L332-L337)`.

## Pass 2 needed?

**No.** Pass 1 covers all 10 sections substantively. The
N/A spread itself is informative (especially §5). The three
spot-check candidates above are surgical and can happen
during synthesis without re-dispatching sub-agents.

Proceed to write `prior-art.md` at the roadmap root.
