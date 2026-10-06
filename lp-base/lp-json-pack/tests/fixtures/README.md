# Fixtures

## `choker-lens-sample.txt`

184 `M!` lines (193,422 bytes) of real wire traffic, both directions, one per
line as `<dir> M!{json}` (`<` board→host, `>` host→board), in stream order.
It is shared: `lp-json-pack`'s codec and learned-table loss tests, and
`lpc-wire`'s wire-type round trip (`ser_write::token_hook_tests`, `pack_sink`
and `packed_frame` tests, which replay it in order through one learned table)
all read this one file.

- **Source:** `json-proto33-choker-2026-10-02.tap` (5,916,072 bytes,
  sha256 `b9f8b664791cef1a6042b4d15753c7cbb728f3894667964053f3da6b644c7e7c`),
  kept beside the plan that uses it:
  `~/.photomancer/planning/lp2025/2026-10-01-1843-c6-repartition/wire-tap/`.
- **Why it was re-recorded:** proto 33, the hello's `hardware.fs` (how the
  board's filesystem came up). It is a required field, so the proto-30
  sample's hellos no longer parsed into the wire types.
- **How it was recorded:** 2026-10-02 ~05:42–05:47 PDT, with the opt-in tap
  in `emu serve`'s byte pump (`LP_EMU_WIRE_TAP=<dir> lp-cli emu serve`),
  from a fresh emu state directory. Configuration `lp-emu:esp32c6:t1`: the
  packaged `fw-esp32c6` image built from the C6 repartition branch (hello
  `proto 33`, `packFormat 2`, `fs: formatted` — a fresh board), direct-loaded
  on the emulated board `c6-a`. Studio — the RELEASE bundle served by a
  one-off script rather than `just studio-dev` (that session could not run a
  dev server), opened with `?wire=json` — was driven headless over CDP
  (`scripts/emu/studio-driver.mjs`): connect, push the PLAYFUL Choker example,
  card 90 s, **Open in editor** 150 s, Fixture 90 s.
  `?wire=json` keeps the host from opting into packed, so every board→host
  message is JSON.
- **Edited since, by hand (proto 34):** the three `accessList` replies'
  `"open":false` became `"open":"nobody"` — the device store's `open` became
  a word (`lpc_access::OpenTo`, main's wire 33, merged under this branch's
  wire 34), and `nobody` is exactly what that board's `false` reads as. The
  hellos still say `proto 33`, the number this branch's firmware carried
  when it was cut; nothing else in the recording changed, and the byte
  counts above are the original cut's.
- **Edited since, by hand (proto 35):** the three hellos' `build` gained
  `"version":"unknown"` after `"package"` — the hello now says the build's
  app version, a required field. The recorded board predates the field, so
  it is given the value an embedder with no version reports rather than a
  version it never said.
- **Wire shapes:** proto 35; 36 (Wi-Fi settings) only added messages,
  and 37 (Bluetooth onto lp-link) moved no message shape, so nothing in
  the sample changed for either. Every line
  re-parses into the wire types and
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

The proto-30 sample it replaced (184 lines, 186,568 bytes, cut from
`json-proto30-choker-2026-09-27.tap`, kept beside
`lp2025/2026-09-27-0215-lp-link-usb-cutover/wire-tap/`) is in the repository's
history; its hellos have no `fs` and no longer parse into the wire types.

The proto-28 sample before that (175 lines, 174,501 bytes, cut from
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
