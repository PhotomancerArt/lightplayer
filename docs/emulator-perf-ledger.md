# The emulator performance ledger

**A living document.** What the ESP32-C6 emulator's performance work has
tried, what it measured, what it rejected and why, and the menu of what is
left. Never delete a row; mark it superseded and date the replacement.

Written 2026-09-12 at the emu-loop-redesign milestone's phase P1b (gate
`G-LOOP0b`). If you have just been handed the emulator and want to make it
faster, read §1 before you take a number and §4 before you pick a lever.

---

## 0. The two goals

Yona, 2026-09-12, verbatim:

> My take is that the emulator really has two goals. 1. correctly simulate as
> best we can the hardware for correctness testing, bug finding, agentic dev.
> 2. simulate the happy path for a user as fast as we can, while remaining
> accurate for what they care about: memory, cpu time, etc.

and, on what goal 2 actually answers for a user:

> what will this pattern look like on hardware. what fps? will it oom? does it
> work?

and, on how fast is fast enough (2026-09-12, superseding "3×"):

> "as fast as we can" should really just be "as close to 1x speed" as
> possible. Running faster isn't needed.

### What that makes the two lanes

**Goal 1 — the correctness lane.** Every peripheral modelled at the pin/FIFO
level, byte-identity against the board's transcript. Its surfaces are the
UART0 bytes, the `stopped after` line, the decoded frames, the 127 k-line pin
log, the MMIO + interrupt trace, the trap log, and the masked stdout/stderr.
The `?emu` walk, the oracle sweeps and CI live here. **Nothing may be traded
away in this lane for speed.**

**Goal 2 — the user lane.** Its bar is **1× real time, held** — not exceeded:
held across spaced presses on a phone that is thermally throttling, across the
product images, on the device a user actually has. Its accuracy contract comes
straight from the three questions above:

| the user asks | the emulator must keep exact | so a lever may not change |
|---|---|---|
| **what fps?** | the guest's per-frame CPU time under the C6 cycle model (`docs/esp32c6-guest-cycle-model.md`) | the guest's retired instructions and cycles per frame — including the RMT refill ISR, which is real work the hardware really does |
| **will it OOM?** | the guest's heap and stack behaviour | anything the guest allocates or the depth it reaches |
| **does it work?** | the frames the pattern produces | the frame contents — though they may be *derived* from the guest's LED buffer or the RMT's words instead of decoded from pins |

**The pin log and the pin fabric are goal-1 surfaces only.** A user never asks
what the wire did. That asymmetry is the whole reason a fidelity tier is worth
measuring, and §4 scores every lever on both columns.

---

## 1. How a number is taken

These are already law; they are collected here because a number taken any
other way is not comparable to anything in §2 or §3.

- **One invocation per table.** The desk is shared and its load average moves
  by ten between invocations. Rows taken in one invocation are comparable to
  each other whatever the load; rows from two invocations are not, unless the
  loads match. `scripts/emu/bench-web/bench-cli.mjs` says so in its own
  comments, and `scripts/emu/tier-probes/rung-rows.mjs` exists precisely
  because comparing four different `emu.wasm` files needs one invocation that
  holds all four.
- **Interleaved, best-of-N.** Repeats go `A, B, C, A, B, C, …`, never
  `A×5, B×5`. Best-of-5 on the desk, best of ≥3 spaced presses on the phone.
- **The load average rides every row.** A wall-clock number with no `loadavg`
  beside it is not a row.
- **A UART0 sha256 on every row.** A speed number from a run that computed
  something else is not a speed number. `render-basic` t2 at 5,500 ms is
  `2407828f80684331`; the trap log is `51ddaf56c96b77d3`.
- **Both desk engines.** node/V8 at 16 blocks a function and bun/JavaScriptCore
  at 8 — JSC is the phone's family (JD19). In the agent harness `node` is an
  nvm shim: use `/opt/homebrew/bin/node` explicitly.
- **`caffeinate -dims`.** The desk sleeps after a minute idle.
- **The phone is the milestone bar; the desk is the phase proxy** (DD41/DD43).
  `render-basic` t2 at 8 blocks/fn, best of ≥3 presses spaced by minutes,
  quoted with the full sequence and the same-press translated ÷ interpreter
  ratio, the interpreter row as the thermal control.
