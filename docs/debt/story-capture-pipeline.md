---
status: active       # re-activated 2026-07-27: criterion (5) was disproven by the PR #149 recurrence; see the narrowed exit criteria at the bottom
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
- 2026-07-27 — **shader-face flip diagnosed: NOT canvas paint** (chip
  task_c8301674 investigated). Pixel-diffing the two CI variants of
  `studio__node__shader-face__advanced-open__sm` (51359 vs 51448 bytes)
  shows the preview canvas region is byte-identical in both runs — and
  byte-identical to the pre-canvas span-grid baseline
  (`image-rendering: pixelated` reproduces the grid exactly). The flip is
  62 pixels, max channel delta 2, confined to the advanced drawer's slot
  rows (value-box / ⓘ-button / binding-chip borders): sub-AA raster noise,
  the same compositor-level class as `version-badge__loaded__sm` (85 px,
  Δ≤6) and NOT an app settling race — no app-level ready gate can remove
  it, only the stable-pair keeps each run self-consistent. The
  paint-timing hypothesis was still closed structurally: an unpainted
  canvas IS a theoretically stable pair terminal (paint runs from an async
  task after mount), so `paint_preview_canvas` now stamps
  `data-preview-painted` after its first blit and the capture ready-wait
  refuses readiness while any `ux-produced-product-pixel-canvas` in the
  story lacks it — same pattern as the font and `data-story-wait` gates.
- 2026-07-27 — **THE CHURN WAS TWO DIFFERENT THINGS, and the pixel
  tolerance already knew.** `check` has always tolerated sub-threshold
  diffs (`STUDIO_STORY_MAX_CHANNEL_DELTA` 64, `MAX_DIFF_PIXEL_RATIO`
  0.0005), and measured against it the three PR #149 flips split cleanly:
  shader-face drawer and version-badge scored **0** significant pixels
  (pure AA/raster jitter — correctly tolerated), while code-editor scored
  489/468000 = 0.001045, twice the limit (correctly rejected). The
  tolerance was right both times; the **auto-commit ignored it**. It
  deleted every baseline and copied the whole fresh capture, so tolerated
  files got committed anyway and flipped back on the next run — the
  ping-pong was a commit-path bug, not a comparison-threshold problem.
  Fixed: `check` now writes `.refresh-manifest.json` naming exactly the
  stale files, and both consumers (CI auto-commit, `story-pull`) apply it
  through `story-apply-refresh.mjs` instead of swapping the set. Tolerated
  bytes stay put, so sub-threshold jitter can no longer churn a baseline.
- 2026-07-27 — **code-editor churn is a REAL USER-VISIBLE BUG the capture
  caught — root-caused, NOT yet fixed.** Reproduced live in the dev build:
  the line-number gutter advances **14px** per row while code lines
  advance **18px**, so numbers walk out of alignment with the lines they
  label (drift grows down the file; the lint marker ends up beside the
  wrong line). Mechanism: CodeMirror sizes gutter elements from
  `viewState.heightOracle.lineHeight`, which caches **14** — `normal` at
  the theme's 12px — while content lays out at **18** (the theme's
  `line-height: 1.5` on `.cm-scroller`). The oracle is measured once and
  keeps the stale value. It is bistable because a later *forced content
  re-measure* does correct it to 18 (observed directly), and whether
  anything forces one is timing-dependent — so aligned and misaligned are
  both stable end states, which is exactly what the two CI runs captured.
  **Hypotheses tested and DISPROVEN** (record so the next attempt does not
  repeat them — each was verified live against `heightOracle.lineHeight`):
  `view.requestMeasure()` does not re-read text size (oracle stays 14); a
  `window` resize event does not; an empty `dispatch({})` does not; a
  selection-only dispatch does not; and moving `line-height: 1.5` onto the
  editor root `&` in the theme leaves the root computing 18px but the
  oracle still 14 at construction. What DID correct it: mutating
  `contentDOM` (appending a probe `.cm-line`) and a real document change —
  i.e. only a genuine content re-measure. **Next step:** find the
  supported CM6 way to force a text-size re-measure after mount, or ensure
  the theme's styles are applied before the first measurement can run
  (this is a theme-CSS/measure ordering race, NOT the webfont-metrics race
  first suspected). Related in spirit to the 2026-07-26 popover
  `fonts.ready` re-measure fix: **components that cache text geometry at
  mount are a recurring hazard class here.** Lesson for the pipeline: a
  churner is a signal to diagnose, not merely noise to suppress — this one
  was load-bearing.

