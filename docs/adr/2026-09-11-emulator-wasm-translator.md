# ADR: The ESP32-C6 emulator translates the whole guest image to WebAssembly

- **Status:** Accepted
- **Date:** 2026-09-11
- **Deciders:** Photomancer (Yona); milestone gates G-M7D (design),
  G-M7P (the phone number), G-M7B and G-M7B′ (the close)
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-09-06-esp-soc-emulator-architecture.md` (the machine this
  runs inside, and PD5 — guest time is the scheduler's);
  `2026-09-06-lp-emu-home-and-mit-fence.md` (the MIT fence this crate lives
  behind, and open question E1, closed below);
  `2026-09-08-emulator-poll-loop-skip.md` (**Rejected** — the rung before this
  one, and the shape of argument this ADR inherits);
  `2026-09-10-an-emulator-captured-trace-is-evidence-not-a-fixture.md`

Plan: `lp2025/2026-09-07-0827-emu-speed-ladder`, milestone **M7** (decisions
JD1–JD26) and its continuation **M7b**. The block cache — the rung below this
one, M5 — never got an ADR of its own; it is documented in
`lp-emu/README.md` §Speed and in `lp-emu-core`'s own source, and this
document references it rather than restating it.

Detail lives with the code and is **cited, not repeated**:
`lp-emu/lp-emu-jit/README.md` is the translator's own page (discovery rules,
exit protocol, two-level dispatch, the cost model, the censuses, the residual);
`lp-emu/esp/lp-emu-esp32c6/README.md` is the flag table;
`lp-emu/README.md` §Translation is the one-page version. This ADR states the
decisions and the numbers that forced them.

---

## Context

### What the ladder was, and where it had got to

The ESP32-C6 emulator has to stand in for a board someone is watching in
Studio, which makes its throughput a product concern rather than a curiosity.
The speed ladder (D1, 2026-09-07) set the target at **1× real time at grade
`t2` in wasm on an iPhone 16 Pro Max**, with a native M2 Max proxy.

Five milestones of interpreter work came first, and the honest summary of them
is that **the interpreter levers ran out**. Release-profile overrides were a
genuine 2.3× (the workspace builds at `opt-level = "z"` for firmware flash,
which is exactly what an interpreter loop cannot afford). After that, the four
remaining independent levers each measured somewhere between **1.00× and
1.14×** in the engine that matters, and one of them — the poll-loop skip —
was measured, proved byte-exact on 81/81 artefacts, and then **rejected**
because its bookkeeping cost more everywhere than its win was worth on the
product's actual workload.

Against that, the region-JIT spike measured **~12.5× on the code it covered**,
in the phone's own engine family, byte-identically. A lever an order of
magnitude larger than the whole remaining lever set is the reason M7 opened at
all.

### What "Amdahl-bound" meant here, and why it decided the design

The spike's 12.5× was on *covered* code. End-to-end gain is therefore bounded
by coverage, and that single observation settled the plan's largest question
before it was asked: **translate the whole image eagerly, rather than chasing
hot regions.** The spike's own region selection was measured and is the number
that killed the hotness tier — **thin regions, at 77 instructions per entry,
ran measurably *slower* than interpreting them**, because a stay that short
pays the entry protocol more than it saves on the instructions inside it.
There is no threshold that fixes that; there is only making the stays long,
which means covering everything.

---

## Decision

### 1. Two translation events, and no hotness anywhere (JD5)

Translation happens at exactly two kinds of moment: **when the image is
loaded**, and **at each guest `fence.i`**. No census, no threshold, no warm-up
tier, no promotion, no hotness counter on any product path.

This is not minimalism for its own sake. A hotness tier is a *policy* that has
to be right, and the measurement above says the policy it would implement is
wrong: entering translated code for 77 instructions is worse than not. The
only regions worth entering are long ones, and the way to make stays long is
to have somewhere to go — the whole image — rather than to pick winners.

`fence.i` is the second event because the guest writes its own code: the
shader JIT publishes into executable RAM and then fences. M7b P1 made that
event **incremental** (DD18): a `fence.i` that only adds newly writable code
adds a module rather than re-emitting the image, which took the boot cost from
three whole-image emits to one plus two increments (1,806 ms → 869 ms of
emit).

`BootMode::RomUp` keeps translation off entirely (**DD19**, asserted as rule 4
of `tests/jit_default.rs`): the mask ROM and the second-stage bootloader
publish code without ever emitting a `fence.i`, so there is nothing to hang
the invalidation rule on.

**That has a consequence worth stating where nobody can miss it: a ROM-up
board does not get any of this.** Every emulated board in Studio-in-a-tab is
ROM-up today — it is the closer twin of flashing and resetting a real board —
so the default flip of §2 changes nothing for them; they still interpret.
ROM-up translation is its own piece of work, it is deliberately **last**
(DD19), and it belongs to the emulator-loop milestone rather than to M7.

### 2. The translated core is the wasm build's core; the interpreter is the oracle and a library (R9, JD9)

In a `wasm32-wasip1` build of `lp-emu-esp32c6` — the browser rig, the phone,
Studio-in-a-tab — **there is no interpreter on the execution path.**
`TRANSLATED_BY_DEFAULT` is `cfg!(target_family = "wasm")`; `lp-emu-jit` is an
unconditional dependency of that target and brings no compiler with it.

The interpreter did not go away. It took three jobs, and each one is why the
flip is safe:

- **The free differential oracle.** `--interpreter` on the same binary, the
  same image, the same flags must print a byte-identical everything.
- **The escape hatch, as a library call.** Translated code that meets an
  encoding the translator does not emit flushes its counters, calls
  `step_one(pc)` for that one instruction, and carries on. **It is a function
  call, not a tier**: nothing decides between two engines at runtime, no
  coordination exists to get wrong, and bring-up therefore has no completeness
  cliff — a module that escaped on 90 % of its instructions would be slow and
  still exactly right. `translate_roundtrip.rs` asserts that as a test, by
  running the same block set twice, once emitted and once with `Emit::NOTHING`,
  and requiring the two to agree on everything.
- **The way back.** A default is reversible; that is the point of flipping one.

The static escape rate on the shipped images is **0.0 %**.

### 3. Natively the interpreter stays the default, and the reason is an inversion (JD9, JD24)

Cranelift compiles the emitted whole-image module in **113–192 s** per
translation event on an M2 Max (measured again in this phase: 108.5 s of a
115.9 s `render-basic` cell, 29.9 s of a 32.9 s `harness` cell). The native
interpreter already reads 1.03–1.29× real time. Every test and every CI job
depends on the native binary starting fast.

So the thing that makes translation a win in the browser — compile once, run
the whole image — is the thing that makes it unusable natively with an
optimizing compiler in front of it. `wasmtime` stays an **optional** feature
(`--features jit` plus `--jit`), used for identity proofs, for the CI gate,
and for the recordings the engine harnesses replay.

**P8 — the phase that would have fixed this — is deferred (JD24), so there
are no Winch numbers and no cached-module numbers. Neither has been measured
anywhere.** The two candidates, in the order JD9 ranked them: serializing and
caching compiled modules keyed by image hash (which dodges compile time
entirely on the second run of a pinned image), and Winch, wasmtime's baseline
compiler (which trades steady-state speed for compile time, in an unmeasured
ratio). JD21 adds a third, independent of both: the native host currently pays
**~4.8×** on the guest-memory path because it cannot use guard-page elision,
and the way out is for the bus to allocate its arena with `mmap` and a real
unmapped guard region rather than as a `Vec`. Those belong to the follow-up
plan, with numbers, not to this ADR as speculation.

### 4. The wasm function-size limit is 7,654,321 bytes, and that is why dispatch has two levels (JD8)

**This is the single structural fact a future reader most needs**, because
nothing about it is discoverable from the spec's prose and every symptom it
produces points somewhere else.

A WebAssembly function body may not exceed **7,654,321 bytes**. The emitted
whole image is ~76–80 MB of wasm. One function per block set is therefore not
merely slow, it is *malformed* — and the failure arrives as an engine-specific
validation error that names neither the limit nor the function.

So a module is **two levels**: an outer **selector** over as many
**sub-dispatcher** functions as the limit needs, each holding a fixed number
of guest blocks. An edge inside one sub-dispatcher is a `br` to a label; an
edge that leaves it is a *cross* through the selector, which flushes and
reloads the live register set through the exchange area. Block-set order
therefore decides which guest edges are free and which cost a cross.

Two consequences, both measured:

- **The blocks-per-function knob is a real performance axis, and the two
  engines disagree about it.** V8 is within 4 % across 8/16/32 blocks a
  function. JavaScriptCore is **1.81× faster at 8 than at 32**. The default is
  **8**, chosen by the phone (DD32/BD6): it won five of seven phone sessions
  and holds the best row ever taken on that device. Switching the default from
  32 to 8 was **+93 % in bun/JSC** and −0.8 % (noise) in node/V8.
- **Block-set order is left as guest address order, and that is a
  measurement.** A depth-first trace layout was implemented, measured and
  removed: **2–5 % worse in both engines, and 6.2 % more cross-function
  edges**, because the firmware's address order already *is* the trace — a
  compiler laid the hot successor next to its predecessor, and a walk that
  also chases `jal` targets breaks those runs up.

Indirect jumps resolve in O(1) across functions through flat target tables
emitted beside the code.

### 5. The bus owns one contiguous guest arena, and the base is folded at emission time (JD4)

The bus allocates **one contiguous arena**; every guest region is a view into
it, and it must not move for the life of the machine. Each emitted access
folds the arena base into its `memarg` **as a constant at emission time**, so
there is no per-access indirection, no wasm global to load, and no address
arithmetic the engine has to prove anything about. A **16 KiB-page permission
byte table** decides RAM-path versus MMIO-import for each access; that is the
shape 0.849 ns per instruction was measured with.

Folding the base is what makes this safe to state as a rule rather than a
convention: **the base is not a runtime value**, so a module is only ever
valid against the arena it was emitted for, and a module that outlives its
arena cannot silently address the wrong thing — it cannot address anything.

The cost is a ~257 MiB reservation, which the phone run is the evidence for.

### 6. The browser seam: what was chosen, what was rejected, and why a synchronous compile is legal (JD11–JD13, JD25)

**Imports, not shims.** A translated module imports the emulator instance's
own `memory` and its own `mmio_load`, `mmio_store`, `step_one` and `poll`
exports. Every call a translated module makes is **wasm→wasm with no JS frame
on it**. The measurement: 1.60 ns for a wasm→wasm MMIO call against 6.05 ns
through a JS shim, and 5.40 ns for a table entry against 9.00 ns for a host
import.

**Entry is by shared function table, by index** — `table.grow(1)` then
`table.set(idx, run)`, never the fused `grow(delta, ref)`, which JSC accepts
and then hands back a slot `call_indirect` traps on as null. The rejected
alternative was **one host import per entry** (`jit_enter(id, entry_pc)`),
whose cost would have been bounded by slice boundaries at ~19.5 k/emulated
second; it was rejected because the table works in every target engine and is
the same shape natively through a wasmtime `Table`, so the host-import
variant would have been a second mechanism to keep correct for no gain.

Exactly **two** JS imports exist, in the namespace `emu_host`: `jit_compile`
and `jit_release`, called **once per translation event** and never on a hot
path. The JS half is one importable ES module,
`lp-emu/lp-emu-jit/js/jit-host.js`, which Studio's own worker imports
unchanged (JD25); the bench rig and the in-tab lane stage copies of it.

**Compilation is synchronous, in host time, and that is legal — here is the
argument rather than the conclusion.** `new WebAssembly.Module` is called
synchronously on a 64 MB module. The reason this cannot corrupt anything is
PD5: **guest time is the scheduler's.** The emulated machine's clock is
advanced by the instruction stream and by scheduled events, never by the wall
clock; no peripheral, no interrupt and no transcript can read how long the
host took. A host-time stall is therefore invisible to the machine *by
construction*, not by being short. The same argument is what lets an emulator
be byte-identical across machines of different speeds, and it is the property
every oracle in this ladder rests on.

Two things follow, and they are requirements rather than conveniences:

- **The emulator must stay in a dedicated Worker.** A synchronous multi-MB
  compile on the main thread would freeze the *user interface*, which is not
  guest state but is still unacceptable. Off the main thread the synchronous
  constructor has no size restriction. This is now a correctness requirement
  of the design, not a deployment preference.
- **No JSPI, no async run loop, no promise on any path the guest can reach.**
  An async boundary inside the run loop would be a place where host time
  *could* reach guest state, and the whole argument above would have to be
  re-made for every await.

### 7. Identity is the bar, stated once (JD15, JD16, JD17)

**A translated run and an interpreted run of the same image at the same grade
must produce identical bytes.** Specifically: the UART0 capture, the
`stopped after N cycles (U us, I instructions)` line, the WS281x frames
decoded off the emulated pad, stdout, stderr with the interpreter's own
block-cache counters masked (those *must* differ, and the raw diff is printed
rather than tolerated), and the MMIO+interrupt `--trace`.

Cycle and instruction counts live in wasm locals and are flushed to the
exchange area at **every point the bus can observe them** — before any MMIO
import, before `step_one`, and at every exit — because peripherals read
`cx.now` mid-block, so charging a block's cycles at entry is *not* exact
(JD17, measured in P1b).

The evidence, as it stands at M7's close:

- Every M7 and M7b phase: **8/8** free-oracle cells byte-identical at the
  20 ms `--trace` window — PRs #673, #678, #680, #689, #691, #697, #700, #703,
  #706, #713, #717, #719, #722, #727.
- The browser identity row is `2407828f80684331` (UART0 sha256), on twenty-plus
  phone rows, twenty-four browser rows, and every desk row in both engines.
- The three-engine differential: the same module bytes and the same recorded
  import answers agree under **wasmtime, V8 and JavaScriptCore** — 34/34
  cases at P3, 16/16 at P6.
- Installed coverage in the browser matches native exactly, engine for engine.

Yona's *"not that worried about the timing being perfect"* is recorded as a
signal and deliberately **not** acted on. Exact cycle accounting is what makes
every oracle above work, and the phone measured 0.849 ns/instruction with the
accounting already in place, so relaxing it buys nothing measured.

### 8. What each CI tier covers (JD14, ruled by JD23)

**Tier (b) — the gate, in the path-gated `emu-c6` job.**
`scripts/emu/jit-identity-image.sh`, wrapped by `just test-emu-jit-image`: one
binary, one pinned image, `--jit` against `--interpreter`, the five readings
above compared byte for byte.

**It is bounded to one image, one grade and a 20 ms `--trace` window, and the
bound is a decision.** The cost is almost entirely cranelift — 29.9 s of a
32.9 s cell on an M2 Max — so a shorter window saves nothing and a second cell
costs another whole compile. The `harness` image is the cell because its ELF is
1.1 MB against the render pair's 9.2 MB (47 k blocks rather than 184 k) while
still covering both translation events, the incremental `fence.i`
retranslation, 95.31 % coverage and a 201,545-line trace. **It does not cover
the render loop, the RMT, or the frame path**; those are the local sweep's job
(`scripts/emu/oracle-sweep.sh` across two binaries, all four pinned images,
both grades) and they do not fit the job's budget.

The gate cannot skip silently: a binary built without `--features jit` refuses
`--jit` outright, and the script turns that into a named failure — a
self-comparison would pass and prove nothing.

**Tier (a) — the tool, not a gate (JD23).** `just test-emu-jit-identity`: the
crate's own suites under `host-wasmtime`, plus the three-engine differential in
which the same module bytes are replayed in V8 and JSC against the answers
wasmtime already asserted. Per case that is the exit pc, both counters, the
after-store flag, all 31 architectural registers, the import call order, and
**the memory granules an escaped instruction's interpreter wrote between
entries**. The granule diff is not an optimisation: without it a replay runs
translated code against memory the real run never had, and the spike's first
replay diverged at entry 21 for exactly that reason and not because anything
was wrong.

**And the default path now says what it is not running.** Both engine suites
are `#![cfg(feature = "host-wasmtime")]` at file scope, so
`cargo test --workspace` at default features — which is what CI's
`validate-x64` runs — compiled them to zero tests and exited 0.
`tests/engine_suites_present.rs` is never compiled out, names them, says what
stops being checked, and guards its own list against drift. The feature stays
off by default (nothing else should pay cranelift), but a green
`cargo test -p lp-emu-jit` can no longer be read as an engine agreeing.

