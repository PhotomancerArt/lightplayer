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
  beside it is not a row. **And refuse to quote a desk row taken above
  loadavg ~8** — other agents' worktrees held this desk at 43–101 for two
  hours during P1c, and a row taken there measures the desk, not the lever.
  Read the load from `uptime` or `sysctl -n vm.loadavg`, and see the bun
  warning below.
- **`bun`'s `os.loadavg()` is a lie** (P1d, 2026-09-13). It returns
  `[≈0, 0, 0]` unconditionally — measured at `2.1e-10` against the kernel's
  `33.92` at the same instant. **Every JSC table this repo has ever printed
  through `rung-rows.mjs` reads `loadavg 0.0`, and that load is unknown, not
  zero** — including `G-LOOP0b`'s JSC rung table and §2's JSC rows below.
  `rung-rows.mjs` now reads `sysctl -n vm.loadavg` and falls back to
  `os.loadavg()`; rows printed before 2026-09-13 keep the false zero.
- **Phone spacing: the lab's cooldown is enough** (P1d, 2026-09-13, from the
  director's S1/S2 pair, §3). Ten presses at 3-minute spacing and ten
  back-to-back on the same build gave translated medians **1.046× vs 1.053×**
  and interpreter medians **0.677 vs 0.669** — **no thermal drift is visible
  back-to-back**. The 3-minute spacing this protocol used is not supported by
  measurement; the floor is the lab's own 60 s cooldown. DD41's "best of ≥3,
  quote the sequence" and the same-press translated ÷ interpreter ratio as the
  thermal control both stay.
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
  The lab auto-stages `origin/main` every ten minutes, so a base build id is
  just main's short sha; only a dirty rung needs a hand stage. **Never restart
  the lab server** — a restart answers every armed `wait` with a 503.
- **A lab report's "byte-identity INCONSISTENT" line is not always a defect.**
  It compares UART shas across every row in the job, so a job that runs two
  *images* reports two shas and calls them inconsistent
  (`2407828f80684331` is `render-basic`, `a570b6597cc0fc31` is
  `render-rocaille`). Read it per image.
- **The pair protocol's trace leg is a 500 ms window** (close-out B1,
  2026-09-13), the same window as its other four readings.
  `scripts/emu/loop-identity.sh` takes five sha256 readings per binary — pin
  log, frames, UART, trap log, and a second run with `--trace` — and until
  2026-09-13 that second run was **20 ms**. On `render-basic` t2 the WS281x
  transmitter has not started a frame by then: the first `RMT ch0 start` is at
  cycle **36,188,821 ≈ 452 ms**, so the trace column read `same` on P1d's
  R5/R5a rungs while UART, frames, pin log and trap log all read `DIFF`. **A
  `trace same` taken before 2026-09-13 is a boot-only reading and is not
  evidence about any peripheral** — every such row in §3 and in
  `scripts/emu/tier-probes/README.md` stands superseded on that column alone.
  At 500 ms the leg contains all 11 `RMT ch0 start` lines of the cell, costs
  **22 s of wall and 5.3 s of CPU per binary** (measured at loadavg 231; a
  quiet desk is faster) and writes a **66 MB** `.trace` per side. The script
  prints its own liveness line — `trace reaches the RMT: N 'RMT ch0 start'
  lines in the after-trace` — and an `N` of 0 means the column is boot-only
  again. Widening changes the trace sha and nothing else: a main-vs-main pair
  reads `pin f23626cda3afd779 · frames ea2745af0a1e3c23 · uart 0ccda7f466879e84
  · trap f06a3513191d759e`, the same four P1 took, with `trace
  350476ce3357c307` in place of the 20 ms `6c21134c626f2af1`.
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
| bun/JSC 1.1.18 | 8 | 6.35 s | 0.867× | **not readable under bun** |

⚠️ The JSC row's `loadavg 0.0` as originally printed was `bun`'s
`os.loadavg()`, which returns ≈0 unconditionally (§1). The load that run met
is **unknown**, not zero. `rung-rows.mjs` reads the sysctl from 2026-09-13.

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

