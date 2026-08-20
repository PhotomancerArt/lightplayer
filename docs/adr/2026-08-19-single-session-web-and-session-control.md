# ADR: Single-session web policy and the session·project control

- **Status:** Accepted
- **Date:** 2026-08-19
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Studio's runtime pool models N attached device sessions plus one simulator
(`2026-08-03-studio-runs-n-device-sessions.md`: `DEVICE_SESSION_CAPACITY = 4`,
`SIM_SESSION_CAPACITY = 1`) — a deliberate SHAPE, "capacity is a policy,
never a shape" in that ADR's own words. The web chrome grew to match: a
session strip of chips (one per live session, D15/D16 of the
gallery-product vision), a ⋯-menu Sessions group, and a header project chip
that popped up the open project's save panel. Three session-shaped UI
surfaces, none of which the single-tab web app actually needed — a browser
tab can only ever be looking at one thing, so the strip's job (which of N
sessions is this?) had exactly one non-trivial answer on web: the one this
tab is on.

The vision conversation (2026-08-18, rulings R7/R8, recorded verbatim in
`~/.photomancer/planning/lp2025/2026-08-18-1858-session-project-control/notes.md`)
converged on retiring the strip in favor of ONE session per tab, with the
tab itself standing for the session — "want another? open another tab."
Direction was fully converged in the UX spike `spikes/studio-chrome/`
rounds 7–8.1 (commits `f92d5be8b`, `d3b79bb46`, `60ed318ff` on
`claude/youthful-payne-e11cb0`); this ADR records the shipped decision, not
the exploration. The G1 visual gate (2026-08-19, this plan's `notes.md`
"G1 verdicts") passed with one amendment folded in below.

## Decision

### One session per tab; the tab is the window manager

The web app runs **exactly one** runtime session at a time: one simulator
OR one connected device, never both. Opening a project or connecting a
device tears the other KIND of session down first (reusing the existing
`StopSimulator` / `DisconnectDevice` teardown verbs), enforced once at the
install funnel (`StudioController::install_session`) so every caller —
device connect, open-from-home, provider link, the docs example — inherits
it for free. An operation in flight refuses the install rather than
silently interrupting it, naming the operation (`InstallRefusal`).

**The pool's shape is untouched.** `RuntimePool` still models N device
sessions and keeps `DEVICE_SESSION_CAPACITY = 4`; this is a WEB UX policy
layered on top, not a model change. A desktop-app shell — which is not
one-tab-one-document the way a browser is — can lift the policy and let
its UI show more than one session again without touching the pool at all.
That is the concrete trigger for revisiting this decision (see
Follow-ups).

### Navigation is studio OR site