The `jit` feature also gets a **lint** seat: `just clippy-emu-jit` runs
`cargo clippy -p lp-emu-esp32c6 --features jit --all-targets -D warnings`, in
`clippy` and therefore in `just check` and CI's `check-lint`. `clippy-host`'s
`--workspace` compiles every member at its *default* features, and `jit` is
not one — so that whole half of the crate was invisible to the lint gate and
sat red for about a month.

### 9. Licence posture, and the closure of open question E1

`lp-emu-jit` declares `license = "MIT"`, as a unit with everything under
`lp-emu/`, while the rest of the repository is AGPL-3.0-or-later.
`just lint-emu-fence` is what keeps the boundary real.

- The **only** default dependency outside the fence is **`wasm-encoder`**
  (Apache-2.0 WITH LLVM-exception) — a permissive byte emitter, not a
  compiler.
- **`wasmtime`** is optional, never a default, and declared only for
  non-wasm targets.
- **No workspace-local AGPL edge is added.** In particular the translator does
  **not** use `lp-riscv-inst`, even though the fence's allowlist would permit
  it: `lp-emu-jit` carries its own decoder, and `tests/decoder_agreement.rs`
  is the price paid for that (JD3) — every 16- and 32-bit word of both pinned
  render images decoded by both decoders, plus an exhaustive sweep of the
  encodings the corpus does not reach, asserting identical `(width,
  InstClass)`. A second decoder is a divergence risk; a disagreement is now a
  build failure rather than a frame divergence three weeks later.