Updated 2026-09-13 with the **phone margin sweep**, lab job
**`j-20260913-0726-c5bb`** (main `9f67d78`, iPhone, iOS 18.7 / Safari 26.6.1,
5 presses at 3 m, every row untainted, UART sha on every row). Its report's
"byte-identity **INCONSISTENT**" line is the lab comparing UART shas across
two *images* — `2407828f80684331` for `render-basic`, `a570b6597cc0fc31` for
`render-rocaille` — not a defect in the rows.

| device | image | grade | best (of real time) | the spread across spaced presses | shortfall to hold 1× | ÷ its own interpreter | source |
|---|---|---|---:|---|---:|---:|---|
| phone (JSC) | render-basic | t2 8/fn | **1.063×** (median 1.008, spread 14.5 %) | 1.008 / 0.909 / 1.042 / 1.063 / 0.995 | **up to 9 %** | — | lab `j-20260913-0726-c5bb` |
| phone (JSC) | **render-rocaille** | t2 8/fn | **1.414×** (median 1.382, spread 5.2 %) | 1.391 / 1.382 / 1.414 / 1.341 / 1.364 | **holds** | — | lab `j-20260913-0726-c5bb` |
| phone (JSC) | render-basic | t2 interpreter | 0.678× (median 0.644) | 0.644 / 0.638 / 0.678 / 0.624 / 0.647 | 36 % | — | same job — the thermal control |
| phone (JSC) | render-rocaille | t2 interpreter | 0.766× (median 0.753) | 0.714 / 0.751 / 0.766 / 0.759 / 0.753 | 25 % | — | same job — the thermal control |
| phone (JSC) | render-basic | same-press 8/fn ÷ interp | — | 1.56 / 1.42 / 1.53 / 1.70 / 1.54 | — | **1.70×** (median 1.54) | same job |
| phone (JSC) | render-rocaille | same-press 8/fn ÷ interp | — | 1.95 / 1.84 / 1.85 / 1.77 / 1.81 | — | **1.95×** (median 1.84) | same job |
| phone (JSC) | render-basic | t2 8/fn (older) | 1.025× | 0.863 / 0.945 / 1.008 / 0.921 / 0.818 — thermal | up to 18 % | — | M7 director log E7 |
| phone | harness, boot-idle-memfs | — | **NEVER MEASURED** | — | — | — | the rig now defines the rows (translator-quality P1); lab `j-20260913-1702-102a` is in flight on `c66d8ac` (3 presses) with no press completed yet |
| desk V8 16/fn | render-basic | t2 | 1.051× | — (best of 5, one invocation) | holds | — | P1b |
| desk JSC 8/fn | render-basic | t2 | 0.867× | — | **13 %** | — | P1b |
| desk V8 16/fn | render-rocaille | t2 | **1.166×** | — (best of 3, one invocation per image) | holds | 1.73× | P9 — corrected 2026-09-13, see below |
| desk V8 16/fn | harness | t2 | **3.062×** | — (best of 3, one invocation per image) | holds | 2.68× | P9 — corrected 2026-09-13, see below |
| desk V8 16/fn | boot-idle-memfs | t2 | **2.808×** | — (best of 3, one invocation per image) | holds | **0.53×** — the one image where the translated core LOSES | P9 — corrected 2026-09-13, see below |

**Correction, 2026-09-13 (translator-quality P1).** The last three desk rows
carried the wrong column. Until today they read `render-rocaille` 1.73×,
`harness` 2.68× and `boot-idle-memfs` **0.53× / 47 % short of 1×** — and those
three figures are the **`translated ÷ interpreter`** column of
`docs/adr/2026-09-11-emulator-wasm-translator.md` §"Per image, and where the
translated core is SLOWER" (the table at `:443-447`), not its `translated`
column. The ADR's own rows are 1.166× / 3.062× / 2.808× of **real time**, at
16 blocks a function, a 5,500 ms emulated bound, one invocation per image with
both legs back to back, best of three, load 2.6–4.3.

So **every product image holds 1× on the desk**, `boot-idle-memfs` included at
2.808×; what is true of `boot-idle-memfs` is the *other* column — it is the one
product image where the translated core is a **net loss against its own
interpreter** (0.53×), because ~1.08 s of fixed translation (discover 144 ms +
emit 892 ms + compile 41 ms) sits on a run that retires 51.0 M instructions
with a 43.7-instruction mean stay, and the wasm build translates by default
since #727. That is a translation-cost finding, not a margin finding. Both
columns are carried here now so the next reader cannot take one for the other.

