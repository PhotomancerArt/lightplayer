# ADR: Device card state vocabulary as a first-class concept

- **Status:** Accepted (amended 2026-07-24 — see the M7′ note below;
  annotated 2026-08-04 — see the state-count note below)
- **Date:** 2026-07-16
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None (the vocabulary stands; only the circle
  presentation is superseded — see the M7′ note)

> **M7′ amendment (2026-07-24).** The state VOCABULARY is intact and
> normative. The status-circle PRESENTATION is retired: the card's
> tinted LEFT EDGE carries the same grammar (solid→filled 3px edge,
> hollow→double-line faded "remembered", pulsing→edge-color pulse
> "working"), per the card-as-control-panel replan (D39;
> `Planning/lp2025/2026-07-24-m7m8-replan/agenda.md`). The core spec
> renamed to match its generality: `RosterCircle`/`RosterCircleShape` →
> **`RosterStateSpec`/`RosterTreatment`** (`roster/roster_state_spec.rs`,
> `RosterCardState::spec()`) — still renderer-independent, still the
> derivation an on-device LED would consume.

> **State-count note (2026-08-04, project-format-upgrades P5).** The
> vocabulary grew past 14 with `RosterCardState::HoldsOldFormatProject`
> (a readable project whose format this build cannot run). The "14" below
> is a historical fact about the direction doc that founded this ADR, not
> a live count — `roster_card_state.rs` is the source of truth for how
> many states exist today. The doc comments elsewhere that used to quote
> "14-state" now say "the roster card-state vocabulary" instead, so
> nothing else needs re-editing at the next addition.

## Context

Studio's gallery used to launder device state through booleans and ad-hoc
card enums: `DeviceState::Unresponsive`, `Gone`, and `BlankFlash` collapsed
into generic chips, `ServerState` booleans decided what the deploy dialog
said, and each surface (device card, node pane, detail popup) invented its
own dots and copy. The 2026-07-16 device-UX direction
(`Planning/lp2025/2026-07-16-device-ux/direction.md`, "Card grammar")
settled a normative 14-state vocabulary for roster cards — state × status
circle × status line × affordance — and noted the vocabulary is
transport-independent: the same states should later be able to drive the
on-device built-in LED (blink patterns for needs-setup / degraded /
not-responding) and richer boards' displays.

## Decision

The card-state vocabulary is a first-class concept module,
`lpa-studio-core/src/app/roster/`, not a rendering detail of any one
surface:

- **`RosterCardState`** (`roster_card_state.rs`) has exactly one variant
  per direction state-table row (14 states). It owns the status-line copy
  and the ≤1 sub-line, so every renderer says the same thing. It is free
  of web/UI types — renderable to DOM, LED patterns, or a display.
- **`RosterCircle`** (`roster_circle.rs`) is the indicator spec: shape
  (solid = live / hollow = remembered / pulsing = working) × the existing
  `UiStatusKind` health families. Shape and motion carry meaning without
  color. One circle per card, showing the worst ACTIONABLE state;
  secondary facts (firmware drift on a running row —
  `firmware_update.rs`) demote to chips.
- **`RosterAffordance`** (`roster_affordance.rs`) carries each row's one
  affordance as an identity; action wiring lands with the flows that make
  each state real (M3 card anatomy, M6 auto-connect, M8 provisioning).
- **`derive_roster_card_state`** (`roster_evidence.rs`) is a pure, total
  function of evidence — link `DeviceState`, connect-as-pull
  `DeviceContent`/`SyncRelation`, registry entry, connect-flow/operation
  narration — with no assumption that a live session exists. Its module
  doc is the normative mapping. Evidence may come from a live session,
  the registry, or future discovery; absence of evidence is itself
  evidence (registry-only = offline).

The vocabulary is complete even where the substrate is not: `Degraded`
has no live signal yet (Q7) and the retry-ladder states become reachable
with M6 — the variants and their stories exist now so the grammar never
grows ad-hoc side channels.

## Consequences

- Cards, panes, and popups converge on one honest state chart; adding a
  surface means writing a renderer, not re-deriving state.
- Copy changes happen in one place (the view-model), and the story sheet
  (`lpa-studio-web/src/app/roster/roster_card_stories.rs`, one story per
  state row) is the visual gate for the whole grammar.
- The future LED/display renderers consume `RosterCardState` +
  `RosterCircle` directly; nothing in the concept module needs to move.
- Until M3 rewires the live gallery, the legacy `UiDeviceCardState` and
  `deploy_environment()` booleans still drive the live feed alongside the
  new derivation; they are scheduled for retirement, not coexistence
  (`UiDeviceCardState` in M3; the deploy booleans die with the dialog in
  M8).

## Alternatives Considered

- **Keep it a UI view-model detail inside `app/home/`**: cheapest, but
  the direction explicitly plans non-UI renderers (LEDs), and home is a
  web-gallery concept; the vocabulary outlives any one surface.
- **A standalone crate**: premature — the states reference studio-core
  concepts (`DeviceContent`, registry entries) and no second consumer
  exists yet. A module boundary is enough; a crate can be split out when
  a firmware/LED consumer actually appears.
- **Deriving directly from `ConnectFlowState`**: rejected; the flow enum
  narrates the picker/open sequence and carries UI types. The derivation
  takes a small `ConnectEvidence` vocabulary instead, which the retry
  ladder (M6) and D32 auto-connect will feed.

## Follow-ups

- M3: full card anatomy renders the vocabulary; the live gallery derives
  cards through `derive_roster_card_state` fed by single-session
  evidence, and `UiDeviceCardState` retires.
- M4: the runtime pool becomes the evidence source (no visual change).
- M6: retry ladder + auto-connect feed `ConnectEvidence` for real.
- Post-MVP: on-device LED renderer for the same states.
