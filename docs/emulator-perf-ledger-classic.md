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

## 4. The menu

Ranked by what is left after §3's landed rungs, not by distance to 3×.

| lever | payoff | status |
|---|---|---|
| **the translator** (this plan's own M7 P04–P10) | the only lever this chip's own numbers say is not optional — §2's 1.7× interpreter ceiling (0.130× → ≈0.22×) is well short of the 1.5× floor even taken perfectly, and §3 shows the landed rungs (≈2.0×, projecting to ≈2.9× with PGO) already exceed that ceiling, which is itself evidence the interpreter's sampled shares understated the real prize (DD105) | **the plan's own next mountain** — P04 onward, `xtensa-emulator-plan.md`'s M7 |
| **the poll-loop skip** | at most 1.7 % (MMIO's whole share of host time here, `m7/notes.md` §2.1) — **closed by the numbers**, not by measurement: the C6's reasoning (86 % of MMIO, a large host-time share) does not transfer, because here MMIO is 1.7 % of host time regardless of what fraction of it is one register | **registered, not pursued** — do not port the C6's ADR to this chip on the C6's reasoning |
| **the block cache's Step B** (a bigger table, tuned collision handling) | P01's own PR body left `collisions=101,705` (0.1 % of lookups, hit rate still 99.85 %) "as measured rather than tuned" | **closed by the RV32 result** — the C6's own Step B (the same idea, a generation earlier) was built and measured at ~1.0× native / 1.12–1.14× phone and never merged (`docs/emulator-perf-ledger.md` §3); the classic's collision rate is already smaller in relative terms than what motivated that attempt, so it is not reopened here |
| **PGO** (§3 rung 3) | **1.457×**, landed as an opt-in recipe (`scripts/emu/pgo-esp32v3.sh`, `just bench-emu-esp32v3-pgo`) | **shipped**, opt-in — never a default build or CI step, same posture as the C6's |
| **a two-binary probe for the classic** (`lp-cli` instantiation) | would let this chip's numbers be taken the way the C6's phone/desk rig takes them | **future work, out of this phase's scope** — no `lp-cli` instantiation of the classic exists today |

---

## 5. Pointers

**Planning.** `~/.photomancer/planning/lp2025/2026-09-13-1008-xtensa-speed-ladder/`
(this plan: `plan.md`, `p01-*.md`, `p02-*.md`, `p03-*.md` — this phase,
`G-M7D-XT-gate.md`). `~/.photomancer/planning/lp2025/2026-09-10-0021-xtensa-emulator/`
(the parent Xtensa emulator plan; `m7/notes.md` is §2's source).

**PRs.** #758 (P01, the block cache), #766 (P02, the window hoist), this
phase's own PR (P03, PGO + the ladder table + this document).

**The C6 ledger.** [`docs/emulator-perf-ledger.md`](emulator-perf-ledger.md)
— the shape this document follows, and never a source of numbers for this
chip (P1b's rule).

**READMEs.** `lp-emu/README.md` §Speed (the classic paragraph, the PGO
recipe beside the C6's), `lp-emu-esp32v3/README.md`, `lp-xt-emu/README.md`
("Machine mode").

**The rigs.** `scripts/emu/bench-esp32v3.sh` (`just bench-emu-esp32v3`),
`scripts/emu/pgo-esp32v3.sh` (`just bench-emu-esp32v3-pgo`),
`scripts/emu/v3-oracle.sh` (the free oracle, flag and pair mode),
`scripts/emu/build-reference-image.sh --chip esp32` (the three pinned
images), `scripts/emu/selfprof-buckets.py` / `scripts/emu/bench-esp32v3-counts.py`
(where the seconds go — both slower than the probe by construction, so only
their *shares* mean anything).
