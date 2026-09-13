# ADR: The emulators cache pre-decoded basic blocks, and the cache is not architectural state

- **Status:** Accepted
- **Date:** 2026-09-09 — decided at the M5 design gate (R6, 2026-09-08 22:31),
  shipped in [#637](https://github.com/PhotomancerArt/lightplayer/pull/637)
  (`6d14b7834`), closed at gate G3 (2026-09-09 18:30).
  **Written 2026-09-13 as a backfill**: M5's closing phase (P6) was the phase
  that would have written it, G3 stopped the milestone one phase short, and the
  ADR went with it. Nothing below is a new decision; every ruling here was taken
  on the dates named.
- **Deciders:** Photomancer (Yona) — the invalidation ruling (2026-09-08 22:13),
  the design gate R6, and G3
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-09-06-esp-soc-emulator-architecture.md` (the machine this
  lives in, and PD5 — a run is a pure function of the instruction stream);
  `2026-09-08-emulator-poll-loop-skip.md` (**Rejected** — the rung beside this
  one; its closing section defers the poll skip to "a block-boundary check in a
  block-cache design", which §9 below answers);
  `2026-09-11-emulator-wasm-translator.md` (the rung above — what the cache's
  job became once a translated core existed);
  `2026-07-28-emu-core-crate-family.md` and
  `2026-07-30-isa-parameterized-host-emu-engine.md` (the crate seam this layer
  had to fit through)

Plan: `lp2025/2026-09-07-0827-emu-speed-ladder`, milestone **M5** (decision D8,
milestone decisions MD1–MD15), archived.

Detail lives with the code and is **cited, not repeated**:
`lp-emu/lp-emu-core/src/block.rs`'s module docs are this design's technical
page (the table, the arena, the invariant, the invalidation funnels, the
sizing); `lp-emu/lp-riscv-emu/src/mach/block.rs` is the RV32 half (the slot,
the classification, where a block ends);
`lp-emu/lp-xt-emu/src/block.rs` is the Xtensa slot;
`lp-emu/lp-riscv-emu/src/mach/mod.rs`'s `run_blocks` is the loop that consults
the cache, and the entry filter beside it is the translator's.
This ADR states the decisions and the measurements that forced them.

---

## Context

### The measurement that forced it

M5 P1 profiled the C6 machine running **the product's own render loop** — 241
lamps with a JIT'd shader over RMT — and read the interpreter's host time as:

| bucket | share |
|---|---:|
| working out what the next instruction is (fetch, decode, dispatch, the bookkeeping around all three) | **~74 %** |
| executing the guest's arithmetic | **~4 %** |

An interpreter that re-decides what every instruction *is* on every execution
spends most of its host time on that decision. The same profile said the memo
for it is cheap and the re-use is near-perfect: the render loop enters **37,517
distinct block starts** across 428 executed 4 KiB pages, and **99.7 %** of block
entries go to a start that has been entered a hundred times or more. No warm-up
problem, no capacity problem — which is what licenses a flat direct-mapped
table with a tag, and no hash map and no LRU anywhere in this design.

### And the workload that had been measured until then was the wrong one

Worth stating plainly, because it is the most useful paragraph here six months
on. **The ladder measured a console-bound image for four milestones.** The
harness and `jit-math-perf` both log everything over UART at baud, and on the
phone the M4 tree read 3.72× real time on one of them — the ladder's target
(D1) looked *met before M5 opened*. M5 P0 then built two reference images from
the product's real render loop (`render-basic`, `render-rocaille`), and the
same tree read **0.55× on `render-basic` at t2**. The block cache is the first
rung aimed at the workload the product actually runs, and the rung beside it —
the poll-loop skip — was rejected the same week for the same reason: it won
5.9× on logging and lost 7–12 % on rendering.

---

## Decision

### 1. The cache is not architectural state, and everything else follows from that

**Nothing observable may depend on a hit or a miss.** The same guest, the same
input and the same grade produce the same cycle count, the same instruction
count, the same transcript and the same waveform with the cache on or off.

Concretely, and each of these is a place in the code rather than an aspiration:

- it is **absent from a snapshot** — `Clone for MachineHart` hands back
  `cache: None`, which *is* "restore invalidates all", and `reboot` goes
  through `restore`;
- **`--no-block-cache`** turns it off entirely and is the bring-up tool, the
  bisection tool and a free identity oracle (MD9);
- a cached run and an uncached run are compared byte for byte — `stopped
  after`, UART0, stdout, stderr, a 20 ms `--trace`, and the WS281x frames
  decoded off the emulated pad — on every image at both grades.

This is the sentence the whole design hangs on, and it is also why the
interpreter was still a usable differential oracle two rungs later when the
translator landed (`2026-09-11-emulator-wasm-translator.md` §oracle). A memo
that cannot be observed can be discarded at any moment for any reason, which is
what makes whole-cache flushing (§7) a legitimate answer rather than a
concession.

### 2. Arch-neutral in `lp-emu-core`, with the slot type injected (D8, amended by MD1)

`lp_emu_core::block` owns block identity, the direct-mapped table, the slot
arena, invalidation and the block's cost bound. It **names no `Bus` and no ISA
type.** The architecture supplies the slot type and the decoder as its own
code.

D8's original sketch had the core naming a bus. That was corrected at the M5
design gate (MD1, finding F4) by a fact about the other consumer: **`lp-xt-emu`
has no bus at all** and no reason to grow one. A layer that demanded one would
have excluded the crate it was explicitly being built arch-neutral *for*.

The core therefore knows exactly two facts about a slot, because those are the
two the layer's own rules need: its **width** (the block's byte span, which is
what an invalidation compares against) and an **upper bound on what it can
charge** (the whole-block budget test, §4).

The two architectures' slots differ, and the difference is not an accident:

| | slot | why |
|---|---|---|
| RV32 | `{ word, handler: fn(..), width, bound }` | the crate's category executors take the raw word; the slot pre-resolves *which* executor, turning a three-level match into one indirect call |
| Xtensa | `{ inst: Inst, len, class }` | the executors already run from a decoded `Inst`, so caching the `Inst` skips `lp_xt_inst::decode` outright rather than adding a second dispatch layer |

The RV32 slot is deliberately the **cheap** one: it keeps the
`ExecutionResult` round-trip that Step B existed to remove. §8 is what happened
to Step B.

### 3. Invalidation is `fence.i`-driven, and that is a change to D8 (MD12, MD13)

D8 said **store-address invalidation**. It is not what shipped. On 2026-09-08
at 22:13 Yona overturned it in one sentence — *"Can't we require a fence
instruction — we own the firmware, we can ensure it's done right"* — and the
design is better for it:

- **The guest publishes with a fence.** `lpvm-native`'s
  `JitBuffer::from_code` issues one `fence.i` after linking a shader, and
  `MachineHart::on_fence_i` flushes the whole cache. That one line is the
  milestone's **only change to product code**, it costs one instruction per
  shader compile, and real silicon with an I-cache requires it anyway — the
  C6 does not, which is exactly why the doc comment at that site says, in as
  many words, *do not delete it because the C6 does not need it*. The contract
  is documented at both ends, each pointing at the other.
- **The emulator's own code writes funnel.** A flash-cache MMU page fill, a
  ROM-hook `ebreak` patch over live mask ROM, an ELF segment at load, the
  lockstep harness's image — none of them is the guest, so none emits a fence.
  All of them go through `SocBus::load_image`, which records the window when
  the region is executable, and the machine drains it at the slice boundary
  into `invalidate_range`. P1 counted 98 cache refills across a whole render
  run; the shipped run reports 96 range invalidations dropping 15 entries.
- **The store-address mechanism survives as a checker, not as correctness.**
  Under `--strict-bus` the bus tracks code pages written and then executed with
  no `fence.i` between, and names that as a *firmware* bug with the page and
  the pc, at the moment it is introduced. It costs one already-loaded `bool` on
  the default path.
- **`BootMode::RomUp` runs with the cache off** (MD13). There the mask ROM and
  the real ESP-IDF second-stage bootloader run as guest code, copy segments
  into hp-sram with ordinary stores and jump into them — and we own neither, so
  neither will ever fence. ROM-up is a boot-*modelling* path measured in
  hundreds of milliseconds, not a speed path, so refusing to cache there is
  free and removes the only unfenced guest code writer.

One more precondition, and it is the bus's to answer: decoding ahead is exact
only over a bus where a fetch **charges nothing** and **traps nothing**.
`Bus::fetch_is_pure` is that claim, it defaults to `false` (a bus opts in
rather than being opted in by a trait default it never read), and it is re-read
at every block boundary — an armed execute watchpoint takes the machine back to
single-stepping mid-slice.

### 4. "Block cost precomputed per cycle model" is achievable only as an upper bound (D8, corrected by F1)

D8's third clause, read literally, is not implementable: `BranchTaken` (2
cycles) versus `BranchNotTaken` (1) is a **run-time** choice, and a branch is
always a block's terminator. What is precomputed is therefore `max_cycles` —
the sum of every slot's *upper* bound — and the budget rule is a hybrid:

- `cycle_count + max_cycles <= end` → run the block **whole**, with no per-slot
  deadline compare. Every interior instruction boundary is then strictly below
  `end`, so the slice stops at precisely the instruction a single-stepping loop
  would have stopped at.
- otherwise → run it slot by slot with the compare the single-stepping loop
  already uses.

**Both branches are exact by construction**, and the cycles actually charged
always come from the class the executor *returns*, never from the bound. That
asymmetry is why the bound is allowed to be coarse (`DivRem` for any `OP` with
the M-extension funct7, `Load` for any compressed body slot): a bound that is
too generous costs a little fast-path coverage near a deadline and can never
miscount a cycle, while a bound that were too *small* would be a correctness
bug. A hart that changes its cycle model must invalidate, and
`set_cycle_model` does.

### 5. A block does not end at a plain store (MD2)

The milestone brief said blocks should terminate at Store/Atomic/System. That
was a misreading of the polling contract and was overturned by measurement:
terminating at stores costs **28–31 % of the mean block length** and produces
**40–45 % more blocks**, against a self-modifying-code pressure of **0.02 % of
stores across five pages**.

A store therefore runs *inside* a block, and after it the block executor
performs exactly the checks `step()` performs today, at the same point and in
the same order — the bus side-band, the resample, the yield. All four
interrupt-polling points survive unchanged; the only thing that changed is how
the handler was found.

The evidence that made the difference material came from the rejected rung
beside this one: M4 found that the mask ROM's `uart_serial_tx_one_char` spills
the character to the stack on **every** iteration, so a rule that refused
stores inside a loop body was not conservative — it was inapplicable.

Blocks end **after** a control transfer and **before** a `SYSTEM`, an atomic or
a fence. Classification is conservative in the safe direction: calling a body
instruction a terminator only shortens a block, so every encoding that might
transfer control is a terminator, and anything not positively recognised is
refused outright — the caller single-steps it, which is always exact. Beneath
that, the block executor re-derives each slot's next `pc` and leaves the block
the moment it is not the one the decoder expected. In a correct build that
fires exactly at a taken terminator; it is there so a classification mistake is
a slower block rather than a wrong one.

### 6. The table index mixes the whole address, and that is not a micro-optimisation

The first implementation indexed on `(pc >> 1) & mask` and the cache was a
**19 % regression**. The C6 runs code out of a 16 MiB flash-cache window,
hp-sram at `0x4080_0000` and mask ROM at `0x4000_0000`, so the low 17 address
bits do not identify a block start: **864,230 collisions and 955,532 decodes
against a working set of 37,517**. A multiply-shift (Fibonacci) mix over the
whole address plus 2^18 entries takes that to **73,162 decodes at a 99.93 % hit
rate**, and the same commit turned the regression into the shipped 1.15×.

It is recorded here rather than left in the diff because it is the failure mode
a future port will hit again: on a chip whose executable regions are far apart
in the address space, the low bits of a `pc` are not a hash. The tag is a
**full** `pc` for the same reason — a partial tag would alias two starts onto
one entry and run the wrong code.

The other half of that commit is the same kind of fact: sweeping all of RVC
quadrant 2 funct3 100 into the terminators put `c.mv` and `c.add` — two of the
commonest instructions a compiler emits — at block ends. Splitting them by
`rs2 != 0`, which is the spec's own split, took the mean realised block length
from 3.75 to **4.98** against P1's predicted 4.69.

### 7. Overflow flushes the whole cache (MD7)

Deterministic, a pure function of the instruction stream, trivially correct, no
LRU to get wrong. `rv32emu`'s `code_cache_flush` makes the same choice. P1's
working set says it should be rare to never, and the counter says so: **0
capacity flushes** on both render images. `BlockStats::capacity_flushes` is
therefore a **finding** if it is ever non-zero, not a number to accept.

### 8. The milestone stops at Step A, and the estimate that justified Step B was wrong

M5 was staged: **Step A** (the cache, the fence, the flag, the oracles, with
the cheap slot) → a measure gate → **Step B** (a slot with pre-extracted
operands, and per-*block* bookkeeping instead of per-instruction). The gate
existed because P1's central estimate, 1.96×, was a *design* difference worth
settling with a working cache in hand.

Step A shipped. Quiet desk (load 2.77–3.29), best of 3, both binaries in one
window:

| image | grade | main | Step A | speedup | rt(user) before → after |
|---|---|---:|---:|---:|---|
| render-basic | t1 | 6.67 | 5.82 | 1.146× | 0.65× → 0.75× |
| **render-basic** | **t2** | **6.03** | **5.23** | **1.153×** | **0.85× → 0.97×** |
| render-rocaille | t1 | 6.09 | 4.53 | 1.344× | 0.69× → 0.93× |
| render-rocaille | t2 | 6.01 | 4.51 | 1.333× | **0.96× → 1.28×** |
| harness | t1 | 3.56 | 3.34 | 1.066× | 0.79× → 0.84× |
| harness | t2 | 2.54 | 2.36 | 1.076× | 1.10× → 1.19× |
| boot-idle-memfs | t1 | 0.36 | 0.33 | 1.091× | 8.33× → 9.09× |
| boot-idle-memfs | t2 | 0.35 | 0.32 | 1.094× | 8.57× → 9.38× |

**1.15× on `render-basic`, below Step A's own 1.25× check** — and that is P1's
measured fetch-only floor, so block *structure* had bought almost nothing
beyond eliding the fetch. Where blocks are long it shows: `render-rocaille`'s
6.97-slot mean turns into 1.33×.

Then the estimate collapsed. P1's 1.96× rested on the `ExecutionResult`
round-trip being 9–17 % of host time. Step A made that directly measurable — a
throwaway branch replaced the 200-byte `Result<ExecutionResult, EmulatorError>`
with a packed `u64` and read **1.008×**. A second probe agreed from the other
side: replacing the `fn` pointer with an inlined `decode_execute` read 6.16 s
against 5.62 s, so the indirect call is *cheaper* than the three-level match it
replaced. The sampled lines were not overhead around the work; they were the
work. P1b then built **every** Step B lever on a throwaway branch and measured
the lot: **~1.0× native, 1.12–1.14× in the phone's engine.**

G3 (2026-09-09 18:30): **stop at Step A.** Yona added a second reason the gate
had not asked for — *"that also makes it more complex"*. Nothing was reverted,
because every Step B lever lived only on a branch that was never merged.

The consequence for the ladder is stated in the rung above: the interpreter
levers ran out here, and translation is what M7 did instead.

### 9. What this does, and does not, do for the rejected poll-loop skip

`2026-09-08-emulator-poll-loop-skip.md` closes by deferring its feature to this
design: *"a poll loop in a block-cache design is a self-looping block — the
same fixed-point argument checked once at the block's own boundary instead of
on every instruction inside it."* That deferral is correct in shape and **was
never taken up.** Recording it precisely, since this is the ADR it was deferred
to:

- The shape does hold. A pure poll loop is a block that branches to itself, so
  the `(pc, register file, no observable store)` fixed-point test is a
  block-entry check — once per iteration of the *loop*, not once per
  instruction inside it. The per-instruction bookkeeping that got M4 rejected
  (a uniform 3–8 % tax on every image, recoverable by none of five attempts) is
  the cost this structure removes, because the block cache already has a
  natural place to put a per-entry test.
- It was not built. M5 stopped one phase early at G3, and the rung after it
  (M7) changed the question rather than answering it: in a **translated** core
  the loop lives inside the emitted wasm, so a block-boundary check would have
  to be something the translator emits, not something the machine does between
  blocks. Nothing has been measured in that shape.
- **The rejection still stands as written.** The ledger's row says *do not
  re-attempt in this shape*, and this ADR does not re-open it. Anything that
  credits a spinning guest starts by reading that ADR.

---

## Consequences

- **The interpreted path costs one table read and one compare** to ask whether
  a block is cached, and the cache is on by default on every non-ROM-up C6 run.
  Memory is a 2^18-entry table at 8 bytes an entry plus a slot arena; the
  working set is ~37.5 k blocks at a mean of 4.98 slots.
- **The layer is shared, and the seam held.** `lp-emu-core`'s block layer is
  the piece `lp-xt-emu` and `lp-riscv-emu` both name, and the Xtensa slot type
  (`XtSlot`) landed under a later plan so that the Xtensa block cache and the
  Xtensa translator would name **one** slot rather than two.
- **The generics containment rule is load-bearing.** Every generic in
  `block.rs` is instantiated only from inside a crate the root `Cargo.toml`
  names at `opt-level = 3`, behind non-generic entry points. A per-package
  `opt-level` override reaches only code codegen'd *in that package*, so a
  generic public entry point silently opts the hot loop back down to the
  workspace's `opt-level = "z"`. M6 lost 25 % on the Xtensa probe to exactly
  that. The two-binary probe (the `lp-emu-esp32c6` bin and `lp-cli emu run`)
  exists to catch it, and measured a 1.5 % gap — no trap.
- **Known, deliberate imprecisions**, all recorded at ship: a block may cross a
  region boundary, because `invalidate_range` compares byte spans rather than
  regions; the `--strict-bus` checker is word-granular and change-sensitive
  rather than page-granular (the page model false-positived on the first fenced
  test); the checker records executed pages from decode-ahead fetches, so it
  can mark a page the guest never reached; and under `--boot rom-up` it reports
  0 rather than the bootloader's copies, because a loader writing into a page
  it has not executed from is not the stale-fetch hazard the checker names.
- **The arena is never compacted.** Space held by range-invalidated blocks is
  recovered only by the next capacity flush. Range invalidations are rare (98
  refills in a whole render run) and a compacting arena would be a
  moving-target bug for no measured gain.
- **ROM-up boards get nothing from either rung.** They do not cache (this ADR)
  and they do not translate (the translator ADR, DD19) — and every emulated
  board in Studio-in-a-tab is ROM-up today.

## Alternatives Considered

- **Store-address invalidation** (D8 as written). Overruled 2026-09-08: we own
  the firmware, so the guest can be made to declare its own code writes, and a
  declared publish is both cheaper and more precise than watching every store.
  It survives as the `--strict-bus` checker, which is the useful half — it
  names the firmware bug rather than silently absorbing it.
- **A read-only-region-only first tier** (MD5). Retired once invalidation
  became a fence plus funnels: it would have excluded a fifth of the workload
  to insure against a hazard the fence already covers.
- **Ending blocks at plain stores.** Measured and refused — §5.
- **An LRU, or any capacity policy other than "flush everything".** Rejected:
  a policy is a thing that can be wrong, and the working set says the capacity
  case never arrives.
- **A partial tag** (to shrink the table). Rejected — it aliases two block
  starts onto one entry and runs the wrong code. The tag is a full `pc`.
- **Step B: the richer slot and per-block bookkeeping.** Built in full on a
  throwaway branch, measured at ~1.0× native and 1.12–1.14× on the phone, and
  never merged — §8.
- **A flat `Riscv32Emulator`** (collapsing the hart/bus split for the hot
  loop). Out of scope for M5 and never revisited here; the translator took the
  structural win instead.

## Follow-ups

- **The Xtensa core never joined the cache.** `XtSlot` exists and implements
  `Slot`; `lp_xt_emu::Emulator::run_loop` still single-steps. *Revisit when*
  the Xtensa emulator plan reaches its own translator/speed work (M7 of
  `lp2025/2026-09-10-0021-xtensa-emulator`).
- **ROM-up boards neither cache nor translate.** *Revisit when* ROM-up
  translation is taken on (translator ADR, DD19) — the same boot mode is the
  blocker for both, and Studio-in-a-tab's emulated boards are all ROM-up.
- **The poll-loop skip as a block-boundary (or translator-emitted) check**
  remains unclaimed — §9. *Revisit when* a workload appears whose spin is not
  console drain; until then the rejected ADR's verdict governs.
- **P6's docs sweep** — the phase that never ran — is why
  `lp-emu/README.md` §Speed described the block cache as a rung "not yet
  climbed", and why the translator ADR pointed at §Speed for a description that
  was not there. Closed with this backfill: §Speed now carries the paragraph
  and points here, `docs/emulator-perf-ledger.md`'s row carries the measured
  numbers, and `block.rs`'s three sizing doc comments were corrected to the
  values the index fix shipped (2^18 / 2^21 / 2^19, quadrupled in
  `8292837c5` without their comments).

