# ADR: The rich-object pattern — sections, rollup, surfaces

- **Status:** Accepted
- **Date:** 2026-07-17
- **Deciders:** Photomancer
- **Supersedes:** None (builds on
  `2026-07-16-device-card-state-vocabulary.md`)
- **Superseded by:** None

## Context

Studio kept rebuilding one anatomy: *a thing with health, a compact
indicator, a place to see everything about it, and one most-important
action*. Three surfaces converged on it independently — the node pane
(merged `UiAffordance` → detail-trigger glyph + header wash + sectioned
detail popover), the SlotPane value hero, and the roster cards (the
2026-07-16 card-vocabulary ADR's 14-state grammar with its own
circle/status/affordance derivation). Each surface re-derived "what is
the one status?" and "what is the one action?" on its own.

A design spike (P1 note, P2 story sheet,
`Planning/lp2025/2026-07-17-rich-object-pattern/`) explored the shared
model. The P2 gate settled five questions:

- **Q1** — the detail trigger on cards is the **node-style
  affordance-following icon on the right** of the header (the quiet "i"
  that escalates with the raised state), not the status circle. The
  circle stays a pure indicator on the left.
- **Q2** — the card keeps its rendered primary-affordance **button**.
- **Q3** — the generalized pane header must be the **real node header**,
  not an approximation: node story baselines are the fidelity test.
- **Q4** — sections render in **fixed schema order** ("users learn where
  things are"), never worst-first.
- **Q5** — the danger zone is an **inline red-tinted section behind a
  hard red separator**, always visible, never collapsed.

## Decision

**A rich object is: pane + status + detail-with-sections + content.**
The model is a first-class core concept
(`lpa-studio-core/src/app/rich_object/`):

- **`RichSection<A>`** — title, tone (`UiStatusKind`), label→value fact
  lines, optional advisory chip, affordance identities (generic `A`;
  wiring to concrete actions is the renderer's job), and a rollup
  **weight**: `Advisory | Actionable | Danger`. Advisory/Actionable
  sections carry ≤1 affordance; Danger sections may list several
  destructive rows.
- **`RichObjectView<A>`** — the ordered section list plus the derived
  rollup, under two rules shared by every surface:
  1. the object's **indicator tone = the worst ACTIONABLE section's
     tone** (Error > Warning > Working > Good > Neutral; ties resolve to
     the earlier section, so schema order is also precedence);
  2. the object's **primary affordance = that same section's
     affordance**.
  `Advisory` facts (firmware drift) may tone a chip but never the
  indicator; `Danger` is always present and never shouts (Q4/Q5).
- **The device is the second first-class consumer**
  (`app/roster/device_rich_object.rs`): fixed schema **Health, Project,
  Technical, Performance, Backup, Danger zone** (danger pinned last).
  Health IS the card derivation — its tone and affordance come from
  `RosterCardState` (`derive_roster_card_state`), so popover and circle
  can never disagree. Sections without data are omitted; their schema
  slot never moves (Performance waits for runtime stats; Backup appears
  when a diverged copy was banked at connect). Danger holds flash+erase
  on manageable live links, forget on offline registered cards — the
  permanent home the interim More-menu rows migrated into, and where
  M8's provisioning entries land.

Surfaces are **renderers over the one model**:

- **Card** (`app/home/device_card.rs`): circle indicator (pure), status
  line, ≤1 sub-line, the rendered primary-affordance button (Q2), and
  the detail trigger at the header's right edge (Q1) — a `DetailPopover`
  icon following the rollup tone via `status_trigger_style`
  (`app/affordance.rs`): quiet "i" for Neutral/Good, toned "i" for
  Working/Warning, red warning glyph for Error. The node's pencil stays
  node-only (edits are a node concept).
- **Detail popover** (`app/home/device_detail_popover.rs` over
  `core/rich_detail.rs`): identity section, then sections in schema
  order via `RichDetailSection`; Danger renders per Q5 with destructive
  menu rows (`UiAction`s with confirmations). The P2 copies are
  exported: `base::detail_popover_card_class`,
  `core::menu_item_destructive_action_class`.
- **Pane** (`app/layout/rich_object_pane.rs`): `RichObjectPane` pins the
  node pane's real header composition on `StudioPane` (Q3) — rollup
  tone washes the header, **no header chips** (the affordance-following
  detail trigger is the whole announcement, P6), detail slot at the
  right edge. The node pane is its first consumer, pixel-identical; the
  M7 runtime pane renders the device rich object through the same
  header.

Nodes keep their glyph vocabulary (`UiAffordance` → pencil/warning);
what they share is the merge/rollup shape and the header anatomy.

## Consequences

- One derivation answers "what tone, what action" for every surface;
  adding a surface means writing a renderer, not re-deriving state.
- The device card's More-menu is gone: destructive verbs live in the
  danger zone of the detail popover, which exists on every non-sim card
  (health detail is always one click away, even mid-operation, when the
  danger zone itself is withheld).
- The 2026-07-16 card-vocabulary ADR stands: `RosterCardState` is now
  the Health section builder and the card-surface derivation, no longer
  the top of its own private hierarchy.
- Roadmap consumers: the M7 runtime pane becomes "render the device
  rich object in a `RichObjectPane`"; M8's provisioning entries live in
  the danger zone; the bus/slot panes are follow-up candidates (noted,
  not built).
- The story sheet grows the popover gates
  (`device_detail_running_behind`, `device_detail_offline`); the P2
  exploration module remains as the spike record, now consuming the
  exported classes it had to copy.

## Alternatives Considered

- **Circle-as-trigger on cards** (P2 centerpiece): rejected at the gate
  — an 8px circle is a poor target, and the node vocabulary already has
  a trigger concept worth keeping ("I see no reason to reinvent the
  concept").
- **Worst-first section ordering**: rejected (Q4) — a stable map beats
  a shouting one; the rollup already surfaces the worst state on the
  trigger and indicator.
- **Collapsed danger-zone summary row**: rejected (Q5) — the inline
  tinted section is honest and still quiet (Danger never colors
  rollup).
- **A `RichSection` with exactly one affordance**: rejected — the
  danger zone honestly lists multiple destructive verbs; the ≤1 rule
  matters only where rollup reads, so it is documented per-weight
  instead of typed.
- **Literal markup extraction of the node header out of `StudioPane`**
  (Q3): unnecessary — the node header's markup already *is* the shared
  `StudioPane`; what was missing (and what P2's drift proved) was
  codifying the node's composition of it. `RichObjectPane` pins that
  composition with zero DOM change, which is exactly what the
  pixel-identity requirement demands.

## Follow-ups

- Plumb the packaged firmware manifest (`build.sourceCommit`) into the
  gallery so `BundledFirmware` evidence reaches live cards and the
  update chip appears outside stories.
- Performance section: feed `ProjectRuntimeSummary` to roster cards.
- Backup section: a real "download copy" flow (no dead buttons until
  then).
- M7: runtime pane = device rich object in a `RichObjectPane`; M8:
  provisioning entries in the danger zone.
