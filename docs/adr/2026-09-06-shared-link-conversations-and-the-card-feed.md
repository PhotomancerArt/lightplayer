# ADR: App conversations ride the shared device link; frames are not evidence

- **Status:** Accepted
- **Date:** 2026-09-06
- **Deciders:** Photomancer
- **Supersedes:** the "synthesized client-side" display-layout clause of
  `2026-08-06-honest-device-preview.md` (that code was deleted in
  45df0da9c; this ADR records what stands instead)
- **Extends:** `2026-08-25-event-fold-device-model.md` (a third traffic
  class on a device link, beside model frames and coarse effects) and
  `2026-09-01-editor-lens-borrows-the-device-wire.md` (the feed pauses
  under the borrow)
- **Superseded by:** None

## Context

The round-2 device card (`2026-09-03-device-card-fixed-height-and-
disconnect-disappears.md`) shipped its 120 px preview slot with a
sentence — "No picture yet — the live feed is coming." — because the
honest live feed of `2026-08-06-honest-device-preview.md` fed the OLD
card, and the teardown deleted that card. What survived was
device-agnostic all the way down: the engine reads the published
`OutputChannels` buffer without rendering, firmware links that path
unconditionally, and the client's feed state, output composition, chunk
reassembly and lamp renderer are pure. Only three sim-only chokepoints in
the studio controller kept real boards out.

The gap was at the link seam. `lpa-devices`' wire mirror is deliberately
lossy — a response body surfaces there as a label, the project handle and
the engine's frame rate are dropped — and `lpa-link` refuses opaque
requests with "coarse effects go through lpa-client, not the device
link". So anything that wanted a response BODY borrowed the wire
exclusively: pump paused, `LinkBorrow` folded, one reader. That is right
for a flash or a push and wrong for a picture that wants to arrive several
times a second on a card that must keep folding heartbeats meanwhile.

Typing the frame into the mirror was the obvious alternative and the
wrong one twice over: the fold journals every input and lines every
unknown frame on the terminal, so a ~5 fps picture stream would drown
both; and the crate cannot hold the `lpc_wire` types the renderer needs
(its only dependencies are serde and serde_json, by rule). A short
exclusive borrow per pull would pause the pump about seven times a second
and fold borrow churn straight into freshness.

## Decision

1. **A reserved request-id range is a third traffic class.** Ids at or
   above `lpa_devices::link::APP_CONVERSATION_ID_BASE` (`0x4000_0000`)
   belong to APP CONVERSATIONS: an `lpa-client` exchange the effects
   layer runs on the shared link beside the model's own frames. The
   transport's demux classifies a reply by its id BEFORE the mirror —
   once, in one place, so the pump and the lens tap cannot disagree —
   and surfaces one in the range as `LinkEvent::Passthrough {
   request_id, line }`, the raw `M!` line verbatim. The effects layer
   routes it to the link's conversation inbox; the fold's arm for it
   does nothing (no terminal line, no freshness, no anomaly count).
   Model-minted ids stay far below; a conversation mints upward from the
   base (`ProtocolSession::starting_at`), so `LpClient` runs UNCHANGED
   over `SharedLinkClientIo` (`SendLine` out, inbox in) — chunked project
   reads, deadlines and cancellation included. No wire proto change.
2. **Frames are not evidence.** The device card's feed
   (`DeviceFrameFeed`, one per wanted device, in the effects layer)
   keeps only frame state and its own conversation bookkeeping; every
   device fact it needs — is the port open, has the board said hello,
   what is it running, is the wire borrowed — is read off the roster's
   evidence at each tick and never cached (invariants I6/I8). The
   picture joins the card at the app view (`DeviceRosterView.feeds`,
   one `DeviceCardFeedView` per fed device); `DeviceView` stays the
   model's verbatim projection. The one always-changing heartbeat fact
   the mirror now carries is the engine's rounded frame rate, for the
   pill — and it is deliberately absent from the terminal line.
3. **Never pull under a borrow.** A coarse effect or the editor lens
   pauses the pump, so a pull then could never be answered. The feed
   checks the link's borrow token before every pull; under the lens the
   slot shows the last frame dimmed with "editor has the wire", under an
   effect the activity's own sentence.
