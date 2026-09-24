# Fixtures

## `choker-lens-sample.txt`

172 `M!` lines (175,602 bytes) of real wire traffic, both directions, one per
line as `<dir> M!{json}` (`<` board→host, `>` host→board), in stream order.
It is shared: `lp-json-pack`'s codec tests, `lpc-wire`'s wire-type round trip
(`ser_write::token_hook_tests`, `pack_sink` and `packed_frame` tests) and the
wire dictionary's frequency ranking (`just wire-dict`) all read this one file.

- **Source:** `after-json-pack-p4-choker-2026-09-23.tap` (1,417,176 bytes,
  sha256 `3dffadf5b4777baf943910f84509fb9e13df7f2c40e4aad878958adeb3b74ce2`),
  kept beside the plan that uses it:
  `~/.photomancer/planning/lp2025/2026-09-23-1701-lp-json-pack/wire-tap/`.
- **Why it was re-recorded:** proto 22 (plan `lp-json-pack`, P4) added
  `packDictionary` to the hello, so the proto-21 sample's hello lines no longer
  parsed into the wire types. Every other message class is as it was.
- **How it was recorded:** 2026-09-23 ~23:24–23:27 PDT, with the opt-in tap in
  `emu serve`'s byte pump (`LP_EMU_WIRE_TAP=<dir> just studio-dev-emu`).
  Configuration `lp-emu:esp32c6:t1`, lp-emu commit `fafb2b574` (no lp-emu
  source change in the tree): the shipped `fw-esp32c6` image built from the
  P4 working tree (hello `proto 22`, stamp `fw-esp32c6 fa49a24a38d7 (dirty)`),
  direct-loaded on the emulated board `c6-a`. Studio was driven headless over
  CDP (`scripts/emu/studio-driver.mjs`): connect, push the public PLAYFUL
  choker catalog project, ~50 s of the device card's reads, **Open in
  editor** (the Studio lens, root module selected) for ~90 s, then the Fixture
  selected for ~45 s. 187 s of tap in all. No host opted into packed, so every
  board→host line is JSON.
- **Wire shapes:** proto 22. Every line re-parses into the wire types and
  re-serializes byte for byte (`lpc-wire`'s
  `recorded_traffic_reserializes_byte_for_byte`).
- **Cut with:** `sample_tap.py` in this directory
  (`python3 sample_tap.py <recording.tap> choker-lens-sample.txt`). It
  reassembles each direction into lines and samples `M!` lines evenly per
  message class: 72 lens and device-card replies (`projectRead.events`), 48
  lens requests (`projectRead.handle`), the 18 heartbeats there were, one
  upload chunk (`filesystem.writeChunk`, host→board), and up to 3 of every
  other class (hello, file writes, loads, listings). The recording has **no
  knob turns** (`panel_write`) and no error replies, so the sample has
  neither.
- **Contents:** no secrets. Board identity in it is the emulated board's MAC
  and build stamp.

The proto-21 sample it replaced (185 lines, 136,056 bytes, cut from
lean-wire's `after-choker-2026-09-23.tap`) and the pre-lean-wire one before
it are in git history; their lines no longer parse into the wire types.

Do not hand-edit it: re-cut it from a recording, and update the counts above
and the numbers the tests assert.
