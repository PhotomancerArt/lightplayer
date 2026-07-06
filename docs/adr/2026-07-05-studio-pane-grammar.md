# ADR: Studio Pane Grammar

- **Status:** Accepted
- **Date:** 2026-07-05
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Before milestone M2a, every Studio editing surface drew its own header
chrome: node panes had a bespoke `NodeHeader` with a status button and an
upper-right select stopgap, the M2 save strip was a one-off shell-mounted
widget with custom Save/Revert buttons, and the device pane grew its own
arrangement independently. Each new surface re-decided where status lives,
how dirty state shows, and how actions are wired — and the dirty counts the
save strip displayed (`ProjectDirtyCounts`) were computed in a separate pass
from the per-field dirty affordances, inviting drift.

M2a asked whether one pane grammar could serve all of these. The abstraction
had an explicit acceptance test (plan
`2026-07-05-editing-chrome-unification`): the component would be built
against the node pane (P2/P3) and then had to be adopted by the project
header (P4) **without modification** — if P4 needed component surgery, no
ADR would be written. P4's verdict: `StudioPane` survived unmodified; the
project header mapped onto the existing slots. This ADR records the grammar
that passed.

## Decision

### Anatomy

One pane layout component, `StudioPane`
(`lp-app/lpa-studio-web/src/app/layout/studio_pane.rs`), renders every
editing pane as a header row over an optional body:

```text
[collapse?] [primary] [title/kind] [state chips] … [actions] [trailing] [detail]
--------------------------------- body ---------------------------------
```

- **collapse** — optional leftmost rail toggling the body; the consumer owns
  the collapsed state (`PaneCollapse`).
- **primary** — primary-affordance element slot, left of the title: the node
  selection toggle, a status icon, whatever the surface's one primary
  control is.
- **title / kind** — pane identity text; the title truncates, everything
  else keeps its width.
- **state chips** — toned pills after the title, fed by the chrome struct.
- **actions** — contextual [`UiPaneAction`]s rendered as icon buttons.
- **trailing** — free-form header extras between the actions and the detail
  popup (node tabs, the legacy upper-right select control). This slot was
  **not** in the planned anatomy; it was added in P2 because tabs and the
  legacy select control needed a home in the header that the pane could host
  without domain knowledge. It is the anatomy's escape hatch — new chrome
  should prefer the named slots and treat `trailing` as a porting aid.
- **detail** — detail-popup slot at the header's right edge, by convention a
  `DetailPopover` (the shared popup base under both the slot detail popup
  and the project popup).
- **body** — `None` renders a header-only pane (the same shape a collapsed
  pane folds to).

### Layout, not config

`StudioPane` is a **layout** component: slots, placement, and spacing only.
It imports no node, project, or device types. Everything it draws that is
not an element slot goes through one neutral chrome struct:

```rust
PaneChrome { tone: PaneTone, accent: bool, chips: Vec<PaneChip> }
PaneTone   { Neutral, Working, Good, Live, Warning, Error }
```

Consumers map their domain state onto it — `UiStatusKind` → `PaneTone`,
`DirtySummary` → chips, focus → `accent`. The tone vocabulary is
deliberately semantic (Warning = needs attention/unsaved, Live = transient,
Error = failed), matching the D6 color language (yellow/amber = unsaved,
blue = live) used at every level from field affordances to the sidebar tree.

The alternative — a config struct with domain-aware props (`node:
UiNodeView`, `project: ...`) — was rejected: it makes the component a
switchboard that must grow a case per consumer, which is exactly the drift
the grammar exists to stop.

### Actions are controller-produced data

Header actions arrive as `UiPaneAction` values
(`lpa-studio-core/src/core/action/pane_action.rs`): an icon token plus a
wrapped `UiAction`. Label, summary, primary emphasis, and enablement are
**not** duplicated on the DTO — they delegate to the wrapped action's
`ActionMeta`, so the action stays the single source of its metadata. The
pane renders one icon button per entry and dispatches the wrapped action
through the ordinary `on_action` conduit; it never knows the concrete
operation.

Concretely: the project controller emits Save / Revert-to-saved on
`ProjectEditorView.header_actions` (present iff persisted edits are
pending), and the web crate contains zero bespoke Save/Revert button wiring
— the M2 save strip's custom buttons are gone with the strip.

### Dirty bubbling feeds the chrome

Dirty chrome is fed by `DirtySummary { persisted, transient, failed }`
(`lpa-studio-core/src/app/project/dirty_summary.rs`), aggregated
slot → node → project inside the same DTO-build walk that computes the
per-field dirty affordances — one aggregation, so the affordance on a node
header, the tint on a sidebar tree item, and the project popup's counts can
never disagree with the fields. Counts render only inside the detail popups
(node and project); headers and tree rows carry no count chips — they wear
the merged affordance below. (`PaneChip` remains part of the pane's neutral
vocabulary for consumers that need text chips; the dirty-state chips the P3
node header briefly rendered were superseded by the affordance model.)

### Affordance model

Every hierarchy surface — node header, sidebar tree row, project pane —
announces its state through exactly **one affordance** (glyph + tone),
`UiAffordance` (`lpa-studio-core/src/app/project/ui_affordance.rs`):

- **Vocabulary and priority** (later wins the merge):
  `Info` < `Busy` < `Live` < `Unsaved` < `Error`.
