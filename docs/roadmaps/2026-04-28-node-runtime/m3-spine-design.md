# Milestone 3: Spine design pass

## Goal

With prior art in hand (M1) and the new crate layout already in
place (M2), produce a design document that fixes the shape of
the spine before M4 / M5 implement it. The design pass exists
*specifically* to be separated from a `/plan` phase: plans are
about implementation order and details, not about high-level
shape.

This is **direct-execution, no `/plan`.** We may run additional
design passes later in the roadmap if M4 / M5 surface
discoveries that warrant them; the precedent for that lives
here.

## Suggested plan location

`docs/roadmaps/2026-04-28-node-runtime/m3-spine-design/`

No plan file. Direct output is `design.md` in the roadmap dir
(or in the milestone subdir if it grows large enough to want
sub-docs).

## Scope

**In scope:**

- **`Node` trait surface.** The new tree-aware trait that lands
  in `lpc-runtime`:
  - Identity accessors (`uid`, `path`, `parent`).
  - Slot view accessors for the four namespaces
    (`params`, `inputs`, `outputs`, `state`).
  - Lifecycle methods (`init`, `render`, `destroy`,
    `shed_optional_buffers`, `update_config`,
    `handle_fs_change`).
  - Children enumeration (read-only at the trait surface;
    mutation goes through the tree).
  - Object safety + dyn-compat with embedded targets
    (`no_std + alloc`, no async at this layer).
- **`NodeTree` container shape.** How parent / child storage
  works; `Uid → Node` and `NodePath ↔ Uid` indices; iteration
  order; lifecycle state (status enum) location.
- **`ArtifactManager` surface.** Load / cache / refcount /
  shed semantics; how it interacts with `NodeTree` (refcount
  on instantiate, decrement on destroy); error model.
- **Slot view shape.** One `Slot` type, four namespaces. How
  the namespaces are typed (newtype wrappers? generic param?
  marker trait? — design decision).
- **Lifecycle / status / frame versioning.** How the existing
  `lp-engine` machinery (status enum on container, frame
  counter, change events, fs-watch routing, panic recovery,
  shed) maps onto the new tree. What stays where (which
  pieces live on `Node` vs on the container).
- **NodePath grammar fixed.** Final answer on segment
  derivation (e.g., `effects_0.effect`), separator, escape
  rules, root denotation, root-relative vs absolute, and
  parsing error model. M3 produces the regex.
- **`PropPath` grammar fixed.** Final answer on dot vs
  bracket, indexing rules, struct field traversal, and how
  it interacts with the slot model.
- **Children-from-two-sources unified mechanism.** Structural
  children (an `Effect`'s `input`) and param-promoted
  children (a `gradient` param sourcing a `Pattern`) both
  end up as ordered `Vec<Uid>` on the parent. Design pass
  decides how that's expressed without two parallel
  pathways.
- **Sync layer surface.** What `lpc-runtime` exposes to
  `lp-server` / `lp-client`: change events, message schema,
  protocol versioning, the boundary between generic
  (`lpc-runtime`) and per-domain (legacy or future
  `lp-vis`) shape.
- **Reconcile design with M2's reality.** The strawman in
  `notes.md` was written before any types moved. M3 walks
  through what M2 actually delivered and corrects course.
- **Walk legacy node behaviours through the proposed trait
  surface.** For each of `Texture`, `Shader`, `Output`,
  `Fixture`: confirm that `init` / `render` / `destroy` /
  `shed_optional_buffers` / `update_config` / lazy demand
  rendering / panic recovery / change events all map onto
  the new shape. If any don't, the trait surface changes
  before M4 starts.

**Out of scope:**

- Implementation. Sketches and stubs are fine if they help
  pin down a decision; finished code is M4 / M5.
- Visual subsystem (`lp-vis`) design — that's the next
  roadmap. M3 only ensures the spine *can* support it
  (recursion, dynamic children, slot binding to bus).
- `lpfx` rendering abstraction surface — also next roadmap.
- TOML format design beyond what artifact loading needs;
  the existing `lpv-model` TOML examples (post-M2) are the
  spec.

## Key decisions

- **One `design.md`, sectioned.** Not split into many small
  files. The design surface is small enough to read in one
  pass; splitting fragments cross-references.
- **Strawman in `notes.md` is the starting point.** M3
  rewrites it from scratch only where M1 prior art or M2
  reality demands. Otherwise, what's already there stands.
- **No code in M3 is final.** Stubs and sketches exist to
  validate the design, not to ship. M4 may rewrite any
  signature M3 sketches.
- **If a discovery in M3 implies M2 was wrong about a type
  location, fix it now in M3.** Don't carry "M2 should have
  done X" debt into M4.

## Deliverables

- `docs/roadmaps/2026-04-28-node-runtime/design.md` covering:
  1. `Node` trait final shape (signatures + dyn safety
     guarantees).
  2. `NodeTree` container shape (storage, indices,
     iteration).
  3. `ArtifactManager` surface (load / shed / refcount).
  4. Slot view shape (the four namespaces, how they're
     typed).
  5. Lifecycle / status / frame versioning placement.
  6. NodePath + PropPath grammars (final, with regex).
  7. Children-from-two-sources unified mechanism.
  8. Sync layer surface.
  9. Mapping table: legacy node behaviour → new trait
     surface.
  10. Open questions deferred to M4 / M5 (with rationale).
- Optional: stub trait definitions in `lpc-runtime` (no
  impls) if they help readers verify the design.

## Dependencies

- M1 (prior art) — design pass references `prior-art.md`.
- M2 (crate restructure) — design pass references the crate
  layout that's actually in place.
- Blocks: M4 (artifact spine) implements against `design.md`.
  M5 (node spine + cutover) also implements against
  `design.md`.

## Execution strategy

**Option A — direct execution.** No `/plan` process, by
explicit user direction.

Justification: This milestone *is* the planning, but for
*shape*, not for implementation steps. A `/plan` phase here
would re-decide what M3 already decided. M4's `/plan` is the
right place to plan implementation. We may run additional
design passes later if M4 / M5 surface discoveries; this
precedent is the model for those.

> I will execute this milestone directly. Output is
> `design.md` in the roadmap dir, with the structure listed
> in Deliverables above.
