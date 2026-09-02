# ADR: The editor is a lens that borrows a device's wire; the tap keeps the fold live

- **Status:** Accepted
- **Date:** 2026-09-01
- **Deciders:** Photomancer
- **Supersedes:** None (extends `2026-08-25-event-fold-device-model.md`'s
  coarse-effect seam to a long-lived borrow; retires the last of
  `2026-07-15-device-session-model.md`'s editor-side `DeviceSession`
  ownership)
- **Superseded by:** None

## Context

Device-model round 2 (plan `2026-08-30-1154-device-model-round-2`) put
every device fact into one evidence fold in `lpa-devices`, with the roster
owning each board's link and a pump draining that link into the fold.
Coarse effects (flash, push, erase) already borrow a link *exclusively* for
their duration: the effects layer pauses the pump, announces the borrow to
the fold as an `Event::LinkBorrow`, runs the effect over the raw port, and
gives the wire back before the end marker folds (the seam in
`2026-08-25-event-fold-device-model.md`).

M5 re-adds the editor as a lens on a connected board. The editor's mirror
speaks the app protocol through an `lpa-client` `LpClient` over a wire
client the runtime pool's session owns. That client needs the same port
the roster's pump is draining — and **two readers on one serial port split
the frames between them; both halves look like a dead board** (the lesson
every push and stamp on this seam is built on). Meanwhile the device card
must stay live while the editor is open: freshness, heartbeats, loaded
projects, console lines are all fold evidence, and a card that goes deaf
the moment the editor opens is the old "stale verdict" bug in new clothes.

The pre-teardown design gave the editor its own `DeviceSession` that owned
the port outright and fed a second store; that is exactly the parallel
store (invariant I8) round 2 exists to end.

## Decision

1. **One wire, one owner, and the lens is a borrow.** Attaching the editor
   to a board takes the link's exclusive borrow the way a coarse effect
   does — pump paused, `LinkBorrow { held: true }` folded — but holds it
   for the lens's lifetime instead of running to completion. The borrow is
   signed with a token the model never mints (`LENS_EFFECT_ID`), so the
   guarded release that keeps stragglers honest works in both directions:
   an activity's late completion cannot hand away the lens's wire, and the
   lens cannot release an activity's.
2. **The tap: the lens io tees every line back into the fold.** The
   session's wire client is built over a transport-provided lens io
   (`DeviceTransport::lens_client_io`) that hands every whole line it
   drains — `M!` frame or console output, verbatim — to a tap. The effects
   layer demuxes each line with the same `demux_line` the pump uses and
   feeds the roster fold as `Event::Link`. The fold cannot tell the pump
   changed hands: heartbeats advance freshness, quiet verdicts stay
   suppressed by the borrow, and the card renders exactly what it would
   have. The tap emits; it never caches (I6/I8). Nothing about the device
   moves into the pool: the session carries only the lens's HANDLE
   (`DeviceLensAttachment` — which device, which link, uid, name, board,
   hello features).
3. **Closing is one road.** Detach, navigate-away, a failed attach, and
   unplug all release the borrow (the pump resumes) and drop the session;
   an unplug is detected as the model ceasing to route the link, so the
   card's own departure evidence and the end of the lens come from one
   fact. A card verb that needs the wire while the editor is on that board
   closes the editor first and then runs — the card's verbs always work,
   the editor is what yields (direct-control doctrine). A rename leaves the
   lens alone.
4. **Readiness belongs to the actor, and an address is an intent.** A
   `/device/<uid>` open that the roster cannot serve yet (rows still
   loading, port still identifying, board busy, port closed) is held and
   attached from the refresh tick the moment the board is registered, open
   and says hello. The gallery renders the board's honest state meanwhile;
   the device route never shows an opening frame. Opening requires an
   OPEN port: an attached-but-closed link has stale hello evidence and no
   wire to lend.
5. **Probe policy follows the lens kind.** A device lens pulls at the
   150 ms device cadence, probes visuals at the 16×16 device tier, and
   subscribes the focused node's products only — every subscribed product
   is frames over serial. An unknown lens kind gets the device policy (the
   conservative default); the sim declares itself.

## Consequences

- The runtime pool regains a device arm (`RuntimePayload::Device`), but as
  a lens handle, not a device store. Single-session policy is kind-
  agnostic: a device lens replaces the sim and vice versa.
- Effects and the lens are mutually exclusive on a wire by construction
  (the borrow token), and both refusals are honest: an effect on a
  lens-held wire ends with the reason; a lens on an effect-held wire is
  refused with it.
- The lens io must tee *before* decoding, and the effects layer must demux
  with the pump's own classifier — a second classifier here would be the
  "two vocabularies" mistake the event-fold ADR retired.
- Build features from the board's hello are not yet read at attach (the
  fold's mirror carries none); the add-node picker offers every kind under
  a device lens until they are. Tracked on the M5 PR.
- The `LinkEvent::Closed` a link queues while borrowed is folded only when
  the pump resumes; the lens learns about a dead port from its own
  client's failures and from the model ceasing to route the link, which is
  what makes the unplug road (3) the honest one.

## References

- Plan: `~/.photomancer/planning/lp2025/2026-08-30-1154-device-model-round-2/`
  (`m5-editor-lens.md`, `2026-09-01-m4-validation-and-m5-recon.md`).
- PR: https://github.com/PhotomancerArt/lightplayer/pull/494
- Code: `lp-app/lpa-studio-core/src/app/devices/device_effects.rs`
  (`attach_lens_wire`, `release_lens_wire`, `LENS_EFFECT_ID`),
  `device_transport.rs` (`lens_client_io`, `LensLineTap`),
  `lp-app/lpa-link/src/providers/browser_serial_esp32/port_client_io.rs`
  (the tee), `runtime_pool/runtime_session.rs` (`RuntimePayload`),
  `studio_controller.rs` (`open_device_lens`, `close_device_lens`,
  `try_pending_device_lens`).
