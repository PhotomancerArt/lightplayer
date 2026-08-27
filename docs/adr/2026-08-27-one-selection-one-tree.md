# ADR: One selection, one tree

- **Status:** Accepted
- **Date:** 2026-08-27
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-08-25-0800-unified-selection-model (PR #452)
- **Supersedes:** the dive/breadcrumb presentation of
  `2026-08-05-map2d-editor-selection-tree-model.md` and the stored-dive
  clause of `2026-08-13-one-project-canvas.md` (both amended in place);
  extends `2026-08-20-walk-up-assignment-selection-model.md`'s "one
  selection" to the last remaining second store.

## Context

The project canvas carried two selection models joined by a hand-rolled
seam: the core's single `Option<UiPatchTarget>` (both views' canvas,
tree, panel, pulse) and the dived session's `MapSelection` (paths,
sibling multi-select — Mapping dive only), forked by a `fixture_mode`
boolean and bridged one way. Consequences the G1 vision session traced to
that fork: fixture multi-select impossible by type, Mapping's undived
tree rows dead, a stored dive that survived deselection (read as "empty
click doesn't deselect"), a toolbar breadcrumb carrying scope, and two
meanings of "this object is selected" in Mapping. The 2026-08-05 ADR had
already solved the model one level down; the fixture level never got it.

## Decision

**The project is the root of the same path tree.** One core-owned
selection (`UiSelection`) serves every surface:

1. **A sibling-level SET plus a derived scope.** `UiSelection { entered,
   targets }`: fixtures are siblings at the project root (multi across
   fixtures), a fixture's objects are siblings within it, wire-side
   places and modules select alone; mixing classes replaces. `entered` —
   the scope — is RECOMPUTED from the targets on every write (object
   grain implies its fixture; anything else implies root) and survives
   independently only while the targets are empty: the entered-EMPTY
   drawing state, the one state pure derivation cannot express.
   Invariants live in the type's write-helpers; surfaces never restate
   them.
2. **The dive is the scope, rendered.** No stored dive state exists;
   the Mapping view renders `entered` as the dive (session load, doc
   layers, dimmed neighbours), and only the Mapping view — grain follows
   activity, so Patching reads the same selection with no dive, and an
   object picked there arrives already-entered on view switch. The
   camera-never-moves clause of the one-project-canvas ADR is untouched.
3. **One grammar, view-parameterized pick grain.** Click selects at
   scope level — the FIXTURE in Mapping (D4), the OBJECT in Patching
   (the walk-up ADR's Q10; the loop taps physical pieces). Double-click
   descends to the clicked object; a neighbour's sprite takes single
   clicks while dived (ascend + select); an empty-canvas tap fully
   ascends (D3, the Figma rule — resolved at release so a marquee still
   bands); Esc ascends by parent at every level, ending in deselect at
   root. The clear-while-dived rung is gone with the state it named. The
   `‹ Project ▸ fixture` breadcrumb is deleted — scope reads from the
   dimmed neighbours, the Props stack, and the tree highlight.
4. **Multi-fixture transforms are one gesture, one undo step.**
   Shift-click toggles root siblings, a background band selects at
   fixture grain, dragging a selected sprite moves the whole set, and
   the shared box's corner handles scale every member uniformly about
   the opposite corner (D5; rotation untouched, per-member scale
   clamped). Writes ride `EditorMetaVerb::SetMany`: one `editor.json`
   round-trip, one byte-stack snapshot. The pulse breathes the union
   (`PatchPulseOp` carries subjects; same-output breath spans merge);
   chase stays a single-object language, and an arm requires exactly one
   end (`is_armable` refuses multi).
5. **The coordinator reconciles the doc grain (A1: lossy by design).**
   `editor_shell/selection.rs` owns the two-way bridge between core
   targets and the session's positional paths: the SEED (core → session,
   keyed on core facts and pipeline readiness, comparing object sets at
   ANY depth so a descended path of the same object stands) and the
   MIRROR (session → core, projecting selected root objects to their
   instance targets — or a resolver-derived `Range` for id-less
   documents — unless the core already names the same object set at a
   finer grain). Core carries the nearest ADDRESSABLE projection; the
   session keeps the precise truth (descent, vertex, drafts).
6. **Reconciliation rules learned at the gate.** Fit reconciliation
   treats CONTENT bounds as a measurement like the viewport — but only
   until the user's first content edit: bounds re-fit exists to settle
   async arrivals, never to rescale a view under a drag. Auto-pack slots
   are ONE workbench-owned store shared by both canvas views, and a slot
   is adopted only from settled facts (arrangement document answered,
   bounds real) — never from a guessed placeholder block.

## Consequences

- The Mapping/Patching views now differ only in furniture (toolbar
  items, patch panel, tree grain, live-color policy, transform handles) —
  the precondition for the still-open view-merge question, decided
  elsewhere.
- Any new selection-bearing surface dispatches whole `UiSelection`
  values built through the helpers; a surface writing either store
  directly and separately would reintroduce the drift this kills.
- Cross-view selection persistence is a FEATURE: entering an object in
  Patch and switching to Map arrives dived. Gate-accepted.
- The arrange scale/rotate toolbar verbs stay single-subject; the canvas
  gestures own the multi set.
- Id-less documents select at `Range` grain everywhere (tree, canvas,
  bridge) — no grain is invented that the patch format cannot store.
- The window-hotkey listeners (view-scoped, editable-target-guarded)
  carry both views' grammars; dock clicks no longer kill keys. Space and
  middle-drag pan everywhere.

## Alternatives considered

- **Keeping two stores with a smarter sync**: rejected — drift is
  structural while two surfaces can write independently; the bridge only
  works because exactly one value is authoritative per grain.
- **Making deep descent core-addressable** (D46 segments for repeat
  interiors): rejected for now — a format change serving only selection;
  the lossy projection (A1) costs nothing observable.
- **Empty-click clearing within scope first** (two-step exit): the D3
  fallback, held in reserve; the gate accepted full ascend.
- **Scope as pure derivation with no `entered` slot**: cannot express
  entering an empty document to draw its first object.
