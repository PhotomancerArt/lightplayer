---
status: open
found: 2026-10-01      # how: report — the D10 io-thread spike's silicon/emulator comparison
area: lp-emu/esp/lp-emu-esp32c6 time grades (t1/t2 install no memory-cost model; cache.rs `CacheCost` is t3 only)
class: fidelity
related:
  - docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md
  - docs/reports/2026-09-08-esp32c6-t3-calibration.md
  - lp2025/2026-10-01-1200-io-thread-spike
---
# The emulated C6 at t1/t2 charges a cold code path ~10× less than silicon, so a preemption cost is invisible there (t3 already charges it)

**Symptom** — the io-thread spike moved the USB link loop onto its own
preemptive esp-rtos thread (variant **B**) and counted each pass of that
loop (`[iowake]`: passes, and the loop's own run time from wake to sleep).
With the PLAYFUL Choker rendering and the link idle, each pass costs, per
5 s heartbeat window:

| where | idle wake | µs per pass (steady windows) | idle fps, base → B |
|---|---|---|---|
| XIAO C6 `10:bd:a3:b0:8e:30`, wall clock | 10 ms | 252, 262, 277, 282, 313, 348, 349 (365 in a busier 113/s window) | 31.00 → 27.93 (**−9.9 %**) |
| XIAO C6, wall clock | 50 ms | 344, 355, 361, 362, 364, 376, 386 | 31.00 → 30.68 (−1.0 %) |
| `configuration=lp-emu:esp32c6:t2`, lp-emu `28d010762` | 10 ms | 30, 31, 31, 31, 31, 31, 36 | 63.47 → 62.97 (−0.8 %) |

Silicon from the spike's `data/silicon-busy/` (`B`, `B-idle50`, `base`),
emulator from its `data/emu-busy/`; the three images are the same ELFs on
both sides (spike branch `bb3c6bdbf`). Over the spike's final runs silicon
lost 9–14 % (idle fps 29.4–30.9 → 26.7) where the emulator lost 0.7 %
(67.21 → 66.75). A frame-rate regression caused by preemption is invisible
in an emulator walk run at the grades walks use: `just walk-esp32c6-emu`
pins `--time-grade t1`, `lp-cli emu run` / `emu serve` default to `t1`, and
the spike's `lp-cli link rtt` ran `t2` and refused `t3`.

**Root cause** — `t1` and `t2` install **no memory-cost model at all**
(`TimeGrade::memory_cost` returns `None` for both, `machine.rs`): every
instruction fetch from the flash window is a RAM read at its class cost, so
a cache-cold code path costs exactly what a warm one does. The C6 runs its
`.text` from flash through a 32 KiB, 4-way cache of 32-byte lines; a render
evicts the link loop, and every wake of the loop on a preempting thread runs
it cold.

The model that charges this already exists: **`t3`**
(`lp-emu:esp32c6:t3`, `cache.rs` `CacheCost`, from
`docs/reports/2026-09-08-esp32c6-t3-calibration.md`) — the geometry from the
mask ROM's `Cache_Get_Mode`, exact LRU, and a fill of **338 CPU cycles**
(2.11 µs at 160 MHz) measured by the `cycle-probe` payload's `code_walk`
kernel on silicon on 2026-09-08, three weeks before this spike, and not
fitted to it. Run on the same three ELFs, with the same project uploaded the same
way (`lp-cli emu run --elf <variant> --host-link --upload
catalog/projects/playful-choker --time-grade t3 --timeout 45s`), lp-emu
`28d010762` (tree `34802bc26`, whose `lp-emu/` differs from it only in an
S3 figure):

| `configuration=lp-emu:esp32c6:t3` | idle wake | µs per pass (steady windows) | fps over 30 s (two pattern periods) |
|---|---|---|---|
| base | — | (shared executor; passes only in the frame gap) | 35.44 |
| B | 10 ms | 249, 254, 277, 279, 322, 324, 331 | 33.73 (**−4.8 %**) |
| B-idle50 | 50 ms | 352, 357, 367, 368, 381, 381, 393 | 35.24 (−0.6 %) |

`t3` reproduces silicon's per-pass cost at both cadences (249–331 µs
against 252–349 µs; 352–393 against 344–386) where `t2` is 10× low, and it
shows the 50 ms cadence buying the frame rate back, as silicon does.

**What the pass is made of** (measured at `t3` with a scratch fill
histogram — a counter on `CacheCost`'s miss path, never committed — over
emulated 10–40 s of `B`, two deterministic runs of one image differenced,
`--link-nonce` fixed, symbols from `rust-nm --print-size` of the same ELF):

