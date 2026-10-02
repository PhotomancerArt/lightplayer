---
status: fixed
found: 2026-10-02      # how: e2e — the C6 repartition's emulator walk (P08 step 0)
fixed: 244d3d59c
area: lp-app/lpa-link providers/host_serial_esp32 (read_flash_region)
class: assumed-context
related:
  - lp2025/2026-10-01-1843-c6-repartition
  - lp-cli/tests/emu_layout_migration.rs
---
# The host's bootloader read throws away the next packet: `ESP_READ_FLASH` "truncated at 8185 of 983040 bytes"

**Symptom** — `lp-cli hardware lpfs save` (the host `ReadRawFilesystem`)
against the emulated C6, over a pty into its download console: the
partition table (3 KB, one packet) read back fine; the 960 KB filesystem
failed `filesystem read truncated at 8185 of 983040 bytes`, then `8147`
on the next run — a different count every time, always in the second
packet. The emulator had handed every byte to the host
(`usb_sj_tried` empty; 11,872 bytes delivered).

**Root cause** — the read asked the stub for **1024 packets in flight**
(`READ_MAX_IN_FLIGHT`, the M1 spike's figure), so the stub streams blocks
ahead of the host's acks. Each ack goes through espflash 3.3's
`Connection::write_raw`, which **clears the port's input buffer** before it
writes. Whatever part of the next block had already arrived when the ack
was written was discarded, and the next frame decoded short. On silicon
the race is usually won because USB is fast against a host that is idle;
the emulated board (2–3x slower than real time, with the host faster
relative to it) lost it every run. A busy laptop can lose it against a
real board too — the code assumed the input buffer was empty at every ack.

**Fix** — one packet in flight (`READ_MAX_IN_FLIGHT = 1`): the stub sends
nothing until it has the ack, so the clear finds nothing to lose. Costs one
round trip per 4 KB block (~240 for a C6 filesystem). The browser path
(esptool-js) never clears its input on an ack and is unaffected.

**Regression coverage** —
`lp-cli/tests/emu_layout_migration.rs::the_bootloader_reads_back_the_chip_byte_for_byte`
(the table and the whole 960 KB region, byte for byte against the chip
file; `just test-emu-layout-migration`).

**Lesson** — a library's "write" can have a read-side effect. Any protocol
that pipelines (in-flight window > 1) through a transport whose writes flush
input is a race the faster side always wins until it doesn't; the emulator
found it because its timing is different, which is exactly what a second
clock is for.
