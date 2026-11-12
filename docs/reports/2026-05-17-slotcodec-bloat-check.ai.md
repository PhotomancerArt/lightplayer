# SlotCodec Firmware Bloat Check

Date: 2026-05-17

## Purpose

Check whether the SlotCodec/domain serialization work reduced firmware code
size, and whether keeping Serde available is still reasonable for non-slot
surfaces.

This measurement was taken after merging `main` into
`feature/lightplayer-serialize` so the native GLSL frontend work was present on
both sides of the comparison.

## Trees Compared

- Branch: `/Users/yona/dev/photomancer/feature/lightplayer-serialize`
- Main comparison: `/Users/yona/dev/photomancer/lp2025`

The branch included:

- slot-native authored project/node loading
- `lpc-source` retired from the active workspace
- `lp-vis/lpv-model` disabled from workspace membership
- Serde kept in `lpc-model`/`lpc-wire` for protocol/tooling surfaces

## Commands

Run from the repository root unless the command says otherwise.

```bash
cd lp-fw/fw-esp32
cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6
```

```bash
stat -f '%z bytes %N' ../../target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32
```

```bash
cd lp-fw/fw-esp32
cargo bloat --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6 -p fw-esp32 --crates -n 40
cargo bloat --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6 -p fw-esp32 --filter serde -n 30
cargo bloat --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6 -p fw-esp32 --filter lpc_model -n 25
```

Important: run `cargo bloat` from `lp-fw/fw-esp32`, not the repository root, so
the firmware crate's `.cargo/config.toml` linker setup is used.

## Results

### ELF Size

| Tree | Firmware ELF |
| --- | ---: |
| `lp2025/main` comparison | `3,615,172` bytes |
| `feature/lightplayer-serialize` | `3,463,444` bytes |

Delta: branch is `151,728` bytes smaller.

### Crate-Level Text Size

| Crate | Main Comparison | Branch | Delta |
| --- | ---: | ---: | ---: |
| `lpc_model` | `263.8 KiB` | `175.0 KiB` | `-88.8 KiB` |
| `lpc_wire` | `27.1 KiB` | `21.5 KiB` | `-5.6 KiB` |
| `serde_core` | `22.7 KiB` | `22.2 KiB` | `-0.5 KiB` |
| `toml` | `28.9 KiB` | `28.2 KiB` | `-0.7 KiB` |

Branch direct `cargo bloat --filter serde` result: `274 B`.

## Interpretation

The measured win is not from deleting Serde wholesale. The win is from removing
large per-domain-model Serde-generated authored loading paths and replacing
them with SlotCodec/generic slot mutation.

`serde_core` remains a modest flat cost around 22 KiB. Keeping Serde for small
protocol envelopes, tests, and host/tooling surfaces is reasonable unless a
future measurement shows it growing.

Policy implication:

- Slot-authored domain data should stay on SlotCodec for firmware.
- Avoid reintroducing serde-derived parsers for full slot/domain trees.
- Keep Serde available for non-slot shells when it avoids custom code and does
  not materially affect firmware size.

## Follow-Up

- Re-run this check after large domain model additions.
- Re-run if new wire paths serialize `SlotData`, `SlotShape`, or `LpValue`
  heavily through Serde.
- Keep `lpc-wire` runtime dependencies no-std clean. In this pass, a normal
  `toml` dependency in `lpc-wire` pulled `toml/std` into firmware; it belonged
  in dev-dependencies only.
