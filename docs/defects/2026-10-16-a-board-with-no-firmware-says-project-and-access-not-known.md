---
status: fixed
found: 2026-10-15      # how: the board card's words, round 1 (Yona: "what isn't known?"); checked on main 2026-10-16 — the earlier "names its subject" fix had not covered a blank board
fixed: 2026-10-16      # PR #1159, the board card's words round 2
area: lpa-studio-core `board_card/project_bar.rs`, `access_bar.rs` (`pending_*_bar`, and a roster board that needs firmware)
class: state-conflation
related:
  - docs/ux/README.md (P7: "Not known yet — what isn't known?")
  - docs/ux/anatomy/board-card.md
---
# A board with no firmware says "Project not known" and "Access not known"

**Symptom** — a blank chip (or one running other firmware) under Connect a
board read "Project not known" and "Access not known" on its project and
access bars, for as long as it sat there. Nothing was going to become
known: there is no LightPlayer on it to say what it plays or who may edit
it. The words promised an answer that would never come, beside the one
step that works (Install).

**Root cause** — the new-board card's project and access bars had one
answer for every pending link, "not known", whether the link was still
identifying or had settled on needing firmware. "Not said yet" and "has
nothing to say" were one state; the 2026-10-14 fix that made every bar name
its subject only changed the words, not the states.

**Fix** — the rows tell the two apart (`PendingLinkView::needs_firmware`):
while the board identifies they read "unknown" and the Hardware row says
"identifying"; once it settles on needing firmware the Project and Access
rows read "none" and the primary is Install firmware. A roster board that
needs firmware reads `Project none` too.

**Test** — `project_bar::tests::a_new_board_is_unknown_then_none`,
`access_bar::tests::a_new_boards_access_is_unknown_then_none`.