Leaving a lens route (a project or device URL) for anywhere else in the
app ENDS the tab's session — the same route-listener arms that already
dispatched `ProjectOp::DetachLens` on Devices/Projects now widen to every
site route, and the back button gets the same guard via `popstate`. This
is safe specifically because the draft overlay is durable: an edit that
was never saved survives on disk (or in the overlay mirror) regardless of
whether the runtime session that produced it is still alive, and boot v2
made re-entry to a project cheap. So leaving is never a confirm-sheet
question — it is silent-with-a-toast ("Simulator stopped — your edits are
saved as a draft.") on both idle and dirty. The ONLY thing that refuses a
nav-away is an operation in flight (a deploy or a flash): refusing there
and nowhere else is what makes "leaving never prompts on dirty" true
without also letting a click strand a hardware write mid-flash.

**Docs and Boards are the deliberate exception.** From a lens route they
open in a NEW tab (`NavTab`'s `new_tab` prop; the router's click
interceptor already skips `target`-bearing links, so the opt-out needed no
new plumbing) — reference material read while building a project should
never cost the tab you were building it in. Explore does not get this
treatment: it is a gallery of *live* projects, a real section of the app,
and going there is going somewhere (ruling R8-3, amended 8.1).

### The session·project control is the one session UI

The header grows one control, `SessionProjectControl`
(`lp-app/lpa-studio-web/src/app/layout/session_control.rs`): a segmented
lockup — kind glyph · status dot · device name · board suffix, then the
project's state glyph · name · unsaved count — that opens a panel (device
zone + the same `ProjectDetailSections` the pane's own [i] renders, so the
chrome and the pane can never disagree about a project). It replaces all
three retired surfaces at once: the strip (there is nothing to pick
between), the ⋯-menu Sessions group (same reason), and the header project
chip (its popup moved onto this control's panel).

**Every segment of the lockup opens the panel** (ruling R8-2): with one
session per tab the device segment has nothing to navigate *to* — you are
always on the one session there is — so the whole trigger is uniformly
"inspect", never "go to X".

**G1 amendment — Save/↺ stand apart from the lockup.** The shipped design
(F1, fix round landed at commit `6d2e0b2b7`) diverges from the spike's
literal concept B in one respect: Save and Revert render as ORDINARY
SIBLING BUTTONS beside the lockup, not as trailing segments inside it. The
first cut carried them inside, and the G1 gate read that as a box that
half-inspects and half-acts — odd on its own terms, and it also forced an
invalid button-in-button nesting through the popover trigger (a top-layer
popover's trigger subtree renders twice while open, so nothing stateful,
including a nested interactive element, may safely live inside it). Pulling
the actions out is a strict improvement on both counts: the lockup is
purely the popover trigger, and Save/↺ are honest top-level buttons to
assistive tech. They still dispatch the SAME
`ProjectEditorView.header_actions` the pane header and the (now-retired)
workbench Tree row used to — one save verb, and after this round exactly
one surface left that renders it outside the popup.

**The folds ride the pieces, not a wrapping gate.** Per the Q10/#426
lesson (`2026-07-05-studio-pane-grammar.md`'s "One ungated mount"
tradition, restated for this control), the control mounts UNGATED — no
`tw:@min-*` container around the whole thing, because a top-layer popover
cannot answer a container query and a second gated mount would give the
header two popovers disagreeing about which is open. Instead: the device
name (+board suffix) hides below the bar's own 900px width, and ↺ hides
below 680px — the same cuts the secondary nav tabs already fold at.

## Records

- **Pool shape/capacity are intentionally unchanged.** This ADR is a web
  policy amendment to `2026-08-03-studio-runs-n-device-sessions.md`, not a
  capacity change; that ADR's own words ("capacity is a policy, never a
  shape") are exactly what makes a policy-only amendment possible without
  touching `RuntimePool`.
- **A desktop-app future re-enables multi-session UX.** The model support
  (N-session pool, per-session teardown, the DTO-per-session projection
  code) all still exists; only the WEB shell's install funnel enforces
  "one at a time". A desktop shell is free to show the strip's UX again —
  or a `2026-07-05-studio-pane-grammar.md`-flavored per-session pane — on
  the same core.
- **Design record.** Direction converged in the UX spike
  `spikes/studio-chrome/` rounds 7–8.1 (commits `f92d5be8b`, `d3b79bb46`,
  `60ed318ff`); the full ruling-by-ruling record (R7-1 through R8-4, Q&A,
  and the G1 verdicts that produced the Save/↺ amendment above) lives in
  `~/.photomancer/planning/lp2025/2026-08-18-1858-session-project-control/notes.md`
  (planning archive, not the repo).
- **Vision supersessions.** D15/D16 of the archived gallery-product vision
  (the session strip and chips-as-places) retire for the web app with this
  ADR; D4 (the header chip's detail popup mounted as-is) is superseded —
  the panel reworks it wholesale rather than porting it; D43's "chips are
  wayfinding — never controls" boundary is amended for exactly the LENSED
  pairing (see the amendment notes on `2026-07-05-studio-pane-grammar.md`
  and `2026-07-26-card-view-state-ownership.md`). These vision documents
  live in the planning archive (Dropbox `Planning/lp2025/`), not this repo.

## Consequences

- Three session-shaped chrome surfaces collapse into one: `SessionStrip`,
  the ⋯-menu Sessions group, and `ProjectHeaderChip` are all deleted
  (`UiStudioView.sessions` — the Vec projection with exactly one reader —
  is removed with them; `UiStudioView.session` is the single-DTO
  replacement). The workbench Tree panel's Save/Revert row retires too —
  the header control is now the save moment's one home; the Tree panel
  keeps only the Debug-active chip, which is not a save affordance.
- A stale "unsaved changes are discarded" toast on `StopSimulator` is
  fixed as part of this work: under the durable-draft story that copy was
  simply wrong.
- The nav-away in-flight guard is untriggerable on a sim-only session (a
  simulator never puts an operation "in flight" the way a flash does) —
  by design; exercising it needs a device deploy/flash, and G1 verified it
  there.
- A pre-existing, now load-bearing gap: a REJECTED or oversize asset-body
  edit (`persisted == 0, failed > 0`) shows in the editor as "Unsaved" and
  paints the header's error tint, but the header offers no count and no
  Save/Revert (`project_header_actions` gates on `persisted > 0`). This
  was equally true of the retired header chip; it becomes load-bearing now
  that the control is the ONE save surface. Filed as debt, not fixed here
  (product call pending — see
  `docs/debt/failed-only-asset-edit-header-blindness.md`).

## Alternatives Considered

- **Keep the strip, shrink it to fit one tab.** Rejected: a strip that can
  only ever show one chip is a list widget pretending to be a list; the
  segmented lockup says the same information without the pretense, and it
  is what let Save/Revert have a natural home beside it.
- **Trailing Save/↺ segments inside the lockup (the spike's literal
  concept B).** Shipped first, rejected at G1: reads as a box that
  half-inspects and half-acts, and forces invalid button-in-button
  nesting through the popover trigger. Superseded by the sibling-buttons
  layout this ADR records.
- **A `tw:@min-*`-gated wrapper for the whole control, folding as one
  unit.** Rejected on the same grounds as the Q10/#426 lesson it repeats:
  a top-layer popover cannot answer a container query, so a gated wrapper
  needs a second mount, and two mounts is two popovers.
- **Confirm-sheet on nav-away with unsaved work.** Rejected (ruling R8-4):
  the draft overlay already makes leaving safe; a confirm sheet would be
  friction for a risk that does not exist. Refusal is reserved for the
  one case where leaving really would lose or corrupt something — an
  operation actually in flight.

## Follow-ups

- **Desktop-app multi-session UX.** Model support stays; only the web
  shell's install funnel enforces the one-session cap. **Revisit when** a
  desktop-app shell is built and wants to show more than one session at
  once.
- **Failed-only header blindness.** See
  `docs/debt/failed-only-asset-edit-header-blindness.md` — whether to
  surface `failed`-only edits in the header count/actions is an open
  product call, not resolved by this ADR.
