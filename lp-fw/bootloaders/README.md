# Vendored second-stage bootloaders

The merged images `lp-cli firmware package` builds start with an ESP-IDF
second-stage bootloader. A build def names the file here it merges
(`bootloader` in `lp-fw/builds/<id>.json`, passed to `espflash save-image
--bootloader`); a def that names none gets the one the installed espflash
bundles (espflash 3.3.0 → ESP-IDF `v5.1-beta1-378-gea5e0ff298-dirt` for the
C6).

**The C6 ships `esp32c6-bootloader-idf-v5.1-quiet.bin`** (`esp32c6-4mb.json`,
and `lp-fw-split`'s default, so every split image — `just fw-esp32c6-split`,
the emulator walks and gates, Studio's install bundle, the release — carries
it). It is espflash 3.3.0's bundled bootloader rebuilt from the same ESP-IDF
commit with its log level set to NONE: the stock one prints ~22 lines on
UART0 at 115,200 baud, ~145 ms of every boot. It is built in Docker, from a
pinned image, reproducibly:

```bash
just build-c6-bootloader                      # rebuild it, and check it against the committed file
scripts/bootloader/build-c6-bootloader.sh --variant stock --check   # espflash's own, byte for byte
```

Everything about it — upstream commit, image digest, the sdkconfig diff, the
proof — is `esp32c6-bootloader-idf-v5.1-quiet.provenance.txt`.

**A bootloader reaches a board only over USB.** Nothing over the air ever
writes `0x0` (the OTA protocol writes the core and the engine inside
`factory`), so a board flashed before this change keeps the loud bootloader
until its next USB install. Both are the same program; only the logging
differs. A bad bootloader is always recoverable: the C6's ROM download mode
does not depend on it.

Changing the bootloader changes the ROM `Saved PC` ranges Studio's
hung-bootloader detection keys on: add the new file's code segments to
`lpa_devices::bootloader` (a table of every bootloader a board in the field
may carry) and, if the union moves, the copy in `scripts/c6-lp-ana-i2c.py`
— the packager refuses an image whose bootloader is not one the table
knows. The emulator's silicon-transcript gates (`rom_up_boot.rs`, the
validate runner, `scripts/emu/build-merged-image.sh`) keep espflash's bundled
bootloader on purpose: they diff a boot log against a capture of THAT
program.

## Files

| File | Source | ESP-IDF | sha256 | License |
|---|---|---|---|---|
| `esp32c6-bootloader-idf-v5.1-quiet.bin` | built by `scripts/bootloader/build-c6-bootloader.sh` (`espressif/idf:v5.1` pinned by digest), `CONFIG_BOOTLOADER_LOG_LEVEL_NONE=y` | `ea5e0ff298e6257b31d8e0c81435e6d3937f04c7` (release/v5.1), unmodified | `933f77385330420aa0101851e8f21d28d76111c7981b7adf009789ced1b372fd` | Apache-2.0 — `licenses/ESP-IDF-Apache-2.0.txt` (byte-identical to ESP-IDF's `LICENSE` at that commit) |
| `esp32c6-bootloader-idf-v5.5.1.bin` | `espflash-4.5.0.crate` (crates.io), `resources/bootloaders/esp32c6-bootloader.bin`, byte-identical | `v5.5.1-838-gd66ebb86d2e` (from the binary's version string) | `402c2c64761034e10b36ca8699d641e2bc0c86fd245a859d78e1aae4e2d201cd` | Apache-2.0 — `licenses/ESP-IDF-Apache-2.0.txt` (ESP-IDF's `LICENSE` at v5.5.1) |

Image header of `esp32c6-bootloader-idf-v5.1-quiet.bin` (13,280 bytes):

```text
seg1 0x4086c410 len 0xa98    (data)
seg2 0x4086e610 len 0x2404   (code)
seg3 0x40875720 len 0x4e8    (code)
```

Image header of `esp32c6-bootloader-idf-v5.5.1.bin` (22,592 bytes; the
partition table sits at 0x8000, so anything under 32 KiB fits). Not shipped:
it does not hang on the LP analog I2C state a factory-fresh board's firmware
leaves (`docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`;
the flashers restore that state instead), and adopting it waits on a
hardware walk.

```text
seg0 0x40875730 len 0x175c   (code, loader)
seg1 0x4086b910 len 0xec8    (data)
seg2 0x4086e610 len 0x31c4   (code)
```

Both binaries are build products of Apache-2.0 sources, unmodified; no GPL
material is involved (AGENTS.md, license discipline).
