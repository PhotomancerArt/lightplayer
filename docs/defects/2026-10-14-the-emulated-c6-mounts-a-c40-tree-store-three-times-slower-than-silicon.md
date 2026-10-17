---
status: fixed          # not reproduced on today's emulator; `t4` times the flash and lands within ~3 % (2026-10-17 note)
found: 2026-10-14      # hardware-walk (the tree store's CX1 sitting, M7)
fixed: e4684bd42       # the `t4` time grade
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

**Resolution, 2026-10-17.** Run on the silicon record's own firmware
(`55422b0e8`, rebuilt with `heap_peak_diag`), today's emulator does not
reproduce the 277.8 ms: `t1` mounts the freshly pushed c40 in 52.7 ms — below
silicon, as a grade with free flash must — so whatever over-charged the
2026-10-10 run (`lp-emu` `535f7d135`) is gone from the emulator, unbisected.
What the defect also asked for is now there: the `t4` time grade
(`lp-emu:esp32c6:t4`, `docs/reports/2026-10-17-c6-flash-timing-emulator.md`)
times the flash interface, and mounts that c40 in **89.6 ms against
silicon's 92.7** (0.97). Across four firmwares and nine stores each (cache
reads with and without the index, ROM-call reads with and without it, 3.8 to
290 ms on CX1), `t4` lands at 0.94–1.11 of silicon, the empty stores aside
(+1–1.7 ms, 1.13–1.31, present at `t3` too). **Regression coverage:** none
that gates — emulated time never does; `emu_tree_store_mount` with
`LP_EMU_GRADE=t4` is the reading. **Lesson:** the mount's silicon time was
CPU through the cache plus SPI1's register and wire time; once the SCK was
read from the registers (80 MHz, not the header's 40) the fill and the
transfer both came out of the arithmetic, and the one fitted term left is
SPI1's register access.
