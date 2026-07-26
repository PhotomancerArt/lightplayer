# ADR: Heavy device ops are card-owned flows with an explicit phase machine

- **Status:** Accepted
- **Date:** 2026-07-26
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-07-26-card-view-state-ownership.md`; the ratified
  state-flow model (`Planning/lp2025/2026-07-26-device-stateflow/model.md`)

## Context

Heavy device operations — flash, erase, factory reset — sever the very
session that used to narrate them. Progress was a session attribute
(`RuntimeSession::operation_label`), so the moment an erase dropped the
serial link the overlay vanished, the card fell back to
remembered/offline, and the user was stranded on "Forget device" until a
page refresh (2026-07-26 hardware walkthrough). The disconnect these ops
cause is not an error: it is a *scheduled step* of the operation.

## Decision

An op is a **flow owned by the card's controller state**, not by the
session:

```
CardOp { label, percent, phase }
phase: Running | AwaitingDevice | Failed { error, exit_label }
```

- The controller installs the flow at dispatch
  (`StudioController::device_card_op`, keyed by the managed device's
  stamped uid; an identity-less blank board rides the live card) and is
  the only writer that clears it.
- The manage event sink feeds it: `DeviceEvent::Progress` keeps it
  `Running` with the live esptool label/percent;
  `DeviceState::Booting` mid-manage flips it to `AwaitingDevice` — the
  op's expected disconnect, worn as reconnect narration while the
  ladder re-attaches.
- Landing (successful server reattach) clears the flow; the card
  re-derives its state and the Status tab announces the next step.
- Any failure renders `Failed` on the card with **one exit to the
  nearest stable state** (`CardUiOp::ClearOp`, e.g. "Back to set up") —
  no in-place Retry, no silent fallback, never refresh-to-recover.

Invariants (model §2): **I1** session death never clears a flow; **I2**
expected disconnects are modeled edges that arm reconnection; **I3**
landing announces; **I4** failure has exactly one exit; **I5** cancel is
per-op policy (push = yes, flash = until-committed, erase = no) —
deferred until `lpa-link`'s manage grows cancel plumbing.

## Consequences

- The erase dead-end class is structurally impossible: the overlay is
  driven by state that outlives the link.
- The renderer stays dumb — three phase renderings of `card.ui.op`.
- One slot suffices while the pool holds one hardware session; the
  key-by-uid shape is ready to become a map when the pool grows.
- Session `operation_label` remains as the derivation input for
  `RosterCardState::OperationInFlight` (edge treatment) and for ops that
  don't sever (push); the card-owned flow wins when both exist.
