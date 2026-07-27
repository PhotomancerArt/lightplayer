---
status: retired      # 2026-07-26: all five exit criteria met — but see the 2026-07-27 incident: criterion (5) "churner set empty" has since been disproven; Yona to decide whether to re-activate
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
- 2026-07-26 — device state-flow reconciliation capture: CDP navigate
  timeout (120s, twice — both passes) on project-pane overview @ lg at
  concurrency 1, while the `studio-dev` dx server ran on the same
  machine. Retry with the dev server STOPPED + 180s timeout passed
  first try. Confirms the "quiet machine" workaround is load-bearing:
  the dev server alone is enough contention to wedge the heavy sheets.
- 2026-07-26 — node-card P2/P2b captures: even with a clean process slate and
  concurrency 1 + 120s, full runs decay and wedge after ~700+ navigations
  (wedge story varies per run — positional, not story-specific; the failing
  story rendered fine in isolation). **The script's fingerprinted RESUME is
  the fix**: re-running the same command kept 764/894 completed viewports and
  finished the tail first try. Lesson: resume beats restart — sweep leaked
  processes, then just re-run; do not diagnose the wedge story itself.
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

- 2026-07-26 — **THE WEDGE ROOT-CAUSED: `python3 -m http.server` hangs
  under capture load.** The class that plagued this pipeline since
  2026-07-08 ("heavy end-of-queue sheets", story-specific wedges,
  load-correlation) finally reproduced deterministically in a 4-cpu
  Linux container and was caught alive: when a CDP timeout kills a
  Chrome page mid-download, the python worker thread serving it blocks
  FOREVER in a kernel socket send (`sock_alloc_send_pskb`) and the
  server stops answering entirely (curl → empty reply). One transiently
  slow story → timeout → page recycled mid-response → server wedges →
  every later navigation on every page/browser times out. This explains
  the red herrings: wedges followed the capture frontier (not stories),
  fresh pages AND fresh browsers inherited the wedge (shared server),
  thresholds varied with load, and the renderer main thread sat in
  pthread_cond_timedwait waiting on fetches that never finished. Fix:
  the capture script now serves the site itself (in-process node static
  server, `response.close → stream.destroy`); python3 is no longer a
  dependency. A defense-in-depth browser restart every
  STUDIO_STORY_BROWSER_RESTART_EVERY captures (default 120, ~1-2s,
  resume from disk) also landed while browser aging was the leading
  theory. Also from the same debugging arc: a crashed partial capture
  could masquerade as drift (fixed via .check-complete sentinel gating
  the CI artifact and story-pull), and pinned Chrome-for-Testing needs
  the AppArmor userns sysctl on ubuntu-24.04 runners.
- 2026-07-26 — **CHURNER SET ROOT-CAUSED** (post-cutover run 6 failed on
  the classic churners; diffed pixel-level): three mechanisms, all
  bistable settling races the ready-wait didn't cover. (a) The font
  gate `document.fonts.status === 'loaded'` is trivially true before
  the first element requests a face — captures raced @font-face
  decoding (select-text ghosting; whole-page layout shifts when
  fallback metrics changed line wrap). Fix: ready-wait force-loads and
  `document.fonts.check()`s every bundled face. (b) [autofocus] focus
  lands a beat after first paint, scrolls the target into view
  (shifting the capture clip), and ring-survival across re-renders is
  itself racy — neither ring state is a stable terminal. Fix: wait for
  autofocus to land, then blur + scrollTo(0,0) — baselines always show
  the unfocused state. (c) Backstop: stable-pair capture (accept only
  two consecutive byte-identical shots, 5 tries, warn-and-keep-last)
  turns any residual settling race into determinism. Verified: 2×267
  captures over the historical churner families (config-slot-row,
  roster-card, slot-value-editor), byte-identical across passes.
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

- 2026-07-27 — **CHURNER SET NOT EMPTY: cross-run bistability recurred**
  (PR #149). Two consecutive CI captures produced different bytes for 3
  stories: `base__code-editor__overview__sm`,
  `studio__layout__version-badge__loaded__sm` (both flipped BACK to their
  pre-first-capture bytes — same class as the churner chip filed during
  PR #148), and `studio__node__shader-face__advanced-open__sm` (possibly
  NEW: the branch's canvas previews paint from an effect, so
  painted/unpainted may both be stable-pair terminals — chip
  task_c8301674). Operational lesson learned the hard way: after the
  auto-commit lands, do NOT approve the bot-triggered `action_required`
  run — approval starts a fresh capture that can flip bistable stories
  and auto-commit AGAIN (ping-pong; it also cancels the in-flight run via
  the concurrency group). The designed steady state is **green one commit
  back**: the run that found drift passes after committing, and the bot
  head ships with unapproved checks.

**Exit-criteria status after the 2026-07-26 paydown** — (1) isolated
pinned CI runner ✓; (2) clean ephemeral runners make restart cheap and
the in-script retry/resume is retained ✓; (3) CI check always compares
the committed tree against a fresh build — the `-if-needed` blind spot
is structurally gone (helper deleted) ✓; (4) blocking CI job, pipefail
guarded ✓; (5) churner set EMPTY ✓ — after the settling-race fixes
(font gate, focus blur, select reflow, stable-pair), PR #139 run 10
(2026-07-26) reproduced all 894 committed baselines byte-identically
on a fresh runner. All criteria met; entry retired. Chip
task_16a65557 (deterministic slot-story drift) is resolved by the same
fixes.
