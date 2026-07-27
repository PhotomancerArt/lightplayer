# P6 — Cleanup, docs, ADR, validation

Size: sm. Depends on: P1–P5 complete.

## Scope

Close out the plan: ADR, doc updates, cleanup sweep, full validation.

## Work

1. **ADR** (`docs/adr/2026-07-27-completion-based-refresh-pacing.md` or
   dated when written): completion-based refresh pacing + runtime-tiered
   probe policy (resolution tier 32×32 sim / 16×16 device; node scope
   all-non-collapsed sim / focused device). Context: fixed 750 ms cadence +
   fixed 32×32 + focused-only; the CDC baud finding (why "raise baud" is not
   an alternative); alternatives considered: fixed-but-faster interval,
   display-driven sizing (deferred follow-on), probe revision-gating
   (future). Consequences: cadence constants are now gaps; the pacing model
   is shared by sim and device. Follow the repo ADR format (see recent ADRs
   in docs/adr/ for the section shape).
2. **Docs**:
   - `docs/lp-core/probes.md` — add a short "sizing & cadence" note: probe
     resolution is client policy per request; Studio tiers it by runtime
     kind.
   - Doc comments in `refresh_cadence.rs` already updated in P3/P4 — verify
     they describe the completion+gap model.
   - Check `lp-app/lpa-studio-core/README.md` and
     `lp-app/lpa-studio-web/README.md` (if they describe refresh/preview
     behavior) — update or note why not.
3. **Future-work capture** in `notes.md` (already drafted there; verify):
   protocol-level items deliberately deferred — probe revision-gating
   (IfChanged precedent from display-layout), binary/transferable protocol
   frames on sim, firmware stop-and-wait/64 B read buffer, display-driven
   probe sizing, wiring `ProjectProductSubscriptionIntent` to UI.
4. **Cleanup sweep** over the branch diff:
   - `git diff main --stat` review; grep the diff for `TODO`, `FIXME`,
     `dbg!`, `println!`/`console.log` debug leftovers, commented-out code.
   - No suppressed warnings (`#[allow]` additions need justification),
     no disabled/ignored tests, no scratch files.
   - Scope creep check: every changed file traceable to a phase.
5. **Validation**: `just check build-ci test` (CI parity). Story baselines:
   if captures drifted (P1 canvas change will drift node-card stories),
   regenerate via the story tooling or rely on CI auto-commit
   (docs/debt/story-capture-pipeline.md; STUDIO_STORY_PNGS_CONCURRENCY=1 if
   heavy sheets wedge).
6. `_DONE.md` is written by the implement workflow at the very end
   (outcome, validation, deviations, docs, ADR, follow-ups), and plan.md
   frontmatter flips to `status: done`.

## Agent reminders

Do not commit unless asked (the overall session was asked to PR — commits
happen at push time per the push workflow). Do not expand scope. Report what
changed, what was validated, deviations.

Review gate: none — PR review is the batched gate (sim feel + hardware walk
steps in the PR body).

## Definition of done

ADR written; docs updated; sweep clean; `just check build-ci test` green;
`_DONE.md` + frontmatter flip done by the implement workflow.

## Implementation Result

Status: done
Completed: 2026-07-27
Commit: pending (close-out commit on `claude/studio-ui-performance-probe-d6da80`)

- Changed: ADR `docs/adr/2026-07-27-completion-based-refresh-pacing.md`
  (+ open-follow-ups rows in `docs/adr/README.md`; the long-deferred
  "event-driven receive" row struck — this plan landed it); sizing & cadence
  note in `docs/lp-core/probes.md`; `lp-app/lpa-studio-core/README.md`
  refresh section rewritten for gap semantics + tiered scoping (it still
  cited the retired `for_flow_state`); `refresh_cadence.rs` docs verified
  (already the pacing spec); notes.md future-work section filled;
  Implementation Result sections on P1–P6.
- Validated: cleanup sweep clean over the full branch diff (no
  TODO/FIXME/dbg!/println!/console.log/#[allow]/#[ignore]/commented-out
  code; every file traceable to a phase); `just check build-ci test` green
  in this worktree (exit 0, no failures; the handoff's first-run
  `rv32n_imm_range` flake did not recur).
- Deviations: work continued on branch
  `claude/studio-ui-performance-probe-d6da80` (fast-forwarded from
  `claude/studio-ui-performance-312c9c`, which had no PR); story baselines
  left to CI auto-commit per plan.
