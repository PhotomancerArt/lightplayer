---
status: fixed          # PR #1080
found: 2026-10-08      # ci (PR #1066's `Emulator C6 lp-cli` job, run 37946715534), then live-debugging
fixed: this change
area: lp-emu/lp-riscv-emu src/emu/executor/atomic.rs (`sc.w`) × esp-rtos / embassy-executor's run queue on the emulated C6
class: fidelity
related:
  - lp-emu/lp-riscv-emu/src/emu/lr_reservation.rs
  - docs/defects/2026-09-26-esp-hals-usb-isr-clears-a-tx-edge-it-did-not-handle.md
  - ~/.photomancer/planning/lp2025/2026-10-08-2050-pictures-through-the-cloud/lrsc_experiment.patch
---
# The emulated `sc.w` ignored its reservation

**Symptom.** On PR #1066's CI images (run 37946715534),
`lp-cli/tests/emu_usb_link_gates.rs::g4_1_the_first_lit_frame_off_gpio18_is_the_host_oracles_frame_under_t2`
failed every time. The board stopped answering, and 8 s of guest time later
the LP watchdog reset it. The same firmware source passed on other builds
(image digest `d3629b02…`). The only difference was the commit stamp, which
moves code and so moves when interrupts land.

**Root cause.** The RV32 executor treated `lr.w` as a plain load and `sc.w`
as a plain store that always reported success (`rd = 0`). It never recorded
`lr.w`'s reservation. The RISC-V A extension says an `sc.w` may succeed only
while the reservation from the most recent `lr.w` still holds and covers the
same word, and that any `sc.w` ends that reservation. The spec also lets an
`sc.w` fail after a trap, and silicon has to get that case right.

esp-rtos's run queue is embassy-executor's `TransferStack`. Its push is an
`lr.w`/`sc.w` loop on the stack head (`push_was_empty`, pc `0x420b8530` in
that image). A timer interrupt landed between the `lr.w` and the `sc.w`, and
its handler pushed another task onto the same head. The resumed `sc.w` should
have failed and retried. Instead it succeeded and wrote the stale head back,
which unlinked the task the handler had pushed. That task was the server
loop's. Its header still said "scheduled", and embassy's `wake_task` only
enqueues a task that is not already scheduled, so nothing could run it again.
Nothing fed the watchdog after that.

**Fix.** `lp-emu/lp-riscv-emu/src/emu/lr_reservation.rs`: each hart (both
`MachineHart` and the user-mode `Riscv32Emulator`) holds an `LrReservation`,
the word the last `lr.w` reserved.
- `sc.w` stores only when the reservation covers the same word, and writes
  `1` to `rd` when it does not.
- Every `sc.w` ends the reservation, whether it succeeded or failed.
- `MachineHart::trap_taken` ends it. It runs after every `trap::deliver_*`,
  the hook the trap log already used.
- The reservation is architectural state, so a hart snapshot carries it.

The block cache and `lp-emu-jit` both refuse every atomic, so the interpreter
is the only LR/SC implementation, and a translated core takes its interrupts
through the same hart. What is still not modelled: an `mret` clearing a
reservation (the spec allows this but does not require it), a same-hart store
ending a reservation (the spec does not require it), and DMA writes.

**Evidence.** These runs used CI's own images from run 37946715534 (#1066's
merge commit `87c0ac6bf98a`, fetched with `just fetch-ci-images 37946715534`)
and the t2 gate `g4_1_the_first_lit_frame_off_gpio18_is_the_host_oracles_frame_under_t2`,
on `lp-emu:esp32c6:t2`:
- **With the fix:** passes. `237 frames on gpio18, first lit n=1 at
  250.327 ms, 234 identical after it`, matching the host oracle. This is the
  same result as the experiment patch.
- **Control, the same tree with `sc.w` forced to always succeed:** fails.
  `the emulated board stopped: the chip asked to reset (LP_WDT stage 0
  (ResetSystem), into App)`, which is the CI failure.

**Regression coverage.**
- `emu::executor::atomic` unit tests: success on a live reservation, failure
  without an `lr.w`, failure on a second `sc.w`, failure on another word,
  failure after the reservation was cleared.
- `mach::tests`: `sc_w_fails_when_a_trap_was_taken_since_its_lr_w` runs the
  interrupt-between-them shape on a hart, with a handler that writes the word.
  Also `sc_w_succeeds_when_nothing_came_between_it_and_its_lr_w` and
  `a_snapshot_carries_the_reservation`.

**The "lost wake" reds are not this bug.** The esp-hal entry
(`2026-09-26-esp-hals-usb-isr-clears-a-tx-edge-it-did-not-handle.md`) and the
2026-09-27 main red counted a frame write whose 250 ms timeout *fired* while
the USB send buffer was free. A timeout that fires means the waiting task ran
again. A task this bug drops from the run queue never runs again, timer
included, because its header stays marked scheduled. That failure is a
silent stall that ends in a watchdog reset, not a counted timeout. Silicon
also reproduced the lost wake on stock esp-hal 1.1.1 and not on the back-port.
The 2026-09-27 red cannot be replayed on CI's own bytes any more: CI keeps
images for 7 days. A red that shows a watchdog reset or a task that never
runs again is worth checking against this entry first.

**Lesson.** On a single-hart emulator, "atomic" is easy to read as "nothing
else can happen in between". That is true of one instruction and false of a
pair: an interrupt is the other agent, and LR/SC exists to notice it. A
timing-dependent failure that follows the commit stamp rather than the
source is often an emulator fidelity bug, not a firmware race.