No row was deleted, per §1's rule. Whether the correction re-ranks the
milestone is **open**, and this document does not answer it.

**The sweep moved the phone's number and narrowed the band.** `render-basic`
best 1.063× with a 14.5 % spread and a median of 1.008 — so the worst spaced
press is 0.909, a **9 %** shortfall rather than E7's 18 %. And
`render-rocaille`, never measured on the phone before, **holds 1× comfortably
at 1.382× median with a 5.2 % spread**: the heavier shader is the *easier*
image for the emulator, because more guest work per emulated microsecond means
the fixed per-slice cost is amortised further. The image that needs the margin
is the light one.

**One shortfall, one unknown** (rewritten 2026-09-13 with the correction
above). The phone's ~9 % on `render-basic` is thermal throttling of
steady-state work, and it is the **only** measured shortfall against 1× left
in this table: `render-rocaille` holds on both devices, and all four images
hold on the desk. What is *unknown* is `harness` and `boot-idle-memfs` on a
phone — **no phone row exists for either**. The rig defines the rows as of
translator-quality P1 and `j-20260913-1702-102a` is in flight; until presses land,
their phone margin is unknown, not good. `boot-idle-memfs`'s 0.53× is a
different question in a different column — fixed translation cost against its
own interpreter, which no steady-state loop lever touches.

---

## 3. What has been tried

Dated. Status is **shipped**, **rejected**, **held**, **registered** (a lever
named and deliberately not pursued) or **candidate**.

