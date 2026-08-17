---
status: fixed
found: 2026-08-08      # how: ci (validate-stories ping-pong after PR #386 merged)
fixed: 003f068bf
area: lpa-studio-web story capture (clock-face + panel-state transport stories)
class: stale-measurement
related:
  - 2026-08-05-clock-face-baselines-oscillate.md
  - 2026-08-05-popover-line-parked-on-a-rounding-tie.md
  - ../debt/story-capture-pipeline.md
  - ../adr/2026-07-26-ci-story-auto-commit.md
---
# Tape transport canvas repeats the clock-face oscillation, one bot commit per CI run

**Symptom** — Since PR #386 (the clock transport panel) merged, the
`validate-stories` auto-commit fired after EVERY CI run on every branch,
moving `studio__node__clock-face__*` and
`studio__module__panel-state__transport*` baselines **back and forth
between two byte states** (e.g. `transport-off-live__md` ping-ponged
14726 ↔ 14320 across four consecutive bot commits). Because bot pushes
don't trigger workflows, branch heads chased baselines and never showed
green checks.

**Root cause** — The same mechanism as
2026-08-05-clock-face-baselines-oscillate, reached through the NEW canvas
PR #386 added. `TapeTransportDriver::paint` sizes the backing store from
`getBoundingClientRect()`, and on a frozen story page the driver latches
`frozen` and stops the rAF loop after the first frame. The app's
stylesheet is injected by the wasm bundle after boot, so a paint that
beats it measures the canvas's unstyled box and bakes a bitmap the
browser then squeezes into the real 62px-tall one. Both outcomes are
stable terminals; the baseline records whichever one that run reached.
The pixel diff of the two committed variants of `clock-face__fast__lg`
is confined to the tape canvas band (y 134–194), the tape-canvas
signature of the earlier defect's trace-canvas one.

The 2026-08-05 fix had three parts — inline box style,
`ux-box-sized-canvas` under the capture's ready gate, and a
`ResizeObserver` repaint — and the tape driver, written months of
context later as "phasor_trace's contract to the letter", carried the
freeze contract but **none of the three geometry guards**. The ready
gate only asserts the backing-store invariant for canvases that opt in
via the class, so the unmarked canvas sailed past it.

**Fix** — The same three parts, applied to the tape canvas:

1. `TapeTransportDriver` installs a `ResizeObserver` over the tape canvas
   and repaints on box change (idempotent: time pins to the anchor on a
   frozen page).
2. The canvas's box moved to an inline `style`
   (`display:block;width:100%;height:62px`), applied on first layout and
   independent of the `width`/`height` attributes the paint writes.
3. The canvas wears `ux-box-sized-canvas`, so the capture's ready gate
   refuses to shoot while its backing store disagrees with its box.

**Verified** — two consecutive local captures
(`STUDIO_STORY_PNGS_CONCURRENCY=1 just studio-story-pngs clock-face
panel-state`) produce byte-identical PNGs.

**Reading the post-fix refreshes (do not mistake them for a failed fix).**
The two CI runs on the fix branch each auto-committed a baseline refresh
that still touched clock-face and transport stories, which looks exactly
like the churn being fixed. Measure before concluding, as ever — the two
variants of `clock-face__shared__lg` say which direction the change went:

| | ruler | label baseline | label height |
|---|---|---|---|
| replaced bytes | 22 px/s (5 s majors 110 px apart) | 47 px below canvas top | 3 px |
| written bytes | **14 px/s** (majors 70 px) | **29 px** | **7 px** |

The written side is the render the code specifies: `TAPE_BASE_PX_PER_SEC`
is 14, and `label_y = h - 32` on a 62 px canvas is 30. The replaced side
is a bitmap drawn for the **300×150 intrinsic default** and stretched
into the real 471×62 box — 471/300 = 1.571 = 22/14 horizontally, and
(150-32)×62/150 = 48.8 vertically. So each refresh was CORRECTING a
degraded baseline, not recording a fresh flap.