- **The perf lab** (`scripts/emu/lab/`, home `~/.photomancer/emu-lab`, port
  41111) automates that protocol. Rows taken with the page hidden or after a
  lost lock are tainted and the lab excludes them — never quote a tainted row.
  A desk-browser row through the lab is a desk number, not a phone number.
- **A replay recording covers boot-phase code only** since P3 (DD59): the
  republished SYSTIMER words are not refreshed by canned imports. Any
  measurement of steady state must be a **live** run.
- **Never gate on emulated microseconds.** Transcripts decide; probes report
  (AGENTS.md).

---

## 2. Where the time goes today

`render-basic` t2, 5,500 ms emulated, on `38b12c848` (the P1 merge).

### The run, at the top

| engine | fn | best of 5 | real time | loadavg |
|---|---:|---:|---:|---:|
| node/V8 25.2.1 | 16 | 5.24 s | 1.051× | 3.1 → 4.9 |
| bun/JSC 1.1.18 | 8 | 6.35 s | 0.867× | 0.0 |

### What a slice boundary costs (P1's decomposition, node/V8)

1,531,923 slices; the whole boundary is **531 ns a slice = 814 ms**, and
`hart.run_slice` is **1,956 ns a slice = 2,996 ms** (66 % of the `run_until`
wall). The five items above the instrument's floor:

| item | ns/slice | ms/run |
|---|---:|---:|
| `run_due_events` | 192.9 | 295.5 |
| `wall_timeout` → `started.elapsed()` — **taken 2026-09-13 (P1c)** | 148.3 | 227.2 |
| `drain_pins` | 117.3 | 179.7 |
| `pending_cpu_interrupt` + `set_external` + `poll_interrupts` | 32.5 | 49.8 |
| the six-term deadline + `next_deadline` + census gates | 26.6 | 40.8 |

The full sixteen-item table, both engines, with the instrument's error bars,
is in the planning archive's `G-LOOP0-gate.md`. **The 531 ns figure is the
one to carry across engines: it is the same in V8 and JSC.**

### What bounds a slice

| | count | share |
|---|---:|---:|
| slices | 1,531,923 | — |
| bound by the scheduler | 1,475,330 | 96.31 % |
| …of which the RMT's per-word event | 1,471,630 | **96.06 %** |
| bound by the slice cap (8,192 cycles) | 56,593 | 3.69 % |
| pin edges drained | 2,961,409 | — |
| events dispatched | 1,485,557 | — |

**96 % of every slice in the run exists because the RMT schedules one event
per WS2812 word.** That single fact is what §4's tier levers are about.

### MMIO crossings, from translated code

5,881,883 operations — 1,569,895 loads, 4,311,988 stores.

| peripheral | crossings | share | loads | stores |
|---|---:|---:|---:|---:|
| SYSTIMER | 2,767,705 | 47.05 % | 444,730 | 2,322,975 |
| RMT | 1,811,925 | 30.81 % | 131,127 | 1,680,798 |
| PLIC_MX | 440,042 | 7.48 % | 251,451 | 188,591 |
| INTERRUPT_CORE0 | 251,538 | 4.28 % | 251,449 | 89 |
| TIMG0 | 240,792 | 4.09 % | 216,819 | 23,973 |
| **UART0** | **8,284** | **0.14 %** | 3,180 | 5,104 |

⚠️ **UART0 is 0.14 % on the render image.** M4's "86 % of MMIO is TX-FIFO
poll" is true of the **harness** image and false of the product images. DD26's
"UART console tap" is a harness-image lever; see §3.

### The guest's own numbers (the goal-2 side)

| | |
|---|---:|
| retired instructions | 542,906,355 |
| decoded frames on gpio18 | 256, all complete, 0 errors, 241 LEDs |
| **frames per emulated second** | **46.5** |
| RMT refills | 61,696 (half = 24 words) |
| external-interrupt traps (500 ms cell) | 2,662 of 2,757 total traps — **96.6 %** |
| …cadence | one every 187.8 µs of emulated time (30,053 cycles) |
| the refill ISR's share of retired instructions | **≤ 0.85 %** — see §3's P1b row |

