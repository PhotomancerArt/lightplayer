# The classic ESP32 (v3) emulator performance ledger

**A living document**, the C6 ledger's twin. What the classic Xtensa
emulator's speed work has tried, what it measured, what it rejected and why,
and the menu of what is left. Never delete a row; mark it superseded and date
the replacement.

Written 2026-09-13 at the Xtensa speed ladder's phase P03 (plan
`lp2025/2026-09-13-1008-xtensa-speed-ladder`, M7 of "plan three"). If you have
just been handed this machine and want to make it faster, read §1 before you
take a number and §4 before you pick a lever.

---

## 0. The two goals

Same two goals as [`docs/emulator-perf-ledger.md`](emulator-perf-ledger.md)
§0 — the C6's ledger states them in Yona's own words and they are not
chip-specific, so they are not restated here. In short: **goal 1** is
correctness (byte-identity against a board's transcript — UART0, the `run:`
line, the decoded frames, the pin log); **goal 2** is speed on the happy
path, and the bar the emu-loop-redesign milestone set is **1× real time,
held**, not "as fast as possible". This document's §3 ladder is measured
against roadmap item 4's own numbers (**≥1.5× is the floor for "delivered",
≥3× the target**, on `render-loop` at t1) because that is what this plan
was scoped against (`plan.md` §"Goal and acceptance" item 3) — G-M7D-XT Q1
is where the two framings are reconciled, and it does not move this plan's
bar.

---

## 1. How a number is taken

**Read [`docs/emulator-perf-ledger.md`](emulator-perf-ledger.md) §1 first —
every rule there (one invocation per table, interleaved best-of-N, the load
average on every row, refuse a desk row above loadavg ~8, `caffeinate -dims`,
never gate on emulated microseconds) applies here unchanged.** What follows
is only what is different about this chip.

- **t1 is the only grade this machine has** (plan decision E1): a cycle IS
  an instruction here. There is no t2 sweep and a "t2 column" on this chip
  would be a column of the same numbers as t1.
- **Both cores, at the default `--core-quantum 256`.** The classic is dual
  core and the run loop hands each unheld core a window per iteration (D3),
  so `instr/s` is reported per hart as well as in total — core 1 spends
  almost all its time parked in `waiti` and a single combined figure would
  hide that. The quantum is a run parameter: a number taken at another
  quantum is not comparable with these.
- **`render-loop` is the row to quote.** It is the product's render loop,
  241 lamps on IO18, the same project and the same pixels the C6's
  `render-basic` renders, retargeted `D10` → `IO18` at firmware build time.
  `boot-idle` and `shader-compile-stress` are an idle image and a compile
  harness and neither is what the product does — see
  `scripts/emu/bench-esp32v3.sh`'s own header for the ⚠️ that says so.
