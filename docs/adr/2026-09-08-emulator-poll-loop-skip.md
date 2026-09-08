# ADR: The ESP32-C6 emulator credits whole iterations of a pure poll loop

- **Status:** Accepted
- **Date:** 2026-09-08
- **Deciders:** Photomancer (Yona), gate held 2026-09-08 00:15
- **Supersedes:** None
- **Superseded by:** None

Plan: `lp2025/2026-09-07-0827-emu-speed-ladder`, milestone M4, decision D7.

## Context

The C6 machine runs the guest's instruction stream one instruction at a
time. On the compile-stress harness image, **1 in 12 instructions is an MMIO
access and 86 % of those are reads of `UART0+0x01c`** — the TX-FIFO status
word — while the console drains at baud in *emulated* time. Real firmware
logs a great deal, and every logged byte costs the host a byte-time of
spinning that produces no guest state at all.

Measured on the pinned harness image (`d6cfaa205-harness`), 700 ms of
emulated time: **4,227,452 reads of `UART0+0x01c`, at one every 12 cycles** —
45 % of every cycle the machine executed in that window. TIMG0's RTC-calibration
poll and the USB-Serial-JTAG `ep1_conf` poll have the same shape.

The machine already skips one kind of nothing: after `wfi` it moves guest
time to the next scheduled event, because nothing can change before then.
This ADR is the same principle applied to a loop the guest is spinning in
with interrupts enabled.

**The invariant that governs the whole design.** Plan PD5 and ADR
2026-09-06: guest time is the scheduler's, wall clock never enters the
machine, and two runs of the same image are byte-identical. A skip that
changed one byte of one transcript would not be a speed-up, it would be a
different emulator.

### What the loop actually looks like

The first draft of this work refused a store anywhere in the loop body, and
on the real firmware it **fired zero times on both reference images**. The
loop that spends 45 % of the harness's cycles is the mask ROM's
`uart_serial_tx_one_char`:

```
40022bc4: mv   a0, s0
40022bc6: sw   a1, 12(sp)      # spill the character being sent
40022bc8: jal  uart_hal_get_txfifo_count
              lui / add / slli / lw 28(a0) / srli / zext.b / ret
40022bcc: lw   a1, 12(sp)      # reload it
40022bce: bltu s1, a0, 40022bc4
```

Two accesses — a stack spill and its reload — that touch no peripheral and
leave memory holding exactly what it held before. A rule that refused them
was not conservative, it was inapplicable.

## Decision

### 1. What a pure poll loop is

A **pure poll loop** is a run of guest execution that returns to the same
`pc` with an identical register file, having executed the same instructions
with the same branch outcomes, whose only *observable* effect is reads of
MMIO registers the peripheral declares side-effect free, whose returned
values were identical, and which performed no observable store, atomic,
fence, CSR access, `wfi`, `ecall`, `ebreak`, trap or interrupt delivery.

Such a loop is a **fixed point of machine state except for time**. Its cost
in cycles and in instructions is constant, so skipping `n` whole iterations
means advancing `cycle_count` by `n × iter_cycles` and `instruction_count`
by `n × iter_instr` and leaving every other byte of state untouched.

The exactness bar is **fixed-point detection**, not an approximation with a
declared error. There is no approximate mode and none is planned: a skip
that is not exact is a different emulator, and the whole point of the
emulator is that its transcripts are the product's transcripts.

### 2. The bus grades an instruction's accesses on three levels

`lp_emu_core::PollSample`, reported by `Bus::take_poll_sample` for the
instruction now retiring:

| level | meaning |
|---|---|
| `Inert` | Changed nothing the guest can observe: touched no MMIO, and any store wrote bytes that were already there. |
| `Pure(PureRead)` | Exactly one MMIO read, of a register the peripheral declares side-effect free. Carries the address and the value. |
| `Impure` | Anything else — an MMIO write, an MMIO read not declared pure, a store that changed memory, an unmapped access, or a bus that does not answer the question. |

`Impure` is the **trait default**, so a bus that makes no claim gets no skip.
`SocBus` resets the sample in `set_issuing`, once per instruction before its
first access, so a load that reached plain RAM can never inherit an earlier
load's answer.

Two of these levels are claims worth stating on their own.

**Claim 1 — a RAM read is inert.** Within the skip's horizon nothing outside
the hart writes guest memory: a peripheral acts only at a scheduled event,
and the horizon stops at the next one.