| code | fills in 30 s | distinct 32 B lines | per pass (≈3,000 passes) |
|---|---:|---:|---:|
| the link loop: `usb_link_task`'s poll (2,548 B of code), `UsbLinkShared::with_link` closures, `lp_link` (`pick_and_emit`, `next_timer`, `probe_deadline`, …), `usb_serial_jtag` read, `log_ring_logger` | 448,818 | 519 | **≈150** |
| the wake around it: `esp_rtos` timer queue (`arm_next_wakeup`), `_embassy_time_schedule_wake`, `ThreadFlag::wait`, the interrupt dispatch, `link_lock` | 227,220 | 94 | ≈75 |
| everything else (the render, the engine) | 6,260,552 | 5,492 | — |

≈150 fills × 338 cycles = 50.7 k cycles = **≈317 µs**, which is the
`[iowake]` bracket (249–331 µs) almost exactly: at `t3` the pass's cost
*is* its cache misses. The ≈75 fills of the wake path (≈160 µs) fall
outside the bracket, which is why the bracket understates the frame-rate
cost on both machines.

**Arithmetic for silicon, inferred, not measured.** Silicon's 252–349 µs
per pass is what ≈105–150 line fills at the measured 2.11 µs would cost on
top of the ≈31 µs of instructions `t2` charges — the same order as the
≈150 the `t3` histogram counts. That is an inference from one model that
matches; silicon has no fill counter and no board was attached for this
entry.

**What `t3` does not close.** It shows **−4.8 %** of frame rate where
silicon showed **−9.9 %** on the same ELFs. The render loses ≈48 ms of every
second at `t3` (≈28.5 ms inside the bracket, the rest the wake path and the
render re-filling what the pass evicted — ≈+195 fills a frame over base,
≈3 %, inferred from fill totals and frame rates) against ≈99 ms on silicon.
Unseparated candidates, all but the last already named in the calibration
record's "What is still owed": `t3`'s one-signed compute under-count
(silicon/`t3` aggregate 1.154 on the calibration set; here silicon's base
31.0 fps against `t3`'s 35.4 is 1.14×), an uncharged interrupt-entry path
and interrupt-controller window, the assumed exact LRU, and a radio
running on the silicon side. The per-pass number is the one `t3` gets
right; the frame-rate number is about half.

**The same gap is why `t2` frames are not silicon's.** Fills are ≈48 % of
all cycles in the base `t3` run (≈226 k fills a second × 338 = 76 M of
160 M; 49 % in `B`). That is most of why `t2` renders the Choker at
63–67 fps where the board does 29–31 and `t3` does 35 — and why the spike had to compare the
emulator "in frames, not ms".

**The other direction.** The graphics-stage defect
(`2026-09-10-…-40x-slower-than-silicon`) is the emulator taking *more*
guest time than silicon; this one is it taking less. They are not one
mechanism: that one executes ~40× the *instructions* (1.308 G in 8.28 s),
which no cycle model can fix, and `t3` would only make it slower; this one
executes the right instructions and charges them too little. What they
share is the question "which grade is a walk's time read at", and here the
answer is that walks read time at a grade that leaves out the C6's
dominant cost.

**Fix** — not taken. No emulator change is needed to make this class
visible: `t3` exists and charges it. What is missing is a measurement that
runs at `t3`:

- the frame-rate comparison the ticket asks for, as a `t3` leg of the C6
  walk (`scripts/emu/m4-walk.sh` pins `t1`, outside this entry's scope), or
  a `--time-grade t3` on whatever ships from the spike's `lp-cli link rtt`;
- whether `t3` becomes the default grade for `emu run`/`emu serve`/the walk
  is Yona's call: it moves every frame count and cycle figure read at the
  default grade (heap figures do not move — memory is identical across
  grades by construction, `t3_memory_equals_t1`), and `t3`'s timing trust
  is still "documented, proposed at G1, not yet blessed" in
  `lp-emu/lp-emu-validate/validate.toml`.

Closing the remaining half (−4.8 % against −9.9 %) is model work behind a
desk kernel — an interrupt-entry / wake-path probe in `cycle-probe` — not a
constant to tune: fitting the fill cost or a per-wake term to this one
number is exactly what the calibration record refused to do.

**Regression coverage** — none. No test or walk runs a frame-rate or
preemption comparison at `t3`; that is the gap.

**Lesson** — a time grade that leaves out a chip's dominant cost does not
make the emulator uniformly fast; it makes it fast *selectively*, and
rearrangements that change cache residency — a new thread, a wake cadence,
code moved into or out of RAM — shift work into exactly the term it
leaves out. On the C6 that term is the flash cache, and the grade that
charges it existed three weeks before the spike that needed it; the defect is
that nothing reached for it.