### The whole-machine bucket table (P6c, superseded framing)

`lp-emu/lp-emu-jit/README.md` §"Where 3× would have to come from" holds a
sixteen-bucket profile of an 8,056 ms run. Its **numbers stand**; its
**framing is superseded** by §0's 1× target, dated 2026-09-12. Read it as
"where the time goes", not as "how far from 3×".

### Margin to 1×

The bar is 1× **held**, so the number that matters is the worst spaced press,
not the best.

| device | image | grade | best | the spread across spaced presses | shortfall to hold 1× | source |
|---|---|---|---:|---|---:|---|
| phone (JSC) | render-basic | t2 8/fn | **1.025×** | 0.863 / 0.945 / 1.008 / 0.921 / 0.818 — thermal | **up to 18 %** | M7 director log E7 |
| phone | render-rocaille | t2 | NEVER MEASURED | NEVER MEASURED | — | — |
| phone | harness | — | NEVER MEASURED | — | — | — |
| phone | boot-idle-memfs | — | NEVER MEASURED | — | — | — |
| desk V8 16/fn | render-basic | t2 | 1.051× | — (best of 5, one invocation) | holds | this phase |
| desk JSC 8/fn | render-basic | t2 | 0.867× | — | **13 %** | this phase |
| desk | render-rocaille | t2 | 1.73× | — | holds | P9 |
| desk | harness | — | 2.68× | — | holds | P9 |
| desk | boot-idle-memfs | — | **0.53×** | — | **47 %** | P9 — ~1.08 s of fixed translation on a 2 s run |

**Two shortfalls are real and they have different causes.** The phone's
10–18 % is thermal throttling of steady-state work. `boot-idle-memfs`'s 47 %
is a *fixed* translation cost amortised over a short run, and no steady-state
lever touches it — it needs cheaper or cached translation, not a faster loop.

---

## 3. What has been tried

Dated. Status is **shipped**, **rejected**, **held**, **registered** (a lever
named and deliberately not pursued) or **candidate**.

