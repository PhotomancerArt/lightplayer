---
status: fixed          # latent: no crash seen; the exposure is proved from the image's symbols and CX1's logs
found: 2026-10-17      # how: report — a layout audit for the own second-stage bootloader plan (R7)
fixed: this change
area: fw-esp32c6 heap placement (`board::esp32c6::init`, `c_heap`) and the update's reset (`ota::reset_now`) × the split image's RAM-only loader
class: untested-path
related:
  - docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md
  - docs/adr/2026-09-02-esp32c6-ram-split.md
  - docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md
  - docs/reports/2026-10-14-tree-store-cx1/ (the overflow on silicon)
  - lp2025/2026-10-17-0130-c6-own-bootloader (R7; notes.md)
---
# The radio heap's overflow could land in the split loader's RAM, and an update's reset skipped the radio quiesce

**Symptom** — none seen as a crash. The exposure is the 2026-10-05
bootloader crash's mechanism, aimed at the split image's loader instead of
the IDF bootloader. Two facts, both read from shipped bytes and logs:

1. **The radio's C heap overflows into the main region on silicon.** CX1
   (`14:c1:9f:e6:54:90`, release 2026.10.14-2, Wi-Fi joined, Cloud relay on)
   logged, in the first heartbeat after a requested reboot:

   ```text
   [radio-heap] heartbeat: high-water 48931 B of 49152 B, 35232 B live, 18520 B overflowed to main
   ```

   Across `docs/reports/2026-10-14-tree-store-cx1/` the overflow is 1,852 to
   50,004 B, in 1,852 B steps: Wi-Fi-buffer-sized blocks, the kind the MAC
   DMAs into.

2. **The top of the main region is the loader's RAM.** The loader is linked
   at `0x40850000..0x40860000` (`loader.x`'s `LRAM`: code from the bottom,
   ~2.7 KB; stack from the top). In CI's shipped split image at main
   `17c6c34d6` (`ESP32C6_SERVER_RADIO_SPLIT/p2.elf`):

   | symbol | range |
   |---|---|
   | `HEAP_RADIO` | `0x40809970..0x40815970` |
   | `HEAP_MAIN` (`Internal`) | `0x40815970..0x408547E8` |
   | other `.bss` | `..0x40862658` |
   | main stack | `0x40862658..0x4086E610` |

   `HEAP_MAIN`'s top 18,408 B (`0x40850000..0x408547E8`) cover the loader's
   code and data (`0x40850000..0x40850A74`) entirely.

The C fallback asks for `Internal`, which `HEAP_MAIN` was registered with as
one region, so an overflowed radio block could be placed anywhere in it,
including over the loader. After an HP-only warm reset the modem keeps
running and writing into its buffers while the ROM, the IDF bootloader and
then the loader run. A write landing in `0x40850000..0x40850A74` after the
IDF bootloader copied the loader there corrupts the code that maps and
copies the core. A write into the stack's top corrupts its return path.

**Root cause** — #985's fix guarded one boot stage's RAM (`dram2_seg`, the
IDF bootloader's) and checked that `HEAP_RADIO` sat clear of it. It did not
check the main region it falls back to, which the split image (#971,
designed the same week) puts a second boot stage under. Separately,
`ota::reset_now` (2ca67d841, the C6 core taking updates) called
`esp_hal::system::software_reset` directly. It was the one firmware reset
that skipped `restart::restart`'s modem quiesce, and every update over
Bluetooth, the LAN or the relay ends with it while the radio is busy.

**Fix**
- **Placement.** `lp_bootctl::BOOT_STAGE_RAM` names the loader's range. The
  firmware registers `HEAP_MAIN` as two regions, cut where the array reaches
  that range (`init::main_region_split`). The part below keeps `Internal`.
  The part from the range up carries no tag, like `dram2_seg`, so only
  Rust's global allocator (no radio DMA) lands there. Rust tries the regions
  in the same address order as before. The cost is contiguity: no block can
  straddle `0x40850000`. On `lp-emu:esp32c6:t1` (the shipped split image's
  first heartbeat) the largest free block falls 187,504 → 169,104 B; free,
  used and total bytes are unchanged.
- **Quiesce.** `ota::reset_now` goes through `restart::restart`, which holds
  the modem in reset before the software reset.
- `radio_dma_diag` flags a live radio block in the loader's RAM as well as
  in `dram2_seg`.

**Regression coverage** — `lp-bootctl`'s `boot_stage_ram` tests: the cut
for the shipped layout, and `BOOT_STAGE_RAM` equal to `loader.x`'s `LRAM`
(the two cannot drift apart unseen). The radio's DMA itself is not
modelled by the emulator, so no test writes through it. The shipped image
boots with the new regions through `test-emu-c6`, and the heap ratchet
(`heap-budget-check-chips-c6`) pins the largest free block they leave.
What is not covered is a host RTS reset, which runs no firmware code: for
it, placement is the whole fix.

**Lesson** — "nothing DMA-capable lives in a boot stage's RAM" has to be
checked against every boot stage and every region a DMA allocation can
reach, fallbacks included, not only the region it normally gets. When a
new boot stage takes RAM, the app's region map is part of its review.
