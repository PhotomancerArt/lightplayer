# ADR: Honest device preview — the ▶ play tab renders actual device frames

- **Status:** Accepted
- **Date:** 2026-08-06
- **Deciders:** Photomancer
- **Supersedes:** the D12 hero strip (gallery-rework P05) as the device
  card's preview surface
- **Superseded by:** None

## Context

The device card's hero strip re-simulated the running project in the
browser and hung the result under the title bar, where it read as a
picture OF THE DEVICE. The 2026-08-05 gallery G2 gate ruled that
dishonest: nothing on screen distinguished "what the board is doing"
from "what a browser simulation of the same project looks like", and the
strip letterboxed besides. A UX spike
(`spikes/device-card-live-fixture/`, PR #359) converged the same day on
a ▶ **play tab** rendering the fixture's lamps from frames read off the
device.

Constraints that shaped the mechanism:

- The device already publishes every finished frame: `OutputNode`
  copies its control samples into a `RuntimeBuffer` (kind
  `OutputChannels`) each tick. Reading THAT is free of render cost.
- The pre-existing `ControlProductProbeRequest` re-renders the fixture
  per probe inside the render tick — usable for editor probes, wrong
  for a card feed that polls forever.
- UART serial is ~90 KB/s shared with all protocol traffic: a
  1500-lamp dome frame (~12 KB encoded) yields ~4–5 fps; a 300-lamp
  strip 10+. Push/streaming would monopolize a mode-exclusive wire.
- Non-lens device sessions previously ran NO wire ops between 2 s
  heartbeats; a card feed is a new pull class and had to declare an
  `ActionClass` (client-pull-loop ADR).

## Decision

**Read the published buffer; never re-render.** Wire proto v12 adds
`ProjectProbeRequest::OutputFrame` — a pull-only read returning each
output node's ALREADY-published bytes plus interpretation metadata
(sample layout, display layout with an `IfChanged` revision gate,
chunked for bulk). The card feed
(`lpa-studio-core` `card_feed.rs` + `run_due_card_feeds`) pulls on a
completion-gap cadence (`DEVICE_CARD_FEED_INTERVAL`, 150 ms gap measured
from each pull's completion, so big frames self-throttle) and ONLY
while the card's ▶ tab is effectively selected on an answering session
(Q3: tab selection is the visibility gate; there is no second
"surface visible" flag). Dome-scale display layouts the wire refuses
are synthesized client-side from the library's copy of the project,
cached per (uid, content hash).

**The sim card rides the same feed** (G1 ruling, overturning the
spike's Q5 which had kept the sim's ▶ as a browser re-simulation): the
in-proc sim session speaks the same server protocol, so its ▶ shows
the frames the sim engine actually published — as real as a board's —
wearing a violet SIM pill as identity dress over the shared
live/stale/offline/waiting states. The browser re-simulation canvas
left the play tab entirely (project thumbnails keep theirs).

**Post-gamma colors ARE the honest view** (Q2): the published buffer
holds what the board drives onto the wire. Gamma is lossy to invert,
and "what it looks like" is the fixture's job to define — this is in
deliberate tension with PR #299's display-policy D1/D2 (a GPU render
tier may later add a display-linear view; that lands as an addition,
not a correction of this surface).

**Treatments say where the picture came from and how old it is.** Calm
green `live · N fps` while frames arrive; amber `last frame · N ago`
only after `FRAME_STALE_AFTER_SECS` (5 s — the spike gate's number,
deliberately generous so hiccups don't teach users to ignore amber);
offline keeps the last in-session frame dimmed + veiled (Q4: last
known, not current); a never-fed card shows a sentence or the
remembered board — never a plausible-looking pattern. The frame wears
the display layout's own aspect ratio (clamped 0.75–4.0, capped by a
matched 280 px height/width budget that letterboxes rather than
re-squishes).

**The card grammar reshaped around the picture** (G1/G1b amendments,
2026-08-05/06):

- ▶ leads the tab row and is the default for a connected card running
  a project; an explicit tab choice stays sticky. No project → no ▶.
- The Status tab retired; its health story folded into the front door,
  which then renamed **Settings → Details** ("it really isn't
  settings"): health narration + everything known and remembered —
  board (registry fact on remembered cards), uid, transport, port,
  firmware, chip.
- Buttons left the front door: the editor entry is the ▶ meta row's
  Editor button (+ the always-visible title-bar ⤢); **Reconnect
  renders inside the ▶ box** where the absence shows (front door keeps
  it only on cards with no ▶). The gone device's empty box names the
  remembered board with Reconnect under it.
- The D42 console strip and pane mode's permanent console region
  retired; Console is an ordinary tab in both card and pane modes.

## Consequences

- The device never renders to answer the feed; feed traffic yields to
  user gestures at frame boundaries (`DEVICE_CARD_FEED_CLASS`) and
  stops entirely when the ▶ tab is not up.
- The wire bump (v12) is a hard compatibility line per the no-shim
  policy: old firmware reads as Incompatible and reflashces.
- The sim's honesty exposed a real engine gap: the browser GPU preview
  tier cannot render control products at all (blocking readback is
  native/CPU-only), so preview-host lamp surfaces pend forever —
  carried as debt
  (`docs/debt/browser-gpu-tier-cannot-render-control-products.md`)
  with an active fix task. The sim WORKER samples on the CPU tier and
  is unaffected.
- Story baselines for the card grammar churned (Status stories
  retired, play-tab sheets added); CI's auto-commit cycle absorbed it.

## Alternatives Considered

- **Keep re-simulating (hero strip / sim ▶):** rejected as the
  founding dishonesty; also duplicated GPU work per card.
- **Re-use `ControlProductProbeRequest`:** re-renders the fixture
  inside the render tick per probe — a polling feed would tax the
  device every 150 ms for work it already did.
- **Device-push streaming / a wire ticker:** rejected at the spike
  gate — push monopolizes the mode-exclusive wire, the ticker read as
  noise (possible Performance-tab material later).
- **Pre-gamma colors:** prettier, but not what the board outputs;
  inverting gamma is lossy.

## Follow-ups

- Persistent last-frame snapshots (offline cards across app runs) via
  the M6 project-thumb `<img>` seam / LibraryStore metadata.
- 3D fixture models inherit the ▶ tab slot when they land.
- GPU render tier (PR #299 parked) relates via the D1/D2 display-policy
  tension recorded above.
- Browser GPU tier control-product rendering — the debt entry above;
  async readback preferred, and the per-tick retry storm must become a
  classified one-time failure.
- Open-in-sim auto-selects the inherited board (G1b ruling split to
  its own task; `note_sim_loaded_project` already inherits
  `sim_board_id` from the project's manifest `target`).
- The ▶ meta row's project chip truncates hard beside the Editor
  button at card width; revisit if it grates.