- `js/jit-host.js` is MIT too, **and its own header says so**, because
  `lint-emu-fence` polices the crate graph and a loose `.js` file is invisible
  to it. That is also why M7 P9 moved the file out of
  `scripts/emu/bench-web/`, where the surrounding tree is AGPL, and into the
  crate.

**Open question E1 is closed by this.** E1 asked whether the MIT unit is
externally self-contained when the ISA and ELF crates it might depend on are
not — raised 2026-09-06 with the fence, open since. The translator is the
hardest case the fence will get: a whole compiler back end, with an obvious
in-repo decoder sitting on the far side of the boundary, under time pressure.
It was built without reaching across, and the fence lint passed unchanged
through every phase of M7 and M7b with **no new allowlist entry anywhere in
the milestone**. The answer to E1 is therefore **yes, the MIT unit is
self-contained, and it stays that way by paying for it** — an agreement test
instead of a dependency. The general question of relicensing the ISA crates is
not reopened here; it is no longer blocking, which is what E1 was asking.

### 10. The boot cost is a product number, and it is reported (JD20)

Emit ms, module bytes, engine compile ms and instantiate ms are printed on
every run and carried in every PR body. JD20's budget was **≈0.4–0.7 s on the
phone** for the executed image, at the phone's measured 27 ms/MB.

