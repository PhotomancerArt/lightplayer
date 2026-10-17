# Vendored second-stage bootloaders

**Since 2026-10-17 the C6 product image boots through LightPlayer's own
second stage** (`lp-fw/fw-esp32c6-loader --features bootloader`, built and
written by `tools/lp-fw-split` — `bootloader.bin`, at `0x0` of every split
`merged.bin` and so of every package, release and Studio USB install; ADR
`docs/adr/2026-10-17-the-c6-boots-through-our-own-bootloader.md`). The IDF
bootloaders below remain **the IDF door**: `merged-idf.bin` (the same
`app.bin`), the boards that have only been updated over the air, the
monolithic dev images the `espflash flash` recipes write, and the emulator
gates that boot every split image through both doors. The packaging guard
accepts our stage by its `LPLB` word and checks it as the ROM does; it needs
no `lpa_devices::bootloader` entry (there is no analog-master wedge to detect
in it).

Every C6 USB flash writes a second-stage bootloader at `0x0`, and **the
`esp32c6-4mb` build def's `bootloader` names which file**
(`lp-fw/builds/esp32c6-4mb.json`). Its mirrors — `lp-fw-split`'s default
(`just fw-esp32c6-split`, the emulator's split images and walks), the
justfile's `c6_bootloader` (its `espflash flash` recipes) and the crate's
`cargo run` runner — are held to it by
`c6_bootloader_mirrors_agree_with_the_def` in lp-cli. From the def it reaches
`lp-cli firmware package`, so the release, Studio's USB install bundle and
`lp-cli firmware install`. A def that names none gets the one the installed
espflash bundles.

**The C6 ships `esp32c6-bootloader-idf-v5.1.bin`**: espflash 3.3.0's bundled
bootloader, rebuilt from ESP-IDF source and byte-identical to it, so naming
it changed no flashed byte. Choosing another bootloader is one line in the
def (and its mirrors), plus an entry in `lpa_devices::bootloader` (below).

Both ESP-IDF binaries here are built in Docker, from a pinned image,
reproducibly, with nothing installed on the host:

```bash
just build-c6-bootloader              # stock: rebuild the shipped one, check it against the committed file AND espflash's
just build-c6-bootloader quiet        # the quiet fallback, checked against its committed file
```

The opt-in `Bootloader reproducible` workflow runs both on x64 whenever the
recipe or a binary changes.

**A bootloader reaches a board only over USB.** Nothing over the air ever
writes `0x0`, so a board keeps the bootloader its last USB install wrote.
A bad bootloader is always recoverable: the C6's ROM download mode does not
depend on it.

Changing the bootloader changes the ROM `Saved PC` ranges Studio's
hung-bootloader detection keys on: add the new file's code segments to
`lpa_devices::bootloader` (a table of every bootloader a board in the field
may carry) and, if the detection ranges move, the copy in
`scripts/c6-lp-ana-i2c.py` — the packager refuses an image whose bootloader
is not one the table knows. The emulator's silicon-transcript gates
(`rom_up_boot.rs`, the validate runner, `scripts/emu/build-merged-image.sh`)
use espflash's bundled bootloader directly on purpose: they diff a boot log
against a capture of THAT program.

## Files

| File | Source | ESP-IDF | sha256 | License |
|---|---|---|---|---|
| `esp32c6-bootloader-idf-v5.1.bin` (**shipped**) | built by `scripts/bootloader/build-c6-bootloader.sh --variant stock` (`espressif/idf:v5.1` pinned by digest), ESP-IDF's defaults; byte-identical to `espflash-3.3.0.crate`'s `resources/bootloaders/esp32c6-bootloader.bin` | `ea5e0ff298e6257b31d8e0c81435e6d3937f04c7` (release/v5.1; `v5.1-beta1-378` in its banner), unmodified | `c569fa7d2363da15f6b60ea288aa2396ca3c33b6eed84616bb506feaba7a0528` | Apache-2.0 — `licenses/ESP-IDF-Apache-2.0.txt` (byte-identical to ESP-IDF's `LICENSE` at that commit) |
| `esp32c6-bootloader-idf-v5.1-quiet.bin` (fallback, not shipped) | the same, `--variant quiet`: `CONFIG_BOOTLOADER_LOG_LEVEL_NONE=y` | the same commit, unmodified | `933f77385330420aa0101851e8f21d28d76111c7981b7adf009789ced1b372fd` | the same |
| `esp32c6-bootloader-idf-v5.5.1.bin` (not shipped) | `espflash-4.5.0.crate` (crates.io), `resources/bootloaders/esp32c6-bootloader.bin`, byte-identical | `v5.5.1-838-gd66ebb86d2e` (from the binary's version string) | `402c2c64761034e10b36ca8699d641e2bc0c86fd245a859d78e1aae4e2d201cd` | Apache-2.0 — `licenses/ESP-IDF-Apache-2.0.txt` (ESP-IDF's `LICENSE` at v5.5.1) |

Provenance for the two ESP-IDF v5.1 builds (upstream commit, image digest,
sdkconfig diff, the reproducibility proof):
`esp32c6-bootloader-idf-v5.1-quiet.provenance.txt`, which covers the stock
variant too.

The quiet bootloader prints nothing on UART0 during a good boot; the stock
one prints ~22 lines at 115,200 baud, ~145 ms of every boot (the A8 first-light
breakdown, `docs/reports/2026-10-16-c6-time-to-first-light.md`). Its asserts
and `abort()` still print (`esp_rom_printf`). It has been booted on the
emulator only, never on silicon.

Image headers (code segments are what `lpa_devices::bootloader` keys on):

```text
esp32c6-bootloader-idf-v5.1.bin        (21,264 B)
seg1 0x4086c410 len 0xd48    (data)
seg2 0x4086e610 len 0x2d68   (code)
seg3 0x40875720 len 0x1800   (code)

esp32c6-bootloader-idf-v5.1-quiet.bin  (13,280 B)
seg1 0x4086c410 len 0xa98    (data)
seg2 0x4086e610 len 0x2404   (code)
seg3 0x40875720 len 0x4e8    (code)

esp32c6-bootloader-idf-v5.5.1.bin      (22,592 B)
seg0 0x40875730 len 0x175c   (code, loader)
seg1 0x4086b910 len 0xec8    (data)
seg2 0x4086e610 len 0x31c4   (code)
```

The v5.5.1 one does not hang on the LP analog I2C state a factory-fresh
board's firmware leaves
(`docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`;
the flashers restore that state instead); adopting it waits on a hardware
walk. Anything under 32 KiB fits: the partition table sits at 0x8000.

All three are build products of Apache-2.0 sources, unmodified; no GPL
material is involved (AGENTS.md, license discipline).