- 2026-07-27 — **`home-gallery/opening-a-project` high-amplitude tolerated
  diff: UNRESOLVED, and the evidence is gone.** Run 30309630982 reported
  md 166/840 px max Δ199 and lg 190/894 px max Δ191 as within tolerance;
  run 30311541469, 32 minutes later on the same branch (only a docs file
  changed between the two commits — app code and capture script
  byte-identical), reproduced both baselines byte-identically. So this is
  a genuine run-to-run flip, not a stale baseline: a stale baseline
  reports the *same* numbers every run, the way
  `studio__layout__studio-shell__overview` has reported 623/895/Δ144 on
  eight-plus consecutive runs across different branches. Neither run
  emitted an `unstable render` warning, so both terminals survived the
  stable pair. It is a singleton across 46 recent CI runs, and `sm` never
  drifted.
  - **Do not re-run the reproduction.** 38 attempts produced
    byte-identical output every time: 20 on macOS (concurrency 1 and 6),
    18 in a Linux container (12 normal, 6 starved at `--cpus=0.8` with 4
    concurrent pages). Worth keeping: a `node:22-bookworm-slim` +
    `chromium` container reproduces CI's **exact** story-box dimensions
    (720×618 / 1080×618 / 390×1080) where macOS does not, so it is the
    right place to test layout questions locally.
  - **Ruled out by measurement**, not by argument (numbers in the defect
    entry's calibration table): compositor layer promotion / raster mode
    (max Δ2 — this IS the version-badge and shader-face churner class, an
    order of magnitude too small); post-ready reflow (story box stable at
    720×618 from t=0 to t=1200ms); preview canvases (home-gallery stories
    mount none); and the CSS transition storm below.
  - **Real but not the culprit:** at the instant the ready-gate fires,
    5 loads in 6 have **48 mid-flight CSS colour transitions** running —
    every `transition-colors` element animating from UA/initial colours
    (white text, black borders) to the theme's over 150ms, i.e. the
    themed-stylesheet swap. Captured mid-flight that is 46 645 px at max
    Δ35 — wide, faint, wrong shape — and it is gone by t=50ms, well
    inside the stable pair. Left alone, but it is a latent hazard: a
    slower runner could freeze a page further up that ramp.
  - **What correlates:** the drifting capture happened in a degraded
    pass. Pass 1/4 died with `ENOTEMPTY: directory not empty, rmdir
    '/tmp/lp-studio-story-chrome-vjE3AC/Default'`, and the story was
    captured ~18 captures into the retry pass on a fresh Chrome,
    immediately after `home-gallery/first-run (sm)` timed out and was
    retried on a fresh page. The clean run captured it mid-stream in an
    uneventful chunk. That is a lead, not a mechanism.
  - **Why it stopped here:** the check only uploads fresh PNGs when it
    FAILS, so a tolerated diff leaves one line of log text and no pixels.
    Filed as
    [story-check-tolerance-ignores-amplitude](../defects/2026-07-27-story-check-tolerance-ignores-amplitude.md)
    — fix the retention before the next one of these, or the next
    investigation dead-ends in the same place.
- 2026-07-27 — **retention fixed + warn-only amplitude line** (the defect
  above, mitigated same day): every complete check with a non-empty
  tolerated set now uploads `story-images-tolerated` (fresh + baseline
  variants of just those files, 14-day retention, pass or fail — the
  auto-commit path also dropped tolerated pixels), and a tolerated file
  with significant pixels prints a loud WARNING instead of one silent
  log line. The condition is `significantPixels > 0`, not a new
  threshold: every benign churner ever measured diffs at 0 significant
  pixels (first post-#153 healthy run: 1036 byte-identical, 5 tolerated,
  all Δ≤6). Gate promotion deferred until a few real runs confirm the
  boundary; the retained artifacts are the calibration data. When the
  `opening-a-project` flip next fires, the pixels will be in the
  artifact — diff them, do not re-run the reproduction.

- 2026-07-27 — **churner recurrence, second consecutive PR** (#154, node
  authoring). The auto-commit refreshed 103 baselines; 99 were the
  change's real drift (gallery New chip, project-pane losing its header
  "+" and gaining the tree add row, workspace add button, playlist ADD
  chip, node delete action, 5 new stories). The other **4 are churn**,
  and they are not full 3-size sets — the tell that a story drifted for
  reasons unrelated to the diff:
  `studio__layout__version-badge__loaded__sm` and
  `studio__node__shader-face__advanced-open__{lg,sm}` (both repeat
  PR #149's set, and shader-face now churns at TWO viewports, not one)
  plus `studio__roster__roster-card__op-overlay-failed__lg` — a **new
  member** of the set, from a story family this branch never touched.
  So the 2026-07-27 finding is not a one-off: the set is non-empty AND
  growing across unrelated branches, which strengthens the case for
  re-activating this entry (exit criterion 5). Nothing was reverted —
  post-paydown the CI capture is canonical and the job passed on its own
  commit. The "green one commit back" steady state held, and the
  bot-triggered `action_required` run was left unapproved per the
  2026-07-27 lesson above.

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

- 2026-07-27 — **first post-fix capture (PR #153, run 30309630982):
  green, no bot commit, 1022 byte-identical + 4 within tolerance.** The
  two #149 churners reproduced their exact signatures — version-badge
  85 any-diff/Δ6 and shader-face 62 any-diff/Δ2, both 0 significant px —
  and were correctly left uncommitted. Caveat for exit tracking: this run
  does **not** exercise the manifest apply path, because auto-commit only
  runs when the check FAILS and this check passed. That is worth stating
  precisely, because it also names the real causal chain on #149: the
  gutter bug made the check fail, the failure triggered auto-commit, and
  only then did the wholesale copy drag the tolerated files in. Tolerated
  stories can therefore only churn when some OTHER story genuinely fails.
  The manifest path stays CI-unproven until a run legitimately fails
  (`story-apply-refresh.mjs` is unit-tested locally for replace/add/
  remove/leave-tolerated plus its refuse-without-manifest guard).
  Also new this run: `home-gallery__opening-a-project` md+lg tolerated at
  0.037%/0.028% but with **max Δ199/Δ191** — under the ratio limit, yet far
  too high-amplitude to be anti-aliasing. Given the code-editor precedent
  below, treat that as a suspected bistable render, not noise (chip
  task_4c540503). **Investigated the same day — see the
  `opening-a-project` entry above.** The suspicion holds (it is a genuine
  run-to-run flip, and Δ199 is thirty times the known raster-churner
  class) but the mechanism is unidentified and unreproducible in 38
  attempts, because this run's fresh PNGs were never retained: the
  artifact upload is keyed to check FAILURE, so a tolerated diff leaves
  counts and no pixels. That retention gap is now its own defect,
  [story-check-tolerance-ignores-amplitude](../defects/2026-07-27-story-check-tolerance-ignores-amplitude.md).

**Re-activated 2026-07-27 — narrowed exit criteria.** Criteria (1)–(4)
remain met and are not revisited; only (5) failed, and the 2026-07-27
investigation showed why it was always going to: byte-exact commits
cannot coexist with a tolerance-based check. What remains:
- (5a) No baseline is ever committed for a diff the check tolerates —
  delivered by the refresh manifest; **confirm on the next two CI captures
  that no tolerated story appears in a bot commit.**
- (5b) Every story that exceeds the tolerance is a real defect that gets
  diagnosed, not suppressed. Precedent set today: the code-editor flip is
  a genuine gutter-alignment bug (root-caused above, fix still open).
  Raising the thresholds to silence a story is explicitly NOT an exit
  path.
Exit when both hold across a few captures. If a new over-tolerance
churner appears and resists diagnosis, that is the architecture signal —
escalate rather than widening the threshold.

**Added 2026-08-05 — criterion (6), the HANG (distinct from the drift
criteria above).** The CI job wedged twice in one day for hours with zero
output; the unbounded wait that let it is fixed, but the reason the discovery
Chrome hangs is not known. See the 2026-08-05 incident. Exit (6) when either:
the discovery hang is reproduced and root-caused (the way the 2026-07-26
python-server wedge was), **or** several weeks of CI pass with no run hitting
`STUDIO_STORY_DISCOVERY_ATTEMPTS` exhaustion, the run watchdog (exit 3), or a
job `timeout-minutes` — i.e. the retry absorbs it and the class is empirically
gone. Bounding a hang is containment, not diagnosis: `timeout-minutes` and the
watchdog address the **carrying cost** (a burned runner and a PR pending for
hours), and neither is an exit path on its own.

- 2026-07-28 — **the `overview` composites are out of the pipeline, and
  the churn they caused was never a settling race.** Both PR #163
  flip-floppers were generated composites, which is what made "composites
  race their transitions" the working theory. Diffing the two committed
  variants of `studio__roster__roster-card__overview__lg` says otherwise:
  279448 px / max Δ243, confined to the five stories in the page that
  mount a `backdrop-filter` overlay (`.ux-card-sheet`, `.ux-card-op`), all
  of them 7×–16× below the 760 px viewport, with everything above y=5658
  byte-identical. In the bad variant those overlays keep their blur and
  lose their own background and children — a partly rasterized surface
  under `captureBeyondViewport` against a **14991 px** clip, not an
  unfinished animation. Transitions were already frozen at capture time
  (`* { transition: none !important }`, since 250ea7ff7), so the fix this
  entry's predecessor proposed was already in the tree and irrelevant.
  Both terminals are per-page-load, so the stable pair passes on either
  and commits a coin flip.
  Composites are now excluded from discovery and their 153 baselines
  deleted (**27 MB of the set's 55 MB**; every state in them is captured
  on its own page anyway, and no non-composite baseline exceeds 3400 px).
  See [the defect](../defects/2026-07-28-overview-composite-capture-races.md)
  and [the ADR](../adr/2026-07-28-overview-composites-are-not-baselines.md).
  For (5b): the other #163 flipper, `base__code-editor__overview__sm`, is
  the open gutter bug — a *different* mechanism that happened to surface
  through a composite. Grouping the two by their shared page shape was the
  wrong inference, and it cost a round of theorizing.
  Two things for the workarounds lore above: the capture pass is now
  materially lighter (the composites were its heaviest pages, the family
  that has wedged runs since 2026-07-08), and **diff the bytes before
  designing a fix** — this pipeline has now produced several "obviously a
  settling race" diagnoses that the pixels overturned.

- 2026-08-05 — **a churner that was NOT a settling race, again: the clock
  face's trace canvases were photographed at the wrong RESOLUTION.** Two CI
  runs on PR #349 (which touches no clock-face code) auto-committed the same
  five baselines back and forth between two blob hashes, the second run
  restoring main's bytes exactly — the signature of a bistable render, and
  the reason to compare a refresh against the PREVIOUS refresh rather than
  against main. Diffing the two committed variants of
  `clock-face__crowd__lg` (7 506 px, max Δ243, confined to the eight trace
  canvases; every other pixel byte-identical) killed the obvious theory
  first: **the waveforms are at the same phase in both** — peaks, troughs and
  square transitions on the same output columns — so nothing about time or
  settling was in play. What differed was that every *device-pixel-absolute*
  constant shrank together: the 3px pad read ≈0.5, the 1.25px stroke read
  ≈0.24, and the 1px 14%-alpha midline was **gone entirely** while the
  vertical risers survived at varying intensity. That asymmetry is the
  fingerprint of a bitmap being scaled down — a single-row feature is
  all-or-nothing under it, a column of pixels is not — and normalized
  quantities (the curve) passing through untouched is the other half.
  Root cause: `paint_card` sizes the canvas backing store from
  `getBoundingClientRect()`, and on a frozen story page the driver stops the
  rAF loop after its first frame, so that one measurement is permanent.
  Studio's stylesheet is injected by the wasm bundle after boot, so a paint
  that beats it measures the canvas's unstyled 300×150 intrinsic size. Both
  outcomes are stable terminals and the stable pair passes on either.
  Reproduced live by serving the story build with the tailwind stylesheet
  delayed 1500 ms: the trace canvases painted at the pre-stylesheet box and
  stayed there. Fixed with a `ResizeObserver` that repaints on box change,
  an inline box on the canvas (an unstyled canvas takes its box FROM the
  width/height attributes the paint writes, which at dpr > 1 makes the new
  repaint feed itself), and a ready-gate assertion that no
  `canvas.ux-box-sized-canvas` may be captured while its backing store
  disagrees with its box. See
  [the defect](../defects/2026-08-05-clock-face-baselines-oscillate.md).
  Three things for this entry's lore. (1) **"Diff the bytes before designing
  a fix" paid again** — the third time now (composites 2026-07-28, the
  code-editor gutter 2026-07-27, this): the settling-race reading was wrong
  and the pixels said so in one look. (2) The freeze pins *time*; it does
  not pin *geometry*. Any surface that stops re-rendering to be photographed
  must have finished reading everything it depends on, and on this app "the
  stylesheet has applied" is not something a first paint may assume. (3)
  Debugging note for the harness browser pane: its tab reports
  `document.hidden === true`, so rAF and ResizeObserver callbacks never fire
  there until something forces a frame — take a screenshot first, or a
  working fix reads as a dead one.
  **Verified across two CI captures of the same tree** (runs 31024986361 and
  31026385720, PR #354) — the comparison nothing in this pipeline does on its
  own, and the only thing that can falsify an oscillation. Capture 1
  reproduced main's `clock-face__crowd__lg` byte-for-byte and named exactly
  two stale files (`crowd__md`, `default__lg`, both max Δ243); capture 2
  reported the whole clock-face family byte-identical, in neither the drifted
  set nor the tolerated-with-significant-pixels warning. Two notes that
  outlive the fix. (a) **main was carrying the degraded render for those two**
  while holding the correct one for the rest, so an oscillating baseline
  leaves the set in a MIXED state — a branch that has not touched the files
  silently takes main's side on merge, and the fix has to pin them back
  explicitly or it re-lands the bad pair. (b) The comparison only worked
  because both runs were on one branch close together; on main, a story that
  flips has nothing to disagree with.
  Left open by this pass: `exploration__node-ui__status-indicators__sm` was
  byte-identical in capture 1 and drifted at 304 significant px / max Δ223 in
  capture 2 — a fresh member of the bistable set, in a story family this
  branch never touched and (unlike the traces) carrying no canvas, so it is a
  different mechanism. It is what (5b) asks for: diagnose, do not suppress.

- 2026-08-14 — **the workbench Mapping-canvas zoom churner: a camera fit
  frozen on a PRE-SETTLE viewport measurement — mechanism fixed, plus a
  geometry guard.** Main run 31776357645 refreshed `workbench-mapping-view`
  and `workbench-mobile-outputs-summoned` one way; run 31777574897
  immediately flagged the lg variant drifted back — the same story rendered
  at **82% zoom in one capture and 157% in the next**, otherwise identical.
  Mechanism: `ProjectCanvasHost` seeds its camera from a fit-all that waits
  for the canvas svg's measured size (`viewport` signal, ResizeObserver) and
  then freezes (`fit_pending` cleared). The fit consumes the FIRST
  measurement, and that measurement races container layout settling (dock
  widths, the workbench's mobile-fold breakpoint, stylesheet arrival) — so
  the same mount fits against different container sizes run to run, and both
  zooms are stable terminals the stable-pair passes on. Same class as the
  2026-08-05 clock-face bitmap (the freeze pins *time*, not *geometry*: a
  surface that stops adjusting to be photographed must have finished reading
  everything it depends on, and "layout has settled" is not something a
  first measurement may assume). Fix, per the ratified never-the-thresholds
  rule: `FitReconcile` (lpa-mapping-editor) records the `(viewport, camera)`
  each fit consumed, and both fit sites (`ProjectCanvasHost`, the composed
  mapping-editor story mount) now RE-fit whenever the measurement moves
  while the camera is still exactly the fitted value — untouched by
  pan/zoom — so the final camera is a function of the settled layout, not of
  measurement timing; the moment the user touches the camera it is theirs
  and reconciliation stops. Guard (the `ux-box-sized-canvas` idiom): the
  canvas wrap stamps `data-fit-viewport` with the size the camera was last
  reconciled against, and the ready gate refuses to shoot while any visible
  stamp disagrees with the svg's real box — a future fit race is a loud
  capture timeout naming the story, not a baseline flap. Hidden mounts (the
  mobile fold's replaced center at sm) are exempt; an empty canvas records
  its reconciliation too (the default camera is deterministic), so the guard
  cannot deadlock on content-less mounts.
  **Verified across two consecutive CI captures on PR #423** (the only
  comparison that falsifies an oscillation): run 31779708023 captured the
  fixed tree and auto-committed exactly 24 baselines — all in the
  mapping-editor composed-story family, the expected one-time re-fit at the
  settled zoom — with ZERO workbench drift; the parked bot-head run
  31780802809 was then approved deliberately (the standing "don't approve"
  lore exists because approval re-armed the flip on the UNFIXED mechanism —
  with the mechanism gone, the re-capture is the convergence proof) and
  passed with no further auto-commit. Locally, three consecutive captures of
  the workbench + mapping-editor families were byte-identical for all 30
  canvas-bearing stories; the single local flipper
  (`workbench-nodes-view__md`, 25 px at max Δ1, story mounts no mapping
  canvas) is the known tolerated sub-AA raster class, diffed before being
  dismissed per this entry's own rule.
  message points at it.** `story-apply-refresh.mjs` parsed `process.argv` and
  called `process.exit(2)` at *module scope*, so `story-pull.mjs` — which
  imports `applyRefresh` from it — exited 2 before doing anything. The
  documented manual fallback for drift has therefore been dead since the two
  scripts were split, and the CI failure message ("`just studio-story-pull`
  is the manual fallback") sends you straight into it. Argument parsing now
  lives inside the `import.meta.url === argv[1]` guard; both the CLI and the
  import work.
  Found while chasing a story-job failure on PR #207 that was **neither
  drift nor a capture crash: it was a wasm compile error.** A new
  `LinkManagementRequest` variant left a non-exhaustive match in
  `browser_serial_esp32/provider.rs`, which is feature-gated to wasm — so
  `just check` and even `cargo check -p lpa-link --all-features` on the host
  both passed, and `dx build ... [wasm32-unknown-unknown]` failed. This is
  the [wasm gap](../../docs/debt/README.md) biting through the story job, and
  it presents *identically* to a wedge: no capture ran, so the
  `.check-complete` sentinel refused to treat it as drift (correctly), the
  upload/auto-commit steps skipped, `Capture crash summary` fired, and the
  guard step announced "unresolved story drift".
  **Reading the signal:** step conclusions distinguish the three cases, the
  step *name* does not. Skipped upload steps + fired crash summary means "no
  usable capture" — which is a build failure at least as often as a wedge.
  Read the tail of the `Story check` step before assuming a wedge; a
  compile error is at the top of the step, thousands of lines above the
  failure line. Local prophylactic for studio-touching changes:
  `cargo check -p lpa-studio-web --target wasm32-unknown-unknown`.

- 2026-08-05 — **CI WEDGE ROOT-CAUSED (the hang, not the drift): story
  DISCOVERY Chrome never exits, and nothing was waiting on it with a bound.**
  Two wedges the same day, both cancelled by hand: run 30993890003 job
  92266222518 (PR #349) burned **5h08m**, and run 30986724958 job 92243105380
  burned **3h35m** — out of ~25 story-job starts that day. Both logs have the
  **identical** signature, and it is not subtle:
  - Last line of output: `Artifacts: target/dx/lpa-studio-web/release/web/public/
    (story build)` — i.e. `dx build` **succeeded**. Then *nothing at all* until
    `##[error]The operation was canceled` hours later. Not one capture line.
  - Orphan processes reaped at cleanup, identical in both: `just`, `tee`,
    `bash`, `node`, **exactly one `chrome` and two `chrome_crashpad_handler`**.
    That is the shape of a single `--dump-dom` browser, not a capture pass
    (which runs 4 pages and would have printed `Capturing N/M …` and `wrote …`).
  Between the build and the first capture line the script does exactly four
  things, and only one of them could hang: `computeBuildFingerprint` (file
  reads), `waitForServer` (bounded, 10s, throws), the static server's `listen`,
  and **`discoverStoryIds()` → `runChrome(--dump-dom)` → `runProcess` → a bare
  `await once(child, "exit")` with no timeout**. Every timeout this pipeline
  has accumulated — `STUDIO_STORY_CAPTURE_TIMEOUT_MS`,
  `STUDIO_STORY_CDP_TIMEOUT_MS`, page recycling,
  `STUDIO_STORY_BROWSER_RESTART_EVERY` — guards the **CDP capture path**, which
  a wedged discovery never reaches. So the run went silent and stayed silent
  until GitHub's 6-hour default would have reaped it.
  **What is fixed** (all verified against a fake Chrome that never exits, and
  against a real Chrome for the success path):
  - `runProcess` is now bounded (`STUDIO_STORY_SUBPROCESS_TIMEOUT_MS`, 180s
    default; also covers the previously-unbounded `oxipng` call). It races the
    exit against a timer and SIGKILLs, so it **always settles** even if the kill
    does not take. Verified: the 5-hour hang becomes a 9-second failure, with no
    leaked processes.
  - Discovery gets its own bound + retries (`STUDIO_STORY_DISCOVERY_TIMEOUT_MS`
    120s, `STUDIO_STORY_DISCOVERY_ATTEMPTS` 3), so a *transient* hang costs a
    minute instead of the job. 120s is sized off a **measured** happy path
    (~21s for a cold Chrome locally), not off `--virtual-time-budget=5000`.
  - A **global watchdog** (`STUDIO_STORY_RUN_DEADLINE_MS`, 40 min) that exits
    **3** — distinct from drift (1) and usage (2) — printing the phase it died
    in. It is a watchdog timer rather than a race on individual awaits
    deliberately: that is the only shape that covers phases nobody has thought
    to bound yet, which is exactly the class this incident belonged to.
  - **Periodic progress output** (`STUDIO_STORY_PROGRESS_INTERVAL_MS`, 30s):
    `[+MM:SS] <phase> — N/M captured`. The capture phase was never the silent
    one; the point is that discovery, oxipng, and comparison now announce
    themselves, so a future wedge says *where* it stopped.
  **What is NOT fixed, and why this entry stays open:** *why* the discovery
  Chrome hangs is **unidentified and was not reproduced**. Per this entry's own
  standing warning — this pipeline has produced several "obviously a settling
  race" diagnoses the pixels later overturned — no theory about Chrome's
  internals is recorded here, because none was tested. What is proven is the
  *location* (the unbounded wait) and that bounding it converts the wedge into
  a fast, labelled red. If it recurs, the new log will name the phase and the
  killed-process message will carry Chrome's stderr — start there.
  **Carrying-cost fix, filed alongside and explicitly NOT a root-cause fix:**
  no workflow in this repo set `timeout-minutes` at all, so any wedge anywhere
  ran to the 6-hour default. Every job in `.github/workflows/pre-merge.yml` now
  has one, sized from the p95/max of the last ~60 runs' successful jobs at
  roughly 2-3x the observed max (story job: p95 23.7 min, max 24.4 → 45).
  Generous on purpose: the goal is to convert a wedge into a fast red, not to
  make a slow-but-healthy run flaky. **This bounds the damage; it does not
  explain the hang.** The entry stays open on the root cause.

- 2026-08-05 — **`exploration/node-ui/status-indicators` @ sm: DIAGNOSED, and
  the pixels overturned the settling-race prior again.** Run 31024986361
  captured it byte-identically; run 31026385720, minutes later on the same
  branch with no app change touching the story, reported 304/352560 px (0.086%)
  over Δ64, max Δ223 — over the ratio limit, so the auto-commit refreshed it as
  75e931304 (pre-refresh bytes in parent 6e694f210). Diffing the two committed
  variants **first**, as this entry keeps insisting: the whole 390×904 frame is
  byte-identical except an 11-row band, and that band is **one line** of the
  five-line rustc-style error block in the error node's status popover, moved
  down **exactly one device pixel**. The other four lines diff at **residual
  zero** under alignment; the moved line's glyphs are bit-identical (pure
  integer translation, residual 3px of AA). Nothing reflowed — the gap above it
  shrank 1px and the gap below grew 1px.
  **Root cause, reproduced not argued.** Two things compose.
  (a) `.ux-node-ui-status-popup-error-detail` sets `font-size: 0.68rem` /
  `line-height: 1.45` → a used pitch of **15.765625px**, so consecutive
  baselines differ in fractional part by 0.765625 and the five lines never
  share a rounding phase. Chrome snaps text baselines to whole device pixels,
  so a sub-pixel move of the block flips only whichever line sits within that
  move of a `.5` boundary — and with five lines spaced 0.766 apart there is
  nearly always one. Measured on the real story the five fractional tops are
  `[.1875, .9531, .7188, .4844, .25]`: one of them is **1/64 px** from the tie.
  (b) `PopoverPosition::style()` emits `top: {:.1}px` — the panel position is
  quantized to **one tenth of a pixel**, which is exactly the step that flips a
  parked line. Reproduced directly with the same font and CSS: moving a
  container top from `100.7px` to `100.8px` moves line 1 from 127.4531 to
  127.5625 (row 127 → 128) and leaves every other line byte-identical —
  **644 any-diff / 368 over Δ64 / max Δ231**, the same shape and amplitude as
  CI's 513 / 304 / Δ223. And the position does wobble: **10 consecutive loads
  of the real story in one headless Chrome emitted `921.2px` nine times and
  `920.2px` once**, same build, same browser, same machine.
  **Ruled out by measurement:** the stale-canvas-backing mechanism just fixed
  for the clock face (this story mounts no canvas at all); webfont/fallback
  metrics (those change glyph shapes — these are bit-identical); AA/raster
  jitter (the version-badge/shader-face class tops out at Δ2–6, this is Δ223);
  mid-flight CSS colour transitions (wide and faint, not narrow and geometric).
  **Filed as** [popover-line-parked-on-a-rounding-tie](../defects/2026-08-05-popover-line-parked-on-a-rounding-tie.md),
  with the fix ranked there (whole-pixel popover positions first — it collapses
  the class for every popover story; integral line box on the error block
  second). **Not fixed**, and thresholds were **not** touched: per (5b),
  raising them is not an exit path. Also still unidentified: *why* the trigger
  measurement varies between loads — the 1px wobble above is larger than the
  sub-pixel one CI captured, and neither proposed fix explains it, they only
  make the render insensitive to it.
  **New lore for this entry's workaround list:** this story's ready gate does
  not converge on macOS Chrome 142 — three `[data-story-wait="1"]` elements
  never clear and `studio-story-pngs.mjs` times out at 30s and retries on a
  fresh page, every attempt. CI's pinned Chrome 151 captures it fine. So the
  flip could not be reproduced end-to-end through the real harness locally; the
  mechanism was proven with a direct CDP probe instead. Worth knowing before
  the next local reproduction attempt on a popover story.
