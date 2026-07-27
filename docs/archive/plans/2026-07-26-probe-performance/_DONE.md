---
kind: implementation-log
status: done
repo: lp2025
plan: plan.md
completed: 2026-07-27
commit: e287c3d5d (P1–P5); P6 close-out is the branch tip of claude/studio-ui-performance-probe-d6da80
adrs:
  - docs/adr/2026-07-27-completion-based-refresh-pacing.md
---

# Implementation log — probe & Studio UI performance

## Outcome

All six phases complete. Studio preview pacing is completion-based on both
runtimes, probe policy (resolution + node scope) is tiered by runtime kind,
sim receive is event-driven, previews render to canvas, and the on-device
sRGB encode is a LUT. No wire/protocol changes anywhere. The two feel gates
(sim feel check, device hardware walk) are deliberately batched at PR
review.

## Completed work

- **P1** canvas previews (`ProductPreviewCanvas`, span grid deleted).
- **P2** worker log hygiene (level-gated envelopes; `logs_dirty` split +
  0.25 s log-only publish throttle; no line dropped).
- **P3** event-driven sim receive (`OutputWait` wakers) +
  completion-based pacing (`ProjectRefreshOutcome::NotDue` gate;
  `NotDue`/`Cancelled` don't stamp completion).
- **P4** device tier: 150 ms gap (was 750 ms period), 16×16 device probes,
  4 KiB sRGB8 LUT (≤1 LSB vs float reference over all 65536 inputs), 2 ms
  in-stream receive poll. Firmware untouched.
- **P5** sim probes all non-collapsed nodes; device stays focused-only +
  primary visual; tracking badge threaded from the real subscription
  decision.
- **P6** ADR + docs + cleanup sweep + full validation (details in
  [06-cleanup-docs-adr.md](06-cleanup-docs-adr.md)).

Per-phase details and file references: phase files' Implementation Result
sections and [handoff.md](handoff.md).

## Validation

- `just check build-ci test` — green (exit 0) in the
  `studio-ui-performance-probe-d6da80` worktree, 2026-07-27. First
  `build-ci` run for this branch.
- `cargo check -p lpa-studio-web --target wasm32-unknown-unknown` — green
  (run during P3; required because `browser_worker` is cfg-gated to wasm32).
- Targeted tests named in each phase result; 638 pass in `lpa-studio-core`,
  289 in `lpc-engine`.
- Known non-issue: a one-time `lps-filetests rv32n_imm_range` failure on the
  predecessor worktree's first `just test` run was a rebuild race; it did
  not recur here.

## Deviations

- Branch continuation: P1–P5 were committed on
  `claude/studio-ui-performance-312c9c` (pushed, no PR); this worktree's
  branch `claude/studio-ui-performance-probe-d6da80` fast-forwarded onto it
  and carries P6 + the PR. The old branch is superseded.
- Story baselines for the P1 span→canvas change are not regenerated
  locally; CI auto-commits the drift (docs/debt/story-capture-pipeline.md).
- Otherwise none.

## Documentation

- New: `docs/adr/2026-07-27-completion-based-refresh-pacing.md`; "Sizing &
  Cadence" section in `docs/lp-core/probes.md`.
- Updated: `docs/adr/README.md` open-follow-ups index (new rows; struck the
  landed event-driven-receive row); `lp-app/lpa-studio-core/README.md`
  refresh/cadence section (was citing the retired `for_flow_state` and
  focus-only scoping); `refresh_cadence.rs` module docs (P3/P4) are the
  pacing spec.
- `lpa-studio-web/README.md` checked — its refresh mentions are
  preemption-policy prose that remains accurate; no change needed.

## ADRs

- `docs/adr/2026-07-27-completion-based-refresh-pacing.md` — completion+gap
  pacing + runtime-tiered probe policy, with the USB-CDC baud finding
  recorded as a non-alternative.

## Follow-ups (outside this plan)

- PR + CI watch (yona-push); expect a story-baseline auto-commit.
- Yona's batched gates: sim feel check + device hardware walk (retune
  `DEVICE_REFRESH_INTERVAL`, judge 16×16 legibility).
- Deferred protocol/futures recorded in notes.md "Future work" and the ADR
  index: probe revision-gating, binary/transferable sim frames, firmware
  transport work, display-driven probe sizing, subscription-intent UI, real
  collapse scope via the ui-state-audit.