- **The `uart` `cmp` and the `blocks` column ride every row**
  (`bench-esp32v3.sh` since P03): `uart` is the identity oracle (`cmp`
  against `prev/`, or `same`/`DIFFERENT` in `v3-oracle.sh`'s pair mode);
  `blocks` is a diagnostic, never an identity column — the cache's mean
  realised block length (core 0) read off the `blocks: core0 … mean=…`
  report line, `off` under `--no-block-cache`, `n/a` on a binary that
  predates the counter.
- **`scripts/emu/v3-oracle.sh`** is the free oracle, `p6-oracle.sh`'s
  classic twin, in flag mode (fast path against `--no-block-cache` today)
  and pair mode (`--bin-a`/`--bin-b`, two binaries with the fast path off on
  both — the leg that proves the interpreter itself did not move). Four
  cells cover the three images: `boot-idle 100ms`, `shader-compile-stress
  2s`, `render-loop 20ms` (a `--trace` — before the first frame, so the
  trace is the fine reading), `render-loop 2200ms` (`--dump-frames`, 256
  decoded frames).
- **`bench-esp32v3.sh --extra-flags "<flags>"`** (P03) appends a flag
  STRING, unquoted, to every invocation of the binary under test — it
  exists so rung 0 (`--no-block-cache`) can be re-taken against *any*
  binary this script is pointed at, PGO included, without a second script.
- **The C6's numbers never stand in for the classic's** (P1b's rule,
  restated in `plan.md`'s Conventions). A projection from the C6 is labelled
  as one.

---

## 2. Where the time goes today

`render-loop`, the full run to its own `[render-loop] === DONE ===` marker
(2,127,688 µs emulated, 510,645,324 cycles, 529,714,654 instructions:
core0=434,061,843, core1=95,652,811). Measured 2026-09-11 (M7 P1b,
`m7/notes.md`) on this Mac, commit **`0773c3fbd`**, before any rung in §3
landed — the interpreter's own shape.

### The bench table, at the top

| image | user s | rt(user) | instr/s |
|---|---:|---:|---:|
| `boot-idle` | 0.79 | 3.797× | 32.6 M |
| `shader-compile-stress` | 1.67 | 0.155× | 37.3 M |
| **`render-loop`** | **16.33** | **0.130×** | **32.4 M** (core0 26.6 M, core1 5.9 M) |

**The one-line answer, P1b's own words: the classic's render loop ran at
0.130× real time at t1 — 11.5× short of the 1.5× floor and 23× short of the
3× target**, before any rung below existed.

### Host time (P1b §1.2, `selfprof`, 16,354 samples over 1,107 pcs, load 56.5 — biases seconds, not shares)

| bucket | share of host time |
|---|---:|
| execute (dispatch + executors + registers) | **34.0 %** |
| decode (`lp-xt-inst`) | **27.5 %** |
| guest RAM access | 12.8 % |
| guest fetch | 7.1 % |
| **window machinery** (`ar_group`, overflow check, ENTRY/RETW) | **6.6 %** |
| host stream + `--exit-on` | 1.9 % |
| MMIO | 1.7 % |
| scheduler + interleave | 1.7 % |
| interrupt sampling | 0.7 % |
| other (incl. 1.8 pts outside the binary) | 6.0 % |

Hottest single frames: `lp_xt_inst::decode::decode` 5.9 %, `XtHart::step`
4.6 %, `RamRegion::len` 4.3 %, `SocBus::fetch_region_index` 2.6 %,
`window::ar_group` **2.4 %**, `decode::word_of` 2.2 %. ⚠️ A sampled share is
an attribution, not an ablation — nothing here was ablated (P1b §1.2).

### MMIO (P1b §1.3, `bench` build, 1.15× cost while counting)

**3,550,668 MMIO accesses — 2.58 % of 137,796,811 data accesses.** 44.06 %
of MMIO is one register, UART0's `status` (the TX-FIFO poll) — the roadmap
predicted this right, but the denominator is what matters: **MMIO is 1.7 %
of host time here**, not 86 % of it the way the C6's *harness* image reads.
A perfect poll skip buys at most 1.7 % (§2.1 below).

### The window check (P1b §1.4): 6.6 % of host time for a 0.09 % event

**458,373 window exceptions on the render loop — one every 1,156
instructions**, 98.98 % the 8-register form. The handlers retire 4,560,255
instructions (0.86 % of the run) — the *check*, not the handler, is what
costs, because `ar_group` runs on every instruction and the exception fires
on 0.09 % of them. This is exactly what P02 (§3 below) hoisted.

### The guest's own code (P1b §1.5): LOOP is dead, 11.9 % is JIT'd

Two zero-overhead loop bodies in the whole image, 0.0009 % of retired
instructions — `LOOP` is a correctness item, not a performance one.
**11.86 % of the render loop's instructions are code the guest wrote at run
time** (the shader JIT's 5 KiB SRAM0 region) — a translation-invalidating
event with no symbols, a design input for any future translator, not for
this phase.

### The prize P1b's own arithmetic gave for a full interpreter-only ladder (§2.7)

Decode (27.5 %) + fetch (7.1 %) + the window check (6.6 %) = **41 % of host
time**; removing all of it perfectly is **1.7×**, i.e. 0.130× → **≈0.22×**.
**The 1.5× floor is not reachable by the interpreter ladder alone** — this
is the number that makes a translator non-optional on this chip, the same
conclusion the C6's own ladder reached (`docs/emulator-perf-ledger.md`).
This document's §3 is about how much of that 1.7× the interpreter ladder
actually banked, and §4 is about what is left after it.

**Correction, P03 (this document, 2026-09-13).** §3's rungs 1–2 together
measured **≈2.0×** on `render-loop` — *above* the 1.7× ceiling this section
computed. Either the sampled shares understated decode/fetch, or the block
cache buys something beyond eliding them (shorter dispatch, better
locality); P01's own PR body flagged the same disagreement. Reported as the
disagreement it is (DD105); this document does not re-sample to resolve it.

---

## 3. The ladder table

Every row below is a **same-window A/B**: the saved binary against the new
one (or the same binary against its own off-switch for rung 0), interleaved
`A, B, A, B, A, B`, best of three, `render-loop` at t1, both cores at
`--core-quantum 256`, the `uart` `cmp` `same` on every row.

| rung | head | `render-loop` t1 user s | rt(user) | instr/s | Δ | load |
|---|---|---:|---:|---:|---:|---:|
| 0 stock (P1b, `0773c3fbd`, 09-11) | — | 16.33 | 0.130× | 32.4 M | — | 10.0 |
| 1 block cache (P01, #758, `d73f1c8da`) | cache off → on, same binary | 16.07 → 9.38 | 0.132× → 0.227× | 33.0 M → 56.5 M | **1.71×** | 5.5–7.2 |
| 2 window hoist (P02, #766) | P01's binary → P02's | 9.85 → 8.37 | 0.216× → 0.254× | 53.8 M → 63.3 M | **1.18×** | 7.6–8.3 |
| 3 PGO (P03, this phase, opt-in) | P02's binary → PGO build of it | 8.23 → 5.65 | 0.259× → 0.377× | 64.4 M → 93.8 M | **1.457×** | 5.0–6.8 |

**Rungs 0–2 are quoted from their own PRs' bodies** (#758, #766), each taken
in its own window, per the director's ruling that they are re-taken together
here rather than re-measured — the director notes below are the record of
that ruling. **Rung 3 was measured fresh in this phase's own window.**

### Rung 3, in full — the PGO A/B

`scripts/emu/pgo-esp32v3.sh` (instrumented build → one training run of each
of the three pinned images at t1 → `llvm-profdata merge` → optimised
rebuild), then `bench-esp32v3.sh --bin <pgo> --no-build --no-promote`
against the P02 binary already on `main` (`3f47fa00b`). Three interleaved
rounds, one invocation per leg per round, taken in a **quiet window** after
an earlier attempt was contaminated by the desk's other agent (below):

| round | stock, user s | load | PGO, user s | load |
|---|---:|---:|---:|---:|
| 4 | 8.29 | 6.84 | 5.67 | 6.57 |
| 5 | **8.23** | 5.44 | **5.65** | 4.96 |
| 6 | 8.26 | 5.35 | 5.66 | 5.34 |

**Best of three: 8.23 → 5.65 = 1.457×.** `rt(user)` 0.259× → 0.377×. Every
round's `uart` read `same` against the P02 binary's promoted output.
`instr/s` 64.4 M → 93.8 M (core0 52.7 M → 76.8 M, core1 11.6 M → 16.9 M).

⚠️ **Three earlier rounds are not in the table above and are reported here
as a deviation.** M6 P07 (the S3's frame walk) was building on this desk
throughout the session; the first three interleaved rounds were taken while
its build ran, loads 17.29–37.91:

| round | stock, user s | load | PGO, user s | load |
|---|---:|---:|---:|---:|
| 1 | 8.20 | 4.51 | 7.80 | 17.29–19.83 |
| 2 | 8.93 | 36.70–37.91 | 5.72 | 24.02–30.87 |
| 3 | 8.26 | 28.41–31.02 | 5.50 | 24.02–25.41 |

Round 1's PGO leg (7.80) is the outlier — its own load climbed from 17.29
to 19.83 mid-run, well above the ~8 bar, and it disagrees with every other
PGO reading (5.50–5.72) by 38–42 %; the stock leg of round 2 (8.93) is
likewise the slowest stock reading recorded, at the session's peak load
(37.91). These three rounds are **not used for the rung's Δ** above; they
are consistent enough with the quiet rounds (PGO clustering 5.50–5.72
throughout) to say the direction is right, but not clean enough to quote.
**No re-take is wanted** — the quiet-window rounds 4–6 already satisfy
acceptance 2 (interleaved, best of three, load under ~8 on every leg).

Every column of `v3-oracle.sh`'s pair mode (`--bin-a` the stock P02 binary,
`--bin-b` the PGO build, fast path off on both — the leg that proves PGO
changes codegen and nothing else) reads `same` on all four cells:

| image | window | mode | uart | run | frames/trace | stdout | stderr* |
|---|---|---|---|---|---|---|---|
| boot-idle | 100ms | pair | same | same | same | same | same |
| shader-compile-stress | 2s | pair | same | same | same | same | same |
| render-loop | 20ms | pair | same | same (trace) | same | same | same |
| render-loop | 2200ms | pair | same | same | same | same | same |

including the masked `blocks:` lines themselves, printed raw beneath each
cell — the counters (`mean=4.26` core 0, `mean=3.50` core 1, `collisions=
101705`, `window hoisted=100871285 … 99.80% hoisted`) are byte-identical
between the stock and PGO binaries, which is the whole claim: **PGO changes
codegen and nothing the guest, the cache or the hoist can observe.**

`just bench-emu-esp32v3-pgo` (the full recipe, run end to end in one
foreground command once the target dirs were warm) reproduces the same
binary and the same `uart same` against the promoted baseline.

### A same-session sanity check, not a rung

`--no-block-cache` on the current (P02) binary read `render-loop` **16.70
user s** (load 8.37–8.90) earlier in this session — the interpreter-only
baseline, on the same tree the PGO binary was built from. Against the PGO
binary's 5.65 user s that is **≈2.96×**, a real same-tree, cross-flag ratio
rather than the rungs' product (1.71 × 1.18 × 1.457 ≈ 2.94× — the two agree
to within noise, which they should: the flag pair and the binary pair are
measuring the same three landed rungs two different ways). Not a rung of
its own; recorded because it is the only number in this document that
touches all three landed rungs in one run.

### `just test-emu-esp32v3-boot` against the PGO artifact: **NEVER RAN**

The recipe has no substitution point for a pre-built binary: it runs
`cargo test -p lp-emu-esp32v3 -- --include-ignored`, which compiles its own
test binaries and drives the `Machine`/`XtHart` API directly rather than
exec'ing the standalone `lp-emu-esp32v3` CLI binary this phase built with
PGO's `RUSTFLAGS`. There is no `--bin` (or equivalent) the gate suite
accepts. The gate suites *did* run, green, on the PR's own tree without PGO
(see the PR body); what did not and could not run is "the gate suite against
the PGO artifact specifically" — that combination is **NEVER RAN**, and it
would need a `-Cprofile-use` `RUSTFLAGS` around the whole `cargo test`
invocation to exist at all, which is future work if anyone wants it.

---

## 4. The browser engines — acceptance 3's rows (M7 P08, 2026-09-14)

**This is where the milestone's number is read.** Natively the interpreter is
the default and always will be (JD24: cranelift needs ~150 s per core on
these images), and nothing could put a classic image on a phone until this
phase. The rows below are the first the classic has ever had in a browser
engine.

### The protocol these rows were taken under

`docs/emulator-perf-ledger.md` §1, plus §1 above, plus the three things
acceptance 3 adds:

- **One invocation per table, interleaved, best of five.** The two legs of a
  ratio come from the same invocation on the same machine or they are not a
  ratio of anything. `xt-bench-cli.mjs --rows … --best-of 5` is one
  invocation that runs the whole row sequence five times and reports the
  fastest reading of each row.
- **Both engines. `bun` is JavaScriptCore — the phone's family — and
  `/opt/homebrew/bin/node` is V8** (JD19). Neither is the answer on its own;
  this table shows why.
- **The UART0 sha256 of the translated leg equal to the interpreter's**, on
  every row, or it is not a row.

`render-loop` at t1, both cores at `--core-quantum 256`, a **5,500 ms
emulated bound** (`GATE_US`, the window every C6 browser row is taken at),
`--dump-frames` and `--uart0` into the shim's memfs and sha256'd in place,
`caffeinate -dims`.

⚠️ **The load column is read after each row and it includes the engine's own
background compilation threads.** On a 90 MB module those are not a rounding
error: in the JSC table below the `fn=64` row drove the desk's one-minute
load from 7.5 to the high thirties by itself, and it stayed there for the
rest of the invocation. The number that answers "was the desk quiet enough"
is therefore the **ambient load at the start of the invocation**, printed in
the CLI's own header; the per-row figure is a diagnostic about the engine.
Both tables below started below the ~8 bar. **Yona's desk was in use
throughout** (Lightroom Classic and Illustrator, each at ~85 % of a core),
which is a deviation from "refuse a row above ~8" read strictly and is
recorded here rather than hidden: a retake on a quiet desk would move the
absolute seconds and should not move the ratios, which is what the
interleaving is for.

### V8 — `node` 25.2.1 (v8 14.1.146.11-node.14)

Best of five, one invocation, interleaved, ambient load 6.20 at the start.

| row | wall s | ns/instr | real time | ÷ interpreter | load after |
|---|---:|---:|---:|---:|---:|
| `--jit --jit-fn-blocks 8` | 13.49 | 25.22 | **0.4077×** | 0.991× | 9.1 |
| `--jit --jit-fn-blocks 16` | 12.89 | 24.10 | **0.4266×** | 1.037× | 13.1 |
| `--jit --jit-fn-blocks 64` | 12.32 | 23.02 | **0.4466×** | **1.086×** | 12.5 |
| `--interpreter` | 13.37 | 25.00 | 0.4114× | — | 10.4 |

### JavaScriptCore — `bun` 1.1.18

Best of five, one invocation, interleaved, ambient load 7.27 at the start.

| row | wall s | ns/instr | real time | ÷ interpreter | load after |
|---|---:|---:|---:|---:|---:|
| `--jit --jit-fn-blocks 8` | 14.99 | 28.03 | **0.3669×** | 1.055× | 37.9 |
| `--jit --jit-fn-blocks 16` | 14.61 | 27.31 | **0.3765×** | **1.082×** | 7.5 |
| `--jit --jit-fn-blocks 64` | 19.60 | 36.65 | **0.2805×** | 0.807× | 39.3 |
| `--interpreter` | 15.81 | 29.56 | 0.3479× | — | 43.9 |

### Identity, across all forty rows of both tables

One UART0 sha256, one frame-dump sha256, one `(instructions, cycles, us)`
triple — every pass, both engines, both modes, all three sizes:

```
uart   e8a4604f21633182270427ff36899b5795d5973b8873f2b8d988da99fe00df42
frames 596c186eaad230cacb9c75a72cee2adf527af8448bdf694dd4ac2b8dac0db7b6
run: cycles=1320000000 instructions=534889786 (core0=439236975 core1=95652811)
```

The translated run reports the same counters in both engines too — coverage
**78.50 %** of retired instructions ran inside translated code and retired
there natively, **8,817,264** entries, **mean stay 47.8** instructions
against the ≈155-instruction runway, escape rate **0.278 %**. They are a
pure function of the instruction stream, which is the invariant, and their
being engine-independent is the cheapest possible demonstration of it.

### Against the bar

Roadmap item 4 reads **≥1.5× real time to be "delivered" and ≥3× to hit the
target**. The classic's best browser reading is **0.4466×** (V8, 64 blocks a
function) and **0.3765×** (JSC, 16). It is **a third of the floor**, and the
translated core barely beats its own interpreter: **1.086× in V8, 1.082× in
JSC**, against the C6's 1.71× on `render-basic` in the same rig on the same
day.

For scale, the same desk, the same day, the C6's rig rows (`render-basic`
t2, 8 blocks a function) — quoted here as the rig's own oracle and not as a
classic number (P1b's rule):

| engine | `--jit` | `--interpreter` | ratio |
|---|---:|---:|---:|
| V8 | 0.931× real time | 0.546× | 1.71× |
| JSC | 0.788× | 0.606× | 1.30× |

### Where the 12.32 s goes (the bucket profile)

`node --cpu-prof` over one `render-loop` t1 `--jit` row, bucketed by
`scripts/emu/p6c-prof.mjs --chip esp32v3` (P08 gave that script a chip arm
rather than a twin). 15.6 s attributed, 10,420 samples:

| | ms | % run |
|---|---:|---:|
| the emulator's own wasm | 11,169.0 | 71.49 |
| the translated modules | 3,368.9 | 21.56 |
| the JavaScript rig (host, WASI shim, node) | 709.3 | 4.54 |
| the garbage collector | 345.8 | 2.21 |

and inside the emulator's own wasm:

| bucket | ms | % run |
|---|---:|---:|
| the machine's own loop (`Machine::run_until`, the two-core interleave) | 3,280.6 | 21.00 |
| **translation** (emit + the engine's compile, by stack ancestry) | 2,648.3 | 16.95 |
| the hart's slice loop (`XtHart::step`) | 1,766.9 | 11.31 |
| **the entry protocol** (`XtJitCore::run`) | 1,345.5 | 8.61 |
| the interpreter (the 21.5 % of instructions no module covers) | 683.6 | 4.38 |
| MMIO dispatch (bus routing) | 376.8 | 2.41 |
| allocator + runtime | 220.8 | 1.41 |
| the pin fabric, the LED strip, GPIO, RMT, everything else | ~440 | ~2.8 |

**Three readings, in the order they matter.**

1. **The translated code is 21.56 % of the run** and it retires 78.50 % of
   the instructions. The emitter is not the problem.
2. **The entry protocol costs 1,345.5 ms over 8,817,264 entries = 153 ns an
   entry**, against the C6's 25 ns in V8. The difference is XD8's exchange
   area: the classic marshals the physical `AR[0..64]` file plus
   `WindowBase`, `WindowStart`, `SAR`, the three loop registers and
   `PS.CALLINC` at **every** entry and exit, where RV32 marshals 32 words and
   nothing else. At a mean stay of 47.8 instructions that is **3.2 ns of
   pure entry overhead per retired instruction**, on a run that costs 23.0
   ns an instruction in total.
3. **Translation is 16.95 % of a 5,500 ms window** — and half of that is
   paid twice, once per core, for the same block set (see the boot cost
   below).

### What ends a stay (`render-loop` core 0, 7,417,502 entries)

| why | exits | share |
|---|---:|---:|
| `budget` — the 256-cycle slice bound | 3,088,412 | 41.6 % |
| `indirect-miss` | 2,514,039 | 33.9 % |
| `undecodable` | 969,248 | 13.1 % |
| `edge-out` | 466,737 | 6.3 % |
| `window` | 372,053 | 5.0 % |
| `escape-target`, `after-store`, `slice-ended` | 7,013 | 0.1 % |

The wasm run reproduces P07's native census (42.8 / 34.5 / 11.2 / 6.4 /
5.0 %) to within a point on every bucket, which is one more reading of the
invariant. **The slice bound and indirect misses end stays; the window does
not.** Halving the entry cost and doubling the runway are the same lever
seen from two ends.

### The boot cost, per event, in a browser engine (JD20)

Nobody had this number before: what a **90 MB** module costs a browser
engine to compile and instantiate.

Read off one `--jit-fn-blocks 64` row in each engine on the same module
(`emu.wasm` sha256 `849d8aae…`, build `0c00f77`). The JSC row is from that
engine's own table above; the V8 row is a separate single-row invocation, so
these are **costs, not a race** — the seconds in the tables above are what
the engines are compared on.

| per translation event | V8 | JSC |
|---|---:|---:|
| the whole-image module | 90,680,592 B | 90,680,592 B |
| `new WebAssembly.Module`, core 0 / core 1 | 49.9 / 50.0 ms | 209.0 / 202.3 ms |
| `new WebAssembly.Instance`, core 0 / core 1 | 0.2 / 0.2 ms | 3.5 / 3.2 ms |
| discover, in the emulator's own wasm, core 0 / core 1 | 276.7 / 156.6 ms | 160.5 / 162.6 ms |
| emit, in the emulator's own wasm, core 0 / core 1 | 1,285.9 / 676.1 ms | 818.6 / 799.2 ms |
| the ten publish-by-store events together | ~198 ms | ~570 ms |
| **all twelve events, both cores, total** | **2,694 ms** | **2,863 ms** |

The emulator's own module, for scale: **2,846,587 B**, compiled in 2.0 ms
(V8) / 8.4 ms (JSC) and instantiated in 0.3 / 10.9 ms.

The arena the modules and their tables live in reports itself on every boot
line: **264,437,760 B of gap**, of which the whole-image module's indirect
target tables take **11.3 MB over 139 pages** per core, leaving ~252 MB. The
two whole-image modules are 90.7 MB of arena each while they are being
handed to the engine, and the engine keeps its own compiled copy of each.
Nothing refused for memory in either engine.

⚠️ **Neither engine is really compiling 90 MB in 43–209 ms.** Both defer:
the synchronous `new WebAssembly.Module` returns after a header pass and the
bodies are compiled on first call, which is also what the load column above
is watching. So "compile" here is the engine's *acceptance* of the module,
not the cost of its code, and the rest of that cost is spread across the run
in whatever tier the engine chose. The C6's phone rows saw the same thing
(a 16 ms "compile" of 9.4 MB in JSC).

Against the C6's 0.4–0.7 s boot budget the classic's is **2.7–2.9 s**, and
the dominant term is **emit**, not the engine.

### DD117 — one compile, two instantiations: measured, and NOT done

P07 deferred it here, scoped as "the module bakes `exchange_offset` in, so
each hart gets its own compile". Measured in the browser, the duplicate
*compile* — the APP core's second `new WebAssembly.Module` over the same
90,680,592 bytes — is **50.0 ms in V8 and 202.3 ms in JSC**, 0.3 % and 1.0 %
of the run. On that number alone the emitter change is not worth making, and
this phase did not make it.

The number that *would* justify it is the duplicate **emit**: **676.1 ms
(V8) / 799.2 ms (JSC)** spent a second time producing the same 90 MB for the
APP core, 4–5 % of the run. That is the same change — the module has to take
its exchange base as an import or a global instead of a constant — but it is
an emitter change (`lp-xt-jit/src/translate/mod.rs`) and a `jit-host.js`
change, and the director scoped this phase to the compile half. **Recorded
as a named, priced follow-on** rather than done here.

### The `--jit-fn-blocks` default: measured, and NOT moved

The desk was asked to settle it and gave two different answers:

| engine | 8 | 16 | 64 |
|---|---:|---:|---:|
| V8, ÷ interpreter | 0.991× | 1.037× | **1.086×** |
| JSC, ÷ interpreter | 1.055× | **1.082×** | 0.807× |

V8 prefers the largest bodies offered; JSC prefers 16 and **loses 20 % at
64** — the same split the RV32 ladder found (JSC prefers 8, V8 16), one
notch larger on both sides because the classic's blocks are shorter.
`JIT_FN_BLOCKS_DEFAULT` therefore **stays at 64**: the director's rule is
"do not move a default on one engine", the two engines disagree, and the
phone (JSC's family) is G-M7P-XT's question. The rig's phone preset asks all
three sizes so that press can answer it.

### The defect this phase's first row found

**The first translated `render-loop` row a browser engine ever took read
0.0517× real time — 0.134× the interpreter.** The profile said 86.25 % of
the run was self time inside `XtJitCore::run`, and what is inlined there and
O(the image) was `verify`: on every pending invalidation it re-read the
guest bytes of *every block of every module*, and the read-only module holds
138,564 of `render-loop`'s 140,101 blocks.

It ran **72,458 times in 5,500 ms of emulated time** and found a changed
byte **zero** times. It could not: the path in is
`TranslatedCore::invalidate`, which is the polling-point-(c) store drain and
a `wsr` to a loop register — and `restore_context` rewrites
`LBEG`/`LEND`/`LCOUNT` on every context switch, which is exactly why DD104
took the loop registers off the *block cache's* flush path. Neither event can
change a byte in a region the bus calls read-only.

Since P08 `verify` takes a scope: an invalidation re-reads the **writable**
modules, and a translation event re-reads **all** of them at a slice
boundary, where `read_only_module_is_stale` is the net it exists for — cast
now at every event rather than only at an event a pending invalidation
happened to precede, so the check is strictly stronger than it was.

| V8, same invocation, same image, same bound | wall s | real time | ÷ interpreter |
|---|---:|---:|---:|
| before | 106.47 | 0.0517× | 0.134× |
| after | 14.99 | 0.3670× | 1.109× |

**7.1×**, with the UART0 sha256 the same on all four rows.

---

## 5. The menu

Ranked by what is left after §3's landed rungs, not by distance to 3×.

| lever | payoff | status |
|---|---|---|
| **the entry protocol's width** (XD8's exchange area) | **153 ns an entry in V8 against the C6's 25 ns**, over 8.8 M entries = 8.61 % of a browser run, because the classic marshals the physical `AR[0..64]` file plus seven special registers at every entry and exit where RV32 marshals 32 words (§4) | **the biggest single lever the browser rows found**, and not this plan's to pull — it is an emitter + seam change. Priced, not scheduled |
| **the runway** (mean stay 47.8 instructions, 41.6 % of exits are the 256-cycle `budget` bound) | the other end of the same lever: the entry cost is amortised over the stay, and the interleave decides the stay | **Q7's machine-side item**, deliberately outside this plan (D3 is not moved here) |
| **one emit, two instantiations** (DD117, in full) | **4–5 % of a browser run** — the whole-image module is emitted twice, once per hart, because `exchange_offset` is baked in as a constant (§4). The *compile* half DD117 names is only 0.3 % (V8) / 1.0 % (JSC) | **measured in P08 and deliberately not done**: the compile half does not justify it and the emit half is an emitter change this phase was not scoped for |
| **the translator** (this plan's own M7 P04–P10) | the only lever this chip's own numbers say is not optional — §2's 1.7× interpreter ceiling (0.130× → ≈0.22×) is well short of the 1.5× floor even taken perfectly, and §3 shows the landed rungs (≈2.0×, projecting to ≈2.9× with PGO) already exceed that ceiling, which is itself evidence the interpreter's sampled shares understated the real prize (DD105) | **the plan's own next mountain** — P04 onward, `xtensa-emulator-plan.md`'s M7 |
| **the poll-loop skip** | at most 1.7 % (MMIO's whole share of host time here, `m7/notes.md` §2.1) — **closed by the numbers**, not by measurement: the C6's reasoning (86 % of MMIO, a large host-time share) does not transfer, because here MMIO is 1.7 % of host time regardless of what fraction of it is one register | **registered, not pursued** — do not port the C6's ADR to this chip on the C6's reasoning |
| **the block cache's Step B** (a bigger table, tuned collision handling) | P01's own PR body left `collisions=101,705` (0.1 % of lookups, hit rate still 99.85 %) "as measured rather than tuned" | **closed by the RV32 result** — the C6's own Step B (the same idea, a generation earlier) was built and measured at ~1.0× native / 1.12–1.14× phone and never merged (`docs/emulator-perf-ledger.md` §3); the classic's collision rate is already smaller in relative terms than what motivated that attempt, so it is not reopened here |
| **PGO** (§3 rung 3) | **1.457×**, landed as an opt-in recipe (`scripts/emu/pgo-esp32v3.sh`, `just bench-emu-esp32v3-pgo`) | **shipped**, opt-in — never a default build or CI step, same posture as the C6's |
| **a two-binary probe for the classic** (`lp-cli` instantiation) | would let this chip's numbers be taken the way the C6's phone/desk rig takes them | **future work, out of this phase's scope** — no `lp-cli` instantiation of the classic exists today |

---

## 6. Pointers

**Planning.** `~/.photomancer/planning/lp2025/2026-09-13-1008-xtensa-speed-ladder/`
(this plan: `plan.md`, `p01-*.md`, `p02-*.md`, `p03-*.md` — this phase,
`G-M7D-XT-gate.md`). `~/.photomancer/planning/lp2025/2026-09-10-0021-xtensa-emulator/`
(the parent Xtensa emulator plan; `m7/notes.md` is §2's source).

**PRs.** #758 (P01, the block cache), #766 (P02, the window hoist), #767
(P03, PGO + the ladder table + this document), #768 / #770 / #771 / #772
(P04–P07, the translator), #773 (P08, the wasm build and §4's browser rows).

**The C6 ledger.** [`docs/emulator-perf-ledger.md`](emulator-perf-ledger.md)
— the shape this document follows, and never a source of numbers for this
chip (P1b's rule).

**READMEs.** `lp-emu/README.md` §Speed (the classic paragraph, the PGO
recipe beside the C6's), `lp-emu-esp32v3/README.md`, `lp-xt-emu/README.md`
("Machine mode").

**The browser rig.** `scripts/emu/build-xt-wasm.sh` (`just bench-emu-web
--chip esp32v3`) builds the `wasm32-wasip1` module and stages it beside the
three pinned images; `scripts/emu/xt-bench-web/{xt-bench-cli.mjs,
xt-bench-run.js, xt-worker.js}` are the rows. It is a **twin** of the C6's
`scripts/emu/bench-web{.sh,/}`, never an edit of it — that directory is the
perf-lab lane's — and it stages the C6 rig's `wasi-shim.js` and the
translator crate's `jit-host.js` as byte-for-byte copies.
`scripts/emu/p6c-prof.mjs --chip esp32v3` buckets a V8 profile of a
translated run.

**The rigs.** `scripts/emu/bench-esp32v3.sh` (`just bench-emu-esp32v3`),
`scripts/emu/pgo-esp32v3.sh` (`just bench-emu-esp32v3-pgo`),
`scripts/emu/v3-oracle.sh` (the free oracle, flag and pair mode),
`scripts/emu/build-reference-image.sh --chip esp32` (the three pinned
images), `scripts/emu/selfprof-buckets.py` / `scripts/emu/bench-esp32v3-counts.py`
(where the seconds go — both slower than the probe by construction, so only
their *shares* mean anything).
