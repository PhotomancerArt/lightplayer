---
status: fixed
found: 2026-07-28      # how: demo walk
fixed: 2026-07-28
area: lpa-studio-core/project (node face derivation)
class: state-conflation
related:
  - "../adr/2026-07-26-node-card-faces.md"
---
# Only the active playlist entry could be edited

**Symptom** — Clicking an entry chip in a playlist card did nothing
visible. Whichever entry was playing was the only one whose card could
be opened; every other entry was inert. Reported from a demo walk as
"you can only edit the first item (or maybe the active one)".

**Root cause** — Not a broken click. The chip's action fired correctly
and focus *was* set on the clicked entry's child — but the face
derivation then removed that child from the render.
`node_face_builder::playlist_face` computed the index of the **active**
entry's child, and `kind_face` truncated the children list to exactly
that one (`node_controller.rs`, the `PlaylistDef::KIND` arm). The
focused child was filtered away before it could render, so the click
produced a state change with no visible consequence.

This was deliberate. `docs/adr/2026-07-26-node-card-faces.md` §"Playlist:
one live surface" specified the invariant: *"the active child's output
renders exactly once, and no other child renders at all."* The ADR was
right that a wall of stacked entry children is unusable; it was wrong to
pin the single rendered surface to **playback** state.

**Fix** — Separate the two axes. The rendered child now follows the
Studio's **selection** (the shared project-wide `NodeState.focused`,
already projected onto `UiNodeChild.focused`), falling back to the
engine's **active entry** when nothing inside that playlist is selected.
`UiPlaylistFace` gained `selected` alongside `active`, and the strip
marks them differently — neutral `selection-border` for selection,
live-blue ACTIVE placard for playback — because they can now name
different entries.

Exactly one child still renders, so the ADR's actual intent survives.
The load-time default needed no work: `default_focus_node_mut` only ever
focuses one of the root's direct children, so it can never land inside a
playlist, and a freshly opened project shows the active entry.

**Regression coverage** — Four unit tests in `node_face_builder`
(nothing selected → active; non-active selected → that child while
`active` is unchanged; selected == active; focus outside the playlist →
falls back), plus the e2e
`playlist_face_derives_and_keeps_one_live_surface`, whose final block
previously asserted the *defect* — that clicking a non-active entry left
the child list showing the active one — and now asserts the fix.

**Lesson** — An invariant stated over the wrong state axis reads as
correct and tests green. "Exactly one surface" was the right constraint;
"the active one" was a detail that quietly removed an editing affordance
for every non-playing entry. When a single-surface rule is written, name
the axis it selects on and ask whether the *user's* axis is the same one
the *engine* uses — here, editing focus and playback are independent, and
conflating them cost every entry but one its editability.

An e2e test encoded the bug end to end and still passed for two days,
because it was written from the implementation's point of view rather
than the user's. A test that asserts "clicking X leaves the view
unchanged" deserves a second look — that is a defect's shape as often as
it is a contract's.
