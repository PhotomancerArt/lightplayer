---
status: carried
since: 2026-07-04
logged: 2026-08-01
area: lpa-studio-core/project save flow
related:
  - "../adr/2026-08-01-debug-slots-taxonomy.md"
  - "plan notes: ~/.photomancer/planning/lp2025/2026-07-31-1736-ephemeral-slots/notes.md (S6)"
---
# Save's zero-write notice assumes the project header dispatched it

**Shape** — `ProjectController::save_overlay`
(`lp-app/lpa-studio-core/src/app/project/project_controller.rs:2665`)
reports `"Save found no persisted edits to write"` when a commit writes
zero files. The wording is written for a caller that could only have
reached Save with something to save: the project header offers Save
**only** while `dirty.persisted > 0` (`project_header_actions`), so from
there a zero-write commit really is a surprise worth naming.

`ProjectOp::SaveOverlay` is not header-only, though. The asset editors —
`lpa-studio-web/src/app/node/asset_editor.rs:791` and
`mapping_asset_editor.rs:267` — mount their own Save button on the same
project-level op, ungated by the project's dirty state. Press Save there
with an applied-but-already-committed body and the notice announces an
anomaly that is simply "nothing to do". The condition is structural
rather than a one-line copy fix: the notice is phrased from the
*caller's* expectation, but the op is project-level and has several
callers with different expectations, and only the dispatcher knows which
reading is right.

D7 (Debug leaves save accounting) narrowed this but did not close it: a
Debug-only overlay no longer counts as pending work anywhere, so the
header correctly offers no Save at all — the asset-editor path is what
keeps the notice reachable.

**Carrying cost** — Small and recurring: a confusing notice on a benign
action, and a trap for anyone who "fixes" the wording without noticing
the header path, where the current phrasing is the correct one. It cost
one investigation during the Debug-slots plan (catalogued as S6) and
survived the P2 sweep for exactly this reason.

**Workarounds** — None needed operationally; the save itself is correct.
When touching this code, remember the two dispatch paths: the header
(gated, "no persisted edits" is meaningful) and the asset editors
(ungated, it is not).

**Incident log**
- 2026-08-01 — re-verified during the Debug-slots plan (P2/P6). Still
  reachable via the asset-editor dispatch; logged rather than fixed
  because the honest fix is per-caller notice context, not new copy.

**Exit criteria** — Either the notice's text is chosen by the dispatch
context (the op carries, or the controller knows, why Save was pressed),
or the asset editors' Save becomes gated the way the header is so the
zero-write case is unreachable and the wording stays true.