> **This claim has a dependency, and it is the one to watch.** It holds only
> while *no peripheral writes guest RAM outside a scheduled event*. A DMA
> model that wrote memory continuously — a real GDMA, a peripheral that
> streams into a descriptor ring between events — would break it, and the
> skip would then be crediting iterations across a memory change the guest
> could see. What would have to change: either such a peripheral declares
> the cycle at which its next write lands and that cycle joins the horizon's
> terms, or `PollSample::Inert` stops covering RAM reads and only covers
> stores. Anyone adding a peripheral that writes memory outside `on_event`
> must revisit this ADR before doing so.

**Claim 2 — a RAM store that wrote the bytes already there is inert.** Memory
after it equals memory before it. `SocBus` compares before it copies, which
earns the claim for any number of stores in any order — where "the same
address and value as last time" would have had to reason about how many
stores the loop makes and in what sequence.

The safety rule this expresses is **"no *observable* store"**, not "no
store". An MMIO store is never inert whatever value it writes, because
writing a register can raise a line, arm a sequence or pop a FIFO.

### 3. What a peripheral declares

`Peripheral::pure_read(off) -> bool` is a per-register claim with two halves,
and a register needs both:

1. **The read changes nothing.** A FIFO whose read pops, a clear-on-read
   status word, a register whose read arms a sequence: none qualify.
2. **The value is not a function of `cx.now`.** A free-running counter fails
   this even though reading it is harmless — a guest spinning on one is a
   *delay* loop, and a delay loop is not a fixed point of machine state.

The default is `false` for every register of every block; a block opts in
register by register. Opted in today:

- `UART0`/`UART1`: `int_raw`, `int_st`, `status`, `fsm_status`,
  `mem_tx_status`, `mem_rx_status`, `afifo_status`. Not `fifo` — reading it
  pops.
- `TIMG0`/`TIMG1`: `rtccalicfg`, `rtccalicfg1`, `rtccalicfg2`.
- `USB_DEVICE`: `ep1_conf`, `int_raw`, `int_st`, `fram_num`, `in_ep1_st`,
  `out_ep1_st`. Not `ep1` — reading it pops the OUT FIFO.

**`SYSTIMER` is deliberately not pure.** Its value is a function of `now`, so
a spin on it is a delay loop. Delay loops are out of scope for this ADR and
have no skip.

### 4. Detection is a fixed point, confirmed twice

`PollDetector` lives in the hart. On a pure MMIO read it keys on
`(pc of the load, address, value)`. The first matching read establishes the
key; the second measures the per-iteration cycle and instruction deltas and
snapshots the register file; the third and fourth must each reproduce **both
deltas and the whole register file** exactly. Four hits, two independent
confirmations.

Three would very probably do. The fourth costs one iteration of a loop that
is about to be skipped thousands of times, and buys the property that a
two-state alternation — a loop whose registers ping-pong — can never be
mistaken for a fixed point.

Conservatism is the rule everywhere else: **any uncertainty, no skip.**
Register-file identity covers a timeout kept in a register; the
observable-store rule covers a timeout kept in memory; a `rdcycle`/`mcycle`
read is a `SYSTEM` instruction *and* changes a register every iteration, so
a cycle-based timeout can never form a fixed point.

The detector is **slice-scoped**: `run_slice` resets it on entry. Every piece
of evidence in it was therefore gathered during one slice, with nothing but
the hart touching the machine in between. That is what makes the fixed-point
argument local — no reasoning is needed about what the owning machine does
at a slice boundary, because no evidence survives one. A store that changed
memory, an atomic, a fence, any `SYSTEM` instruction, a trap, an interrupt
delivery, or a move of guest time also forgets it.

### 5. The horizon, and why the slice cap does not bound it

`run_slice` takes a `horizon`: an absolute cycle, at or after the slice's own
end, being the earliest cycle at which anything outside the hart can change
what the guest sees. It is the minimum of

- the next scheduled event (`sched.next_deadline()`),
- the next host service (a scripted command, the socket poll cadence),
- the host's next ready byte (`host.next_ready()`),
- the next probe,
- the stop cycle.

`n = (horizon − cycle_count) / iter_cycles`, floored; `0` means no skip.

