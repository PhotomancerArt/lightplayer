---
status: fixed
found: 2026-09-24      # how: hardware-walk (BLE M4 desk check, Run J)
fixed: 2026-09-24      # 92e1b1c2c, 45319689f, 959101ce9 (PR #810)
area: fw-esp32c6 heap placement (c_heap, init.rs) × lpa-server/lpc-hardware/lpvm-native long-lived containers × the project-load gate (PROJECT_LOAD_MIN_HEADROOM_BYTES)
class: budget-exhaustion
related:
  - docs/adr/2026-09-02-esp32c6-ram-split.md
  - docs/adr/2026-09-24-ble-transport.md
  - docs/defects/2026-09-04-read-gate-refuses-on-largest-block-proxy.md
  - lp2025/2026-09-23-1428-ble-remote-control/ (spike-results.md, Run K)
---
# A BLE-enabled C6 on the cut heap refused to switch from the choker to zook: largest free block 65,534 B < 65,536 B

**Symptom** — desk XIAO `A0:F2:62:87:B4:8C`, shipped image `0abc29b44`
(heap main region 236,000 B, the 2026-09-24 RAM ruling), device store
`bleEnabled: true`, PLAYFUL Choker running. Loading Zook dome was refused
over USB (`lp-cli upload catalog/projects/zook-dome serial:…`, 3 of 3
tries) and over BLE (`stopAllProjects` then `loadProject`, 1 of 1):

    [mem] stop_all_projects after: 212096 B free / 89440 B used (207k / 87k)
    Core error: load refused: heap headroom too low (largest free block 65534 B < 65536 B); power-cycle the device or load a smaller project

212 KB was free, but no single block reached the gate's 64 KiB. With BLE
disabled, the same image made the switch. A scratch build with a 260,000 B
main region also made it with BLE enabled. `lp-cli upload` resets the
board, which auto-loads the choker, so on this image with BLE on, **every**
zook upload failed until the store disabled BLE.

**Root cause** — the load gate reads the largest free block. The heap has
two regions. The reclaimed `dram2_seg` is 65,536 B, so its largest block is
at most 65,534 B and can never pass the gate: only the main region can.
LLFF is first-fit from low addresses. So an allocation made **while a
project runs** lands above the project's memory. If it **outlives the
project**, it splits the space the project frees. Two sets of such
allocations were found with a heap map (`heap_map_diag`: holes and live
spans by address) and a backtrace for each live block
(`heap_track_diag`).

1. **Server-side containers that kept memory they had grown during the
   project.** Found on the emulated C6, with BLE's boot heap stood in by
   `heap_diag_ble_standin` (the emulator never starts BLE). That run
   reproduced the refusal at `65528 B < 65536 B`.
   - `ProjectManager`'s two tables: 3,688 B (a `Project` is stored inline)
     and 72 B. They were cleared but not freed, and sat at main-region
     offset +128,037.
   - The JIT instance's vmctx buffer, 56 B. **It was never freed at all**:
     a leak per `instantiate`, from a throwaway `NativeHostMemory` whose
     buffer no one owned.
   - The engine's `NativeHostMemory` live table (64 B), and `HwRegistry`'s
     two claim tables (96 B each). All were emptied but kept.
   - USB's read buffer, grown to an upload's longest line (5.6 KB).

   These alone were enough to refuse on the emulator: largest main hole
   51,423 B.
2. **The BLE link layer's own blocks.** On silicon, after (1) was fixed,
   the switch was still refused (`65526 B < 65536 B`). The heap map on the
   board (`LP_HEAP_TRACK_FROM=86000`, commit `8eccd665a`) showed live
   `r_ble_ll_mem_alloc` blocks of 296, 296 and 80 B at main-region offsets
   +187,663 and +127,621 at the moment of refusal (largest free block
   59,952 B in that diagnostic build). The BLE controller allocates them
   when advertising restarts, and advertising restarts when the advertised
   name follows a newly loaded project (`LP-b48c` → `LP-PLAYFUL Choker`,
   5 s after load). So they sit above the project and are still there at
   `stop_all`. Five seconds later the name falls back, advertising restarts
   again, and the blocks move low. The heap heals itself (the next map
   showed a 122,108 B hole), which is why a retry a few seconds later would
   have passed and an upload's immediate load did not.

**Fix** — placement. No gate or heap size changed.
- The radio blobs' C heap now lives in the reclaimed segment first
  (`lp-fw/fw-esp32c6/src/c_heap.rs`). esp-alloc's `compat` `malloc` is off.
  The firmware provides the C symbols, which ask for the segment first
  (it is tagged with a capability in `init_board`) and fall back to the
  whole heap. A block there can never pass the gate anyway, so the radio
  costs the gate nothing there. On the desk with BLE up, 44,584 B of the
  segment holds radio allocations, and the main region's free space after
  BLE start is one 198,816 B hole (`[heapmap] after-ble`, silicon).
- Nothing a project outlives keeps memory it grew during the project.
  `ProjectManager`, `NativeHostMemory` and `HwRegistry` give their tables
  back when emptied. USB's read buffer is dropped when empty and over
  1 KiB. The JIT instance frees its vmctx buffer on drop.

**Proof**
- Silicon, final head `959101ce9`, BLE enabled, over USB (`lp-cli upload`,
  which resets the board, so the startup project is loaded first):
  zook→choker 2 of 2, choker→zook 2 of 2 (and zook→zook 1 of 1). `[mem]
  stop_all_projects after: 216168 B free / 85368 B used`, then
  `compilation succeeded`, then `Project uploaded and running.`
- Silicon, `45319689f` (the same fix, except that the registry's tables
  were reserved at boot instead of released): USB choker→zook,
  zook→choker, choker→zook, 3 of 3. **Over BLE** with a logged-in
  connection (the Mac's Chrome): zook, choker, zook, 3 of 3, each
  `loadProject` answered `{"loadProject":{"handle":N}}`. On `959101ce9`
  the BLE-link switch is **NEVER RUN**.
- Emulated C6 (`lp-cli emu run`, standin build): choker → zook → choker →
  zook accepted, 4 of 4. Before the fix it was refused every time.
- The heap-budget ratchet is unchanged on the C6: idle `usedBytes` 60,604
  B, `totalBytes` 301,536 B, `stackTotal` 62,664 B.

**Regression coverage** — none automated. No emulator gate enables BLE
(radio is not modelled). No gate switches between two catalog projects on
a board that already runs one, and a switch gate on the emulator would
not see the BLE link layer's blocks. The diagnostics stay in the tree,
feature-gated and off.

**Lesson** — a heap cut's cost is not "the compile margin narrows by the
same amount". The gate refuses on contiguity, and contiguity is set by
*where* long-lived allocations sit. Anything allocated during a project
and kept past it (a cleared-but-kept table, a grown buffer, a leak, or a
C blob reacting to the project's name) splits what the project frees.
Measure a heap change by *switching projects*, and find splitters by
address (`heap_map_diag`), not by totals.
