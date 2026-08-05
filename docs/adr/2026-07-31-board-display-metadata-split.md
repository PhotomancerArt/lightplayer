# Board display metadata lives in app-side sidecars, not the runtime manifest

- Status: accepted
- Date: 2026-07-31
- Deciders: Yona, Claude

## Context

Board selection is becoming a first-class feature (boards catalog page,
provisioning picker, device-card hardware pane, pin discovery — see the
`spikes/hardware-boards/` spike and the 2026-07-31 hardware/board-selection
roadmap). Those surfaces need catalog data the runtime board manifest does not
carry: support tier, approximate price, purchase URLs, capability chips, and a
drawing block (per-pin roles/capability cells, module/USB/button/terminal
geometry) for the metadata-driven board diagrams.

The runtime manifest (`boards/<vendor>/<product>.json`,
`lpc_hardware::HardwareManifestFile`) is compiled into firmware via
`include_str!`, and measurement has shown the serde surface is the dominant
flash lever on the C6. It is also safety-sensitive: it is the authority on
claimable resources, with a calibration workflow and an "omit what you cannot
verify" policy, because a wrong GPIO is a physical-damage class of mistake.

## Decision

Display/catalog metadata lives in a **sidecar file per board**
(`boards/<vendor>/<product>.display.json`), owned by the new app-side
`lp-app/lpa-boards` crate. The runtime manifest types and firmware serde
surface are untouched.

- The sidecar carries identity/commerce fields and the drawing block; its
  schema (`schemas/board-display.schema.json`) is generated alongside the
  existing schemas by `lp-cli schema gen`.
- `lpa-boards` embeds every checked-in sidecar so wasm consumers (catalog
  page, studio) need no filesystem.
- **Drift tests** in `lpa-boards` are the consistency contract between the two
  files: display↔runtime pairing both directions, `board_id` = path = runtime
  id, silkscreen-label→GPIO agreement with `board_label` entries (calibration
  `not-found` forces the display gpio absent), runtime-omitted GPIOs must
  present as non-claimable roles, and runtime-reserved GPIOs may not display
  as plain io without a warning cap.
- A board may be **display-only** (catalog presence without a runtime
  manifest) only while its SoC has no `HardwareTarget`; such boards sit on an
  explicit allowlist in the drift tests with the reason recorded (today:
  classic-ESP32 boards, pending the v3 firmware target).
- Support tiers are display data: gold (tested every release), silver
  (tested occasionally), bronze (community-verified; the ceiling for boards
  whose firmware target does not exist yet, with a `support_note`).

## Consequences

- Firmware flash cost of board data is unchanged, now and as catalog fields
  grow.
- Pin facts exist in two files with different stakes; the drift gate — not
  discipline — keeps them agreeing, and the runtime manifest stays
  authoritative for anything claimable.
- The catalog can list boards we cannot run yet, honestly labeled, which the
  boards page needs as a shop window.
- Datasheet-derived (uncalibrated) runtime profiles are marked in their
  `board_label` notes and descriptions; the calibration workflow upgrades them
  in place.

## Alternatives considered

- **Extend `HardwareManifestFile` with optional display fields.** Rejected:
  every field is firmware serde surface and flash; display churn would ride
  the safety-sensitive file; build-time stripping adds machinery for no
  boundary gain.
- **A separate registry/config tree elsewhere in the repo.** Rejected: the
  sidecar-next-to-manifest layout keeps a board one directory entry, makes the
  pairing obvious, and lets the drift tests walk a single tree.
- **Deriving display pins from the runtime manifest.** Rejected: the drawing
  needs pins the runtime manifest deliberately omits (power/ground/NC,
  reserved, in-package), plus geometry and roles it will never carry.

## Amendment — 2026-08-05: `default_led_wires` joins the sidecar

The setup flow generates a first project for a board, which needs one fact
this ADR's split had no home for: **where the pixels plug in**. It landed
as `default_led_wires` on the sidecar — an ordered list of this board's own
silkscreen labels, best first — and not on the runtime manifest, for the
reasons this ADR already decided:

- It is **app-side only**. Generation, the provisioning picker, and the
  output face read it; firmware never does, and the runtime manifest is
  `include_str!`'d into flash where every serde byte is paid for.
- It is a **choice among facts**, not a claimable resource. The runtime
  manifest stays the authority on which GPIOs exist and which are reserved;
  the sidecar says which of them a first project should take. Two consumers
  of pin facts already work this way — `output_face_decoration` resolves an
  endpoint's wire through `lpa_boards::board_by_id`, and the board editor's
  lint reads sidecar roles.
- Its vocabulary is **the silkscreen label**, which only the sidecar has:
  the runtime manifest records `board_label` entries for calibrated pins,
  but the drawing tables name every pin the reader can see.

Guarded on both sides: `BoardDisplayFile::validate` refuses a wire that is
not an output-eligible pin or terminal with a gpio, and the drift gate
(`lpa-boards/tests/manifest_drift.rs`) requires every catalog board to
declare one and cross-checks it against the runtime manifest — a wire whose
GPIO the runtime manifest reserves, or does not offer at all, fails the
build. Wrong wire = short circuit is the same stake the rest of this ADR is
written around.

Order carries a board fact where the board has one: the DOM-Z-102 lists its
four fused DATA terminals and deliberately omits the un-level-shifted spare
(IO13). Single-wire generation takes the head of the list; generating onto
several wires at once is future work.

## Follow-ups

- M2 (row-engine renderer) consumes the drawing block and moves the design
  language into `docs/design/board-diagrams.md`.
- The display-only allowlist shrinks as firmware targets land (classic ESP32
  first).
- **Endpoint-name vocabulary drift** (surfaced writing the amendment above,
  not caused by it): on the two Espressif devkits the sidecar's silkscreen
  label is a bare number (`18`) while the runtime manifest's `display_label`
  is `GPIO18`, and the ESP32 WS281x drivers only offer wires whose runtime
  display label differs from `GPIO<n>` — so those boards offer no LED
  endpoint on device today regardless of what a project authors. The fix is
  a runtime-manifest calibration question (board labels for the header
  pins), not a display one.
