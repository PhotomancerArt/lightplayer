---
status: fixed
found: 2026-07-28      # how: demo walk
fixed: 2026-07-28      # by PlaylistActivateOp (#158), not by this branch
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
that one. The focused child was filtered away before it could render, so
the click produced a state change with no visible consequence.

This was deliberate. `docs/adr/2026-07-26-node-card-faces.md` §"Playlist:
one live surface" specified the invariant: *"the active child's output
renders exactly once, and no other child renders at all."* The ADR was
right that a wall of stacked entry children is unusable; it was wrong to
pin the single rendered surface to **playback** state.

**Fix** — `PlaylistActivateOp` (the activate-by-click op the same ADR
already listed as missing). A non-active chip now activates its entry
through the runtime command channel; that makes it the active entry,
which renders its card. The active chip keeps the child's select action,
since re-activating it would be a no-op poke.

**A second fix was built and dropped — worth recording.** In parallel,
this branch made the rendered child follow **selection** (the shared
project-wide `NodeState.focused`), falling back to active, on the
reasoning that editing focus and playback are different axes. It worked
and was tested, but the two do not compose: activation is a pure runtime
poke that does not move focus, so with both in place, activating entry B
while entry A's child was focused would leave the card showing A — the
newly-playing entry would not come up. Rather than have activation also
move focus (coupling the two axes back together from the other
direction), the selection variant was dropped in the merge.

**Regression coverage** — `playlist_entry_click_activates_on_the_real
_server` and the strip-action assertions in
`studio_face_e2e_tests.rs`: the active chip carries a select action, every
other chip carries the activate op, and after activation the roles swap.

**Lesson** — An invariant stated over the wrong state axis reads as
correct and tests green. "Exactly one surface" was the right constraint;
"the active one" was a detail that quietly removed an editing affordance
for every non-playing entry.

The sequel is its own lesson: when one symptom admits two fixes on
different axes, they may not be additive. Selection-follows-focus and
activate-by-click each resolve the report in isolation and conflict when
combined, because only one of them can own which card is shown. Pick the
axis, then make everything follow it.

An e2e test also encoded the bug end to end and passed for two days,
because it asserted "clicking a non-active entry leaves the child list
unchanged" — written from the implementation's point of view rather than
the user's. A test that asserts a click changes nothing deserves a second
look.

**Residual** — Selecting a non-active playlist child from the **project
tree** still shows the active entry's card, since the rendered child
follows playback. Narrower than the reported defect, and open.
