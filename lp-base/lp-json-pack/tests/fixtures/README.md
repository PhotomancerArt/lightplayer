# Fixtures

## `choker-lens-sample.txt`

175 `M!` lines (174,501 bytes) of real wire traffic, both directions, one per
line as `<dir> M!{json}` (`<` board→host, `>` host→board), in stream order.
It is shared: `lp-json-pack`'s codec and learned-table loss tests, and
`lpc-wire`'s wire-type round trip (`ser_write::token_hook_tests`, `pack_sink`
and `packed_frame` tests, which replay it in order through one learned table)
all read this one file.

- **Source:** `json-proto28-choker-2026-09-25.tap` (1,545,259 bytes,
  sha256 `e225f43b2661af059f535210499d0541b4d4647cca6f9427fd4c7113a280784f`),
  kept beside the plan that uses it:
  `~/.photomancer/planning/lp2025/2026-09-25-0006-learned-wire-dictionary/wire-tap/`
  (with its packed twin, `packed-proto28-choker-2026-09-25.tap`, the same
  flow with Studio's default packed replies).
- **Why it was re-recorded:** proto 28, the learned wire dictionary. The
  hello's `packDictionary` became `packFormat` and `SetEncoding`'s
  `dictionary` became `format`, so the proto-24 sample's hellos no longer
  parsed into the wire types. Every other message class is as it was.
- **How it was recorded:** 2026-09-25 ~12:50–12:56 PDT, with the opt-in tap
  in `emu serve`'s byte pump (`LP_EMU_WIRE_TAP=<dir> lp-cli emu serve`),
  from a fresh emu state directory. Configuration `lp-emu:esp32c6:t1`, lp-emu
  at `7190618fc`: the shipped `fw-esp32c6` image built from the learned-wire-
  dictionary branch (hello `proto 28`, `packFormat 2`), direct-loaded on the
  emulated board `c6-a`. Studio (`just studio-dev`, opened with `?wire=json`)
  was driven headless over CDP (`scripts/emu/studio-driver.mjs`, through the
  plan's `wire-tap/scripts/session.sh json-150 wire=json 90 150 90`):
  connect, push the public PLAYFUL choker catalog project, ~90 s of the
  device card's reads, **Open in editor** (the Studio lens) for ~150 s, then
  the Fixture selected for ~90 s. `?wire=json` keeps the host from opting
  into packed, so every board→host line is JSON.
- **Wire shapes:** proto 28. Every line re-parses into the wire types and
  re-serializes byte for byte (`lpc-wire`'s
  `recorded_traffic_reserializes_byte_for_byte`).
- **Cut with:** `sample_tap.py` in this directory
  (`python3 sample_tap.py <recording.tap> choker-lens-sample.txt`). It
  reassembles each direction into lines and samples `M!` lines evenly per
  message class: 72 lens and device-card replies (`projectRead.events`), 48
  lens requests (`projectRead.handle`), up to 24 heartbeats (18 in this
  recording), one upload chunk (`filesystem.writeChunk`, host→board), and up
  to 3 of every other class (hello, file writes, loads, listings, the access
  list). It never samples `accessAdd` requests, which carry a browser's access
  key. The recording has **no knob turns** (`panel_write`) and no error
  replies, so the sample has neither.
- **Contents:** no secrets (see the `accessAdd` exclusion above). Board
  identity in it is the emulated board's MAC and build stamp; the access list
  replies carry a salt and a label, never a key.

The proto-24 sample it replaced (178 lines, 192,577 bytes, cut from
`after-merge-proto24-choker-2026-09-24.tap`), the proto-23, proto-22 and
proto-21 ones before it, and the pre-lean-wire one are in git history; their
lines no longer parse into the wire types.

Do not hand-edit it: re-cut it from a recording, and update the counts above
and the numbers the tests assert.
