---
status: open
found: 2026-09-11      # hardware-walk (server-less): just walk-no-board --tab, C6-in-tab plan
class: unexplained-transient-stall
area: scripts/emu/walk-no-board.mjs
related: [lp2025/2026-09-10-1707-c6-emulator-in-tab/G1-handoff.md, lp2025/2026-09-10-1707-c6-emulator-in-tab/G2-handoff.md]
---
# The tab walk's upload step went quiet for ~30s in one of three runs, heartbeats alive throughout

**Symptom** — `just walk-no-board --tab`'s upload step (push a project onto
the tab-hosted board) reported `WentQuiet` for about 30 seconds mid-push in
one run out of three, while the board's own heartbeats kept arriving —
i.e. the machine was not wedged, not rebooted, and the worker was answering
control lines; specifically the push conversation itself stalled.

**Root cause** — not diagnosed. This entry exists so the failure is on
record before anyone decides when to chase it, per the registry's
found-not-yet-fixed convention.

**What is already known, so a repeat attempt does not redo it:**

- The two runs that did not flake completed the same push in the same walk
  script, on the same branch, with nothing else different that the walk's
  own logging distinguishes.
- Heartbeats being alive during the stall rules out the whole-machine
  freezes this plan's other defects produce (a reset replaying the board's
  lifetime freezes the *entire* worker, heartbeats included — see
  [2026-09-11-a-reset-replays-the-boards-lifetime.md](2026-09-11-a-reset-replays-the-boards-lifetime.md)).
  This is narrower: something specific to the push conversation stalled
  while the rest of the link kept working.
- It has not been correlated with board age, dilation, or any other number
  the walk records — that correlation is the natural next diagnostic step,
  given how directly board age explains the sibling defect above.

**Fix** — none; not attempted in this phase (docs/ADR/cleanup only, and the
mechanism is unknown).

**Regression coverage** — none; there is no reproduction recipe yet, only an
observed rate (1 run in 3).

**Lesson** — none yet; a one-line lesson would be premature before a
mechanism is named. Reclassify this entry (and give it a more specific
`class`) the day one is found.
