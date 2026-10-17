---
status: fixed
found: 2026-10-15      # how: the board card's words, round 1 (finding 18); drawn again in round 2's spike (S5)
fixed: 2026-10-16      # PR #1159, the board card's words round 2
area: lpa-studio-core `board_card/board_picture.rs` × the saved picture (`DeviceCardFeedView`, `FeedLiveness::Offline`)
class: state-conflation
related:
  - docs/ux/anatomy/board-card.md (Corner: "no picture — running nothing")
  - docs/defects/2026-10-14-a-freshly-installed-board-is-told-its-files-need-restoring.md
---
# A fresh install shows its previous life's picture with a "fine" dot

**Symptom** — a board Studio had just installed firmware on, running
nothing, still drew the lights of the project it ran before the flash
(dimmed, "4 d ago" in the corner) beside a blue "fine" dot. The card said
"No project" in its project bar and showed a project's picture above it:
two answers to "what is this board doing?" on one card.

**Root cause** — the picture was chosen from the feed alone. The last frame
the browser saved for the board survives a flash (it is keyed by the
board, not by what it runs), so a board that now runs nothing kept the
picture of what it used to run. "The last picture of this board" and "a
picture of what this board runs" were read as one fact; the corner's dot,
meanwhile, said only that the link was up.

**Fix** — a board that runs nothing has no picture: it said it runs no
project, or there is no LightPlayer on it (`runs_nothing`), so the saved
frame is not drawn and the corner's details say "No project — no picture
until one runs." The corner no longer draws a "fine" dot at all (round 2:
quiet when good). A board held by another tab keeps that tab's picture.

**Test** — `board_picture::tests::a_board_that_runs_nothing_has_no_picture`.