**The horizon is deliberately not capped by the machine's 8,192-cycle slice
cap, nor by `--strict-bus`'s 1,024.** Those caps exist because a slice's
deadline is fixed when the slice starts, and an MMIO *write* inside the slice
can schedule an event sooner than that deadline — the cap bounds how late
such an event can be delivered. **A pure poll loop performs no write**, so
the reason for the cap does not apply to it. Under `--strict-bus` the cap
additionally exists so the machine notices a recorded violation promptly, and
a pure poll loop can record none: it touches nothing unmapped.

Why landing at or before the horizon is exact: the run *without* the skip
passes through the very same machine state at the very same cycle, because
`n` is a whole number of iterations and nothing outside the hart moved in
between. Every slice boundary the unskipped run crosses in that window is a
no-op — no event is due, no host service is due, no pad edge was produced, no
flash page was written, and the interrupt matrix's answer cannot have
changed.

### 6. `--strict-bus` does not disable the skip

The two are orthogonal. Strict mode refuses accesses nothing claims; a pure
poll loop makes none. Test 7 asserts that a strict run and a non-strict run,
each with the skip and without, reach the same cycle with the same
instruction count and the same UART0 bytes, and that no strict violation is
recorded.

### 7. One trace note per skip, and the trace is a diagnostic

`--trace` writes one line per skip:

```
cyc=17825308 pc=0x400294a6 POLL-SKIP UART0+0x01c status x1149
```

naming the load's `pc`, the register the way every other trace line names
one, and how many iterations were credited. The counts sum to the total the
exit report prints, which is asserted by a test.

**The consequence, accepted deliberately:** over a window that contains a
skip, a trace is **not** byte-identical to the same window without one. The
skipped iterations' MMIO reads are not in it — they did not happen — and one
`POLL-SKIP` note stands in their place. Measured on the harness image over
300 ms of emulated time: 1,455 `UART0` trace lines without the skip, 275
lines plus 2 `POLL-SKIP` notes with it.

The invariant is the **product transcript** — the `stopped after` line, the
UART bytes, stdout — and that is byte-identical. The bus trace is a
diagnostic, and one note saying "1,149 iterations, same value" is more useful
to a reader than 1,149 identical lines. `--no-poll-skip` restores the full
record, and **any trace used for identity checking must be captured with
`--no-poll-skip` on both sides**.

### 8. What is deliberately not skipped

- **Delay loops.** A spin on `SYSTIMER`, or on any value that is a function
  of `now`. Not a fixed point; out of scope (the guest is *waiting*, and the
  wait is the semantics).
- **Anything impure.** A FIFO pop, a clear-on-read register, an MMIO write,
  an unmapped access.
- **A timeout in a register or in memory.** Caught by register-file identity
  and by the observable-store rule respectively.
- **A loop with two pure reads at different `pc`s.** The key alternates, the
  run of evidence never reaches four, nothing is credited. Conservative, and
  no real loop we have seen has this shape.
- **The `wfi` idle skip**, which is older and separate; both counts appear in
  the exit report.

## Consequences

### The oracle

Identity is checked three ways, all at `boot_idle.rs`'s real
`GATE_US = 5_500_000` deadline — not the 3 s window the probe defaults to —
on three pinned reference images (`harness`, `jit-math-perf`,
`boot-idle-memfs`) at both time grades, comparing the `stopped after` line,
the UART0 bytes, full stdout and full stderr, plus a 20 ms `--trace` of the
harness with `--no-poll-skip` on both sides:

1. **With the skip vs `--no-poll-skip`, same binary.** 25/25 artefacts
   byte-identical. This is the strongest of the three: the same code, the
   same image, the only difference being whether iterations were executed or
   credited.
2. **Against `origin/main`'s binary.** 25/25 byte-identical, re-run after the
   `origin/main` merge that touched `bus.rs` (DD9 — a clean automerge in the
   hot path is exactly the case byte-identity exists to catch).
3. **On the phone.** Yona's wasip1 measurement on a different device and a
   different build reported instruction and cycle totals byte-identical to
   the previous day's no-skip run.

Seven unit tests carry the rules: a pure loop is skipped and the run is
unchanged; a countdown in a register is not; a countdown in memory is not; a
store that changes nothing is skipped; an MMIO store is never inert; a loop
reading `mcycle` is not skipped; an interrupt asserted during a skip is taken
at the same cycle; a poll on an impure register is never skipped; and a
traced, strict run skips and notes every skip.

### What it costs, and what it buys

Same-window interleaved A/B, minimum USER seconds over 4–6 round-robin
rounds, this desk at load average 60–110:

