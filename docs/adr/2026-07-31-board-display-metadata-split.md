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

## Follow-ups

- M2 (row-engine renderer) consumes the drawing block and moves the design
  language into `docs/design/board-diagrams.md`.
- The display-only allowlist shrinks as firmware targets land (classic ESP32
  first).
