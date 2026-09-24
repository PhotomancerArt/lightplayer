# Fixtures

## `choker-lens-sample.txt`

185 `M!` lines (136,056 bytes) of real wire traffic, both directions, one per
line as `<dir> M!{json}` (`<` board→host, `>` host→board), in stream order.
It is shared: `lp-json-pack`'s codec tests, `lpc-wire`'s wire-type round trip
(`ser_write::token_hook_tests`, `pack_sink` tests) and the wire dictionary's
frequency ranking (`just wire-dict`) all read this one file.

- **Source:** `after-choker-2026-09-23.tap` (1,287,914 bytes, sha256
  `8853bfde07590c7a087687f3488063c500f9c58c9f4276971ddde804f2e66ea7`), the
  lean-wire pass's "after" recording. A copy is kept beside the plan that
  uses it: `~/.photomancer/planning/lp2025/2026-09-23-1701-lp-json-pack/wire-tap/`
  (original in `…/2026-09-23-1501-lean-wire/wire-tap/`; its provenance is
  that plan's `_measurements.md`, "Live tap (after)").
- **How it was recorded:** 2026-09-23 ~20:35–20:44 PDT, with the opt-in tap in
  `emu serve`'s byte pump (`LP_EMU_WIRE_TAP=<dir> just studio-dev-emu`).
  Configuration `lp-emu:esp32c6:t1`, lp-emu commit `14ef539d7`: the shipped
  `fw-esp32c6` image built from lean-wire's head (hello `proto 21`, stamp
  `fw-esp32c6 03c299b8fbd6 (dirty)`), direct-loaded on the emulated board
  `c6-a`. Studio was driven headless over CDP (`scripts/emu/studio-driver.mjs`):
  connect, push the public PLAYFUL choker catalog project, the device card's
  reads, **Open in editor** (the Studio lens, root module selected) for ~80 s,
  then the Fixture selected for ~40 s. 319 s in all.
- **Wire shapes:** post-lean-wire (PR #791). Nothing under `lp-core/lpc-wire`,
  `lpc-model` or `lpc-engine` changed between `14ef539d7` and the main that
  merged it, and every line re-parses into the wire types and re-serializes
  byte for byte (`lpc-wire`'s `recorded_traffic_reserializes_byte_for_byte`).
- **Cut with:** `sample_tap.py` in this directory
  (`python3 sample_tap.py <recording.tap> choker-lens-sample.txt`). It
  reassembles each direction into lines and samples `M!` lines evenly per
  message class: 72 lens and device-card replies (`projectRead.events`), 48
  lens requests (`projectRead.handle`), 24 heartbeats, one upload chunk
  (`filesystem.writeChunk`, host→board), and up to 3 of every other class
  (hello, file writes, loads, listings, an error). The recording has **no
  knob turns** (`panel_write`), so the sample has none.
- **Contents:** no secrets. Board identity in it is the emulated board's MAC
  and build stamp.

The pre-lean-wire sample it replaced (114 lines, 231,534 bytes, cut from the
BLE plan's "Run D" recording) is in git history; its lines no longer parse into
the wire types.

Do not hand-edit it: re-cut it from a recording, and update the counts above
and the numbers the tests assert.