| date | lever | where the numbers live | measured effect | status |
|---|---|---|---|---|
| 2026-09-06 | `opt-level = "z"` → `3` for the five host emulator crates | `docs/reports/2026-09-06-lp-riscv-emu-speed-probe.md`; `lp-emu/README.md` §Speed | **2.3× free** | **shipped** |
| 2026-09-06 | bookkeeping reduction in the interpreter loop | speed-ladder research | **+2.1×** | **shipped** |
| 2026-09-06 | PGO (`scripts/emu/pgo-c6.sh`) | speed-ladder research | **+1.45×** | **registered** — the build-side cost was never taken on |
| 2026-09-07 | the block cache | `lp-emu/lp-emu-jit/README.md` | on the interpreter path | **shipped** |
| 2026-09-08 | **poll-loop skip** — a whole-iteration credit for a spinning guest | `docs/adr/2026-09-08-emulator-poll-loop-skip.md` | never reached a fixed point on the render images | **rejected** — do not re-attempt in this shape |
| 2026-09-07→11 | **M7, the wasm translator** (P1–P7) | `docs/adr/2026-09-11-emulator-wasm-translator.md`; archive `2026-09-07-0827-emu-speed-ladder/m7/` | the wasm build's core is the translator by default; phone **1.025× best of 3** at 8/fn | **shipped** |
| 2026-09-11 | M7b P1, incremental `fence.i` translation | m7b archive; JD20 | **1,050 ms of 2,058**, 400 ms handed back at the module boundary | **shipped** |
| 2026-09-11 | M7b P6, the escape-site shape | DD60 | **−1 %** at the 5.5 s bar | **held** |
| 2026-09-11 | **DD27's ruling** — "the fidelity-tier cords are not planned: all peripheral models together are 3.4 %" | m7 director log DD26/DD27 | 3.4 % is the models' **self** time | **superseded 2026-09-12** — it mis-scoped the tier. The tier does not remove the models; it removes the per-word *world* (96 % of slices, the fabric, the log, the decoder, the guest's ISR). P1b measured that; see the rows below |
| 2026-09-11 | P4's tombstone peel in `next_deadline` | P4's phase notes | shipped | **shipped** |
| 2026-09-11 | P4's RMT-cadence analysis — pad edges early | P4 | not taken | **registered** |
| 2026-09-11 | P3's published SYSTIMER words | P3 | three words the module reads without crossing | **shipped** |
| 2026-09-12 | **P1's decomposition** — `tick`, the boundary as an import | `G-LOOP0-gate.md`; planning `2026-09-11-1731-emu-loop-redesign/` | the plumbing `tick` removes is **≈ 376 ms of 5,500 ms (6.8 %)**; `run_due_events` (295 ms) and `drain_pins` (180 ms) *relocate*, they do not vanish | **shelved at G-LOOP0** (Yona chose (c): measure first). Re-enters §4 as headroom, not as a milestone |
| 2026-09-12 | P1's `--trap-log` | `G-LOOP0-gate.md` | a third identity surface; the log **on** costs 0.5–0.8 % in V8 | **shipped** (#731) |
| 2026-09-12 | **P1b R1 — pins off** (the middle tier) | this document §4; `scripts/emu/tier-probes/R1-pins-off.patch` | **1.027× in V8, 1.010× in JSC**, with UART, frames, trap log and retired instructions **all byte-identical** | **candidate** — the only tier lever that keeps the guest exact |
| 2026-09-12 | **P1b R2 — coalesced words** | `scripts/emu/tier-probes/R2-coalesced-words.patch` | slices 1,531,923 → **61,168 (25×)**, but the guest breaks — see the finding below | **rejected as built** |
| 2026-09-12 | **P1b R3 — instant RMT** (the full DD26 tier) | `scripts/emu/tier-probes/R3-instant-rmt.patch` | slices → 61,155; **zero frames**; the guest never renders | **rejected as built** |
| 2026-09-12 | **P1b R4 — the UART tap** | §2's MMIO census | **not taken**: UART0 is 0.14 % of crossings on `render-basic`, a hundredth of the 2 % floor the phase set | **rejected — wrong image**. It remains a *harness*-image lever |
| 2026-09-12 | **P1b R3′ — the firmware's own "no real output under emulation" switch** | `lp-fw/fw-esp32c6/src/` | **does not exist**. `bench/render_loop.rs` mentions emulation only to pick a shorter run; the RMT driver is unconditional | **registered** — an unbuilt firmware-side lever |

### The P1b finding that matters most

**Coalescing the RMT's per-word event makes the transmitter read RMT RAM ahead
of the guest that writes it.** `vision.md` §2 predicted the obstacle would be
the pin log's `(at, seq)` ordering. It is not; it is a data race in emulated
time. The transmitter's fetch runs at the leading edge of the schedule, so any
word it consumes in bulk is a word whose cycle the CPU has not reached — and
the refill ISR that fills that word runs in the cycles in between. R2 stopped
its bulk *before* the threshold word precisely so the interrupt's cycle could
not move, and it still raced: the run took 47 words to be answered where the
product path takes 15, refills fell from 2,651 to 18, and the frames came out
4 LEDs long instead of 241.

**So the per-word event is not removable by rescheduling. It is removable only
by not modelling the words** — which is the fidelity tier, and which costs the
guest's own ISR.

### Why R2's and R3's wall times are not speed numbers

R2 reads 1.49× and R3 1.48× against R0 in V8. **Neither is a speed
measurement**, because the guest they ran is not the guest: R2 retired
337,590,617 instructions against R0's 542,906,355 and produced 4-LED frames;
R3 retired 336,797,583 and produced none. Per retired instruction the emulator
was **slower**, not faster — 10.48 ns against R0's 9.65 in V8, 13.00 against
11.70 in JSC — because the broken guest spins in a tighter, more exit-heavy
loop. What R2 and R3 establish is the **slice count**, and that is computable
rather than raceable:

> 1,531,923 − 61,168 = **1,470,755 slices removed**, at P1's measured
> **531 ns a slice** = **781 ms of a 5,500 ms run ≈ 14 %**.

That is the tier's emulator-side prize, derived from P1's own instrument
rather than from a broken guest's stopwatch.

### And the ISR is small

R0 − R3 was supposed to give the refill ISR's instruction share directly. It
does not: at the 500 ms cell the delta is 365,594 instructions (**0.85 %**),
and at the 5,500 ms cell it is 206 M (**38 %**) — the same hack, two answers,
because the delta tracks the guest's *changed work*, not the ISR. Taking the
smaller as the upper bound (R3's guest also stops completing frames, so the
delta over-counts):

> the refill ISR is **≤ 0.85 % of retired instructions**, ≤ 138 instructions
> per entry over 2,651 entries — which is what a 24-word refill loop costs.

**The guest's ISR is not where the tier's money is. The emulator's per-word
slice cadence is.**

---

## 4. The menu

Every remaining lever, with **both** exactness columns §0 demands. Ranked by
headroom against the **1×-held** bar, not by distance to 3×.

| lever | payoff | goal-1 cost (correctness lane) | goal-2 cost (the user's numbers) | size | what would have to be true |
|---|---|---|---|---|---|
| **R1 — the middle tier: pins off, frames from the RMT's words** | **measured: 1.027× V8, 1.010× JSC** (140 ms of a 5,240 ms run). Removes 2,961,409 pin edges | the **pin log** is gone; `Gpio::observe_edges` and `Rmt::observe_edges` stop seeing the wire, so the GPIO input latch and the RMT receiver are dark. UART, frames, trap log, `stopped after` all **byte-identical** | **none.** Retired instructions identical (542,906,355), frames identical (256 × 241 LEDs, same sha), fps identical at 46.5. This is the tier that answers "what fps / will it OOM / does it work" without changing any of the three answers | **sm** — a fabric tap and a decoder feed; the patch is 87 lines | it ships as a *mode*, not a default: the correctness lane keeps the fabric. A run that has ever had the tier on may never be compared against a board |
| **the per-word slice cadence** (what R2/R3 tried to remove) | **computed: ~781 ms ≈ 14 %** (1,470,755 slices × 531 ns) | total in the pin lane | **fatal as attempted.** Both built rungs break the guest: no frames, wrong instret. The transmitter cannot read ahead of the CPU | **lg**, and **no known correct shape** | someone finds a shape that does not read RAM ahead of the guest. The one unexplored candidate is a **write-watermark bulk**: the RMT already tracks the guest's last RAM write (`refill_wrote`, `RefillProbe::Filling { last_write }`), so a transmitter could bulk only up to the highest word the guest has actually written. That bounds the bulk correctly and is **NEVER MEASURED** |
| **R4 — the UART TX-FIFO tap** | on `render-basic`: **~0.14 % ceiling**, i.e. nothing. On the **harness** image M4 measured 86 % of MMIO as TX-FIFO poll | the UART0 byte transcript — the primary identity surface | the console text a user reads | sm | someone is optimising the *harness* image. For product images this lever is dead |
| **R3′ — the firmware turns real output off under emulation** | not measurable: the switch does not exist | the pin log and everything downstream of the wire | **the guest's own ISR stops running**, so the reported fps is one the hardware will not deliver. This is exactly the trade §0 forbids in the user lane | md (firmware) | it would have to report fps from a *modelled* refill cost rather than a real one, which is a cycle-model change, not a switch |
| **`tick`** — the boundary as an import the stay calls | **≈ 376 ms ≈ 6.8 %** (P1, measured) | none — P1 proved the trap hook is in exactly one place and translated code never writes `mcause`/`mepc`/`mtvec` | none | **lg** | shelved at G-LOOP0 as a *milestone*; it re-enters here as **headroom**. Against a 10–18 % thermal shortfall, 6.8 % is a third to two thirds of the gap |
| ~~**`wall_timeout` → `started.elapsed()`**~~ **— SHIPPED 2026-09-13, P1c (#736)** | **227 ms ≈ 4.1 %** predicted, one hunk | none | none | **xs** — one hunk | **taken.** The check strides: the clock is read every 64th slice, so the net may fire up to 63 × `MAX_SLICE_CYCLES` emulated cycles late. Yona ruled the granularity acceptable at `G-LOOP0b`. See §3's dated row for what it measured |
| **the published-register table** (P3 generalised) | NEVER MEASURED. SYSTIMER is 47.05 % of crossings and 2,322,975 of its 2,767,705 are *stores* to `unit0_op` — the published-read side addresses the 444,730 loads | none if the disarm rules hold (trace / strict / after an escape) | none | md | the store side stays: the interpreter polls after *every* MMIO store |
| **per-tick work** (P4's phase) | NEVER MEASURED as a phase. `run_due_events` 295 ms + `drain_pins` 180 ms = 475 ms is the target, and `tick` relocates it rather than removing it | none | none | md | |
| **translator quality** | **4.5 ns per translated instruction at 8/fn against ~2 warm**; the cold-code 2.3× residual is unexplained | none | none | **lg** | the one lever that is pure win in both lanes. It is also the only lever that touches `boot-idle-memfs`'s 47 % shortfall, which is fixed translation cost |
| **PGO** | **+1.45×** measured, 2026-09-06 | none | none | md (build) | the build-side cost has to be worth carrying |
| **a lazy / coalesced RMT transmitter** | ~45 ms of crossings | the pin log's `(at, seq)` interleaving | — | md | **superseded by P1b**: the ordering trap is not the binding constraint; the RAM read-ahead race is |

### Reading the menu against 1×

The phone's shortfall is **10–18 %** on `render-basic`. Nothing on this menu
closes it alone, and the two biggest entries are unavailable:

- **Available and exact in the user lane:** R1 (1–2.7 %) + `wall_timeout`
  (4.1 %) + `tick` (6.8 %) ≈ **12 %** — which does reach the bottom of the
  band, and every one of those three leaves the guest's instret, frames and
  fps untouched. **`wall_timeout` was taken on 2026-09-13 (P1c, #736)**, so
  what is left of that 12 % is R1 and `tick`.
- **Unavailable:** the per-word cadence (14 %) has no correct shape; the full
  tier (R3) reports an fps the hardware will not deliver.
- **Untouched by all of it:** `boot-idle-memfs` at 0.53×, which is translation
  cost, not loop cost.

---

## 5. Pointers

**Planning.** `~/.photomancer/planning/lp2025/2026-09-11-1731-emu-loop-redesign/`
(this milestone: `vision.md`, `plan.md`, `G-LOOP0-gate.md`,
`G-LOOP0b-gate.md`). Archived: `_archive/2026-09-07-0827-emu-speed-ladder/`
(M7 and M7b director logs, DD1–DD63), `_archive/2026-09-11-1706-perf-lab/`.

**Reports.** `docs/reports/2026-09-06-lp-riscv-emu-speed-probe.md`,
`2026-09-07-emu-web-bench-baseline.md`, `2026-05-12-jit-math-perf.md`.

**ADRs.** `docs/adr/2026-09-11-emulator-wasm-translator.md`,
`docs/adr/2026-09-08-emulator-poll-loop-skip.md` (Rejected — read it before
proposing anything that credits a spinning guest),
`docs/adr/2026-09-06-esp-soc-emulator-architecture.md`.

**The cycle model.** `docs/esp32c6-guest-cycle-model.md` — the document that
defines what "what fps?" means.

**The rigs.** `scripts/emu/bench-web.sh` + `scripts/emu/bench-web/` (the
browser/phone rig and its `bench-cli.mjs` desk half), `scripts/emu/bench-c6.sh`
(`just bench-emu-c6`), `scripts/emu/p6b-rows.mjs` (desk rows),
`scripts/emu/p6c-prof.mjs` (bucket profile), `scripts/emu/p6-oracle.sh` (the
free oracle), `scripts/emu/loop-identity.sh` (the pair protocol),
`scripts/emu/tier-probes/` (this phase's rungs, as patches),
`scripts/emu/lab/` (the perf lab).

**READMEs.** `lp-emu/README.md` §Speed (the opt-level rule and the probe),
`lp-emu/lp-emu-jit/README.md` (the bucket table, the census sections, the
identity protocol).
