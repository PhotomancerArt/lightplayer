---
status: open
found: 2026-09-24      # how: hardware-walk (BLE M4 desk check, Run J)
area: fw-esp32c6 RAM split (board/esp32c6/init.rs) × lpa-server load-headroom gate (PROJECT_LOAD_MIN_HEADROOM_BYTES)
class: budget-exhaustion
related:
  - docs/adr/2026-09-02-esp32c6-ram-split.md
  - docs/adr/2026-09-24-ble-transport.md
  - docs/defects/2026-09-04-read-gate-refuses-on-largest-block-proxy.md
  - lp2025/2026-09-23-1428-ble-remote-control/
---
# A BLE-enabled C6 on the cut heap refuses to switch from the choker to zook: largest free block 65,534 B < 65,536 B

**Symptom** — desk XIAO `A0:F2:62:87:B4:8C`, shipped image `0abc29b44`
(heap main region 236,000 B, the 2026-09-24 RAM ruling), device store
`bleEnabled: true`, PLAYFUL Choker running. Loading Zook dome is refused,
over USB (`lp-cli upload catalog/projects/zook-dome serial:…`, 3 of 3
tries) and over BLE (`stopAllProjects` then `loadProject`, 1 of 1):

    [mem] stop_all_projects after: 212096 B free / 89440 B used (207k / 87k)
    Core error: load refused: heap headroom too low (largest free block 65534 B < 65536 B); power-cycle the device or load a smaller project

212 KB is free but no single block reaches the gate's 64 KiB. The same
image with BLE disabled makes the same switch. A scratch build of the same
tree with the main region at 260,000 B, BLE enabled and the same sequence,
**accepts** it, with the used bytes after `stop_all` identical (89,440 B).
Zook → choker is accepted on the cut image. `lp-cli upload` resets the
board, which auto-loads the choker, so on this image with BLE on a zook
upload fails every time until the store disables BLE.

**Root cause (partial)** — the load gate reads `largest_free_block`, and
the heap is two regions. `dram2_seg` is exactly 65,536 B, so its largest
block is at most 65,534 B and can never pass the gate. Only the main region
can. After the choker is stopped, its 236,000 B holds the BLE controller's
~24 KB plus whatever outlived the project (allocations made after the
project started, which split the freed space). With 24,000 B less region,
no hole is left at 64 KiB. Which live allocations split the region is
**not yet identified**. Candidates are the BLE controller blob's own
allocations on advertising restart (the name changes once the project
loads) and the Wi-Fi/ESP-NOW driver's buffers.

**Fix** — none yet. The options, for the ruling's owner:
- find and pin the splitting allocation, so it lands below the project's
  memory;
- read the gate against the main region only, with a threshold that can
  actually pass;
- give back part of the cut.

**Regression coverage** — none. No emulator gate enables BLE (radio is not
modelled), and no gate switches between two catalog projects on a board
that already has one running.

**Lesson** — a heap cut's cost is not "the compile margin narrows by the
same amount". The gate refuses on contiguity, and contiguity depends on
where long-lived allocations sit. Measure a heap-split change by *switching
projects*, not only by loading one onto a fresh board.
