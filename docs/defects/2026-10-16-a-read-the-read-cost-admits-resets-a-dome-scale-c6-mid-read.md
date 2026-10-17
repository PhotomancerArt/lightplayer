---
status: open
found: 2026-10-16      # report: RAM research M1's stack census, zook-dome with editor reads on the bigger-heap image (lp-emu:esp32c6:t1)
area: lpa-server read admission (ReadCost, #1088) × fw-esp32c6 heap regions
class: config-masked-defect   # main's smaller heap refused the read cleanly, so the estimate's flaw never ran
related:
  - 2026-10-09-a-phones-bluetooth-link-leaves-the-choker-under-the-read-gate.md   # the per-request ReadCost this entry is about
  - ../adr/2026-09-02-esp32c6-ram-split.md   # 2026-10-16 amendment: HEAP_MAIN +71,008 B
  - lp2025/2026-10-09-1203-ram-research   # M1 evidence: census-B consoles
---
# A read the ReadCost check admits resets a dome-scale C6 halfway through

**Symptom** — `catalog/projects/zook-dome` (1,500 lamps, five strips) on the
emulated C6 with the 2026-10-16 heap (372,544 B), Studio's staged first sync
after the load: the output-frame probe (`{"output_frame":{"geometry":"always",
"samples":"srgb8"}}`) is admitted with 176,472 B free and a 91,064 B largest
block, and the board resets inside it:

```
allocation failed: requested=40960 align=4 free=89440 used=283104 largest_free=35528 retry_ok=false context=project read: probe
[OOM] FRAGMENTED: 89440 B free in total but only 35528 B in one piece
```

On main's heap (301,536 B) the same read is refused in words and the board
keeps running: `read refused: board memory busy (free 104796 B, largest block
65536 B; needs 128480 B free and a 41472 B block)`.

**Root cause** — not established; what the figures show. The estimate asks
for 128,480 B free and a 41,472 B block, and both held at admission. The read
then used ~87 KB (free 176,472 → 89,440 B) before it asked for 40,960 B in one
piece, by which time its own earlier allocations had cut the 91,064 B block to
35,528 B. The block term is checked against the heap as it is before the read,
not as the read leaves it; 40,960 B looks like a buffer doubling from 20,480 B.
Main never ran this path because its free total was under the estimate.

**Fix** — open (RAM research M3, admission for every big job): an estimate
that reserves its block first, or a probe whose reply never needs one buffer
of 40 KB.

**Regression coverage** — none yet. Reproduce with
`LP_STACK_PEAK_CENSUS=1 LP_STACK_PEAK_EDITS=10
LP_STACK_PEAK_PROJECTS=catalog/projects/zook-dome cargo test -p lp-cli
--release --test emu_stack_peak -- --include-ignored stack_peak_census` on a
split image of the 2026-10-16 heap.

**Lesson** — a gate that refuses on main's tight heap hides the estimate
behind it; giving the heap more room is what runs the estimate for real.
Zook is past the C6's 128–512-lamp design target, so this is not a product
case today, but the shape (an admitted read that fragments its own block)
applies at any size.
