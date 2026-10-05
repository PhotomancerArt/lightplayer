---
status: fixed
found: 2026-10-05      # how: hardware-walk — PR #971's P10 silicon smoke, bench C6 A0:F2:62:87:B4:8C
fixed: this change
area: tools/lp-fw-split app_image + lp-cli firmware package + lpa-link host_serial_esp32 (host flasher)
class: silent-drop
related:
  - docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md
  - lp2025/2026-10-04-0005-ota-split-image-ships (P10; evidence in data/p10-a0f26287b48c/)
  - docs/defects/2026-10-02-the-host-filesystem-read-throws-away-in-flight-packets.md
---
# The host flasher dropped the split image's last 254 bytes, and the engine faulted on every boot

**Symptom** — the packaged split image of `76959e7a4`
(`fw-esp32c6-merged.bin`, 3,102,206 B, sha256 `bb7cf97c…`), written to the
bench C6 by the host flasher (espflash 3.3.0's `write_bin_to_flash`, driven
through Studio's own layout decision), booted the loader, the core
`(proven)` and the hello, then faulted in the engine on every boot:
`Exception 'Illegal instruction' mepc=0x425bd5f0, mtval=0xffffffff`. After
four crashes the firmware's recovery disabled the board's project. Reading
the flash back showed it equal to the image up to `0x2F54FF`. The last 254
bytes (`0x2F5500–0x2F55FD`) read `0xFF`: the engine's last functions
(`OUTLINED_FUNCTION_137` and others) were never written.

**Root cause** — two facts met.
- `lp-fw-split` laid out `app.bin` to end exactly where the engine ends, and
  the packager cut the merged image at `app.bin`'s end. The engine's length
  is whatever the link produced, so the image's length (`0x2F55FE`) was not
  a multiple of 4.
- espflash 3.3.0's `write_segment` gives its stub (`FLASH_DEFL_BEGIN`) the
  raw length and does not pad it. The flash held the image up to its last
  256-byte program-page boundary; the partial page after it was never
  written. (Read from the evidence: the stub's own code was not traced.)
  Nothing checked:
  the host flasher's connections run with `verify` off, and nothing read the
  range back. esptool-js 0.6.0 (Studio's browser flasher) pads every image
  to 4 bytes (`padTo(image, 4)` in `writeFlash`), which is why Studio's path,
  and so `walk-no-board`, never showed it. The emulator's flash model has no
  partial-page rule to break.

The core started an engine whose body was short because the engine header's
CRC covers the header only. Plan D20 chose that deliberately: no per-boot
body hash. A full body check is the update protocol's job, which verifies
SHA-256 before it commits a header. A flasher bypasses that.

**Fix** —
- Every flashed image now ends on a 4 KiB flash sector, padded with `0xFF`
  (`lp_fw_split::image_end`). A sector is a multiple of every unit a flasher
  works in, and the padding is the bytes the flasher's erase leaves anyway.
  `app_image::assemble` pads `app.bin`, so the split image the packager cuts
  from it ends on a sector. `lp-cli firmware package` pads every packaged
  image (the monolithic ones too) and refuses one that does not end on a
  sector.
- The host flasher proves each write. `write_verified` asks the stub for
  the written range's MD5 after the write has finished and compares it with
  the image's. A mismatch fails the flash loudly, naming the range, before
  the board is reset. It is used by the plain flash, the layout plan's
  `WriteFirmware` and its file-moving writes. (The boot-control record's
  16-byte write resets on finish and is not checked; its own CRC guards it.)
- Studio's browser flasher pads already, and the packaged image no longer
  needs it. It does not verify (no `calculateMD5Hash`). That gap was not
  exposed here and is left as a follow-up.

**Regression coverage** —
`lp-fw-split` `app_image::tests::the_76959e7a4_sizes_end_on_a_sector` (the
failing image's own sizes; it fails with the padding removed),
`every_engine_length_ends_on_a_sector`,
`an_engine_that_fills_the_region_still_fits_padded`, `image_end::tests::*`;
`lp-cli` `firmware::package::tests::every_packaged_image_ends_on_a_sector`;
`lpa-link` `host_esp32_flash::tests::a_short_write_is_refused_by_its_md5`.
The MD5 check itself talks to a real stub and has no host test; P10's
re-run exercises it on silicon.

**Lesson** — "the flasher writes these bytes" was an assumption no gate
held. The same image written by two flashers was two different flashes, and
the emulator, being a third, agreed with the one that padded. A write is
not done until its range has been read back or hashed. An image's length is
a format fact: give it a rule and a test, rather than leaving it to
whatever the linker produced.