| image | grade | `main` | M4 | |
|---|---|---|---|---|
| harness | t1 | 4.79 s | 0.80 s | **5.99×** |
| harness | t2 | 2.83 s | 0.57 s | **4.96×** |
| jit-math-perf | t1 | 1.99 s | 0.71 s | **2.80×** |
| jit-math-perf | t2 | 1.43 s | 0.60 s | **2.38×** |
| boot-idle-memfs | t1 | 0.73 s | 0.90 s | **0.81×** |
| boot-idle-memfs | t2 | 0.78 s | 0.88 s | **0.89×** |

**The open trade.** On `boot-idle-memfs` — a `wfi`-idle boot with a memory
filesystem and almost no console traffic, where the skip fires **zero** times
— the machine is 11–19 % slower. The cost is the bookkeeping the bus and the
hart do per instruction to be *able* to notice a poll loop.

It was chased and not eliminated. Measured: arming the detector only on a
pure read rather than on every load recovered nothing; replacing the store
path's slice comparison (a `memcmp` call) with a fixed-size array compare
recovered nothing; carrying the horizon in the hart rather than as a `step`
argument recovered nothing; boxing the detector so `MachineHart` does not
carry a second register file recovered nothing. Removing *all* per-instruction
bus bookkeeping recovered about 40 % of the gap; the rest is diffuse codegen
change from the extra branch and the wider trait. The first two of those
changes were kept anyway — they are the right shape — and the rest reverted.

Three of the four measured image/grade pairs gain 2.4× to 6×, including
`jit-math-perf`, whose JIT'd Q32 kernels are the shape of the product's
render loop. The one that loses is the one that spends its time asleep.

### Where the numbers came from, and a reconciliation

M2's profile counted ~4.22 M `UART0+0x01c` reads on the harness, while M4's
counters credit 31.2 M iterations at t1. Both are right: the profile sampled
a **700 ms** window, the counter covers the full **5.5 s** deadline, and the
polling does not start until ~111 ms. The totals matching is not the claim —
the `stopped after` line matching is, and it does.

### Interrupt polling points are unchanged

The four points in `mach/mod.rs`'s module docs still hold. A skip ends the
slice with `SliceEnd::BudgetExhausted`, so the machine fires every due event
and resamples the matrix exactly as it does after `wfi`. An interrupt
asserted at cycle X during a skip is taken at the same cycle as without it —
the horizon never reaches past the event that raises the line.

### Snapshot and restore

Detector state is not part of a snapshot; a restored hart starts with an
empty detector. The counters, being a report of what a run did, carry.
`advance_to_cycle` stays monotonic and forgets the detector.

## Alternatives Considered

- **An approximate mode with a declared error budget.** Rejected at the gate.
  The emulator's value is that its transcripts *are* the product's
  transcripts; a mode that spends that is not worth the speed.
- **Refusing any store in the loop body** (the milestone's first draft).
  Measured: fires zero times on both reference images, because the real loop
  spills to the stack. Exactness is not served by a rule that never applies.
- **Recording the last store's `(address, value, width)`** and requiring it
  to repeat, instead of comparing memory. Cheaper on the store path, but it
  has to reason about how many stores a loop makes and in what order; the
  compare is one rule that covers every case.
- **Hashing the register file** instead of comparing it, to shrink the
  detector. Rejected: a collision would allow an unsound skip, and exactness
  is the bar.
- **Skipping delay loops on `SYSTIMER`.** A different mechanism (the value
  moves with time, so there is no fixed point) and a different risk profile.
  Out of scope; M5's block cache is the next rung.
- **Disabling the skip whenever `--trace` is on**, so traces are always
  complete. Rejected at the gate: the note carries the information, and a
  reader who wants every access has `--no-poll-skip`.

## Follow-ups

- **Watch claim 1.** Any peripheral that writes guest RAM outside a scheduled
  event invalidates it. See the box in §2.
- **The `boot-idle-memfs` regression** is an open trade, recorded here rather
  than hidden. If the ~11–19 % on skip-free images matters more than it looks,
  the lever is the per-instruction bus bookkeeping (about 40 % of the gap);
  a design that stamps the sample with the cycle it belongs to, instead of
  resetting it every instruction, would remove the per-instruction store.
- **M5 (block cache)** is the next rung and will change the same hot path;
  its ADR should re-measure this trade rather than inherit the numbers.
