---
status: open           # found on CX1; mechanism not yet named
found: 2026-10-15      # hardware-walk (the tree store's M7 perf sitting on CX1)
area: lp-emu/esp/lp-emu-esp32c6 (t1 timing) against fw-esp32c6 request and load paths
class: emulator-fidelity
related:
  - lp2025/2026-10-08-1017-tree-store-device-round
  - docs/reports/2026-10-15-tree-store-hardware-validation.md
  - docs/reports/2026-10-10-tree-store-emulator-walk.md
  - docs/debt/emulated-flash-commands-take-no-time.md
  - docs/defects/2026-10-14-the-emulated-c6-mounts-a-c40-tree-store-three-times-slower-than-silicon.md
---
# The emulated C6 answers saves and project loads 2–3x faster than silicon

**Symptom.** M5's emulated perf table and M7's silicon sitting ran the same
operations on the same project shape (c40). The emulated side was
`lp-emu:esp32c6:t1+net=lan`, lp-emu `535f7d135`, round trips in emulated
time. The silicon side was CX1 `14:C1:9F:E6:54:90`, firmware `55422b0e8`
(fs-tree) and `7a95d4d46` (littlefs, the same `lp-fw` sources), round trips on
the host's wall clock over USB. Medians:

| operation | fs-tree emulated | fs-tree silicon | littlefs emulated | littlefs silicon |
|---|---:|---:|---:|---:|
| save (1.7 KB document, written whole) | 88.0 ms | 235.8 ms (2.7x) | 115.3 ms | 326.4 ms (2.8x) |
| 4 KiB append | 87.3 ms | 199.4 ms (2.3x) | 89.3 ms | 272.9 ms (3.1x) |
| read the largest c40 file | 35.7 ms | 54.7 ms (1.5x) | 34.9 ms | 54.3 ms (1.6x) |
| whole-project load (stop + LoadProject c40) | 71.3 ms | 233.8 ms (3.3x) | 75.8 ms | 250.4 ms (3.3x) |

**Why it is a defect and not just flash time.** The emulator's flash commands
take no time (the debt entry), so silicon should be slower by roughly its
flash work. It is slower by much more.

- A save's flash work is 89 programs, 2,443 B and no erase: an ESTIMATE of
  ≈ 5 ms on silicon. Silicon adds ≈ 150 ms.
- A whole-project load writes almost nothing, and it is still 3.3x.

The gap is the same for both filesystems, so it is not the tree store's.
The relative figures transfer: fs-tree ÷ littlefs is 0.72 for a save on
silicon (0.76 emulated), 1.0 for a read (1.02), and 0.93 for a load (0.94).

**Not known yet.** Where silicon spends the time the `t1` grade does not
charge. `t1` counts one cycle per instruction. On silicon, code runs from
flash through the cache (DIO, 40 MHz on this board), and a compile- or
lookup-heavy path misses it. esp-storage's reads also disable the cache
while they run. Both are candidates. So is the radio (on, unjoined), which
the emulated run had in a different state. A first step: read `mcycle` and
`minstret` across one load on silicon and compare them with the emulated
instruction count.

**Until fixed.** Quote emulated request and load times only relative to
another emulated figure (image against image), never as a silicon estimate.
The silicon figures are in the M7 report.
