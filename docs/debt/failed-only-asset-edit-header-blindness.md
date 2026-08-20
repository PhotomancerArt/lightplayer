---
status: carried
since: 2026-08-01
logged: 2026-08-19
area: lpa-studio-web session·project control + lpa-studio-core project header actions
related:
  - "../adr/2026-08-19-single-session-web-and-session-control.md"
  - "plan notes: ~/.photomancer/planning/lp2025/2026-08-18-1858-session-project-control/notes.md (G1 fix round)"
---
# A failed-only asset edit shows in the editor but not in the header

**Shape** — `project_header_actions`
(`lp-app/lpa-studio-core/src/app/project/project_controller.rs:8377`)
offers Save/Revert only while `dirty.persisted > 0`:

```rust
fn project_header_actions(dirty: &DirtySummary) -> Vec<UiPaneAction> {
    let mut actions = Vec::new();
    if dirty.persisted > 0 {
        actions.push(...); // Save
        actions.push(...); // Revert
    }
    actions
}
```

A rejected or oversize asset-body edit (a GLSL/SVG save the server refuses)
lands as `dirty.failed > 0` with `dirty.persisted == 0` — persisted and
failed are independent buckets (`DirtySummary { persisted, failed }`), so
"failed-only" is a real, reachable state, not a hypothetical one. In that
state:

- The asset editor's own status bar (the CodeMirror wrapper's persistence
  half) reads "Unsaved" — it has local, honest evidence the body did not
  save.
- The header session·project control's project segment paints its ERROR
  tint (`UiAffordance::from_dirty`: `failed > 0` maps to `Error`
  regardless of `persisted`) — so the chrome visibly agrees something is
  wrong.
- But the control's amber COUNT pill never appears
  (`ProjectDetailContent::unsaved_count()` reads `dirty.persisted` only —
  `session_control.rs`'s project segment gates the pill on
  `project.unsaved_count() > 0`), and neither does Save/Revert
  (`save_and_revert` picks them out of `header_actions()`, which is empty
  here by construction).

So the header names a problem (red wash) without naming its size or
offering an action, and — since the P4 retirement of the workbench Tree
panel's Save/Revert row — the panel's own per-entry Revert (opened from
the control) is the only exit left. Before that retirement the same gap
existed and was equally silent (the old header chip used the identical
`project_header_actions` gate), but it was one blind surface among two;
now the header control is the ONE save-moment home the plan
(`2026-08-19-single-session-web-and-session-control.md`) built around, so
the gap is load-bearing in a way it was not before.

**Carrying cost** — Low-frequency (an asset save has to actually fail —
oversize body or a server-side rejection) but confusing when it happens:
the chrome says "error", not "how many" or "what do I do", and the fix is
one click into a panel the header gives no numeric reason to open. Found
during the G1 gate for the session·project control
(2026-08-19, "Q4 defect" in the plan's notes.md) while chasing what looked
like a GLSL-dirty-projection bug; that hypothesis turned out FALSE (core
already projects asset edits into `pending_edits`/`DirtySummary` and the
repro did not reproduce on a clean bundle) — this failed-only gap is what
was actually left standing once the false lead was ruled out.

**Workarounds** — Open the header control's panel; the "Failed edits"
section (`ProjectDetailSections`, gated on `dirty.failed > 0 ||
!failed_entries.is_empty()`) lists the failed entry with its reason and a
per-entry revert regardless of the header's blindness. That is the one
reliable path today.

**Incident log**
- 2026-08-19 — surfaced at the G1 visual gate for the single-session
  session·project control; investigated as a suspected dirty-projection
  bug (F3 in the plan's fix round), ruled out, and re-filed here as the
  real, pre-existing condition. Left open — see Exit criteria.

**Exit criteria** — A product call (asked at the G1 re-gate, not yet
ruled): either `project_header_actions`/`unsaved_count()` grow a
failed-aware branch so a failed-only project shows *some* count and *some*
action in the header (even if that action is just "open the panel"), or
the header's silence-on-failed-only is ruled intentional and this entry
retires with that ruling cited. Until then, the failed section inside the
panel stays the one place the count and the per-entry revert are honest.
