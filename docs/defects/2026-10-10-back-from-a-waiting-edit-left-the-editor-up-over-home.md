---
status: fixed
found: 2026-10-10      # how: e2e (walk-no-board, both backings, emulated, lp-emu:esp32c6:t1)
fixed: 2a8c955fa
area: lpa-studio-core studio_actor (`CommandPlan`) × `ConnectedBoard::note_page_moved`
class: edge-rule-over-latest-wins
related:
  - lp2025/2026-10-08-2330-connected-in-the-card (P4's waiting Edit, P7's `card-back`)
  - docs/adr/2026-10-08-the-board-card-and-one-home-page.md
---
# Back from a waiting Edit left the editor up over the home page

**Symptom** — `just walk-no-board --serve-release` (and, on the director's
run, `--tab`) failed at `card-back`: "waiting for the card's panel, back at
`/`: … wait deadline". The page at `/` drew only the examples — no tab
strip, no boards, no projects — which is `HomePage` with no home view. The
session was kept (no `CloseDeviceLens`, no new `pool install`); core simply
never published a home view again.

**Root cause** — Edit on a watched card connects first and records the
session with `editor_waiting`; that wait ends at the first place report that
moves the user to another page (`ConnectedBoard::note_page_moved`, the
director's P2–P3 ruling). The actor folded each batch's place reports to the
last one (`Place(reported) => place = Some(reported)`). Edit is a long action
(3.4 s on the emulated board, plus the tick's project read after it), and the
web's two reports queued behind it — the editor's page (`/p/…`, the lens
sync's move) and Home (Back) — landed in one batch. Only Home was applied,
and Home equals the place before Edit, so `set_place` saw no change,
`note_page_moved` never ran, and the editor stayed wanted over `/` for good.
The lens sync did not move the address again: the editor had already
appeared once.

**Fix** — the actor keeps every place report in a batch, in order
(`CommandPlan::places`), and applies each. A move and a move back are both
seen; nothing else reads the place differently.

**Regression coverage** —
`studio_actor::tests::every_place_report_in_a_batch_is_kept_in_order`, and
`connected_tests::a_move_and_back_behind_a_long_edit_still_ends_the_wait`
(which also pins the hazard: Home reported over Home is no move). The walk's
`card-back` is the live check, and it is a race: a green lane is evidence the
road works, not that the race was hit.

**Lesson** — a rule that fires on an edge cannot read a stream something
upstream folds to its latest value. Latest-wins was right for what place fed
before (the agent's readout, ⌘K's ranking, both reading state); it became
wrong the day a rule started reading moves. When a new reader of a coalesced
signal reads transitions, the coalescing has to change with it.
