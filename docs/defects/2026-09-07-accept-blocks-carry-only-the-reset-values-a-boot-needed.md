---
status: open — one instance fixed, the class is not swept
found: 2026-09-07      # M4 of the esp-emulator plan, on the second-boot gate
fixed: SPI1 and SPI0 only
area: lp-emu/esp/lp-emu-esp32c6/src/periph/ (every `accept::*` RegFile)
class: silent-wrong-default
related: [lp2025/2026-09-06-1001-esp-emulator/m4-flash-and-upload.md, docs/adr/2026-09-06-esp-soc-emulator-architecture.md]
---
# An accept block with a wrong reset value is silent, and can look like success

**Symptom** — the flash-backed C6 image booted, mounted `lpfs`, formatted it,
printed both `[FS]` lines, reached the idle heartbeat, and reported the spike
report §5.1 heap and stack figures. Then a **second** boot from the same
`--flash` file reformatted a filesystem whose littlefs superblock was
demonstrably in the image at `0x0031_0000`.

**Cause** — SPI1's `user` register resets to `0x8000_0000`, with
`usr_command` already set, and the mask ROM's read path never sets it:

```text
_esp_rom_spiflash_read       clears usr_mosi, sets usr_miso
esp_rom_spi_set_rd_cmd_bit_len  sets usr_addr and the dummy length
                              — nothing touches usr_command
```

The block was a `RegFile` whose registers all reset to **0**, so every flash
read went out with no command phase and moved no bytes. Writes were
unaffected (`SPI_page_program` ORs the phase bits in explicitly), which is
what made the failure invisible: a boot that formats a chip it cannot read
looks exactly like a boot that formats a blank one, down to the heap figures.

**Fix, so far** — `periph/spi1.rs` and `periph/spi0.rs` carry every non-zero
reset value the PAC gives (`esp32c6-0.23.2/src/spi{0,1}/*.rs`, each register
module's `impl Resettable`) — derived data with the same provenance as the
generated register-name tables. `tests/flash_persistence.rs` is the gate that
would have caught it, and now does.

Two things fell out of the fix and are worth recording, because both were
*differences that had looked like facts*:

- `largestFreeBlock` went from 199,076 to **199,173**, which is esp-emu's own
  §5.1 figure. A 97 B "the one heap figure that does not transfer" was a bug.
- `[BOOTCTL] unusable record (invalid)` disappeared. An erased boot-control
  sector is *no record*, not an unusable one; the line was the firmware
  reading garbage.

**The open part** — every other accept block in the C6 boot set carries only
the reset values a running boot was *observed* to need (`PCR.sysclk_conf`,
`IO_MUX`'s 31 pads, `RMT.sys_conf`, `LP_CLKRST.lp_clk_conf`, …). The rest
read 0 whether the part does or not. Nothing has bitten yet; the next one
will bite the same way, silently, and be found by a gate that reads back what
a previous run wrote.

**The shape of the sweep, when someone does it** — `scripts/emu/pac-regnames.py`
already reads the PAC to generate `RegNames`; the same pass can emit each
block's non-zero reset values as derived data with the same provenance
header, and `accept::*` can seed from it. That turns "the resets we noticed"
into "the resets the PAC states", and makes a hand-written exception
something a reader sees. It was out of M4's scope.
