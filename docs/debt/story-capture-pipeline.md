---
status: paying-down
since: 2026-07-08      # first recorded capture pain (M4-gallery era)
logged: 2026-07-23
area: studio-web/story-capture
related:
  [
    "../defects/README.md",
    "chip task_16a65557 (deterministic slot-story drift)",
    "../adr/2026-07-26-ci-canonical-story-capture.md (the paydown ADR)",
  ]
---
# Story-capture pipeline: slow, flaky, and load-sensitive

**Shape** — Story baselines are captured by driving a headless Chrome
over CDP against a freshly built wasm bundle, locally, serially with
whatever else the machine is doing. Failure causes have shifted over
time (mobile-emulation nondeterminism — fixed; a concurrency wedge on
heavy end-of-queue sheets; CDP navigate timeouts under CPU/disk
pressure), which is what makes this structural: the pipeline has no
isolation from the machine's load and no resume, so any 10–15 minute
run can die at viewport 750/810 and start over. The `-if-needed` gate
diffs the working tree, so it silently skips when changes are already
committed. Recipe failures have been masked by shell piping more than
once (`… | tail; echo rc=$?` reports the tail's status).

**Carrying cost** — ~10–15 minutes per UI-touching change, times
retries (three consecutive failed runs on 2026-07-22/23 while a live
debugging session competed for the machine); visual gates and commits
queue behind it; byte-noise churn in a known set of stories
(slot-row/editor family) must be manually reverted on every capture;
each new agent session re-learns the incantations from memory notes.

**Workarounds** (current lore, keep updated — since the 2026-07-26 paydown
these apply only to local SCRATCH captures; canonical baselines come from the
`validate-stories` CI job via `just studio-story-pull`):
- `STUDIO_STORY_PNGS_CONCURRENCY=1` (2 on a quiet machine) and
  `STUDIO_STORY_CDP_TIMEOUT_MS=120000`.
- Run on a quiet machine — not while the dev server + live debugging
  are active; captures compete for CPU, disk, and Chrome.
- After committing UI changes, `-if-needed` will skip: use
  `just studio-story-baselines` directly.
- Always check the recipe's own exit line, not a piped `$?`.
- Revert the known churners before committing: `config-slot-row`,
  `slot-option-presence`, `slot-value-editor`, `version-badge`,
  `code-editor` (chip task_16a65557 tracks the churn itself).

**Incident log**
- 2026-07-25 — shader-agent merge captures: three consecutive CDP
  navigate timeouts (30s, then 90s) at concurrency 1, wedge story
  varying per run. Root aggravation: **seven zombie headless Chromes**
  accumulated from the failed passes themselves — each failure leaks
  Chrome processes that starve the next attempt. Fix: `pkill -f
  -- --headless` before retrying + the documented 120s timeout; passed
  first try on a clean slate. Lesson: check for leaked Chromes FIRST
  when captures wedge repeatedly.
- 2026-07-08 — M4-gallery era: baseline regeneration flakes noted at
  first gallery visual gate (CDP timeout, retries needed).
- 2026-07-16 — capture flake during M2 story-sheet work; retry passed.
- 2026-07-17 — CDP navigate timeout mid-run during M3; completing
  required concurrency 1 + longer timeouts; disk-near-full aggravated.
- 2026-07-17 — mobile-emulation nondeterminism root-caused and fixed
  (bb46ec32c); drift sequel 2026-07-20: pre-fix side-branch captures
  resurrected 13 contaminated baselines on main (refreshed d0b339262).
- 2026-07-20 — concurrency-4 wedge on heavy end-of-queue sheets;
  resume required CONCURRENCY=1.
- 2026-07-22/23 — three consecutive failed runs during the device
  debugging session (CDP timeouts under load); `-if-needed` gate
  discovered skipping committed-tree changes; rc masking discovered;
  orange/popover baselines deferred as debt.

- 2026-07-24 — M4 closeout capture: QUIET machine, concurrency 1,
  120s CDP timeout — still wedged, third kill by the SAME story
  (`project-workspace/project-pane` sm, a heavy end-of-queue sheet);
  780/810 viewports fine. Resume-from-disk retry used for the tail.
  The wedge is now story-specific, not load-correlated: the exit
  criteria's "resume-instead-of-restart" exists (used today) but the
  per-story hang deserves its own diagnosis.
- 2026-07-24 — M7′ single capture (the circle→edge churn, 867
  viewports): quiet machine, concurrency 1, 120s timeout — run 1 died
  on a CDP `Page.navigate` timeout at `project-workspace/overview`
  (lg) — a DIFFERENT project-workspace sheet than the July-24 wedge,
  same heavy-sheet family. Run 2 resumed from disk and completed
  clean. Two-run capture is now the working norm.

- 2026-07-26 — **PAYDOWN**: capture moved to CI
  (ADR `../adr/2026-07-26-ci-canonical-story-capture.md`). `validate-stories`
  job captures on a pinned x64 runner (Chrome for Testing 151.0.7922.47,
  oxipng 10.1.1, bundled Inter/JetBrains Mono), fails loudly on drift, and
  delivers fresh sets as the `story-images-fresh` artifact;
  `just studio-story-pull` stages them; `-if-needed` deleted. Full baseline
  set regenerated in the canonical environment at cutover.

**Exit criteria** — All of: (1) captures complete deterministically at
default concurrency on a loaded machine, or run somewhere isolated
(the "is local PNG generation worth it" decision — likely a paydown
ADR weighing CI-side capture vs local determinism hardening);
(2) resume-instead-of-restart on failure; (3) the gate detects
committed-as-well-as-working-tree UI changes; (4) failures are loud
(non-zero all the way out); (5) the churner story set is empty.

**Exit-criteria status after the 2026-07-26 paydown** — (1) isolated
pinned CI runner ✓; (2) clean ephemeral runners make restart cheap and
the in-script retry/resume is retained ✓; (3) CI check always compares
the committed tree against a fresh build — the `-if-needed` blind spot
is structurally gone (helper deleted) ✓; (4) blocking CI job, pipefail
guarded ✓; (5) churner set: pending the cutover green-run evidence —
flip this entry to `retired` when the post-cutover re-run shows the
historical churners byte-identical/tolerated.
