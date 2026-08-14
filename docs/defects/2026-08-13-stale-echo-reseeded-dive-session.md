---
status: fixed
found: 2026-08-13      # how: G1 review gate (one-project-canvas P4, Yona)
fixed: b61fdaf5c
area: lpa-studio-web editor_shell mapping_session (the dive's asset pipeline)
class: newest-only-inflight-memory
related:
  - ../adr/2026-08-13-one-project-canvas.md
---
# Stale apply echo re-seeded the dive session; undo looped between two states

**Symptom** — While editing a dived fixture's mapping, a drag would
"sometimes just revert after letting go", and from then on ⌘Z "cycled
between the last two states forever" instead of walking history back
(G1 review, 2026-08-13).

**Root cause** — The dive's asset pipeline suppresses the snapshot echo
of its own `ApplyBody` writes by remembering the applied JSON and
skipping content that matches. That memory was a **single slot**: when a
second commit dispatched before the first apply's echo settled, the slot
held apply 2's bytes, so apply 1's echo failed the text match, parsed to
a document differing from the mount-time seeded copy, and was treated as
an external change — `set_doc` re-seeded the session back to the FIRST
commit's state (the visible revert) and wiped the undo stack. Each
subsequent undo committed, echoed, and re-seeded again: the two-state
loop.

The hazard existed in the old per-callback apply path too, but the
one-canvas rework routed ALL commits (canvas gestures, editor keys,
Props-pane edits) through one render-cycle-deferred bump counter, which
widened the race window from "hard to hit" to "every quick pair of
drags" — a latent defect unmasked by a timing change, not introduced by
a logic change.

**Fix** (`b61fdaf5c`) — The applied-bytes memory is a bounded **queue**
of in-flight applies. An echo matching ANY entry is ours: entries before
the match are superseded applies whose echoes the store skipped, and the
matched entry is KEPT as the settled marker (the settled content
re-renders many times, and each render must keep reading as our own
echo — the old single slot was persistent for the same reason).

**Lesson** — Echo suppression around an async write pipeline must
remember every write still in flight, not just the newest; and a
"harmless" refactor that adds a scheduling hop (callback → bump counter)
can turn a theoretical race into a routine one. When moving a write path
onto a deferred queue, re-derive the suppression logic's assumptions
instead of porting it verbatim.