- **Priority merge, computed in core.** Each level's affordance is the max
  of its own status projection and its subtree `DirtySummary` projection
  (`UiAffordance::merged(UiStatusKind, &DirtySummary)`); children's edits
  arrive through the bubbled summary. One function feeds every surface —
  the same cannot-disagree principle as `DirtySummary` itself. The web side
  has exactly one affordance → glyph/tone table
  (`lpa-studio-web/src/app/affordance.rs`): quiet "i" for `Info`,
  working-toned "i" for `Busy`, the edit pencil in blue/yellow for
  `Live`/`Unsaved`, the red warning glyph for `Error`.
- **Render rule.** The affordance renders only on the detail trigger (node
  header, project pane) or as the tree row's small indicator — and the tree
  shows it **only when announced** (not `Info`). All text — status words
  ("Ready", "Running"), dirty counts — lives in popups (or, for tree rows,
  the tooltip). Announced affordances also drive the header wash tone;
  a silent pane falls back to its status tone.
- **OK is not announced.** `Good` and `Neutral` statuses project to `Info`:
  no checkmark, no status-colored trigger. A healthy, clean surface is
  silent chrome.
- **Busy is activity, not "running".** `Busy` maps only from genuine
  in-flight work — a `Working` status (sync/save/provision/connect) or
  buffered edits awaiting their ack. Steady-state "Running" is a `Good`
  status (domain data, shown in the popup), never `Busy`. `Warning`
  statuses join `Error` in the attention class (the affordance says "needs
  attention"; the popup's status pill keeps the warn/error distinction).

The slot rows' `UiSlotAffordance` (the model's reference implementation)
keeps its richer slot-level vocabulary (`Bound`, `Invalid`, …); the
hierarchy type is deliberately separate because the two levels announce
different things, but both follow the same grammar: one priority-merged
affordance on the detail trigger, text in the popup.

### Consumers

Adopted by the node pane (P3: selection toggle in `primary`, tabs in
`trailing`, merged status/dirty popup in `detail`) and the project pane
(P4 + the M2a gate-feedback round: the whole project card is one pane —
project name as title, controller actions, node tree as the body; the
status word, dirty counts, and project stats live in the detail popup,
and the merged affordance drives the trigger and the header wash).
**The device pane is the intended third consumer**: its header should map status onto
`PaneTone`, its detail popup onto `DetailPopover`, and its contextual
operations onto `UiPaneAction`s the next time device-pane UX work is
scheduled (Follow-ups).

## Consequences

- New editing surfaces get chrome by mapping onto slots, not by drawing
  headers; UX consistency is structural.
- The acceptance test held: the project header adopted the pane with zero
  component diff, so the slot set is evidently at the right altitude for at
  least two very different consumers (full editing pane vs header-only
  strip).
- Dirty counts have one source; deleting `ProjectDirtyCounts` removed the
  two-types drift risk permanently.
- The pane's neutrality means chrome decisions (tones, chip shapes, action
  buttons) are made once and inherited everywhere — but it also means a
  consumer needing genuinely new chrome must extend the neutral vocabulary
  rather than special-casing, which is intended friction.
- `trailing` is an acknowledged soft spot: it can hide things that deserve
  named slots. Watch for repeated patterns living there (tabs may earn a
  named slot if a second consumer grows them).
- The save strip's shell mount died with the strip: Save/Revert now live on
  the project header, which scrolls with the sidebar instead of being
  always-visible (Follow-ups).

## Alternatives Considered

- **Status quo (bespoke headers per surface).** Rejected: three surfaces
  had already diverged in status placement, dirty display, and action
  wiring; every new surface re-litigated the same decisions.
- **Domain-aware pane component (config props per consumer).** Rejected:
  grows a case per consumer, re-couples layout to domain types, and would
  have failed the P4 unmodified-adoption test by construction.
- **Custom Save/Revert buttons on the project header.** Rejected (D5):
  actions-as-data through the existing `UiAction` conduit keeps the web
  crate free of per-operation wiring and lets any pane gain contextual
  actions without new render code.
- **Duplicating label/enablement onto `UiPaneAction`.** Rejected in P1:
  `ActionMeta` already carries them; duplication invites divergence between
  the header button and the same action rendered elsewhere.
- **A separate dirty-counts pass for header display.** Rejected (M2's
  `ProjectDirtyCounts` was exactly this); aggregation moved inside the one
  `SlotEditJoin` walk instead.

## Follow-ups

Per the deferred-decision convention, these are indexed in
`docs/adr/README.md`.

- **(a) Device-pane adoption.** The device pane still draws pre-grammar
  chrome. Map it onto `StudioPane` (status → `PaneTone`, popup →
  `DetailPopover`, operations → `UiPaneAction`). **Revisit when** the next
  device-pane UX work is scheduled.
- **(b) Save visibility while scrolling.** The M2 save strip was
  shell-mounted and always visible; the project header that replaced it
  scrolls with the project sidebar, so a long tree can push Save off
  screen while dirty. Candidate fixes: sticky-position the header within
  the sidebar scroll container, or float a minimal dirty indicator when
  the header is scrolled out. **Revisit when** the M2a UX gate or later
  real use flags losing always-visible Save as a problem.
- **(c) Tint-variant loser's story removal.** Both D7 dirty-tint variants
  (header-only, the live default, vs full-surface) ship as stories for the
  user's pick at the M2a gate; the pick has **not** happened yet. Once it
  is recorded, delete the losing variant's stories (and, if header-only
  wins, consider whether `NodeDirtyTint` itself simplifies away) or note
  their deliberate retention. **Revisit when** the tint pick is recorded in
  the M2a plan notes.
