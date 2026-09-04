# A wire-version mismatch is a warning, not a verdict; the view says no less than the fold knows

Date: 2026-09-04
Status: accepted (ruled by Yona on the bench; shipped with the fold, view,
words and stories changes in this PR)
Related: `docs/adr/2026-09-01-editor-lens-borrows-the-device-wire.md`,
`docs/adr/2026-09-03-device-card-fixed-height-and-disconnect-disappears.md`,
review `~/.photomancer/planning/lp2025/_reviews/2026-09-04-device-verdict-projection/review.md`

## Context

A classic ESP32 on the bench ran firmware speaking wire proto 19. Studio
had moved to proto 20 two days earlier (the Clear-faults verb). The board
heartbeated every second; the card's own terminal decoded the hello
(`hello · proto 19 · ? · fw-esp32v3 7c80a27…`) and every heartbeat
(`heartbeat · studio · FAULT red`). The faces above the terminal said:

- chip: `Incompatible firmware (wire proto 19)`
- identity line: `… · no firmware`
- firmware zone: `Blank flash — needs firmware`
- preview slot: `Nothing running — a blank chip has no picture.`
- project zone: empty

Four causes, one class:

1. The fold stored only the proto number of a mismatched hello and dropped
   the `HelloFacts`, so the firmware label, the board id and the chip
   (derivable from the label) were gone before any face was drawn.
2. `Classification::Incompatible { ProtoMismatch }` had no room for the
   hello, so `hello()` answered nothing for it.
3. The view handed the renderer `needs_firmware: bool` (five verdicts in
   one bit) and the renderer re-invented the noun: every needs-firmware
   verdict drew as "Blank flash".
4. `loaded_project` and `degraded` asked "is it a LightPlayer?" before
   stating what the board had reported — a fact the fold had accepted.

Nothing tested that the card says as much as the fold knows, and no story
rendered any needs-firmware face but Blank.

## Decision

### 1. A hello on another wire version is a LightPlayer we warn about and then talk to

Yona's ruling: *forcing a firmware update is dangerous — what if it goes
wrong? The real use case is me at Burning Man at 2am wanting to make a
small change and not break the device. There is no versioning on the
serial format yet; for now hope-it-works is fine, so long as we are aware
it is old (or new).*

So:

- `observe_frame` keeps **every** hello. Any hello makes the board
  `Classification::LightPlayer { hello }`. `IncompatibleReason::ProtoMismatch`
  is deleted; `NoHello` (frames flowed, no hello ever came) remains.
