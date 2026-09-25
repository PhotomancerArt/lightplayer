# Fixtures

## `choker-lens-sample.txt`

178 `M!` lines (192,577 bytes) of real wire traffic, both directions, one per
line as `<dir> M!{json}` (`<` board→host, `>` host→board), in stream order.
It is shared: `lp-json-pack`'s codec tests, `lpc-wire`'s wire-type round trip
(`ser_write::token_hook_tests`, `pack_sink` and `packed_frame` tests) and the
wire dictionary's frequency ranking (`just wire-dict`) all read this one file.

- **Source:** `after-merge-proto24-choker-2026-09-24.tap` (2,420,025 bytes,
  sha256 `0687fd7d2675a088b169869c09695dc8a726d16c38711547ef11b73c23f1e00a`),
  kept beside the plan that uses it:
  `~/.photomancer/planning/lp2025/2026-09-23-1701-lp-json-pack/wire-tap/`.
- **Why it was re-recorded:** proto 24. Merging `origin/main` (lean-wire's
  follow-ups, which took proto 23 and retyped `RevisionGateRead::IfChanged`
  from `{ known_revision }` to `{ known: [KnownRevision] }`) into
  `lp-json-pack` re-bumped the proto, and the proto-23 sample's lens requests
  no longer parsed into the wire types. Every other message class is as it
  was.
- **How it was recorded:** 2026-09-24 ~22:40–22:45 PDT, with the opt-in tap in
  `emu serve`'s byte pump (`LP_EMU_WIRE_TAP=<dir> lp-cli emu serve`), from a
  fresh emu state directory. Configuration `lp-emu:esp32c6:t1`, lp-emu at the
  merge's tree (`f2d11a5f9` + `origin/main` `43451528d`): the shipped
  `fw-esp32c6` image built from the merge working tree (hello `proto 24`,
  stamp `fw-esp32c6 f2d11a5f97aa (dirty)`), direct-loaded on the emulated
  board `c6-a`. Studio (`just studio-dev`, opened with `?wire=json`) was
  driven headless over CDP (`scripts/emu/studio-driver.mjs`): connect, push
  the public PLAYFUL choker catalog project, ~90 s of the device card's
  reads, **Open in editor** (the Studio lens) for ~150 s, then the Fixture
  selected for ~90 s. `?wire=json` keeps the host from opting into packed, so
  every board→host line is JSON.
- **Wire shapes:** proto 24. Every line re-parses into the wire types and
  re-serializes byte for byte (`lpc-wire`'s
  `recorded_traffic_reserializes_byte_for_byte`).
- **Cut with:** `sample_tap.py` in this directory
  (`python3 sample_tap.py <recording.tap> choker-lens-sample.txt`). It
  reassembles each direction into lines and samples `M!` lines evenly per
  message class: 72 lens and device-card replies (`projectRead.events`), 48
  lens requests (`projectRead.handle`), the 24 heartbeats there were, one
  upload chunk (`filesystem.writeChunk`, host→board), and up to 3 of every
  other class (hello, file writes, loads, listings). The recording has **no
  knob turns** (`panel_write`) and no error replies, so the sample has
  neither.
- **Contents:** no secrets. Board identity in it is the emulated board's MAC
  and build stamp.

The proto-23 sample it replaced (170 lines, 171,982 bytes, cut from
`after-merge-proto23-choker-2026-09-24.tap`), the proto-22 and proto-21 ones
before it, and the pre-lean-wire one are in git history; their lines no
longer parse into the wire types.

Do not hand-edit it: re-cut it from a recording, and update the counts above
and the numbers the tests assert.
