# ADR: map2d editor selection — tree paths, one scope, no co-selection

- **Status:** Accepted
- **Date:** 2026-08-05
- **Deciders:** Photomancer
- **Related:** `2026-08-05-map2d-format-2-repeat-and-gaps.md` (the construct
  that made the document a tree), `2026-07-04-studio-editing-model.md`
  (the editing-model lineage this extends)
- **Supersedes:** None
- **Superseded by:** None (model intact; **amended 2026-08-27**: the
  model is LIFTED to project scope by
  `2026-08-27-one-selection-one-tree.md` — the project is the tree's
  root, fixtures are root siblings, scope derivation now includes the
  fixture level, and the popover/breadcrumb presentation of decision 4
  is fully gone: scope reads from the dimmed neighbours, the Props
  stack, and the tree)

## Context

`Map2dShape::Repeat` made the mapping document a tree: a root list of
objects, each shape possibly nesting a structural child (arity 0 or 1
today). The editor's selection was still flat — `BTreeSet<usize>` of
object indices — which cannot even *name* "the inner sector of the
repeat", let alone answer what it means when a group and something
inside it are both of interest. The first group construct is exactly
when this abstraction gets set, deliberately ("usually it's worth
getting abstractions right around these things" — the product call that
opened this design).

The driving workflow is **live tessellation authoring** (the dome G2
ask): author ONE sector while seeing all N instances update live, each
visually distinct, so missing runs and overlaps are obvious. Explicitly
NOT wanted: per-instance overrides in the document.

## Decision

1. **Selection is a set of tree paths** (`ShapePath { object,
   descent }`). Documents carry no node ids, so paths are positional;
   they can dangle after structural edits, and consumers resolve-and-drop
   rather than panic.
2. **An ancestor of a selected path is *context* — the scope — never a
   co-selection.** Inserting a path evicts selected ancestors and
   descendants. The scope is **derived** (the shared parent of the
   selected paths); there is no separate scope state to drift.
3. **Multi-select is sibling-level**: all selected paths share one
   parent. Selecting across parents replaces the selection.
4. **Prior-art interaction grammar** (Figma/Illustrator lineage): click
   selects at the top level; **double-click descends** into a group;
   **Esc ascends** (after the existing vertex/draft backout steps);
   the popover breadcrumbs the scope (`sector ▸ path`).

   > **Amended 2026-08-14: the breadcrumb presentation is superseded by
   > the Props STACK** (B′, design record `spikes/props-stack/index.html`,
   > ratified with the workbench Props dock). The pane now shows one
   > editable card per level of the selected path, deepest first — the
   > selection always the top card, ancestors unwinding beneath, the
   > fixture's placement card (shell composition) at the bottom, the
   > module chain as a context strip. The selection MODEL is untouched:
   > paths, derived scope, no co-selection, and the esc ladder all hold —
   > esc popping the selection now reads as popping the top card. The
   > repeat card's descend button is gone with the breadcrumb; descent
   > lives on the tree (full depth since the tree-depth pass) and canvas
   > double-click. Clicking an ancestor card's header ascends exactly as
   > the breadcrumb click did.
5. **Only the primary is interactive.** Inside a scoped repeat, the
   authored sub-object (rendered at instance 0) takes hits, handles, and
   edits; instances 1..N-1 are **inert, live-updating previews** — they
   are never click-through handles to the group.
6. **Instance coloring is a model rule, not a feature**: within a
   selected or scoped repeat, instances color by span index so the
   tessellation reads at a glance.
7. **The wiring rail is the tree.** No separate layers panel: in this
   format, wiring order and structure are the same fact, and the rail
   already owns wiring order. Repeat rows disclose their child with an
   instance-count badge.
8. **Edits through a descended path write through to the authored
   shape** — the resolver mirrors them to every instance by
   construction. Delete-at-depth deletes the whole object (a repeat
   cannot exist empty; unwrap is the keep-the-inner op).

## Consequences

- `editor_core` carries the model (`shape_path.rs`,
  `map_selection.rs`) pure and host-tested; view surfaces consume it
  and cannot restate the invariants.
- Every path consumer handles arbitrary child arity even though today's
  arity is ≤1 — which makes a future format `Group(Vec<...>)` **purely
  additive**: format 3, loud refusal on old builds (the
  format-2 machinery), no editor rework. That deferral is deliberate:
  no driving use case yet, and the format posture makes late addition
  cheap. (Deferred-decisions row added; the "editing individual
  instances of a live repeat" row from the format-2 ADR closes here —
  answered as write-through + inert instances, not overrides.)
- Selection remap on structural edits moves from index arithmetic to
  path validity (`retain_valid`) — dangles drop instead of silently
  retargeting.

## Alternatives considered

- **Co-selection (group AND child selected together):** ambiguous
  keystroke targets, no prior-art precedent, and every op needs a
  precedence rule. Rejected — context vs selection is the distinction
  that keeps one answer to "what do my keys hit".
- **Separate scope state** (an "entered group" alongside selection):
  drifts from selection on every structural edit and needs its own
  invalidation. Rejected — deriving scope from the selection's shared
  parent makes drift unrepresentable.
- **Per-instance overrides in the document** (delta transforms or
  replacement shapes per instance): rejected by the product call — the
  point of the group is authoring the ONE shape live; overrides would
  also demand format work with no driving need.
- **A separate layers panel:** duplicates the rail's ordering truth and
  invites divergence. Rejected.
- **Node ids in the document** (stable selection across edits): a
  format change serving only editor state; positional paths + drop-on-
  dangle is enough at authoring scale. Rejected for now.
