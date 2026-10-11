---
status: open
found: 2026-10-09  # live-debugging (RAM research E8, emulated C6)
area: fw-esp32c6 heap_map (feature `heap_track_diag`)
class: ungated-variant
related:
  - docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md
  - lp2025/2026-10-09-1203-ram-research (E8)
---
# A `heap_track_diag` image panics as soon as Bluetooth starts

**Symptom** — an fw-esp32c6 split image built with
`--features esp32c6,server,radio,heap_track_diag` (`just fw-esp32c6-split
esp32c6,server,radio,heap_track_diag`) boots, logs a few `[bigalloc]` lines,
reaches `[ble] enabled … — starting`, and panics 58 ms in:

```
====================== PANIC ======================
panicked at <redacted>:0:0:
unwrap of `self.scheduler.try_borrow_mut()` failed:
```

Decoded (`addr2line` on the image's `p2.elf`):
`stage_and_reset` ← `<esp_rtos::scheduler::GlobalState>::scheduler` ←
`__pender` ← `embassy_sync … Signal::signal` ← `RingLogger::log` ←
`heap_map::track::log_big` (inlined into `_esp_alloc_alloc`) ←
`EspHeap::alloc_caps` ← `InternalMemory::allocate` ← `esp_rtos_task_create`
← `esp_radio::ble::npl::task_create` ← `r_ble_controller_enable`.
Reproduced on `lp-emu:esp32c6:t1+net=lan` (direct load of the split image's
`loader.elf` over its `merged.bin`, `lp-cli emu run --host-link`); not tried
on silicon.

**Root cause** — `heap_track_diag`'s allocation hook logs every allocation of
2 KiB or more (`[bigalloc]`) from *inside the allocator*. `esp_rtos_task_create`
allocates the new task's stack while it holds the scheduler's `RefCell`
borrow; the log call rings the log ring's doorbell, embassy's `Signal` pends
the executor, and `__pender` borrows the scheduler again — a double borrow,
so the `try_borrow_mut().unwrap()` panics. Bluetooth's controller creates its
task during `r_ble_controller_enable`, and BLE is on by default, so every
`heap_track_diag` boot with the default features dies there. The feature was
written (2026-09-24) for images whose radio work came later; no gate builds or
boots it, so the conflict arrived unseen when BLE went on by default.

**Fix** — none yet. The shape: the allocator hook must not log (nor do
anything that can pend an executor); record big allocations in the LP SRAM
table and print them from `heap_map::log`, outside the allocator, as the live
table already is. On the emulator, `alloc_trace_emu` (research branch
`research/ram-e08`: fw-esp32c6 feature + `lp-cli emu run --alloc-trace`)
replaces the table entirely — every allocation and free, with backtraces,
written by the host — and logs nothing from the allocator.

**Regression coverage** — none: no gate builds `heap_track_diag`, and a
diagnostic feature is not worth a CI build. A boot of it on the emulator would
be the test.

**Lesson** — a hook that runs inside the allocator runs inside every caller
that holds a lock while allocating: the RTOS's task creation is one. Logging,
signalling, or anything that can wake an executor does not belong there.
