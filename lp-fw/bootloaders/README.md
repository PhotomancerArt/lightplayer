# Vendored second-stage bootloaders

The merged images `lp-cli firmware package` builds start with an ESP-IDF
second-stage bootloader. By default that is the one the installed espflash
bundles (espflash 3.3.0 → ESP-IDF `v5.1-beta1-378-gea5e0ff298-dirt` for the
C6). A build def can name a file here instead (`bootloader` in
`lp-fw/builds/<id>.json`, passed to `espflash save-image --bootloader`).

Why we would: the v5.1-beta1 C6 bootloader drives the analog bus through the
LP aperture and hangs after any HP-only reset when the previous firmware
gated the LP analog I2C clock — the first flash on a factory-fresh board.
Current ESP-IDF bootloaders enable that clock themselves and use the HP
aperture. See
`docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`.

Changing the bootloader changes the ROM `Saved PC` ranges Studio's
hung-bootloader detection keys on: update
`lpa_devices::bootloader::bootloader_code_ranges` (and the copy in
`scripts/c6-lp-ana-i2c.py`) from the new file's image header — the packager
refuses the image until the table matches.

## Files

| File | Source | ESP-IDF | sha256 | License |
|---|---|---|---|---|
| `esp32c6-bootloader-idf-v5.5.1.bin` | `espflash-4.5.0.crate` (crates.io), `resources/bootloaders/esp32c6-bootloader.bin`, byte-identical | `v5.5.1-838-gd66ebb86d2e` (from the binary's version string) | `402c2c64761034e10b36ca8699d641e2bc0c86fd245a859d78e1aae4e2d201cd` | Apache-2.0 — `licenses/ESP-IDF-Apache-2.0.txt` (ESP-IDF's `LICENSE` at v5.5.1) |

Image header of `esp32c6-bootloader-idf-v5.5.1.bin` (22,592 bytes; the
partition table sits at 0x8000, so anything under 32 KiB fits):

```text
seg0 0x40875730 len 0x175c   (code, loader)
seg1 0x4086b910 len 0xec8    (data)
seg2 0x4086e610 len 0x31c4   (code)
```

The binary is a build product of Apache-2.0 sources, vendored unmodified;
no GPL material is involved (AGENTS.md, license discipline).
