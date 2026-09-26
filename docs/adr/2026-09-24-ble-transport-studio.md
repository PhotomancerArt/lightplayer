# ADR: BLE Transport, Studio Side — folded into `2026-09-24-ble-transport.md`

- **Status:** Superseded (folded, 2026-09-25)
- **Date:** 2026-09-24
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** `2026-09-24-ble-transport.md`, section "Studio side
  (BLE M5/M6)"

This ADR was written as its own file only because the firmware half's ADR
was not on `main` yet when BLE M5 landed; it said it would be folded in once
both had. It has been, unchanged except for the numbering: its decisions 1–7
are **S1–S7** in `2026-09-24-ble-transport.md`, so a reference to "§3" here
is S3 there (silent reconnect, visibility is "state unknown"). Its
2026-09-25 amendment (items 6 and 7 replaced by
`2026-09-24-easy-bluetooth-access.md`) moved with it.

The file stays as a pointer so the references to it in code comments,
defects and the easy-access ADR still resolve.
