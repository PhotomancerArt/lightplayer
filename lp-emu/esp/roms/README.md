# Vendored Espressif mask-ROM images

These are the on-chip mask ROMs, as ELF files, exactly as Espressif publishes
them. They are **not** built here and nothing here is derived from them: they
are committed verbatim, with their checksums and their licence.

## Provenance

| | |
|---|---|
| Repository | <https://github.com/espressif/esp-rom-elfs> |
| Release | `20260528` |
| Artifact | `esp-rom-elfs-20260528.tar.gz`, 4,902,355 bytes |
| Tarball sha256 | `caa463d3cbef2430a5a35847c1d9f2f152403b17a802050927ff60c8da54fe46` |
| Licence | Apache-2.0 — `LICENSE` here, and `licenses/Apache-2.0.txt` at the repo root |
| Fetched by | `scripts/emu/fetch-rom-elfs.sh` |

`SHA256SUMS` records the tarball and each vendored file. The tarball's own
line is provenance, not a local check — the tarball is not committed. The ELF
lines are checked two ways: `scripts/emu/fetch-rom-elfs.sh --check` (offline,
no cargo) and a unit test in `lp-emu-esp32c6`, so a corrupted or swapped ROM
fails the build's tests rather than a boot at cycle 400,000.

## What is here

| File | Chip | Bytes | Notes |
|---|---|---|---|
| `esp32c6_rev0_rom.elf` | ESP32-C6, revision 0 | 489,768 | 3,876 symbol-table entries; the only chip plan one needs |

The classic ESP32 (rev0 and rev300) and the S3 arrive with plan three; the
release tarball carries seventeen chips and we vendor only what code loads.
Adding one is one line in `WANTED` in the fetch script, then re-running it.

## Why the ROM is here at all

Vision D6 and plan PD7: **the ROM is loaded in every configuration.** The
application calls into the mask ROM at runtime whatever booted it —
`rtc_get_reset_reason` from `__pre_init` before `.bss` is even zeroed,
`ets_delay_us` from every clock path, `uart_tx_one_char` from esp-println,
`esp_rom_spiflash_*` from esp-storage. Running the real ROM instead of
intercepting it removes a whole class of divergence by construction (the
vendor emulator's printf padding was one). What stays optional is running the
boot chain from reset, which is M7.

## Rules

- **Never edit an ELF, and never edit `SHA256SUMS` to make a check pass.**
  Re-run `scripts/emu/fetch-rom-elfs.sh`, which re-derives both from the
  published tarball. A sums file edited to agree with a changed binary is a
  checksum that has stopped meaning anything.
- The images are Apache-2.0 and this directory is inside the `lp-emu/` MIT
  fence. That is not a contradiction: Apache-2.0 material may be redistributed
  under its own terms alongside MIT code, which is why `LICENSE` sits next to
  the files rather than being folded into `lp-emu/LICENSE-MIT`.
