## Generic ESP32 GPIO Button Dispatch

- **Idea:** Support opening any manifest-approved GPIO input endpoint instead of only the first
  D9/GPIO20 production button path.
- **Why not now:** The example needs the soldered D9 button, and a full dynamic HAL pin dispatch
  table is larger than this slice.
- **Useful context:** `lp-fw/fw-esp32/src/board/esp32c6/init.rs`,
  `lp-fw/fw-esp32/src/hardware/button.rs`, and the Seeed XIAO ESP32-C6 manifest.

## Radio Button Bridge

- **Idea:** Add `RadioSendNode` and `RadioReceiveNode` so button events can drive the sign over
  ESP-NOW.
- **Why not now:** This slice proves local authored button-to-shader behavior first.
- **Useful context:** `docs/roadmaps/2026-05-18-firmware-hardware-io/m3-basic-radio-messages/summary.md`
  and `docs/roadmaps/2026-05-19-events-playlists-radio-nodes/notes.md`.

## Playlist Trigger Consumer

- **Idea:** Add a playlist or trigger-state node that restarts a special visual effect on `down` or
  `held` and returns to idle after a duration.
- **Why not now:** The first example can prove graph-level button messages with one shader circle.
- **Useful context:** `docs/roadmaps/2026-05-19-events-playlists-radio-nodes/notes.md`.

