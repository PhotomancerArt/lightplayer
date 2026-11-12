---
status: fixed          # fixed before it shipped, in the change that adds the stage
found: 2026-10-17      # emulator bring-up of LightPlayer's own C6 second stage (lp-emu:esp32c6:t1, ROM-up)
fixed: 2026-10-17      # prologue/rom_flash_chip.rs
area: fw-esp32c6-loader (bootloader flavour) × the mask ROM's legacy flash descriptor × esp-storage
class: assumed-context # state the IDF bootloader left that the core silently relied on (plan risk R1)
related:
  - 2026-10-17-the-c6-boots-through-our-own-bootloader   # ADR: the core entry contract, C3
---
# Our second stage left the ROM's flash descriptor at its default size, and the core locked the board's files

**Symptom.** The first ROM-up boot of a split image through LightPlayer's own
second stage reached the engine on the emulator, and the core said
`[FS] this board's files are in the tree store, a newer filesystem than this
firmware reads — not formatting; files kept; using memory FS, access locked`.
The same `app.bin` behind the IDF bootloader mounted its filesystem. The
emulator's split-boot gate caught it (the direct load and the ROM-up boot
disagreed on the core's console), before any board was flashed.

**Cause.** esp-storage reads, writes and erases through the mask ROM's flash
routines, and those refuse any address past the ROM's legacy flash
descriptor's `chip_size` (`rom_spiflash_legacy_data`, a pointer at
`0x4087_FFEC` to six words). The ROM's default is smaller than the 4 MB part.
The IDF bootloader corrects it from the image header
(`esp_rom_spiflash_config_param(id, 4 MB, 64 KiB, 4 KiB, 256, 0xFFFF)`, which
only stores the words); our stage did not. Reads below 2 MB (the partition
table, the loader's identity) worked; every read of `lpfs` at `0x35_0000`
returned an error, and `holds_tree_store` counts an unreadable block as a
tree store. An SPI1 trace of both doors showed the first divergence: the IDF
door read `0x350000`, ours never issued the command.

**Fix.** `prologue/rom_flash_chip.rs` stores the same five words (size,
block, sector, page, status mask), keeping the ROM's device id. It is item C3
of the core entry contract. On silicon (CX1) the filesystem mounts through
our door and OTA writes its records.

**Regression coverage.** `lp-emu-esp32c6/tests/split_boot.rs` boots the
split image through both doors and compares the direct load with the ROM-up
boot line for line; `[INIT] Flash filesystem mounted` is one of those lines.

**Lesson.** "Minimum faithful" needs the measured inventory, and the
inventory is MMIO only: this was a RAM write by the IDF into the ROM's data.
The register diff at core entry was clean before the fix. The behavioural
comparison of the two doors is what caught it.
