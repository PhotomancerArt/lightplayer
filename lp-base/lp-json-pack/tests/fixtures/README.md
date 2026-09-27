# Fixtures

## `choker-lens-sample.txt`

184 `M!` lines (186,568 bytes) of real wire traffic, both directions, one per
line as `<dir> M!{json}` (`<` board→host, `>` host→board), in stream order.
It is shared: `lp-json-pack`'s codec and learned-table loss tests, and
`lpc-wire`'s wire-type round trip (`ser_write::token_hook_tests`, `pack_sink`
and `packed_frame` tests, which replay it in order through one learned table)
all read this one file.

- **Source:** `json-proto30-choker-2026-09-27.tap` (6,482,291 bytes,
  sha256 `f2851318ea3860573b62d1bada7a415675c6362b03c446e6661c366304266b4f`),
  kept beside the plan that uses it:
  `~/.photomancer/planning/lp2025/2026-09-27-0215-lp-link-usb-cutover/wire-tap/`.
- **Why it was re-recorded:** proto 30, lp-link on the USB link. The
  heartbeat's `link` object became lp-link's counters, so the proto-28
  sample's heartbeats no longer parsed into the wire types (a load-time
  translation stood in until this re-cut, and is gone).
- **How it was recorded:** 2026-09-27 ~06:03–06:09 PDT, with the opt-in tap
  in `emu serve`'s byte pump (`LP_EMU_WIRE_TAP=<dir> lp-cli emu serve`),
  from a fresh emu state directory. Configuration `lp-emu:esp32c6:t1`: the
  shipped `fw-esp32c6` image built from the lp-link cut-over branch (hello
  `proto 30`, `packFormat 2`), direct-loaded on the emulated board `c6-a`.
  Studio (`just studio-dev`, opened with `?wire=json`) was driven headless
  over CDP (`scripts/emu/studio-driver.mjs`, the same connect / push / card
  90 s / **Open in editor** 150 s / Fixture 90 s flow as the proto-28 one).
  `?wire=json` keeps the host from opting into packed, so every board→host
  message is JSON.
- **Wire shapes:** proto 30. Every line re-parses into the wire types and
  re-serializes byte for byte (`lpc-wire`'s
  `recorded_traffic_reserializes_byte_for_byte`).
- **Cut with:** since proto 30 the tap holds lp-link frames, so it is first
  rewritten as a tap of `M!` lines, then sampled:
  `lp-cli wire unpack --tap < recording.tap > unpacked.tap`, then
  `python3 sample_tap.py unpacked.tap choker-lens-sample.txt`. The sampler
  reassembles each direction into lines and samples `M!` lines evenly per
  message class: 72 lens and device-card replies (`projectRead.events`), 48
  lens requests (`projectRead.handle`), up to 24 heartbeats, one upload chunk
  (`filesystem.writeChunk`, host→board), and up to 3 of every other class
  (hello, file writes, loads, listings, the access list). It never samples
  `accessAdd` requests, which carry a browser's access key. The recording has
  **no knob turns** (`panel_write`) and no error replies, so the sample has
  neither.
- **Contents:** no secrets (see the `accessAdd` exclusion above). Board
  identity in it is the emulated board's MAC and build stamp; the access list
  replies carry a salt and a label, never a key.

The proto-28 sample it replaced (175 lines, 174,501 bytes, cut from
`json-proto28-choker-2026-09-25.tap`, archived with
`lp2025/_archive/2026-09-25-0006-learned-wire-dictionary/wire-tap/`) and the
ones before it are in git history; their heartbeats no longer parse into the
wire types.

The proto-24 sample before that (178 lines, 192,577 bytes, cut from
`after-merge-proto24-choker-2026-09-24.tap`), the proto-23, proto-22 and
proto-21 ones before it, and the pre-lean-wire one are in git history; their
lines no longer parse into the wire types.

Do not hand-edit it: re-cut it from a recording, and update the counts above
and the numbers the tests assert.
