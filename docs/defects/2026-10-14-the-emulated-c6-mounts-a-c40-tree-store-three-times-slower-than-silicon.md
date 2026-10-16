---
status: open           # found on CX1; mechanism not yet named
found: 2026-10-14      # hardware-walk (the tree store's CX1 sitting, M7)
area: lp-emu/esp/lp-emu-esp32c6 (t1 timing) against fw-esp32c6 `fs-tree`'s mount
class: emulator-fidelity
related:
  - lp2025/2026-10-08-2339-tree-store-firmware-and-emulator
  - docs/reports/2026-10-10-tree-store-emulator-walk.md
  - docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md
---
# The emulated C6 mounts a c40 tree store three times slower than silicon does

**Symptom.** The firmware's `[perf] fs mount <us> us` line (`heap_peak_diag`)
times the filesystem's mount. With the same c40-shaped project stored
(132 files, ~216 KB) and the same image features, it reads:

| | emulated (`lp-emu:esp32c6:t1+net=lan`, lp-emu `535f7d135`) | silicon (CX1, 2026-10-14) |
|---|---:|---:|
| `fs-tree`, c40 | **277,785 µs** | **92,700 µs** (n=20, 92,682–93,969) |
| `fs-tree`, empty | 3,723 µs | 7,630 µs |
| littlefs, c40 | 3,116 µs | 7,466 µs |
| littlefs, empty | 233 µs | 738 µs |

The emulator's flash commands take no time
(`docs/debt/emulated-flash-commands-take-no-time.md`), so every emulated row
should be *below* silicon's. Three are, by 2–3x, as expected. The c40
tree-store mount is **3x above** silicon. That is CPU time alone over-running
silicon's CPU plus flash. `t1` charges one cycle per instruction at 160 MHz,
and silicon cannot beat one instruction per cycle. So either the emulated mount
retires about 3x the instructions silicon's does, or the emulated clock
advances by something other than instructions during it.

**What it misled.** The emulator walk reported the tree store's mount at
"~89x littlefs's". On silicon it is 12.4x (92.7 ms against 7.5 ms). The
emulated number was quoted as the reason to measure mount on CX1, which
was the right call.

**Not known yet.** Which side does the extra work. The mount reads the
directory tree (38,964 SPI1 reads at c40, against 1,122 empty) and hashes
through the SHA accelerator (`busy` always reads 0 in the model). A first
step: count retired instructions across the emulated mount, then compare
them against a silicon `mcycle`/`minstret` read across the same span.
This may share a mechanism with the graphics-stage defect (related), which
also over-charges guest time on a load path.

**Until fixed.** Do not quote emulated tree-store mount time as an estimate
of silicon's. The silicon figure is in the walk report's CX1 section.

**More silicon, 2026-10-15 (M7, CX1, `55422b0e8`).** The perf sitting read
`[perf] fs mount` at these states:

- empty: 8.16 ms;
- a c40 pushed by `replace_and_load_project`: 136.0 ms (n=3; #1135's c40,
  uploaded with `lp-cli upload`, read 92.7 ms);
- nearly full (c40 plus 127 × 4 KiB, 3 sectors free): 780.8 ms;
- a partition written out by the power-cut soak (two basic projects to a c40
  live, the rest garbage and GC copies): 560 ms median.

The emulated c40 figure (277.8 ms) is still about 2x silicon's. A sibling
defect, which runs the other way for request and load paths, is
`2026-10-15-the-emulated-c6-answers-saves-and-loads-two-to-three-times-faster-than-silicon.md`.