**At G-M7P (P6) that budget was missed, badly, and by a mechanism worth
recording.** Three whole-image events, 64 blocks a function:

| | node/V8 | bun/JSC |
|---|---:|---:|
| boot cost, all three events | 2.34–2.39 s | 7.62–14.55 s |
| of which emit (this translator) | 1.84–1.90 s | 1.57–2.19 s |
| of which engine compile | 0.23–0.29 s | 4.91–11.94 s |
| module bytes at the last event | 65,473,145 | same |

node was 3–4× over budget and JSC 11–20× over. JSC's **third** compile is the
anomaly and it reproduced in all four runs: 250 ms, 271 ms, then **11,416 ms**
for a module 4 % larger. That is a memory-pressure cliff — by the third event
the linear memory holds the 257 MiB arena, a 65 MB module byte vector, and the
previous module and instance, because `install` built the replacement before
dropping what it replaced.

**Q1's answer, and it is a product decision rather than a performance one:
the boot cost is accepted, and it is bought down by having fewer events rather
than by making an event cheaper.** M7b P1's incremental `fence.i` is that
decision implemented: one whole-image emit plus increments instead of three
whole-image emits. The desk now reports the whole thing as `built in 1209 ms
(discover 191 + emit 969 + compile 47 + instantiate 1)` for two modules. The
phone has not been re-measured against the 0.4–0.7 s budget since P1, and this
document does not claim it has been.

