---
status: open
found: 2026-10-06      # how: e2e (walk-ble-emu on PR #880 after merging #971)
area: scripts/emu/emulated-lane.mjs `{fw}` × lp-cli emu serve `kind=elf` × the C6 split image (#971)
class: stand-in-divergence
related:
  - docs/defects/2026-10-06-a-bluetooth-reconnect-reads-the-old-links-loss.md
  - docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md (#971)
---
# The walks' `{fw}` board boots the split image core-only

**Symptom** — after #971, `walk-ble-emu`'s board `c6-a={fw}` never
identified. The card read "Unrecognized firmware" and its console said
`[CORE] boot state not trusted (`factory` cannot hold this layout under
this MMU page)` and `[OTA] core-only: no engine (engine does not fit)`.

**Root cause** — `{fw}` substitutes the packaged ELF
(`target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6`) and `emu
serve` direct-loads it (`kind=elf`) over a blank flash file. Since the C6
ships split (loader, core, engine), the engine lives in flash at its own
offset, not in the ELF's loaded image, so the core finds no engine and stays
core-only. The ELF used to be the whole firmware.

**Fix (partial)** — `emulated-lane.mjs` gains `{merged}`, the packaged
whole chip (`target/studio-web-assets/firmware/esp32c6-4mb/fw-esp32c6-merged.bin`),
and `walk-ble-emu` boots `c6-a={merged},kind=rom-up`: a writable chip
seeded from it, from the reset vector. **Still on `{fw}`**:
`walk-wifi-emu`, `walk-drop-emu` and the device scenarios s2, s3, s7, s8
and s9 (`scripts/device-scenarios/*.json`). They should get the same change,
with each scenario's golden trace re-checked, since a ROM-up boot says more
before the hello than a direct load does.

**Regression coverage** — none yet: the walks are not CI jobs.

**Lesson** — a token that names "the firmware" must name a whole bootable
thing. When the image became several pieces, every place that loaded one
piece kept working at the build step and stopped working at boot.
