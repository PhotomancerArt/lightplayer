# ADR: Device card UI view-state is core-owned

- **Status:** Accepted
- **Date:** 2026-07-26
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The device card is the device's control panel (M7′, D39–D43): a tabbed
surface with card-resident sheets (D41) that can grow into the editor's
right-side pane (D43). Three pieces of *view*-state describe what a card
is currently showing:

- the selected tab (`DeviceCardTab`),
- the open card-resident sheet, if any (confirm / name / drift /
  troubleshoot), and
- the in-place progress of a heavy op (flash / erase / reset).

This state lived in the web renderer's `use_signal`s (`device_card.rs`),
with `initial_tab`/`initial_sheet` props for story captures. That had
three costs:

1. **Amnesia.** A signal dies with its component instance. Growing a card
   into the editor pane (D43), or a session replace, dropped the selected
   tab and any open sheet — the panel forgot what you were looking at.
2. **Untestable.** e2e could dispatch the underlying action but could not
   drive the *view* past the dispatch boundary — no test could open a
   card, land on a confirm sheet, and assert it, because the sheet only
   existed inside a web signal the headless core never saw.
3. **No shared identity.** The scene-direction fork's `view-transition-name`
   needs a stable per-card key; the card view-state needs the same key to
   follow the device. Two independently-derived keys would drift.

The confirm sheet compounded (2): the web enum carried a fully-wired
`UiAction` (`DeviceCardSheet::Confirm(UiAction)`), so the "open sheet"
state was a boxed `dyn` op — not `Eq`, not a plain value, impossible to
hold in core state or assert on.

## Decision

Card UI view-state is **core-owned**, in
`lpa-studio-core/src/app/home/card_ui_state.rs`:

- **`CardUiState { tab, sheet, op }`** rides `UiDeviceCard.ui`, keyed by a
  single canonical **`UiDeviceCard::identity_key()`** (uid → reserved
  sim token → name fallback; `render_key()` delegates to it). The
  controller keeps a `card_ui` map by that key and overlays the saved ui
  — plus the live session's `operation_label` + percent as `op` — onto
  every home/lens card at view time. The state follows the *device*, so
  it survives the card ⇄ pane growth and a session replace.
- **`HomeOp::CardUi(CardUiOp)`** carries the pure, synchronous view-state
  flips (select tab / open sheet / close sheet). The renderer dispatches
  these like any other action; there is no web-local mutation.
- The sheet state is a **plain value**: `CardSheet { Confirm(CardVerb),
  Name, Drift, Troubleshoot }` with `CardVerb { Erase, Forget, StopSim,
  PushDrop{key}, Flash }`. The renderer maps **verb → wired action at
  draw time** (`verb_to_action`), so core state never holds a boxed op
  and every sheet is `Eq`-comparable and e2e-assertable.
- `initial_tab`/`initial_sheet` props are retired; stories drive the same
  `card.ui` the live app does.

The **identity_key()** is the one canonical key: it is both the `card_ui`
map key and the value the scene-fork's `view-transition-name:
card-{key}` consumes (the fork switches its `render_key()`-derived
`vt_name` over to it).

**Amendment 2026-08-19.** The single-session web policy
(`docs/adr/2026-08-19-single-session-web-and-session-control.md`) reuses
this ADR's `identity_key()` as the header session·project control's own
session key — the same key that names a teardown target
(`DeviceTarget::card`) now also names which card's `CardUiState` a
session belongs to, so the header's leave-the-studio teardown and the
card's own state can never point at different boards. It also extends
this ADR's D43 boundary the same way `2026-07-05-studio-pane-grammar.md`
records: the LENSED pairing's one remaining chrome surface (the header
control) carries Save/Revert and opens a rich panel, where D43's
original "chips are wayfinding" rule would have forbidden it. Non-lensed
session chips do not get the exception — they retired instead.

## Consequences

- **Drivable.** e2e opens a card, dispatches `CardUi(OpenSheet(...))`,
  and asserts the projected sheet — the D41 sheet flows are testable end
  to end. The P3 editor-sever tests rely on this.
- **Durable.** Tab + sheet survive the D43 grow/shrink and session
  replace, because the state is keyed by the device, not the widget.
- **One source of truth.** No web signal shadows the core value; the
  renderer reads `card.ui` and dispatches. Stories exercise the real
  path.
- **Cost: a verb→action map.** The renderer reconstructs a confirm
  sheet's `UiAction` from its `CardVerb` (five verbs, one match). This is
  the deliberate price of keeping core state a plain value; it also keeps
  the "one-click provisioning is not a confirm verb" boundary explicit
  (SetUp/UpdateFirmware carry no verb — they dispatch directly).

## Alternatives Considered

- **Keep it in web signals.** Rejected: the amnesia and untestability
  above are exactly what the state audit
  (`Planning/lp2025/2026-07-25-ui-state-audit/`) set out to remove.
- **Two-way sync (seed signals from core, mirror back).** Rejected: two
  writers, race-prone, and it still can't be the single source the audit
  wanted.
- **Store the `UiAction` in core `CardSheet::Confirm`.** Rejected: a
  boxed `dyn ControllerOp` is not a plain, `Eq`, comparable value; it
  can't be held in `CardUiState` (which derives `Eq` for `HomeOp`) nor
  cleanly asserted. The verb indirection is the fix.

## Follow-ups

- The **node-arm** UI signals stay in the state audit's later wave; this
  ADR covers the device card arm only.
- The **in-card setup FORM** (a blank device's tab as a date-named setup
  form) is not yet built — only the one-click provision dispatch landed.
- **Scene-fork coordination:** switch the fork's `vt_name` to
  `identity_key()` before it lands (branch `claude/spike-view-transitions`).
- The **post-reset auto-reconnect** tuning and the stuck-state root cause
  need a hardware re-test; the editor-sever/return-to-gallery half (P3)
  is in and tested against the fake device.