4. **The feed pulls only for a wanted card on a visible page.** A
   mounted `DeviceRosterCard` holds a lease (`DeviceFeedOp`); the tab's
   `visibilitychange` reaches the actor as `StudioCommand::PageVisibility`.
   Cards below the fold still pull — each board has its own wire — and
   the last frame survives the card unmounting.
5. **Cadence, class and honesty are inherited, not re-decided.**
   `DEVICE_CARD_FEED_INTERVAL` (150 ms completion gap, so a big dome
   frame self-throttles), `DEVICE_CARD_FEED_CLASS` (passive: a gesture
   cancels the read at its next frame boundary), `FRAME_STALE_AFTER_SECS`
   (5 s → amber "last frame · N ago"), post-gamma colours, the last
   in-session frame dimmed offline, a sentence — never a plausible
   pattern — when there is nothing honest to draw. Three unanswered
   pulls park the feed until the next port open, hello or
   loaded-project change; the model's quiet detection keeps owning the
   card's freshness.
6. **Dome-scale layouts over the wire's read budget are named, not
   synthesized.** The client-side synthesis the 2026-08-06 ADR described
   was deleted (45df0da9c); the engine's `display_layout_budget` is the
   only source of geometry, and a frame that arrives without it draws
   "Frames are flowing, but this project's lamp layout is too large to
   preview over this link." Reinstating synthesis is its own decision.

## Consequences

- A device link now carries three classes of traffic: model frames
  (small ids, mirrored, folded), app conversations (reserved ids,
  passthrough, never folded) and coarse effects (exclusive borrow, the
  pump paused). The direct-control plan's content probe may choose
  either of the first two; anything wanting a body without pausing the
  card should choose the second.
- The pure feed state the sim card and the lens already shared moved to
  `lpa-studio-core/src/app/frame_feed/` unchanged; three surfaces now
  use it.
- `async-trait` is an unconditional `lpa-studio-core` dependency (the io
  compiles on every target).
- The device model's projection gains one fact (`engine_fps`) and one
  link-event variant it ignores; nothing else about the fold moved.
- Verified against the fake device's real host server (host e2e: the
  picture arrives with the board's own layout and the revision advances;
  zero pulls under the lens and prompt resumption; park after three
  failures and re-arm on a reconnect's hello; the journal never sees a
  passthrough) and on a real board at the G1 gate (a live picture at
  the engine's rate).

## Alternatives Considered

- **A mirror-typed frame in `lpa-devices`:** forces `lpc_wire` types or
  opaque bytes into the model and a picture stream through a journaled
  fold. Rejected.
- **A short exclusive borrow per pull:** pauses the pump ~7×/s and folds
  `LinkBorrow` churn into freshness. Rejected.
- **Device-push streaming:** rejected already at the 2026-08-05 spike
  gate; a push monopolizes the mode-exclusive wire.
- **Joining the lens session's own frames into the card while the
  editor is open:** landed 2026-09-07 (`LensFrameSource`, joined at
  `device_card_feed_view`): while the lens holds a device's wire the card
  draws the lens mirror's composed frame as `Live`, aged by the lens's own
  frame clock; `FeedLiveness::Lens` (the dimmed last frame, "editor has
  the wire") remains only until the lens has produced a frame. No second
  pull, and the feed's own pull still never runs under the borrow.

## References

- Plan: `~/.photomancer/planning/lp2025/2026-09-06-1407-device-card-live-feed/`
- PR: https://github.com/PhotomancerArt/lightplayer/pull/540
- Code: `lp-app/lpa-devices/src/link.rs` (`APP_CONVERSATION_ID_BASE`,
  `LinkEvent::Passthrough`), `lp-app/lpa-link/src/device_link/demux.rs`
  (the classification), `lp-app/lpa-studio-core/src/app/devices/
  {shared_link_client_io.rs, device_frame_feed.rs,
  device_card_feed_view.rs, device_feed_op.rs}`, `app/frame_feed/`,
  `lp-app/lpa-studio-web/src/app/home/device_roster_card.rs`
  (`preview_slot`).
