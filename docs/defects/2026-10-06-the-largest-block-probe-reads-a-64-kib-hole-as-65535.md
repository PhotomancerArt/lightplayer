---
status: fixed
found: 2026-10-06      # how: e2e (PR C's emulated Wi-Fi walk, an upload over the LAN)
fixed: this change
area: fw-esp32c6 / fw-esp32s3 / fw-esp32v3 `recovery::panic_path::largest_free_block` × lpa-server `check_load_headroom`
class: rounded-measurement-at-threshold
related:
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989; PR C's emulated walk)
---
# The largest-block probe reads a 64 KiB hole as 65,535 B

**Symptom** — an emulated C6 joined to the virtual LAN refused an upload of
`projects/test/basic`:
`load refused: heap headroom too low (largest free block 65535 B < 65536 B)`.
The board's own heap map, printed in the same run, showed a free hole of
exactly 65,536 B: the whole of the C6's second heap region
(`0x4086e610..0x4087e610`).

**Root cause** — `largest_free_block()` bisects for the largest allocation
that succeeds, and stopped once its bracket was within `GRANULARITY` (16 B),
returning the lower edge. Its answer could be up to 16 B below the truth.
That is harmless as a report and wrong as a gate input: the project load
gate (`PROJECT_LOAD_MIN_HEADROOM_BYTES`, 64 KiB) compares it with a round
floor, and the C6 has a heap region that is exactly that floor. When that
region is the largest hole, a board that could take the 64 KiB ask is
refused. The same function, with the same constant, is on `main` for the
C6, the S3 and the classic.

**Fix** — the bisection is `fw_esp32_common::largest_block::largest_fitting`,
which searches to the exact byte (about log2(free) probes, four more than
before). All three chips' `largest_free_block` call it. This fixes `main`
too: nothing about it is specific to Wi-Fi.

**Regression coverage** — `fw_esp32_common::largest_block` tests:
`a_whole_64_kib_hole_reads_as_65536` (the observed case, under several
upper bounds), `every_size_is_found_exactly`,
`the_upper_bound_itself_can_fit`.

**Lesson** — a measurement fed into a threshold must be at least as precise
as the threshold, or rounded *towards* the threshold's side. A "close
enough" estimate is close enough only for reports. Here the rounding was
16 B and the margin that mattered was 1 B, because a hardware region and
the software floor are the same power of two.
