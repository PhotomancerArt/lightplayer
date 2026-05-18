# M4 Status

## Current Direction

M4 no longer removes Serde wholesale from `lpc-model`.

The attempted removal proved that Serde is still useful around non-slot
surfaces, and the post-merge firmware measurement showed the real win already
comes from moving authored domain loading to generic SlotCodec paths.

Policy:

- Slot-authored domain data uses SlotCodec on firmware.
- Serde remains available for protocol shells, tests, host tooling, and small
  non-slot surfaces.
- Revisit specific Serde paths only when firmware bloat measurements identify
  them as a problem.

## Implemented

- Removed `lpc-source` from the active workspace and dependency graph.
- Deleted the obsolete `lpc-source` crate.
- Moved `ArtifactReadRoot` into `lpc-model`.
- Removed `lp-vis/lpv-model` from workspace membership and documented it as
  disabled reference material.
- Kept Serde derives/dependencies in `lpc-model` and `lpc-wire`.
- Fixed `lpc-wire` so its `toml` dependency is test-only again; the runtime
  dependency pulled `toml/std` into `fw-esp32`.

## Firmware Size Evidence

Measured after merging `main` into `feature/lightplayer-serialize`:

- `feature/lightplayer-serialize`: `3,463,444` bytes
- `lp2025/main` comparison: `3,615,172` bytes
- Delta: branch is `151,728` bytes smaller

Relevant `cargo bloat --crates` entries:

| Crate | Branch | Main Comparison |
| --- | ---: | ---: |
| `lpc_model` | `175.0 KiB` | `263.8 KiB` |
| `lpc_wire` | `21.5 KiB` | `27.1 KiB` |
| `serde_core` | `22.2 KiB` | `22.7 KiB` |
| `toml` | `28.2 KiB` | `28.9 KiB` |

Direct `cargo bloat --filter serde` on the branch reported `274 B`.

See `docs/reports/2026-05-17-slotcodec-bloat-check.md` for commands and
notes.

## Validation

- `cargo check -p lpc-model`
- `cargo check -p lpc-wire`
- `cargo check -p lpa-server`
- `cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6`

## Remaining Cleanup

- Add or update integration tests proving real authored project examples load
  through the slot-native path.
- Keep project writing/TOML output as a separate follow-up.
- Add future bloat checks after large domain model additions.