- The comparison is a **fact**: `Evidence::wire_version()` →
  `WireVersion::{Match, BoardOlder, BoardNewer}`, journaled once per
  observation window (`JournalNote::WireVersionMismatch`) with one terminal
  line in wire words ("firmware speaks wire proto 19, Studio speaks 20 —
  older firmware, proceeding anyway").
- **Status is unaffected.** An older board is Ready or Degraded like any
  other; the awareness lives on the Firmware zone's line in user words
  ("older than Studio, update recommended" / "newer than Studio"), next to
  the re-flash verb a running board already has — offered, never forced.
- **Every wire verb stays offered**: push, Open (lens), Remove project,
  Clear faults. The lens attachment no longer refuses an older board. A
  request the old firmware cannot answer fails the way any request fails
  (request deadline → outcome line), which bounds hope-it-works to "one
  verb did not work", never "the app hung".
- The post-flash hello check no longer fails a flash on a proto mismatch:
  the image wrote and the board boots it; the fold's notice says the
  served package is not what Studio speaks.

### 2. The projection carries a typed face, and the words live in core

`DeviceView::firmware_face: FirmwareFace` — `Unknown`, `LightPlayer {
firmware, wire }`, `NoHello`, `Blank`, `Bootloader`, `Foreign { label }`,
`Silent` — replaces `needs_firmware: bool` and `firmware: Option<String>`.
It is the one-to-one image of the classification; `wants_flash()` is the
verb gate the bool was. The pending link carries the same face.

The **words** for each face live in `lpa-studio-core`
(`device_firmware_face.rs`: the firmware line, the pending line, the
preview sentence), one exhaustive match each with a test per variant. The
renderer draws what core decided and owns no verdict vocabulary — the
next variant is a compile error in one tested function, not a silent
"blank" downstream.

### 3. Facts are stated when reported; verdicts gate verbs, never facts

`loaded_project` and `degraded` answer from the observations alone (plus
the open-port guard: a closed port has nothing current to say). The verbs
— `can_receive_project`, `can_remove_project`, Open, Clear faults — still
require a hello. If a fact should not be believed from some board, the
fold must refuse it in `observe_frame`, so the two can never disagree.

### 4. Two tests and one story sheet pin the class

- `properties.rs::the_view_says_no_less_than_the_fold_knows`, over every
  lifecycle × gesture: face ↔ classification one-to-one; a hello's
  firmware label reaches the face; a loaded report on an open port reaches
  the loaded face.
- `scenarios.rs::a_proto_mismatch_is_a_warning_on_a_ready_board_not_a_verdict`:
  Ready, board id kept, push offered, journaled once, said once in the
  terminal.
- Story `devices_card_firmware_faces`: older (board known — Update in
  one click), older with the board unknown (the bench case verbatim —
  Update opens the pick), newer, pre-hello, foreign, bootloader, silent —
  so no face ships unseen. Story `device_update_pick_open` shows the pick
  with its reason line.

### 5. Two verbs for two situations: Flash on a needs-firmware face, Update on a running LightPlayer

The card carried one label, "Flash firmware", for two different gestures.
Yona's ruling (2026-09-04, on the bench classic):

- **Flash firmware** stays on the needs-firmware faces (`Blank`,
  `Bootloader`, `Foreign`, `NoHello`, `Silent`), always with the board
  pick, because nothing is known.
- **Update firmware** on a running LightPlayer. Board known → one click, no
  pick (`reflash_choice` resolves). Board unknown — several catalog boards
  fit the joined chip and the registry has no board — the SAME verb opens
  the pick once, and the panel says why in one line: *"This board hasn't
  said which board it is. Pick once; Studio stamps it at flash, and next
  time this is one click."* The firmware line already reads "older than
  Studio, update recommended", so verb and line now match.

Why the bench board asks: its hello reports board `?` because the board
id comes from the `/hardware.json` manifest Studio stamps at flash, and
that board was flashed from the CLI; a classic ESP32 chip fits three
served boards.

The decision lives in core — `FirmwareVerb` and `firmware_verb(view)` in
`device_flash.rs`, with the label, the hover summary, the pick reason and
the one-click action — and is tested per situation, including the
line/verb agreement. The card and the popover only draw it.

## Consequences

- The bench board now reads: chip `Degraded`; identity line
  `esp32 · <mac> · fw fw-esp32v3 7c80a27…`; project line `Recovery red: …`
  with Open · Clear faults · Remove; firmware line `fw-esp32v3 7c80a27… —
  older than Studio, update recommended` with Update firmware · Factory
  reset — the verb opening the board pick once, since this board's hello
  names no board; the pick narrowed to classic ESP32 boards on its own
  (the chip rung off the firmware label runs again). Open attached the
  editor lens on the proto-19 board.
- A *newer* board against an older deployed Studio is the same warning the
  other way, on the same hope-it-works terms. When wire versioning is
  wanted, the seam is already there: the hello carries the proto and the
  fold keeps it.
- The chip's wording for the remaining attention verdicts (pre-hello,
  foreign, bootloader) is still a sentence; the UX pass that shortens the
  chip and moves verdict sentences into the Firmware zone is a separate
  session (the proto case no longer needs it — it reads Ready/Degraded).
- `lpa-link`'s legacy `DeviceSession` hello gate (`HelloGate::Incompatible`
  on proto mismatch) still exists behind the host-serial/CLI paths and was
  not changed; the Studio device model no longer goes through it.
