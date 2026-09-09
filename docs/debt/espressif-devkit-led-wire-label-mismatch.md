---
status: carried
since: 2026-07-31
logged: 2026-09-06
area: lpa-boards sidecars + lpc-hardware runtime manifests (Espressif devkits)
related:
  - docs/adr/2026-07-31-board-display-metadata-split.md
  - docs/adr/2026-09-06-catalog-content-tree.md
  - lp-app/lpa-boards/tests/manifest_drift.rs
  - Planning/lp2025/2026-09-06-1009-catalog-content-organization (board audit in notes.md; follow-up plan "templates and board rewire")
---
# The Espressif devkits' LED wire label is `18` in the sidecar and `GPIO18` on the device

**Shape** — a board's LED wires are named twice: the display sidecar's
`default_led_wires` (what the board-project generator authors into an
output node's endpoint, `ws281x:local:<label>`) and the runtime
manifest's `display_label` per GPIO (what the firmware's WS281x drivers
offer). On the two Espressif devkits the sidecar says `18` (the
silkscreen) while the runtime says `GPIO18`, and the ESP32 WS281x
drivers refuse pins whose runtime label is still the bare `GPIO<n>`
form — so those boards offer **no LED endpoint on device**, whatever a
project authors. Surfaced writing the 2026-07-31 display-split ADR
(its Follow-ups), never registered, never fixed. It is a runtime-manifest
calibration question (header-pin labels for the devkits), not a display
one.

Two adjacent gaps keep it invisible: `manifest_drift.rs`'s ordering
assertion compares `default_led_wire()` to itself (vacuous), and
`esp32-devkitc-v4` / `dig-uno` have no runtime manifest at all
(DISPLAY_ONLY), so the drift test skips them.

**Carrying cost** — a generated starter project for either devkit loads
and then fails to open its output; the catalog's positional board
rewire (the follow-up plan's op) cannot map onto these boards until the
labels agree; any "labels are portable" assumption (`D10` is GPIO18 on
the XIAO C6 and GPIO9 on the XIAO S3 Plus) is re-learned per board.

**Workarounds** — author against a board with a `board_label` table
(the XIAO C6 is the reference), or add header-pin labels to the devkit
runtime manifest so `display_label` reads `18` before generating for
it. Do not pad `default_led_wires` on the devkits (the sidecar README
policy: best-first, not every usable output).

**Incident log**
- 2026-07-31 — noticed in the display-metadata split ADR's Follow-ups;
  no entry filed.
- 2026-09-06 — the catalog plan's board audit re-found it while sizing
  the positional rewire; registered here and named as a blocker for
  that follow-up plan's rewire op.

**Exit criteria** — the devkit runtime manifests carry header-pin
labels matching their sidecars, `manifest_drift.rs` asserts the
sidecar's first wire against the runtime label (not against itself),
and a generated devkit starter opens its output on the device.