Those degraded baselines are the pre-fix era's, still sitting on main:
an oscillating baseline leaves the set MIXED (the 2026-08-05 entry says
so in as many words), and **merging main re-imports main's copy for
every file the branch has not itself touched** — which is why a second
refresh followed the merge commit and touched a DIFFERENT subset than
the first. `clock-face__paused__lg` is the control: byte-identical
(`b4844c34ea80`) across refresh #1 → merge → refresh #2, two
consecutive CI runs of the fixed code.

The signature to actually watch for, per the 2026-08-05 lesson, is a
blob hash returning to an EARLIER value with no merge in between. A
return that straddles a merge from main is main's stale baseline coming
back, not the app rendering two ways.

**Convergence, measured.** Refresh sizes over four consecutive CI runs
on the fix branch: 61 → 43 → 83 → **2** PNGs. (The 83 is not a relapse:
the run before it WEDGED at ~700/1628 captures, so that pass was the
whole missed remainder catching up in one go — a wedged capture reports
"Story check failed and was not auto-resolved", which reads like a drift
verdict and is not one.)

The final two files are the mechanism above caught in the act, and they
are the proof rather than the exception:

| | `clock-face__crowd__md` | `clock-face__unknown__md` |
|---|---|---|
| branch, pre-merge | `6102ab620d` | `288bf5cb15` |
| main | `8d22e5245a` | `0f477d8d33` |
| after merging main | `8d22e5245a` | `0f477d8d33` |
| next CI capture | **`6102ab620d`** | **`288bf5cb15`** |

Two independent CI runs of the fixed code produced the SAME bytes for
each file; the only thing that moved them in between was the merge
importing main's pre-fix copy. Main will keep re-supplying those stale
baselines — and keep colliding with this branch on add/add PNG
conflicts — until this lands, which is the argument for merging it
promptly rather than letting it sit.

**`git checkout --ours` on a baseline conflict is NOT a safe default —
it re-landed the bug on main (2026-08-08).** Resolving the repeated
add/add PNG conflicts, "take ours, my side is the fixed-code capture"
looked obviously right and was wrong: after several merges from main,
the branch's OWN copy of a file was, for some paths, main's degraded
pre-fix bitmap that an earlier merge had imported. Applying that rule
blanket-wise put 11 pre-fix baselines (9 clock-face, 2 transport) onto
main, and main's next run failed with those exact 11 in its refresh
manifest.

The tell is that neither `--ours` nor `--theirs` is meaningful for a
PNG whose content is a *capture of code*: the only correct bytes are a
fresh capture of the merged tree. So on a baseline conflict, do not
pick a side — take either to get the merge committed, then let CI
re-capture and read the REFRESH MANIFEST (not the drift list) to see
what is genuinely stale. `.refresh-manifest.json` inside the
`story-images-fresh` artifact lists only files the check judged stale;
the console drift list is much longer because it also prints
sub-threshold jitter it tolerated. Recovering is `just
studio-story-pull` on the branch whose run captured them — and when the
run was on main (whose protected branch the bot cannot push to), that
script finds nothing because it keys off the current branch: download
the artifact from main's run and call `applyRefresh(freshDir,
baselineDir)` from `story-apply-refresh.mjs` directly, which applies
the same manifest semantics.

**Lesson** — A defect fixed in one component recurs when the mechanism is
re-implemented elsewhere; the class guard (`ux-box-sized-canvas` + gate)
only protects canvases that opt in. Any NEW imperatively-painted canvas
that stops repainting to be photographed must ship all three guards, not
just the time pin — "paint depends on two inputs, time and geometry, and
both must be pinned" (the earlier entry's deeper lesson, now violated
once and re-learned). Grep check when adding such a canvas:
`ux-box-sized-canvas` must appear beside every `data-preview-painted`
writer whose loop can stop.

**2026-08-17 addendum** — the committed-baseline delivery loop this entry's
recovery lore targets is gone (ADR
`2026-08-17-story-baselines-companion-repo.md`): oscillation can no longer
ping-pong commits or block merges. The mechanism (unpinned paint inputs)
is unchanged and now surfaces as phantom rows in PR story comments and in
the main-push run's delta-vs-parent step summary — that summary is the
standing monitor for this class. `studio-story-pull`/`applyRefresh`
recovery steps above are era history.