| date | lever | where the numbers live | measured effect | status |
|---|---|---|---|---|
| 2026-09-06 | `opt-level = "z"` → `3` for the five host emulator crates | `docs/reports/2026-09-06-lp-riscv-emu-speed-probe.md`; `lp-emu/README.md` §Speed | **2.3× free** | **shipped** |
| 2026-09-06 | bookkeeping reduction in the interpreter loop | speed-ladder research | **+2.1×** | **shipped** |
| 2026-09-06 | PGO (`scripts/emu/pgo-c6.sh`) | speed-ladder research | **+1.45×** | **registered** — the build-side cost was never taken on |
| 2026-09-09 | **the block cache** — pre-decoded basic blocks on the interpreter path | `docs/adr/2026-09-09-emulator-block-cache.md`; `lp-emu/lp-emu-core/src/block.rs` | **1.15× `render-basic` t2, 1.33× `render-rocaille` t2** (1.07× harness, 1.09× boot-idle); mean block 4.98, hit rate 99.93 % | **shipped** — Step A only; every Step B lever was built and measured at ~1.0× native / 1.12–1.14× phone and never merged (G3) |
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
| 2026-09-13 | **P1c — the `wall_timeout` check strides** (`WALL_TIMEOUT_SLICE_STRIDE = 64` in `machine.rs`) | this document §3's note below; PR #736 | **desk, best of 11 in one invocation per engine, `render-basic` t2 5,500 ms: V8 16/fn 5.15 s → 4.89 s (−260 ms, −5.0 %, 1.055×) at loadavg 5.8–7.1; JSC 8/fn 6.33 s → 6.13 s (−200 ms, −3.2 %, 1.031×)**. **Phone (lab `j-20260913-0758-2bb9`, iPhone, 5 spaced presses each): translated 8/fn median 1.049× → 1.091× (+4.0 %), best 1.100× → 1.129×; the interpreter row +6.2 % median.** Predicted 227/214 ms. Every identity leg `same` | **shipped** |
| 2026-09-13 | **B — the phone margin sweep** | lab `j-20260913-0726-c5bb`; §2's margin table | `render-basic` t2 8/fn best **1.063×** median 1.008 spread 14.5 %; **`render-rocaille` t2 8/fn best 1.414× median 1.382 spread 5.2 %** — the heavier image *holds 1×*. Interpreter controls 0.678 / 0.766 | **measured** — no code |
| 2026-09-13 | **the phone spacing experiment** (S1 3 m vs S2 back-to-back, same build, 10 presses each) | lab `j-20260913-0755-1213` and `j-20260913-0755-14e2` | translated median **1.046× vs 1.053×**, best 1.106 vs 1.070; interpreter median 0.677 vs 0.669, spread 3.6 % vs 12.6 % (S2's two low interpreter presses are p1 and p10 at 0.60 — the ends, not a drift). **No thermal drift is visible back-to-back** | **shipped into §1** — the protocol's spacing floor drops to the lab's 60 s cooldown |
| 2026-09-13 | **P1b R0 vs R1 on the phone** (the job G-LOOP0b could not run) | lab `j-20260912-1718-5940` (`f8c44c2-dirty-d65f8c` vs `-15a80d`, 5+5) | R1 over R0 best **+0.1 %**, median +6.3 % (inside the noise band), **same-press ratio identical (1.55 both)** | **candidate, unchanged** — the phone agrees with the desk that the middle tier is worth 1–3 % |
| 2026-09-13 | **P1d — the write-watermark bulk** (R5 / R5a) | `G-LOOP0c-gate.md`; `scripts/emu/tier-probes/README.md` §P1d; `R5-write-watermark.patch`, `R5a-watermark-plus-guard.patch` | **the guest breaks on every surface.** UART, frames, pin log and trap log all `DIFF`; frames truncate at 73 words. Two independent causes, both quoted from the trace: the driver's **STOP guard** is a write the watermark cannot read, and **`mem_raddr_ex`** (read twice per threshold, 5,302 times over 2,651 refills) is what the ISR uses to choose *which half to refill* | **rejected as built** — and see the cadence row in §4, now closed |
| 2026-09-13 | **P1d R5c — the shadow census** (no behaviour change; byte-identical on all five identity readings) | `R5c-shadow-census.patch` | a watermark+guard bulk would absorb **95.80 %** of word fetches on **both** `render-basic` (63,646 → 2,673 events) and `render-rocaille` (17,358 → 729), run length exactly the 24-word half. Scaled to the 5,500 ms cell: **1,409,821 slices × 531 ns ≈ 749 ms ≈ 13.6 %**. And the watermark itself **never binds**: `unwritten 0` on both images; the threshold ends every run | **measured** — the size of the prize, on the healthy cadence |
| 2026-09-13 | **translator-quality P1 — the gate preset grows to all four product images**, and §2's three desk rows are corrected | `scripts/emu/bench-web/bench-run.js` (`GATE_ROWS`); §2's margin table and its correction note; lab `j-20260913-1702-102a` | **no code under `lp-emu/`.** The desk proof that the two new rows are real, node/V8 25.2.1 at 8/fn on build `c66d8ac`, one invocation, both legs per image: every row stops at **exactly 5,500,000 us emulated (880,000,000 cycles)** — the emulated bound, not `harness`'s `--exit-on` marker and not the wall guard — and the two legs of each image agree byte for byte on UART (`harness` `b6da0bd777529664…`, `boot-idle-memfs` `17a0a1a3869073fe…`), on the trap log and on retired instructions (289,953,877 and 51,052,668). Coverage and mean stay reproduce the ADR exactly (99.63 % / 1354.0 and 97.82 % / 43.7). **The wall times are NOT margin numbers — loadavg was 181–186** (53 concurrent `rustc`); the same-invocation *ratios* still land near the ADR's, 2.46× and 0.514× against 2.68× and 0.53×. **No phone row yet**: the job was still on its first press when this row was written | **measured** (rig + doc only) |
| 2026-09-13 | **the two-channel `(at, seq)` pin-log ordering trap** (`vision.md` §2, registered since the tier work began) | `scripts/emu/tier-probes/README.md` §"It fires, and it is bigger than a same-cycle tie"; `lp-emu-esp32c6` `rmt.rs` `tests::two_tx_channels_put_the_pin_log_in_dispatch_order_not_at_order` | **the assumption had NEVER RUN** — no product image drives two TX channels (`render-basic` and `render-rocaille` start ch0 only; ch1 is configured and never started), so it was constructed. It **fires, and wider than registered**: `push_pulse` emits both halves of a word at the fetch, so a second channel's word lands *behind* the first's already-future edge and the combined `at` column is not monotone at any point — `0, 64, 0, 64, 200, 264, …`. Start ch1 first and the wire is byte-identical while the log's order flips; `Machine::drain_pins` writes `Fabric::take_edges` into `--pin-log` with no sort. **Not a defect on the product path**: every consumer is per pad, each pad's own edges are strictly increasing, and there is one `Ws281xDecoder` per pad | **registered — now tested, not merely named.** The exposure is a reader who treats the combined pin log as a time-ordered stream, and a byte-identity comparison of two runs that dispatch the channels in different orders |
| 2026-09-13 | **Xtensa #735, `SocBus::add_ram_alias`** — one `bool` test (`has_ram_alias`) per memory access on the C6's hot path (E5) | PR #735, merged 2026-09-13 09:39Z; the A/B is in that PR's own body, `scripts/emu/bench-c6.sh --no-build --no-promote`, stock vs branch back to back | **quoting #735's pair 3** (post-rebase binaries, best of 3, load 4.2–5.1 on 12 cores, user seconds): `render-basic` t1 5.55 → 5.46 (**−1.6 %**), t2 5.00 → 4.94 (**−1.2 %**); `render-rocaille` t1 4.48 → 4.42 (−1.3 %), t2 4.49 → 4.45 (−0.9 %); `harness` t1/t2 **+0.9 %** each; `boot-idle-memfs` t1 0, t2 −3 %. Their words: "every render-loop row is within ±2 %, both signs — inside the bench's own run-to-run noise." Binary text +8.9 KB (2,034,850 → 2,043,774). **Not re-measured here** — the row quotes theirs | **shipped (theirs)** — no measurable C6 cost, inside the bench's noise. The C6 registers no alias, so the cost is the `bool` and nothing else |

### The P1c note: what the stride removed, and what three instruments said

The lever is one hunk. `run_until` asked `started.elapsed()` at every slice
boundary to see whether `--wall-timeout` had expired; it now asks on the first
slice of a run and every 64th after. The net is a **diagnostic** stop (exit
code 4) and no guest state depends on it, so the only thing the stride costs is
*when* it fires: up to 63 × `MAX_SLICE_CYCLES` = 516,096 emulated cycles late,
≈ 3 ms of wall at 1×. Yona ruled that acceptable at `G-LOOP0b`.

**The load-independent number is the computed one.** A `render-basic` t2 run
makes 1,531,923 slices, so the stride removes 1,508,000 of its 1,531,923 clock
reads; at P1's measured 148.3 ns (V8) and 139.5 ns (JSC) that is **224 ms and
210 ms**. Nothing else on the loop path reads a host clock: after P1c the only
unconditional `Instant::now()` in `run_until` is the one at entry, and every
read in `jit.rs` is behind `LP_EMU_JIT_ENTRY_TIME` or one-shot at translation.

**And the desk agrees.** Best of 11, interleaved `BEFORE, AFTER, …` in one
invocation per engine, base `8ee2e1510`, on a quiet desk:

| engine | before | after | delta | × | loadavg | P1b's baseline |
|---|---:|---:|---:|---:|---:|---:|
| node/V8 25.2.1, 16/fn | 5.15 s (1.067×) | **4.89 s (1.125×)** | **−260 ms (−5.0 %)** | **1.055×** | 5.8–7.1 | 5.24 s |
| bun/JSC 1.1.18, 8/fn | 6.33 s (0.869×) | **6.13 s (0.897×)** | **−200 ms (−3.2 %)** | **1.031×** | 9.6 (from `node`) | 6.35 s |

The before rows reproduce P1b's clean baselines (5.24 s / 6.35 s), and the
deltas land on the computed prize from the other side: −260 against 224
predicted in V8, −200 against 210 in JSC. Every one of the 44 rows is
byte-identical — UART0 `2407828f80684331`, frames `830bcc3f3088d4ac`, trap
`51ddaf56c96b77d3`, 542,906,355 retired instructions, 256 frames.

**The phone says the same** — lab job `j-20260913-0758-2bb9`, an iPhone, five
spaced presses per build, base `9f67d78` against that base plus this hunk:

| row | before, best / median | after, best / median | median Δ |
|---|---:|---:|---:|
| `render-basic` t2 8/fn | 1.100× / 1.049× | **1.129× / 1.091×** | **+4.0 %** |
| `render-basic` t2 interpreter | 0.683× / 0.681× | 0.735× / 0.723× | +6.2 % |

UART0 `2407828f80684331` on every phone row of both builds. The interpreter
row moves too, and slightly more, which is what a lever in `run_until` — the
loop both cores share — should do; it is also why the same-press translated ÷
interpreter ratio reads −1.9 %, which is not a regression in the translated
core.

⚠️ **A bun row's `loadavg` is a fiction.** `rung-rows.mjs` records
`os.loadavg()[0]`, and **bun's `node:os.loadavg()` returns ~0 unconditionally**
(measured 2026-09-13: `1.8e-10, 0, 2.8e-12` against node's `70.8, 83.2, 63.8`
on the same desk in the same second). Every JSC row this runner has printed
therefore reads `loadavg 0.0` whatever the desk was doing — including §2's and
`G-LOOP0b`'s "loadavg 0.0" JSC tables, whose load is **unknown, not zero**.
§1's law that "a wall-clock number with no `loadavg` beside it is not a row"
is not satisfied by a bun row today. Take the load from `node` or `uptime`
beside the invocation until the runner is fixed.

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
| ~~**the per-word slice cadence**~~ **— CLOSED 2026-09-13 (P1d), unless the read pointer is virtualised** | **measured: 95.80 % of word fetches absorbable, ≈ 749 ms ≈ 13.6 %** of a 5,500 ms run (R5c's shadow census on the *healthy* cadence — 63,646 → 2,673 events on `render-basic`, the same 95.80 % on `render-rocaille`). Supersedes P1b's 781 ms computed from a broken guest's slice count | total in the pin lane | **fatal in every shape built.** R2 (threshold-bounded), R5 (+ the write watermark) and R5a (+ a guard bound) all break the guest on all four transcript surfaces | **lg**, and the only shape left is a redesign | **The write-watermark bulk is measured and closed.** Two causes, independent, each fatal on its own: (1) the driver plants a **STOP guard** in the half it just left and then overwrites it while refilling, so "the guest has written it" is a property the driver deliberately gives a word it means to withdraw — the bulk takes the guard 24 word-times early and every frame truncates at 73 words; (2) **`ch0_tx_status.mem_raddr_ex`** is read **twice per threshold** (5,302 reads over 2,651 refills, at 398 and 1,370 cycles into a 4,800-cycle half) and `in_second_half = pos_before >= half` is how the ISR chooses **which half to refill** — a bulk moves the pointer to the half's end at the half's *first* cycle, so the ISR refills the half the transmitter is standing in. And the watermark bound itself is **inert**: `unwritten 0` on both product images. What is left is not a bound at all — the read pointer would have to be **virtualised to the observation cycle** (lazy bookkeeping caught up on every RMT access, the threshold a scheduled event re-predicted when `tx_lim` moves). That is `tick`-style exactness applied to one peripheral: **designed at P1d, NOT BUILT** |
| **R4 — the UART TX-FIFO tap** | on `render-basic`: **~0.14 % ceiling**, i.e. nothing. On the **harness** image M4 measured 86 % of MMIO as TX-FIFO poll | the UART0 byte transcript — the primary identity surface | the console text a user reads | sm | someone is optimising the *harness* image. For product images this lever is dead |
| **R3′ — the firmware turns real output off under emulation** | not measurable: the switch does not exist | the pin log and everything downstream of the wire | **the guest's own ISR stops running**, so the reported fps is one the hardware will not deliver. This is exactly the trade §0 forbids in the user lane | md (firmware) | it would have to report fps from a *modelled* refill cost rather than a real one, which is a cycle-model change, not a switch |
| **`tick`** — the boundary as an import the stay calls | **≈ 376 ms ≈ 6.8 %** (P1, measured) | none — P1 proved the trap hook is in exactly one place and translated code never writes `mcause`/`mepc`/`mtvec` | none | **lg** | shelved at G-LOOP0 as a *milestone*; it re-enters here as **headroom**. Against a 10–18 % thermal shortfall, 6.8 % is a third to two thirds of the gap |
| ~~**`wall_timeout` → `started.elapsed()`**~~ **— SHIPPED 2026-09-13, P1c (#736)** | **227 ms ≈ 4.1 %** predicted, one hunk | none | none | **xs** — one hunk | **taken.** The check strides: the clock is read every 64th slice, so the net may fire up to 63 × `MAX_SLICE_CYCLES` emulated cycles late. Yona ruled the granularity acceptable at `G-LOOP0b`. See §3's dated row for what it measured |
| **the published-register table** (P3 generalised) | NEVER MEASURED. SYSTIMER is 47.05 % of crossings and 2,322,975 of its 2,767,705 are *stores* to `unit0_op` — the published-read side addresses the 444,730 loads | none if the disarm rules hold (trace / strict / after an escape) | none | md | the store side stays: the interpreter polls after *every* MMIO store |
| **per-tick work** (P4's phase) | NEVER MEASURED as a phase. `run_due_events` 295 ms + `drain_pins` 180 ms = 475 ms is the target, and `tick` relocates it rather than removing it | none | none | md | |
| **translator quality** | **4.5 ns per translated instruction at 8/fn against ~2 warm**; the cold-code 2.3× residual is unexplained | none | none | **lg** | the one lever that is pure win in both lanes. It is also the only lever that reaches **`boot-idle-memfs`'s 0.53× against its own interpreter** — ~1.08 s of *fixed translation* on a 5.5 s run, which no steady-state loop lever touches. (That image holds 1× of real time at 2.808×; the 0.53× is the ratio column, corrected in §2 on 2026-09-13) |
| **PGO** | **+1.45×** measured, 2026-09-06 | none | none | md (build) | the build-side cost has to be worth carrying |
| **a lazy / coalesced RMT transmitter** | ~45 ms of crossings | the pin log's `(at, seq)` interleaving | — | md | **superseded by P1b and closed by P1d**: the ordering trap is not the binding constraint, and neither is the RAM read-ahead race. The binding constraint is that the guest's driver *reads the read pointer* and branches on it. P1d also found that the ordering trap could not be tested at all — `render-basic` and `render-rocaille` both drive exactly **one** TX channel, so two channels' edges never interleave. `Fabric::push` appends in call order, `Edge` carries no sequence number: the trap is real for a two-channel image and stays **NEVER MEASURED** |

### Reading the menu against 1×

Rewritten 2026-09-13 after B's sweep and P1d.

The phone's shortfall on `render-basic` is now **~9 %** (worst spaced press
0.909 of five, best 1.063 — job `j-20260913-0726-c5bb`), not 10–18 %. And
`render-rocaille`, the heavier image, **already holds 1×** at a 1.382× median.

- **Available and exact in the user lane:** `wall_timeout` (4.1 %) **taken**
  on 2026-09-13 (P1c, #736, phone +4.0 % median). What is left is R1
  (1–2.7 % desk, +0.1 % best / +6.3 % median on the phone — inside the noise)
  and `tick` (6.8 %). Together ≈ **8–9 %**, which now meets the band rather
  than falling short of it — but `tick` is the large-engineering item in it.
- **Unavailable:** the per-word cadence — **13.6 %, measured, and closed by
  P1d**: the guest's driver branches on the read pointer, so no bulk that
  moves the pointer early can be exact, and only a virtualised read pointer
  (a redesign of the RMT engine, not a patch) could collect it. The full tier
  (R3) reports an fps the hardware will not deliver.
- **Untouched by all of it:** `boot-idle-memfs` at **0.53× of its own
  interpreter** — a *fixed* translation cost, not loop cost, and the only
  number in this document no loop lever reaches. It is **not** a margin
  shortfall: that image runs at 2.808× of real time on the desk and holds 1×
  with room (§2's correction, 2026-09-13). And `harness` and `boot-idle-memfs`
  still have **no phone row at all**, so the desk is all either of them has.

---

## 5. Pointers

**Planning.** `~/.photomancer/planning/lp2025/2026-09-11-1731-emu-loop-redesign/`
(this milestone: `vision.md`, `plan.md`, `G-LOOP0-gate.md`,
`G-LOOP0b-gate.md`). Archived: `_archive/2026-09-07-0827-emu-speed-ladder/`
(M7 and M7b director logs, DD1–DD63), `_archive/2026-09-11-1706-perf-lab/`.

**Reports.** `docs/reports/2026-09-06-lp-riscv-emu-speed-probe.md`,
`2026-09-07-emu-web-bench-baseline.md`, `2026-05-12-jit-math-perf.md`.

**ADRs.** `docs/adr/2026-09-11-emulator-wasm-translator.md`,
`docs/adr/2026-09-09-emulator-block-cache.md` (the rung below it, and why a
decode memo is not architectural state),
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