The reason this is acceptable to decide rather than escalate: it is paid
**once, at boot, in host time**, it cannot reach a transcript (§6), and the
alternative — a hotness tier that translates less — is the thing §1 measured
and rejected.

---

## The numbers, against the bar

**M7 closes at the floor, by Yona's ruling at G-M7B (2026-09-11 13:55) and
G-M7B′ (17:10).** Writing them plainly, because a ladder that only records its
wins is not evidence:

**The bar:** 1× real time is the floor, **1.5× is the pass bar, 3× is the
target**, all at `render-basic` t2 on an iPhone 16 Pro Max.

**The phone** (iPhone 16 Pro Max, iOS 18.7 / Safari 26.6.1, `render-basic` t2,
5.5 s emulated, best of three presses spaced by minutes, 8 blocks/fn, on the
P4 head; M7 P7 measured neutral, so this is M7's closing phone number):

| row | presses | best |
|---|---|---:|
| translated, 8 blocks/fn | 0.851 / 0.972 / 1.025 | **1.025×** |
| translated, 16 blocks/fn | 0.882 / 0.918 / 0.943 | 0.943× |
| `--interpreter` | 0.589 / 0.611 / 0.586 | 0.611× |
| same-press translated ÷ interpreter | 1.44 / 1.59 / 1.75 | **1.75×** |

**So: about 1.0× of real time, and about 1.75× of the emulator's own
interpreter on the same device and the same press. The pass bar of 1.5× against
real time is not met. The target of 3× is not met.**

### Per image, and where the translated core is SLOWER

`render-basic` is the image the whole ladder is quoted on, and quoting only
`render-basic` hides something real. node/V8, 16 blocks a function, a 5,500 ms
emulated bound, one invocation per image running both legs back to back,
best of three invocations, load 2.6–4.3 throughout, UART0 sha identical
between the two legs of every image:

| image | grade | translated | `--interpreter` | translated ÷ interpreter | mean stay | coverage |
|---|---|---:|---:|---:|---:|---:|
| `harness` | t2 | **3.062×** | 1.143× | **2.68×** | 1354.0 | 99.63 % |
| `boot-idle-memfs` | t2 | 2.808× | **5.293×** | **0.53×** | 43.7 | 97.82 % |
| `render-basic` | t2 | 0.990× | 0.557× | 1.78× | 68.9 | 97.94 % |
| `render-rocaille` | t2 | 1.166× | 0.673× | 1.73× | 30.0 | 99.30 % |

**On `boot-idle-memfs` the translated core is a little under half the speed of
its own interpreter.** That is not a defect and it is not noise; it is the
design's cost model meeting a workload the design is not for, and the
mechanism is arithmetic:

- The run is **sparse**. It retires 51.0 M instructions in 5.5 emulated
  seconds, against `render-basic`'s 542.9 M — the guest boots and then idles.
  The interpreter does the whole thing in 1.04 s of wall (5.29× real time).
- The **translation cost is fixed and is paid anyway**: discover 144 ms +
  emit 892 ms + compile 41 ms ≈ **1.08 s**, which is most of the translated
  leg's 1.96 s. Translating 184,890 blocks for a run that executes a small
  fraction of them is the whole loss.
- What is left over — about 0.88 s of steady state against the interpreter's
  1.04 s — is only a ~1.2× win, because a boot/idle workload is **short-stay
  and exit-heavy**: mean stay 43.7 instructions, 1.14 M entries, 626 k
  indirect-misses, 1.11 M instructions interpreted between stays. Every one of
  those is an entry protocol paid to retire a handful of instructions, which is
  §1's 77-instruction measurement showing up as a whole image rather than as a
  region.

The general rule, stated so it does not have to be rediscovered: **a translated
core wins where stays are long and loses where they are short or where the run
is too small to amortize its own translation.** `harness` is the far end of
that — 1,354-instruction stays and 2.68× over its interpreter; the in-tab lane
measured the other far end, a mask-ROM ELF direct-booted into an MMIO-poll-bound
loop, at 0.24× translated against 0.48× interpreted in node.

**This ADR does not change the default over it.** The default is the wasm
build's, it is reversible with `--interpreter`, and `render-basic` and
`render-rocaille` — the images the product's own workload looks like — both
gain. Whether a per-image or per-workload policy is worth having is a ruling
for the director and a candidate for the emulator-loop milestone, not
something to decide from four rows.

**The desk proxy** on `main` at M7's close, one invocation per engine,
interleaved, best-of-N, load quoted in each PR body, UART0
`2407828f80684331` on every leg: **node/V8 at 16 blocks/fn ≈ 0.98–1.00×**,
**bun/JSC at 8 blocks/fn ≈ 0.79–0.82×** (P7's own rows: 0.982× and 0.818×;
the same head's baselines 1.002× and 0.823×). Against its own interpreter in
the same invocation that is **2.36× in V8** and **1.46× in JSC**.

**The ladder, on the desk in V8 at 16 blocks/fn:**

| head | real time |
|---|---:|
| M7b start (after M7 P6c) | 0.720× |
| + P1 incremental `fence.i` | 0.842× |
| + P2 after-store exit becomes a poll | 0.854× |
| + P3 SYSTIMER reads served in-module | 0.902× |
| + P4 slice cadence and the scheduler's tombstones | 0.971× |
| + P5 function size, default 8 | ≈0.98× |

**Both binaries** (JD19 — a generic instantiates at the caller crate's
opt-level, and `lp-cli` links at `opt-level = "z"`, which is how M6 lost 25 %):
native throughput is unchanged by the translator work, because natively the
translator is off. `lp-emu-esp32c6` reads 1.03× on `render-basic` t2 and 1.29×
on `render-rocaille` t2; `lp-cli`'s instantiation of the same machine tracks
it within the probe's 5 % rule.

### Why 3× was not reachable from here, arithmetically

M7 P6c profiled a translated run one level up from the translator. Of a
`render-basic` t2 run in node/V8: **61.1 % is the emulator's own wasm**,
32.7 % is the translated modules, 4.6 % is the JS rig, 1.5 % is GC. Inside
that 61.1 %: translation itself 22.0 % of the run, the hart's slice loop
10.0 %, the entry path 5.5 %, the scheduler 4.4 %, MMIO routing 3.8 % — and
**every peripheral model together is 3.4 %**.

With translation removed entirely, the emulator's own steady-state wasm is
3,251.8 ms of a 5,500 ms emulated run: **1.69× on its own.** 3× needs 1,833 ms
of wall. So the emulator's own loop — slice loop, scheduler, bus routing,
per-slice machinery — would have to fall by **1.78×** before 3× is
arithmetically possible, and nothing in M7 or M7b touches that loop as a
whole.

Two of the ladder's named suspects were refuted along the way, which is worth
recording because both were load-bearing assumptions: MMIO traffic is
**79.3 % SYSTIMER** (one `SystemTimer::now()` sequence issued 2.3 M times, and
now served inside the module), **12.3 % RMT**, and **0.06 % UART0** — the
UART0 TX-FIFO poll that the ladder's research named as 86 % of MMIO is 8,254
operations in a whole run. And the RMT's per-word slice cadence, priced at
~500 ms in the plan's projection, turned out to be **synchronisation with the
guest** rather than a cadence anyone chose: the transmitter runs at most 20
words ahead of an ISR writing 24 words into the same RAM, so it cannot be
coarsened exactly.

**The next milestone is the emulator's own loop and its MMIO interactions**,
with its own plan started from P4's census
(`lp2025/2026-09-11-1731-emu-loop-redesign/`). This ADR does not predict what
it will find.

---

## Consequences

- **The wasm emulator's execution path is generated code.** A translator bug is
  a wrong frame rather than a crash, which is why the identity contract (§7)
  is the bar and why both CI tiers exist.
- **The emulator must run in a dedicated Worker in every browser embedding.**
  Not a preference; §6's argument depends on it.
- **The arena must not move for the life of a machine**, because bases are
  folded into emitted code.
- **A new pinned reference image changes what the gate proves.** Images are
  pinned by firmware commit and checked by sha; a re-pin is a decision (JD22
  was one).
- **`lp-emu-jit` is MIT and stays externally self-contained**, at the cost of
  its own decoder and an agreement test.
- **Natively nothing changed.** Every existing test, CI job and walk runs the
  block-cached interpreter exactly as before.
- **A wasm embedder that never calls `jitHost.attach(instance)` now fails at
  boot** instead of quietly interpreting. That is deliberate: a silently
  interpreting wasm build would be a wrong *number* rather than a loud error.
- **A ROM-up board still interprets.** The flip is a wasm-target default, and
  ROM-up boots refuse translation (DD19), so Studio-in-a-tab's emulated boards
  are unaffected until ROM-up translation is done.
- **On a sparse, short-stay image the translated core is slower than the
  interpreter** — `boot-idle-memfs` at 0.53× of its own interpreter. The
  default is kept and the number is on record; a per-workload policy is a
  ruling nobody has been asked for yet.
- **M7 is closed at ~1.0× real time and ~1.75× over the emulator's own
  interpreter on the phone**, below the 1.5× pass bar. The work is reversible
  at the flip of `--interpreter`, and the evidence for the next milestone is
  P6c's profile and P4's census.

---

## Alternatives Considered

- **A hotness tier / region JIT.** Rejected on measurement: thin regions at 77
  instructions per entry ran slower than interpreting them, and end-to-end gain
  is Amdahl-bound by coverage. §1.
- **One wasm function for the block set.** Impossible: the 7,654,321-byte
  function-body limit. §4.
- **A trace (adjacency) block layout.** Implemented, measured, removed: 2–5 %
  worse and 6.2 % more crosses, because guest address order already is the
  trace. The number is kept in `lp-emu-jit/README.md`; the implementation is
  not.
- **One host import per entry point** instead of a shared function table.
  Rejected: the table works in every target engine and is the same shape
  natively. §6.
- **A JS shim between the module and the bus.** Rejected on measurement:
  6.05 ns against 1.60 ns per MMIO call. §6.
- **Asynchronous compilation / JSPI.** Rejected: unnecessary, because host time
  cannot reach guest state (PD5), and harmful, because an await inside the run
  loop would put a place where it could. §6.
- **Guard-page trap elision on the native host** (JD18's configuration).
  Mutually exclusive with JD4's bus-owned `Vec` arena as written —
  `MemoryCreator`'s contract lets cranelift *elide* bounds checks on the
  strength of unmapped space that a `Vec` does not have, so honouring it would
  turn a translator bug into a silent write into the emulator's own heap. The
  native host takes explicit bounds checks and the resulting ~4.8×. The browser
  has no such conflict. The fix — `mmap` with a real guard region — belongs to
  the deferred native phase. (JD21)
- **Relaxing cycle accounting**, on the strength of a signal that timing need
  not be perfect. Rejected: it is what makes every oracle work, and it was
  measured to cost nothing. §7.
- **Using `lp-riscv-inst` for decoding.** Permitted by the fence's allowlist
  and rejected anyway: an agreement test is a cheaper price than a dependency
  across the licence boundary. §9.

---

## Follow-ups

- **The emulator's own loop** — the next milestone, its own plan at
  `lp2025/2026-09-11-1731-emu-loop-redesign/`. P6c's profile and P4's census
  are its first pages.
- **The native host policy (deferred P8, JD24):** cached compiled modules
  keyed by image hash, Winch, and JD21's `mmap`-with-guard-region arena.
  None measured.
- **F5 — the image-scale replay differential.** A `--jit-record` recording can
  only be taken after the `fence.i` (nothing enters translated code before it
  on these images), and by then the published-read fast path is armed; a
  replay's canned import answers do not refresh the published SYSTIMER words,
  so the module asks for reads the recording never recorded. Fixing it means
  recording the republished words beside the store that caused them.
  `scripts/emu/jit-image-bench.mjs --check-only` is the shape that will run it.
  Separately, such a recording cannot be a committed fixture: `render-basic`
  is a 76 MB `module.wasm` and a 24 MB `memory.bin` beside 67 KB of entries.
- **The boot cost on the phone**, re-measured against JD20's 0.4–0.7 s budget
  on a head that has M7b P1's incremental `fence.i`.
- **ROM-up translation** (DD19, last) — without it no in-tab emulated board
  runs translated code at all.
- **Whether the default should be per-workload**, given `boot-idle-memfs` at
  0.53× of its own interpreter. A director's ruling, with the emulator-loop
  milestone as its natural home.
- **M7b P6** (PR #726) removes the undecodable exit class entirely (710,536 →
  1, coverage 97.94 % → 98.07 %, mean stay 68.9 → 75.2) and reads **−1 %** at
  the 5.5 s bar because 15,896 escape sites cost 328 B each. Held unmerged
  pending a ruling; the measurement is on record either way.
