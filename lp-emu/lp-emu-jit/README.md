# `lp-emu-jit` — RV32IMC to WebAssembly

The translator behind M7 of the emulator speed ladder. Guest blocks become
WebAssembly, the host engine compiles them, and the emulator's hot path stops
being an interpreter loop.

The spike this crate is harvested from measured **~12.5×** on the code it
covered, in the phone's own engine family, byte-identically — against the 1.14×
the whole remaining interpreter lever set was worth. End-to-end gain is
Amdahl-bound by coverage, which is the argument for translating the whole image
eagerly rather than chasing hot regions. See
`planning/2026-09-07-0827-emu-speed-ladder/spikes/jit-region-spike.md`.

## What is here

| module | what it is |
|---|---|
| `decode` | RV32IMC word → a small `Inst` with the emulator's own `InstClass` cost class and the instruction's width |
| `blocks` | the guest blocks a translation is *of* |
| `discover` | how they are found: a symbol-seeded, width-following sweep over the whole image |
| `translate` | one **sub-dispatcher**'s body: a run of the block set as one wasm function |
| `dispatch` | the block set → one wasm **module**: an outer selector over as many sub-dispatchers as the function-size limit needs, plus the flat tables that make an indirect jump an in-module branch |
| `host` | what an emitted module is run against, and the exit protocol below |
| `host_wasmtime` | a native host, behind `host-wasmtime`, so identity can be proven on the desk |
| `host_browser` | **the product host**: the browser's own engine, on every wasm target, behind no feature |
| `replay` | the identity harness's record and compare shapes, including the per-entry memory-granule diff |

## Discovery: the five rules, and why each one is there

Every one of these was paid for in measurement. A reader who skips this
section will re-derive it over about a day.

1. **Symbols are the seeds.** The entry point alone reaches **two
   instructions**: `_start` runs into a CSR write the translator refuses and
   the walk stops. So the seeds are the entry point, the hart's trap vector,
   and every symbol in an executable region — of the app **and of the mask
   ROM**, which is 2.7–9.2 % of the instructions a render image retires.
   Biggest symbol first, because a bound is a budget and the order decides
   what it is spent on; and a seed is explored to exhaustion before the next
   is added, because queueing them all first spends the budget on seeds and
   follows no edge at all (511 blocks of one instruction each, measured).

2. **Follow decoded widths, never a stride.** **48.99 % of real block starts
   sit at 2 mod 4.** A 4-byte scan desyncs on half the image; a 2-byte one
   manufactures instructions out of the upper halves of 32-bit encodings. A
   symbol is a known-good start and a decoded width is the only honest way to
   the next one.

3. **A call names its return address.** `jal`/`jalr` with `rd != 0` is a call,
   and the instruction after it is where control comes back. This is the
   single largest rule in the file: without it the whole remainder of every
   calling function is invisible, and adding it moved discovery coverage on
   `render-basic` from **63.9 % to 89.2 %**.

4. **A refused encoding ends the block but not the walk.** RISC-V puts an
   instruction's length in the low bits of its first halfword whatever the
   rest of the encoding means, so the address after a `csrr`, an `ecall`, an
   atomic or an `fence.i` is a real block start. The step is **bounded** —
   four in a row and the walk stops — because on data the length bits are a
   guess, and an unbounded guess walks the whole of RAM: zeroed memory does
   not decode, so every two bytes of it becomes another start (259,843
   starts, 420 ms, measured on a bare machine).

5. **Everything else degrades (JD7).** A word the decoder refuses, a fetch
   that cannot be served, a block that runs into another's start, a branch to
   a pc not in the set: all of them end a block and hand the pc to the
   interpreter. A walk over a whole image meets literal pools and jump tables
   where a census never does, because a census supplies real observed block
   starts and a symbol table does not. Data must cost coverage and can never
   cost correctness.

**What discovery cannot find**, and it is worth stating because it is most of
what is left: code the guest **writes itself** and never publishes with a
`fence.i`. No symbol names it and no static edge reaches it — the firmware's
shader JIT calls its buffer through a function pointer — so the only signal
is the publish. The machine records the spans of executable memory the guest
writes and seeds the retranslation from them at each `fence.i`; a firmware
that writes code and does not fence is invisible to this by construction, and
is what `--strict-bus`'s missing-fence checker exists to catch.

## The exit protocol

The thing a future reader needs written down more than anything else here.

The emitted function is

```
run(entry_block: i32, cycle: i64, instret: i64, end: i64,
    watch_lo: i64, watch_hi: i64) -> i32
```

and it returns **the guest pc to resume at**. That is the exported selector;
inside the module each sub-dispatcher has the same parameters and returns an
`i64` whose bit 32 says the low half is a global block index to continue at
rather than a pc to leave at (see `dispatch`). Everything else goes through the
*exchange area*, a fixed 256-byte window of the imported memory whose offset is
folded into the module at emission time:

| offset | what |
|---|---|
| `+0`   | `regs[32]`, one `i32` each, `x0` first |
| `+128` | `mcycle` |
| `+136` | `minstret` |
| `+144` | flags — bit 0 "left after a store", bit 1 "the slice ended", bit 2 "an MMIO load left a yield pending" |
| `+148` | the status the last `step_one` reported |
| `+152` | cross-function transfers this stay made (`i64`) |
| `+160` | indirect jumps the target table did not resolve (`i64`) |
| `+168` | why the stay ended — one of `host::why`'s codes |

Bit 2 of the flags is not the host's: it is how one sub-dispatcher hands the
next its pending-yield obligation across a cross-function edge, in the same
flush-and-reload every other piece of stay state does (P5). The selector clears
the whole field when the host enters, so a stale bit cannot be read as this
stay's.

The last three are **reported, never acted on**, and they are the reason a
coverage shortfall names its own cause instead of being attributed by argument.
`--jit-report` prints them per reason, and `LP_EMU_JIT_EXITS=1` adds the
twenty-four exit sites that cost the most, each saying whether the module could
have been entered there at all.

The **whole** register file is in the exchange area, not just the registers the
block set touches, because `step_one` runs an arbitrary guest instruction and
that may read or write any register. The registers the block set *does* touch
additionally live in wasm locals for the length of a stay, and are flushed and
reloaded around every escape.

**Four** imports since M7b P2, all from the module named `emu`, plus that
module's `memory`:

| import | shape | says |
|---|---|---|
| `mmio_load` | `(pc, cycle, address, kind) -> (status << 32) \| value` | 0 ok · 1 the access did not happen, leave at this pc · 3 ok, and a yield is now pending |
| `mmio_store` | `(pc, cycle, address, kind, value, post_pc, post_cycle, post_instret) -> (status << 32) \| pc` | 0 ok, **carry on** · 1 did not happen · 2 the poll moved the hart, leave at `pc` · 4 the bus took the slice, leave at `pc` with the slice-ended flag |
| `step_one` | `(pc) -> pc` | the escape hatch: run exactly one guest instruction the way the interpreter would |
| `poll` | `(pc, cycle, instret) -> (status << 32) \| pc` | polling point (c) on its own, for the store the bus never saw. Same four answers, minus `MMIO_REFUSED` |

**`cx.now` discipline (JD17).** The counters live in wasm locals and are handed
back at every point the bus can observe them: as arguments to every MMIO import,
in the exchange area before every `step_one`, and in the exchange area at every
exit. Charging a block's cycles at entry is *not* exact — P1b measured
peripheral models reading `cx.now` mid-block.

### Polling after a store (M7b P2, DD17)

Every store is a polling point. Until P2 every MMIO store was also an **exit**:
`SocBus::write_mmio` raises the side-band unconditionally, so `mmio_store`
always answered "leave", the stay ended, and the hart ran polling point (c) out
in `run_blocks`. On `render-basic` t2 that was **4,245,005 of 11,843,442
exits — 35.8 %**, each buying the interpreter 0.73 instructions before control
came straight back.

The polling point has not moved. It still runs after exactly the instruction it
always did, with the pc and the counters the interpreter would have had, and it
is still **the hart's own code** that runs it —
`MachineHart::resample_external`, which is `pub` for this one caller and
carries the contract beside it. What changed is only *where it is called from*,
and the answer it can now give is "carry on".

Two imports carry it, because there are two kinds of store:

- a store the bus serves **already crosses to the host**, so the poll is fused
  into that call. It costs no second crossing; it costs three more arguments —
  the post-store pc, `mcycle` with the store charged, and `minstret` plus one —
  and an `i64` result instead of an `i32`.
- an inline RAM store made while bit 2 of the flags is set — an earlier MMIO
  *load* left a yield, and the interpreter's polling point fires at the next
  store of **any** kind — has no call to fuse into, so it gets `poll`.

Per **BD1** the poll is its own import rather than a widened `step_one`:
`step_one` runs a guest *instruction*, `poll` runs a *polling point*, and
giving the escape hatch's status codes two meanings would put a poll on a path
where none belongs.

The host half is exact by construction (**BD2**) rather than by argument: set
the hart's pc and counters, then `take_sideband()` → `resample_external` →
`take_yield()`, in that order, and answer with the hart's own pc. It does not
decide whether the poll *would* have mattered; it runs the poll and looks at
where the hart ended up. `deliver_interrupt` takes only the CSR file, so the 32
registers can stay in wasm locals for the length of the stay.

One case deliberately is **not** a slice end: a bus that stops claiming
`fetch_is_pure`. `run_blocks`'s answer to that is `cache.invalidate_all()` and
`run_slice_stepping`, not a `SliceEnd`, and that check already runs after
**every** `core.run` — so the poll answers `MMIO_LEAVE_AFTER`, the stay leaves
ordinarily, and the hart's own check does what it always did.

**What the emitted code pays.** The common path — an inline RAM store with
nothing owed — is one `or` and one never-taken branch, which is *cheaper* than
the `status == MMIO_LEAVE_AFTER || pending` test it replaces. The `poll` call,
the result unpack and the exit are emitted once per store inside the cold arm.
Emitted bytes still cost: the module goes **68,538,511 B → 76,166,092 B
(+11.1 %)** on `render-basic` t2 at 16 blocks a function, and emit + compile go
866 ms → 1,010 ms. That ~144 ms of one-time translation is real and is counted
against the phase's steady-state win.

`FLAG_AFTER_STORE` stays in the protocol even though this machine's core never
sets it again: it is the answer a core that *cannot* poll still gives, and
`run_blocks`'s `after_store` arm is the fallback that serves it.

#### What `FLAG_PENDING` means, and what it means after an escape (M7b F1)

`FLAG_PENDING` is bit 2, and the local behind it holds **that bit** rather than
a bare 1 — the epilogue `or`s the local straight into the exit flags and the
next sub-dispatcher's prologue reads it back with `& FLAG_PENDING`, so a value
of 1 was read by nobody at the far end of a cross-function edge and by
`run_blocks` as `FLAG_AFTER_STORE` at an exit. Two consequences, both fixed
here: an obligation that crossed into another function was **lost**, and the
inline RAM store over there skipped a polling point the interpreter runs; and
an exit while a load was pending reported a flag the protocol says a stay no
longer sets. The host reads the two bits together (`jit.rs`'s `POLL_OWED`), so
an exit still hands the poll to the hart's own `after_store` arm — the stay
must not carry the obligation into a *later* stay, because the selector clears
the flags word at every entry.

After the **escape hatch**, `FLAG_PENDING` means exactly what it meant before
the escape. `step_one` runs the interpreter's own polling point for a Store-
or Atomic-class instruction and not for a Load, so a yield an escaped *store*
left comes back as `STEP_SLICE_ENDED`; a yield an escaped **load** left comes
back as nothing at all, and the local stays clear. That is safe only because
`Emit`'s single `memory` bit gates loads and stores together: a build whose
loads escape has no inline store to skip a poll at, and a build with inline
stores escapes no load. **Splitting that bit needs `step_one` to report
`Bus::sideband_or_yield_pending` first** — guessing does not work, because
setting the local after every escaped load makes an all-escape build disagree
with an emitted one about a plain RAM load that reached no bus, and narrowing
the guess to off-RAM loads leaves the same disagreement for an MMIO load that
left nothing.

## The browser seam (P6, JD11–JD13, JD25)

The exit protocol above says what a module expects. This says who gives it to
it in the browser, which is the host the milestone's number comes from.

**One namespace, and it is `emu`** — `translate::IMPORT_MODULE`. P2 left the
name open; it is fixed here and nothing else may use it. A translated module
imports `emu.memory`, `emu.mmio_load`, `emu.mmio_store`, `emu.step_one`, and
all four are bound to the **emulator instance's own** memory and exports, so
every call a translated module makes is wasm→wasm with no JS frame on it.

| what | where |
|---|---|
| `jit_mmio_load`, `jit_mmio_store`, `jit_step_one` | exports of the emulator's wasip1 module (`host_browser`) |
| `jit_table_probe`, `jit_table_selftest` | the entry-encoding round trip, run once at wiring time |
| `emu_host.jit_compile`, `emu_host.jit_release` | the **only** two JS imports, called once per translation event |
| `scripts/emu/bench-web/jit-host.js` | the JS half, as one importable ES module |

**How a second Worker uses it.** `jit-host.js` exports `makeJitHost()` and
nothing else it needs to be told about. Instantiate the emulator with
`{...yourWasiShim.imports, ...host.imports}`, call `host.attach(instance)`
**before `_start()`**, and run. `attach` throws rather than returns on a seam
that will not work, and `host.events` afterwards is one entry per translation
event — module bytes, the arena base, the engine's own compile and instantiate
milliseconds, the table slot. It does no I/O, takes no options, and knows
nothing about the bench page, which is what makes JD25's "Studio's worker
imports it unchanged" a fact rather than an intention.

**Three rules the browser adds, each paid for in measurement:**

1. **`table.grow(1)` then a separate `table.set(idx, run)`.** Never the fused
   `grow(delta, ref)`: JSC accepts it, `table.get(idx)` reports the funcref
   back, and `call_indirect` on that same slot traps as a null entry (P2, S4).
2. **The build needs `--export-table` *and* `--growable-table`.** Without the
   first there is no `__indirect_function_table` to put an entry point in;
   without the second wasm-ld pins the table's maximum to its initial size
   (940 entries on this binary) and `table.grow` fails with a bare
   `RangeError` naming neither the linker nor the flag. The
   `#[unsafe(no_mangle)] pub extern "C"` exports need nothing at all — they
   survive `--gc-sections` on their own in a `wasm32-wasip1` bin, with no
   `-C link-arg=--export=<name>` and no `#[used]`. Checked against the
   module's own export list, both ways round.
3. **The arena is not at offset zero.** Natively the module's memory *is* the
   arena; in the browser the module imports the emulator's whole linear memory
   and the arena is an allocation inside it, so `Layout`'s `arena_offset`,
   `perm_offset`, `exchange_offset` and `indirect` all shift — **and so do the
   indirect page map's entries**, which are pointers into the memory rather
   than into the host's view of the arena. Natively those are the same number
   and nothing can tell them apart; in the browser a page map built the other
   way sends every resolved `jalr` to a block index read out of guest data,
   and installed coverage measures 0.00 % with a guest that never leaves the
   second-stage bootloader.

**A trap is fatal here.** Natively a wasm trap comes back as an `Err` and the
run continues interpreted. In a browser engine a trap through `call_indirect`
kills the instance, so `BrowserCore::enter` only ever returns `Err` for a
host-side refusal and the rig reports the Worker's `error` event instead. The
shape is kept identical to the native host's so the machine crate has one code
path.

## The escape hatch, and why bring-up cannot cliff

`Emit` says which instruction classes the translator emits itself; everything
else becomes a `step_one` call inside the same block. `Emit::NOTHING` therefore
produces a **complete and correct translation that emits no guest semantics at
all**, and it is kept as a test rather than as a stage that was passed: it is
the proof that a partial translator can only be slow, never wrong (R9).

P3 measured it on all four pinned images at both grades, and on `render-basic`
t2 with 34,189,069 guest instructions going out through the hatch — identical
UART0 bytes, identical `stopped after` line, identical frames off the pad.

An encoding `decode` does not recognise cannot be escaped that way, because its
*width* is unknown and so the next pc is unknown. Those end the block instead.
Two escapes, one rule: never guess.

## Two-level dispatch, and the size it is set to (P5, JD8, JD26)

wasm caps **one function body at 7,654,321 bytes**, and a whole-image walk of
`render-basic` finds **201,244 blocks**. One function was never a tuning
choice; it is arithmetically impossible. So the module is an outer **selector**
over N **sub-dispatchers**, each holding a contiguous run of the block set and
each keeping the `loop`/`br_table` shape inside. `dispatch`'s module docs have
the shapes; what follows is the number the split is set to and how it was
measured.

**`--jit-fn-blocks` is the knob and 256 is the default.** JD26 says the size is
chosen by measured steady-state throughput in JavaScriptCore, not by the limit.
So it was: one native `--jit` run recorded 12,000 entries into translated code
— every import answer in call order, the memory the interpreter changed between
them, and what each entry produced — and `scripts/emu/jit-image-bench.mjs`
replayed that recording in both engines against the **same block set** emitted
at every size. A replay is an identity check as well as a stopwatch, and every
row below is `"identity":"checked"`.

`render-basic` t2, 201,244 blocks, 12,000 entries / 623,284 guest instructions
per iteration, desk load average 11–17:

| blocks/fn | functions | module | largest body | crosses /1k instr | **bun steady** | bun 1st sec | v8 (opt) | v8 (baseline) |
|---:|---:|---:|---:|---:|---:|---:|---|---:|
| 64 | 3,145 | 64.8 MB | 133,939 B | 64 | **9.16** | 10.84 | — | 7.20 |
| 128 | 1,573 | 64.1 MB | 238,611 B | 60 | **15.91** | 17.66 | 6.84 | 7.16 |
| 256 | 787 | 64.3 MB | 405,659 B | 59 | **10.86** | 17.90 | 18.01 | 6.91 |
| 512 | 394 | 64.4 MB | 599,964 B | 57 | **20.87** | 23.56 | **OOM** | 7.02 |
| 1,024 | 197 | 64.4 MB | 1,129,053 B | 57 | **16.91** | 19.72 | **OOM** | 7.10 |
| 2,048 | 99 | 64.4 MB | 1,948,594 B | 39 | **18.11 / 27.08** | 18.39 | **OOM** | 6.05 |
| 4,096 | 50 | 64.3 MB | 2,783,062 B | 38 | **20.64** | 21.14 | **OOM** | — |
| 8,192 | 25 | 64.4 MB | 4,382,997 B | 38 | **17.07** | 24.25 | **OOM** | — |
| 12,288 | 17 | 64.4 MB | 5,989,893 B | 37 | **14.87 / 14.98** | 19.28 | **OOM** | — |

ns per guest instruction. Emit is 0.50–0.56 s at every size; instantiate is
0.14–0.27 ms in bun and ~1 ms in node; compile is **180–242 ms in bun** for a
64 MB module (2.9 ms/MB) and 36–42 ms in node.

Three readings, and the third is the finding:

1. **JavaScriptCore has no preference this desk can measure.** 9–27 ns across a
   192× range of function sizes, and re-running one size gives 18.11 then
   27.08. The spike's flat-`br_table` result is not contradicted; nothing about
   *dispatch* argues for a size.
2. **The cross-function edge rate is 37–64 per thousand guest instructions**,
   and it stops improving above 2,048 blocks a function. The edges that remain
   are between the mask ROM, HP SRAM and the flash-cache window, which no
   contiguous chunking can put in one function.
3. **V8's optimizing tier dies at 512 blocks a function and every size above**
   — `Fatal process out of memory: Zone`, inside `WasmLoweringPhase`, after the
   module has already compiled and instantiated. It survives at 128 and 256.
   Its **baseline** tier compiles every size in 36–42 ms and runs them all at a
   flat 6.05–7.20 ns, which is what proves the JSC curve is a tiering effect
   and not a dispatch-shape one.

So the default is **256**: the largest size at which every engine measured runs
the module in its optimizing tier. Bigger buys a lower cross rate and nothing
else that could be measured, and costs one engine entirely.

> **Amended by P6b.** The table above stops at 64 because
> `lp-emu-esp32c6`'s `MIN_BLOCKS` was 64 and nothing smaller could be asked
> for. It is 8 now, the range below 64 is measured, and the shape of the
> answer changes: **V8's `Zone` OOM bounds the size from BOTH sides**, and
> JavaScriptCore's curve was still falling all the way down. See the next
> section. Whether 256 stays the default is the G-M7P gate's decision, not
> this file's.

**No sub-dispatcher may exceed 80 % of the limit** (`dispatch::BODY_BUDGET`,
6,123,456 B). It is a test, not a habit: `no_sub_dispatcher_exceeds_the_body_budget`
emits 40,000 blocks in one function, asserts it is over, and asserts every
split of it is under. The host refuses a module over the budget and halves
`--jit-fn-blocks`, because a block set is not the block set a size was chosen
on — `--jit-escape-all` emits several times the bytes per block.

### Where an entry's time goes — the cost model (P6b)

G-M7P read a whole-run fit as **448 ns per entry, 57 % of a `render-basic` t2
run**, and **5.16 ns per translated guest instruction** against the spike's
1.41 ns in the same engine on region modules. P6b asked where both numbers
come from. Every number below is on `render-basic` t2 at the 5,500 ms bound
unless it says otherwise, and every row that ran a real image carried the same
UART0 sha256, `2407828f…`, as the interpreter on the same binary.

**An entry, measured rather than fitted** (`LP_EMU_JIT_ENTRY_TIME=64`, native,
wasmtime, one entry in 64 sampled, 61,448 broken-down and 61,447 control
samples of 7,865,280 entries):

| | ns | share |
|---|---:|---:|
| the whole of `TranslatedCore::run` — an entry **and the stay it runs** | **387.9** | 100 % |
| the module call (`enter`) | 305.3 | 78.7 % |
| the host's two `BTreeMap` entry-index lookups | 24.5 | 6.3 % |
| the 32-register copy to the exchange area and back | 6.1 | 1.6 % |
| the refusal rules, the counters and the exit bookkeeping | 52.0 | 13.4 % |

So **the hart's own re-entry path is 82.6 ns**, a fifth of what a stay costs,
and a third of that fifth is two ordered-map lookups — one of which
(`index.contains_key(&exit.pc)`) exists only to feed the report's
`exits known/unknown` counters. The same run reported the same 7,865,280
entries, 31,054,609 crosses and `stopped after` line as the uninstrumented one,
so the timer changed nothing but the clock.

The breakdown's own six `Instant::now()` calls are not free: the same entry
measures 387.9 ns with only the outer clock pair and 585.5 ns with the
breakdown in it — about 20 ns a clock read, which is why the control sample
exists and why each inner figure above has one read (≈20 ns) taken off it.

**The module's own share, in both engines**, from
`scripts/emu/p6b-entry-split.mjs` — one recording, cut into 100 windows of 200
consecutive entries that differ 40× in instructions per entry (26.4 to
1,070.8), least-squared, with the JavaScript rig's own per-entry cost measured
separately (`P6B_NO_RUN=1`) and subtracted:

| | per entry | per translated instruction |
|---|---:|---:|
| node/V8, 64 blocks/fn | 70.7 − 31.5 = **39.2 ns** | 1.401 − 0.049 = **1.35 ns** |
| bun/JSC, 64 blocks/fn | 388.5 − 62.4 = **326.1 ns** | 11.204 − 0.032 = **11.17 ns** |

**1.35 ns per guest instruction in V8 is the spike's number** (1.41 ns on a
region module in the same engine). The translated code this crate emits is not
slower than the spike's; what G-M7P read as a per-instruction gap is somewhere
else.

**The cross-function hop, priced on its own** — `scripts/emu/p6b-hop.mjs`
against the `a_ring_emitted_whole_and_one_block_to_a_function_differs_only_in_crosses`
fixture, which is one ring of 64 guest blocks emitted whole (no crosses) and
one block to a function (a cross per block), asserted under wasmtime to retire
the same instructions for the same cycles and leave the same registers:

| the ring's blocks touch | node/V8 | bun/JSC |
|---|---:|---:|
| one guest register | **3.2 ns** | **2.0 ns** |
| all 31 | **15.6 ns** | **7.5 ns** |

`live_regs_in` is per sub-dispatcher, so a hop costs what the two chunks'
live sets cost: the wide ring is the hop a real image pays. At 31,054,609
crosses against 507,411,830 translated instructions that is **0.48 s of a 9.6 s
V8 run, about 5 %** — real, and not the story.

**Locals do not grow with the split.** `scripts/emu/p6b-module-anatomy.mjs`
over the same block set emitted at six sizes:

| blocks/fn | sub-dispatchers | locals per sub-dispatcher | largest sub-dispatcher | **the selector** |
|---:|---:|---:|---:|---:|
| 8 | 25,156 | **46** (43 i32, 3 i64) | 40,244 B | **730,452 B** |
| 16 | 12,578 | **46** | 69,521 B | **351,947 B** |
| 32 | 6,289 | **46** | 83,241 B | 175,855 B |
| 64 | 3,145 | **46** | 132,815 B | 87,823 B |
| 128 | 1,573 | **46** | 238,611 B | 45,380 B |
| 256 | 787 | **46** | 405,659 B | 22,586 B |

`body_locals()` is a fixed list and the bytes agree: 46 slots in 6 groups at
every size. A 256-block function carries exactly the locals an 8-block one
does, so wasm's zero-every-local-at-entry rule is a constant, not a size
effect.

That table also holds a fact `emit_module` does not report: **`max_body_bytes`
covers the sub-dispatchers and not the selector**, and below 64 blocks a
function the selector is by far the largest function in the module. It is what
V8 refuses at the small end — every sub-dispatcher at 16 blocks a function is
smaller than functions V8 optimises happily at 64, and the only thing bigger is
the selector. `node --liftoff-only` runs the same module without complaint, so
it is the optimizing tier and nothing else. **A `BODY_BUDGET` check that does
not include the selector has a blind spot at the small end**, and it is
recorded here rather than fixed, because fixing it is a size the gate has not
chosen yet.

**The size sweep, on real runs, one invocation per engine, interleaved and
best-of-3** (`scripts/emu/p6b-rows.mjs`; `render-basic` t2, 5,500 ms; identity
green on every row):

| blocks/fn | node/V8 real time | bun/JSC real time |
|---:|---:|---:|
| 8 | **process aborts** (`Zone` OOM, `WasmLoweringPhase`, background tier-up) | **0.489×** |
| 16 | **process aborts** | 0.453× |
| 32 | **0.631×** | 0.287× |
| 64 | 0.458× | 0.136× |
| 128 | 0.383× | — |
| that engine's own `--interpreter` | 0.377× | 0.528× |

Read it as two different answers:

- **V8 wants the smallest size it can survive, and that is 32.** 0.631× against
  an interpreter at 0.377× is **1.68×**, and a quieter invocation of the same
  sweep gave 0.645× against 0.499×, **1.29×**. Either way 32 beats the 64 the
  gate measured, and 16 is not available: V8 does not refuse it, it calls
  `FatalProcessOutOfMemory` from a background compile job, which no `install`
  retry can catch.
- **JavaScriptCore wants smaller still**, and the size knob is worth **3.6×**
  to it: 0.136× at 64 blocks a function, 0.489× at 8. That does not overturn
  G-M7P's JSC verdict — 0.489× against its own interpreter's 0.528× is 0.93×,
  still a shade behind — but it turns "four times slower than its own
  interpreter" into "level with it", on one constant.

**And the cost model that comes out of all of it is not the one the gate
drew.** A `node --cpu-prof` of a real run attributes wall clock by module, and
two images whose entry rates differ 4.5× say the same thing:

| | `render-basic` t2 | `render-rocaille` t2 |
|---|---:|---:|
| entries | 7,865,280 | 1,977,249 |
| translated instructions | 507,411,830 | 569,755,861 |
| **the translated modules' share of the run** | **39.3 %** | **46.6 %** |
| the emulator's own wasm | 55.4 % | 47.8 % |
| module ms ÷ entries | 642 ns | 1,744 ns |
| module ms ÷ translated instructions | 9.96 ns | 6.05 ns |

An image that enters translated code **4.5× less often per instruction** does
not spend a smaller share of its run inside the module — it spends a larger
one. Read as a per-entry cost the two runs disagree by 2.7×; read as a
per-instruction cost they disagree by 1.6×, on a desk whose load moved between
them. Fitting `a × entries + b × instructions` to a profiled pair taken
thirteen seconds apart gives **+325 ns an entry**, and to a pair taken a minute
apart gives **−113 ns an entry**. Two images and two unknowns is an exactly
determined system with no residual to check, and on this desk it is not stable
enough to carry a conclusion.

So: **448 ns per entry is not a measured quantity.** What is measured is
82.6 ns of hart per entry, 39 ns of module per entry in V8, 1.35 ns per
translated instruction in V8 — and a real run in which the module's time scales
with instructions and not with entries, and in which the emulator's own wasm,
not the translator's output, is the larger half.

### The other half — where the emulator's own wasm goes (P6c)

P6b established that the larger half of a translated run is **the emulator's
own wasm**, and that nothing in the translator can move it. P6c profiled that
half by Rust function. `render-basic` t2, node/V8, 5,500 ms emulated, 32 blocks
a function, `scripts/emu/p6c-prof.mjs` (the `wasm32-wasip1` build's name
section demangled through `rustfilt`, and every sample classified by its
ancestry as well as its leaf, so the run's own translation work is not
attributed to the bus):

| bucket | ms | % run | % of the emulator's wasm |
|---|---:|---:|---:|
| **translation (emit/compile/install)** | 1,759.7 | 22.03 | 36.05 |
| hart slice loop (`Esp32C6Machine::run_until`) | 798.7 | 10.00 | 16.36 |
| entry path (`JitCore::run`) | 439.2 | 5.50 | 9.00 |
| scheduler (`Scheduler::next_deadline`) | 351.1 | 4.40 | 7.19 |
| MMIO dispatch (bus routing) | 301.7 | 3.78 | 6.18 |
| pin fabric + LED strip | 212.5 | 2.66 | 4.35 |
| interpreter (the 6.54 % remainder) | 165.2 | 2.07 | 3.38 |
| entry index (two `BTreeMap` lookups) | 144.3 | 1.81 | 2.96 |
| MMIO dispatch (host callback) | 134.6 | 1.69 | 2.76 |
| other (emulator wasm) | 120.4 | 1.51 | 2.47 |
| peripheral: RMT | 104.6 | 1.31 | 2.14 |
| allocator + runtime | 86.3 | 1.08 | 1.77 |
| translation (emit, off `install`'s stack) | 83.6 | 1.05 | 1.71 |
| peripheral: SYSTIMER | 57.3 | 0.72 | 1.17 |
| peripheral: GPIO/IO_MUX | 55.5 | 0.70 | 1.14 |
| peripheral: INTMTX/INTPRI | 51.5 | 0.64 | 1.05 |
| WASI I/O | 10.6 | 0.13 | 0.22 |
| peripheral: other / TIMG / UART0 | 5.9 | 0.07 | 0.12 |
| **total** | **4,881.3** | **61.11** | **100.00** |

**Two things this overturns.** The peripherals are not the story — every
peripheral model together is 275 ms, 3.4 % of the run. And **22 % of a
`render-basic` run is the emulator translating itself**: three translation
events, `wasm_encoder::Instruction::encode` the second-hottest function in the
process. That is JD20's boot cost, and it is the single largest bucket in the
emulator's half.

### The MMIO census: it is SYSTIMER, and nothing else is close

`LP_EMU_JIT_MMIO_CENSUS=1` counts every MMIO operation translated code issues,
by peripheral, register and guest pc. `render-basic` t2, 32 blocks a function:
**14,663,423 operations** (10,390,586 loads, 4,272,837 stores) over 1,046
distinct registers and 794 distinct guest pcs — one MMIO operation per 34.6
translated instructions.

| peripheral | operations | share |
|---|---:|---:|
| **SYSTIMER** | **11,626,419** | **79.29 %** |
| RMT | 1,808,592 | 12.33 % |
| PLIC_MX | 440,006 | 3.00 % |
| INTERRUPT_CORE0 | 251,535 | 1.72 % |
| TIMG0 | 210,714 | 1.44 % |
| (unmapped) | 158,016 | 1.08 % |
| USB_DEVICE | 51,079 | 0.35 % |
| everything else together | 117,062 | 0.80 % |

and three registers are 79.3 % of the whole census:

| register | operations | share |
|---|---:|---:|
| `SYSTIMER+0x0044 unit0_value.lo` | 4,704,617 | 32.08 % |
| `SYSTIMER+0x0004 unit0_op` | 4,599,074 | 31.36 % |
| `SYSTIMER+0x0040 unit0_value.hi` | 2,322,728 | 15.84 % |

with five consecutive guest pcs — `0x420804d6`, `0x420804d8`, `0x420804e0`,
`0x420804e4`, `0x420804e6` — issuing 79.3 % of all MMIO between them. That is
one `SystemTimer::now()` sequence: latch `unit0_op`, read `hi`, read `lo`.

**The ladder's two named suspects are not it.** The RMT refill is 12.33 % and
the UART0 TX-FIFO poll is **0.06 %** (8,254 operations in a whole run).

### The published-read path: what a peripheral fast path has to refuse

**M7b P3.** 79.3 % of the census is `SystemTimer::now()`, and the run makes
**2,322,975 of those calls in 5.5 s of guest time — 420,000 a second, one
every 234 translated instructions.** esp-hal's `read_count` is five accesses:
a `unit0_op` store carrying `update`, then reads of `unit0_op`,
`unit0_value.lo`, `unit0_value.hi` and `unit0_value.lo` again.

**The sequence is pure only as a sequence.** `unit0_value` does not compute
anything: `Systimer::read_word` returns the stored field `latched[u]`, and the
latch is written in exactly one place — `write_word`'s `unit_op` arm,
`latched[u] = count(u, cx.now)`. `unit_op` itself reads the constant
`OP_VALUE_VALID` and nothing else.

So what translated code serves is the four **reads**, and a module may serve
them because the machine **publishes** the model's own latched word:

| word | what |
|---|---|
| `armed` (`i32`) | non-zero while the published words are current |
| the published `i32`s | `Systimer::value_words(0)` — the *same expressions* `read_word`'s own arm answers with, because that arm calls this method |
| `served` (`i64`) | reads translated code answered itself, counted by translated code |

`$fast_load` is one private function of the emitted module with
[`HostOps::mmio_load`]'s exact signature, emitted past the selector so every
other function index is unchanged. An MMIO load's call site is byte-for-byte
what it was; only the callee index changed, which costs **one byte per load
site** (`+130,425 B`, +0.17 % of module) because the callee's LEB index got
longer. The permission table is untouched: `SocBus::permission_table` is the
bus's honest view of memory and every other consumer depends on that.

**The store is not served.** Serving it would mean skipping
`SocBus::write_mmio`'s unconditional `sideband = true` and the polling point
(c) that M7b P2 fused into the store's own crossing — a point the interpreter
runs after *every* MMIO store. There is no way to serve the store inline and
still claim the poll ran where the interpreter's did without paying a crossing
anyway. Leaving it alone is also what makes the rest exact: the host
republishes on that crossing, so what a module reads is the model's value
rather than a re-derivation of it, and `offset[0]`, `frozen[0]`, `CONF` and
`unit0_load` never enter the argument. **A `conf` write that stops the unit
changes what the next latch stores; it cannot change a word already latched.**

#### The refusals are the design, not an optimisation detail

`armed` is clear — and the read goes out through the import and is served by
the bus with its trace line, its grade check, its census note and its
watchpoints — whenever **any** of these holds. Every one is a *refusal*, none
is an assertion, and refusing is always safe.

| condition | why |
|---|---|
| a trace is running | `read_mmio` emits an `MmioEvent` per access and the oracle compares those lines; an inline read emits none |
| a strict grade is set | `check_grade` runs on every access and can refuse one before the peripheral sees it |
| `--strict-bus` | it watches guest stores for the missing-fence checker and shortens the slice cap |
| the interpreter has run since the last exit | cleared at every `JitCore::run` entry: a stay only ever trusts a word it saw published **inside itself** |
| an instruction escaped | cleared in `step_one`, because an escaped instruction can be a store to `unit_op` |
| the address is not published, or the load is not a **word** | `Peripheral::read` serves sub-word lanes through `lane_of`, and unit 1 is a different register |
| the block is not where this machine expects it, or is reached through an **alias** | an alias is a second address for the same register that this path cannot see, so nothing is published at all |

`JitCore::run`'s own entry rules already cover the rest: no load watchpoints,
at most one store watchpoint, and `Bus::fetch_is_pure` — which is where a `t3`
memory-cost model and an execute watchpoint are refused.

A **yield** is deliberately *not* a condition. `$fast_load` answers `MMIO_OK`
where the import might have answered `MMIO_PENDING`, and that is observable
only if `Bus::yield_now` is set while the stay's `FLAG_PENDING` is clear —
which cannot happen inside a stay: entry refuses on
`sideband_or_yield_pending()`, an MMIO load that leaves a yield answers
`MMIO_PENDING` and sets the flag itself, and a store the bus served has its
polling point take the yield and end the stay.

**The trace refusal is what the free oracle's 20 ms cells measure.** They run
with `--trace`, so in those four cells the path never arms and what they prove
is the *refusal*. The path itself is proved by four more cells at a 500 ms
window without a trace, and by the 5,500 ms browser identity rows.

#### What it was worth

`render-basic` t2, `LP_EMU_JIT_MMIO_CENSUS=1`, 16 blocks a function:

| | before | after |
|---|---:|---:|
| MMIO operations translated code issued | 14,740,602 | **5,881,883** |
| SYSTIMER | 11,626,424 (78.87 %) | **2,767,705 (47.05 %)** |
| RMT — the new leader | 1,811,925 (12.29 %) | 1,811,925 (**30.81 %**) |
| SYSTIMER loads | 9,303,449 | 444,730 |
| stores, every peripheral | 4,311,988 | 4,311,988 |

**8,858,719 reads — 95.2 % of the sequence's loads, 60.1 % of the whole
census — never leave the module**, and the 4.8 % residue is the loads that
happened while the block was disarmed. The exit census does not move by one
exit: this removes host *crossings*, not exits.

`render-basic` t2 goes **0.862× → 0.902×** in node/V8 (−280 ms) and **0.757× →
0.782×** in bun/JSC (−240 ms), which prices an MMIO crossing at **31.6 ns in
V8 and 27.1 ns in JSC**.

That 1.17 ratio is the part worth remembering, because **an exit's is 23**
(61 ns in V8, 3 ns in JSC — the section below). A translated core's two host
costs do not scale together across engines, and a phase that reasons about one
from the other will be wrong by an order of magnitude in the engine the phone
runs.

### The module's time in a real run, against the same module replayed

P6b measured 1.35 ns per translated instruction replaying windows, and 9.96 ns
in a real run at 64 blocks a function — a 7× gap nobody had explained. It is
not a constant: it is a **function of how big the emitted functions are**.
Four `node --cpu-prof` runs of the same image, same stage, same bound:

| blocks/fn | largest body | the module's ms | ns per translated instruction | warm replay, same sizes | ratio |
|---:|---:|---:|---:|---:|---:|
| 8 | 40,685 B | 2,301.9 | **4.54** | 2.004 | 2.26× |
| 16 | 52,098 B | 2,259.9 | **4.45** | 1.811 | 2.46× |
| 32 | 74,598 B | 2,608.2 | **5.14** | 1.626 | 3.16× |
| 64 | 140,530 B | 3,432.4 | **6.76** | 1.698 | 3.98× |

and the emulator's own wasm is **flat at 5.17–5.22 s** across all four, which
is the control: the size knob moves the module's time and nothing else's.

The cold-against-warm test says the same thing from the other side.
`jit-image-bench.mjs` replays one recording and reports the first pass beside
the steady state:

| blocks/fn | first pass (cold) | steady (warm) | cold ÷ warm |
|---:|---:|---:|---:|
| 8 | 2.370 | 2.004 | 1.18× |
| 16 | 2.440 | 1.811 | 1.35× |
| 32 | 3.448 | 1.626 | 2.12× |
| 64 | 8.976 | 1.698 | **5.29×** |

**So the gap is the working set, and it is bounded by the function size.** A
window replay runs the same few thousand entries over and over: one small,
hot, fully-tiered slice of a 65 MB module. A real run walks 201,244 blocks
across the whole of it. Both halves of the gap shrink together as the
functions shrink — which is the same direction DD20 moved the default for
entirely separate reasons — and about **2.3× of it survives at 8 blocks a
function**, unexplained by size.

### What a module-side entry is made of (P6c Q4)

`scripts/emu/p6c-jsc-entry.mjs` is a ladder of five modules, each the previous
one plus one thing, all exporting the real selector's signature:

| rung | node/V8 ns | bun/JSC ns |
|---|---:|---:|
| A the JS→wasm call boundary | 2.09 | 4.35 |
| B + the selector's six locals | 25.13 | 7.18 |
| C + the exchange prologue | 25.24 | 8.07 |
| D + `call_indirect` into a 46-local body | 24.90 | 9.41 |
| E + the 31-register reload and write-back | **24.95** | **11.28** |

Rung E is a whole entry that retires no guest instruction, so it is the floor
under every real entry. **In JSC that floor is 11.28 ns against a measured
module-side entry of 329 ns** — the entry protocol is 3.4 % of it. The
selector's prologue, the locals and the register traffic together cost 6.9 ns
in JSC and 0.2 ns in V8, and none of them is the lever the brief expected.

What the 318 ns remainder *is* tracks the module, not the protocol: on one
recording, `p6b-entry-split.mjs` in bun reads **385.4 ns per entry at 64
blocks a function and 196.4 ns at 8** (56.3 ns of harness in both, measured
with `P6B_NO_RUN=1`) — the same entries, the same guest work, the same import
answers, halved by the size knob alone.

(V8's rung A is 2.09 ns because V8 inlines a small wasm body into optimized
JS. Every rung carries one memory load so that no rung is a constant; without
it the bare rung read 4.6 ns against 60 ns for every rung above it.)

### The selector's shape, and V8's low-end wall (P6c Q5)

P5's selector is `count` nested `block`s around a `br_table`, one direct
`call` per arm — **O(count)** bytes, and `count` is `blocks / fn_blocks`, so
the selector grows as the size knob shrinks. At 8 blocks a function on
`render-basic` t2 that is a **730,452-byte function**, and V8's optimizing
tier answers it with `FatalProcessOutOfMemory: Zone` in `WasmLoweringPhase`,
from a background compile job no `install` retry can catch.

`Selector::Flat` is one `call_indirect` through a function table with one
entry per sub-dispatcher. In the same block set it is **152 bytes at every
size**:

| blocks/fn | sub-dispatchers | largest sub-dispatcher | nested selector (P6b) | flat selector |
|---:|---:|---:|---:|---:|
| 8 | 19,318 | 40,685 B | 730,452 B | **152 B** |
| 16 | 9,659 | 52,098 B | 351,947 B | **152 B** |
| 32 | 4,830 | 74,598 B | 175,855 B | **152 B** |
| 64 | 2,415 | 140,530 B | 87,823 B | **152 B** |

**It removes the wall outright.** V8 runs 8 and 16 blocks a function, which it
could not before, and 16 is where it is fastest.

`Emitted` now reports `max_sub_body_bytes` and `selector_bytes` beside a
`max_body_bytes` that is the larger of the two, so `BODY_BUDGET` — the check
that lets `install` refuse a module and retry smaller — sees the selector. It
did not before, and below 64 blocks a function the selector was the module's
largest function.

### Where 3× would have to come from

The milestone asks for **3× real time**: 5,500 ms emulated in **1,833 ms** of
wall clock. The best this desk has produced is **7.64 s (0.720×)** —
`render-basic` t2, node/V8, 16 blocks a function, flat selector, best of three
interleaved repeats at load 10.7–14.8. The decomposition below is the profiled
run of that same configuration (8,056.2 ms attributed, load 11.7), which is
4.40× off the target.

| where the wall clock goes | ms | % run | ÷4.40 (its share of 1,833 ms) | the lever, if there is one |
|---|---:|---:|---:|---|
| **the translated modules** | 2,259.9 | 28.05 | 514 | smaller functions (4.45 ns/instr at 16 against 6.76 at 64); the 2.3× that survives is unexplained |
| **translation (emit + install)** | 2,058.4 | 25.55 | 468 | JD20. Three events a run; incremental `fence.i` translation is M7b's |
| hart slice loop (`run_until`) | 744.8 | 9.25 | 169 | the slice cap is 8,192 cycles and 34.4 % of exits are budget exits |
| entry path (`JitCore::run`) | 438.0 | 5.44 | 100 | 7,865,280 entries; `after_store` alone is 53.2 % of exits |
| scheduler (`next_deadline`) | 426.2 | 5.29 | 97 | called per slice, not per entry |
| MMIO dispatch (routing + callback) | 405.7 | 5.04 | 92 | 14.66 M operations, 79.3 % of them SYSTIMER |
| the JavaScript rig (host, WASI shim) | 374.8 | 4.65 | 85 | the import boundary |
| peripherals, all of them together | 277.0 | 3.44 | 63 | **not a lever** — 3.4 % of the run |
| pin fabric + LED strip | 222.8 | 2.77 | 51 | per RMT refill |
| entry index (two `BTreeMap` lookups) | 201.5 | 2.50 | 46 | one of the two exists only to feed a counter (P6b) |
| garbage collector | 189.0 | 2.35 | 43 | the 65 MB module byte vectors |
| interpreter (the 6.54 % remainder) | 161.7 | 2.01 | 37 | coverage is 93.46 % |
| other (emulator wasm) | 152.8 | 1.90 | 35 | — |
| allocator + runtime | 118.5 | 1.47 | 27 | — |
| WASI I/O, idle, program | 25.2 | 0.31 | 6 | — |
| **total** | **8,056.2** | **100** | **1,833** | |

**What the arithmetic says, before any lever is argued.**

1. **No single bucket reaches it.** Delete *all* translation — a perfect
   incremental `fence.i`, boot for free — and the run is 5,997.8 ms, **0.917×**.
2. **Nor do the two largest together.** Delete translation *and* every
   instruction of translated code, and the run is 3,737.9 ms, **1.47×**.
3. **The emulator's own steady-state wasm is the binding constraint.** With
   translation taken out it is 3,251.8 ms. Even at zero module, zero rig, zero
   GC, that alone is **1.69×**. So the emulator's own non-translation wasm has
   to fall by **1.78×** before 3× is arithmetically reachable at all, whatever
   happens to the translator.
4. **The peripherals are not where to look.** Every peripheral model together
   is 277 ms, 3.4 % of the run — and the SYSTIMER that answers 79.3 % of all
   MMIO is 80.1 ms of it.

**So 3× is a whole-machine number, not a translator number.** The four
candidates it would have to be assembled from, in the order their measured
size puts them:

| candidate | what it is worth here | measured by |
|---|---:|---|
| ~~incremental `fence.i` translation~~ **done (M7b P1): 1,050 ms of the 2,058, and 400 ms of it handed back at the module boundary** | up to 2,058 ms (25.6 %) | this table; JD20 |
| the module's remaining 2.3× over a warm replay | up to ~1,250 ms (15.5 %) | the cold/warm table above |
| the exit rules (`after_store` is 53.2 % of exits, buying 0.68 instructions each) | up to ~830 ms of entry path + hart loop | P6b's census |
| the slice loop and the scheduler, which run per slice and not per entry | 1,171 ms (14.5 %) | this table |

All four, taken in full, are 5,309 ms of 8,056 — **2.94×**, and every one of
them is an upper bound that assumes the work disappears rather than shrinks.

### Indirect targets, in O(1), across functions

Every guest **return** is a `jalr`, and P3 left the module at every one of
them. Two flat tables in the arena's largest gap, beside the permission table,
make it a branch instead: a **page map** over the whole 32-bit space at the
permission table's own 16 KiB granularity, and a **slot array** per page that
holds a block start — 8,192 `i32`s, one per two bytes, holding the global block
index or `-1`. Every page with no block start points at one shared array of
`-1`, so a wild address needs no bounds check and no branch of its own. Two
loads, no search.

On `render-basic` at the last translation event the tables are **7,307,264 B**
(1 MiB page map plus 1 KiB… 32 KiB per populated page); on `render-rocaille`,
7,176,192 B. Two-byte granularity is not an economy — 48.99 % of real block
starts sit at 2 mod 4.

**Per module since M7b P1.** A slot holds a *block index*, and a block index
only means something inside the module it was emitted with, so two live modules
need two sets of tables and the second set goes in its own slice of the same
gap. That gap is **218,103,808 bytes** on the C6 — between the mask ROM's data
and HP SRAM — and a `render-basic` run with two modules live uses 8,388,608 of
it and leaves 209,452,800. The permission table and the exchange area are not
duplicated: every module's `Layout` folds in the same offsets for them, and
they hold the same bytes whichever module is reading.

## Incremental translation at `fence.i` (M7b P1, DD18)

JD5 has two translation events: boot, and every `fence.i`. Until M7b P1 both
of them did the same thing — walk the whole image, emit 60–65 MB of wasm and
ask the engine to compile it — and `render-basic` t2 does that **three times**,
which P6c priced at **22.0 % of the run**.

A `fence.i` is the guest saying "I wrote some code". Almost nothing else
changed, so almost nothing else needs re-emitting. What stops that being
obvious is the **whole-module retire**, three sections up: a block whose bytes
changed makes the module holding it stale, because that block's body is
reachable from every other block's compiled-in edges. On `render-basic` the
first `fence.i` changes **four blocks of 154,544** — mis-swept data in HP SRAM
that nothing executes — and under one module those four words retire the whole
image.

**So the image is split at boot on whether the guest can write it.**

| module | what it holds | what a `fence.i` does to it |
|---|---|---|
| read-only | the flash-cache window and the mask ROM — 96 % of the image | nothing: those bytes cannot change, so it can never go stale |
| writable | HP SRAM, LP SRAM, and every byte the guest publishes | retired and replaced **whole**, exactly as the single module was before |

The retire is unchanged; it applies to the only module whose bytes a publish
can have touched. Three things make the split exact:

1. **One module answers for any pc.** The walk that builds the replacement is
   given the read-only module's block starts as a **stop set**
   (`discover::discover_from`): a seed in it is skipped, an edge into it is
   neither followed nor claimed, and a block being built ends there. Two
   modules answering for one pc would give the hart's `EntryIndex` two answers.
2. **Each module has its own target tables**, for the reason the section above
   gives.
3. **Every edge out of a module is an ordinary exit** — `why::EDGE_OUT` or
   `why::INDIRECT_MISS` — and the hart re-enters through the entry index,
   landing in whichever module holds the target. Cross-module control flow
   costs one host entry each, and that is the whole price of the split.

**What it costs, measured before it was built.**
`LP_EMU_JIT_SPLIT_CENSUS=writable` with `--interpreter --blockprof` classifies
every block dispatch by which side of a proposed boundary its pc is on and
counts the ones that change side. On `render-basic` t2 it predicted
**3,770,996** crossings; the run delivers **11,843,442** exits against
7,865,280, which is **3,978,162** more — the census was within 5 %. Almost all
of it is `indirect-miss` (288,065 → 3,990,254): a `jalr` that used to resolve
through the target table into the same module now resolves to `-1` and leaves.

**What it buys.** `render-basic` t2, node/V8, 16 blocks a function:

| event | before: emit / compile | after: emit / compile |
|---|---:|---:|
| boot | 753.4 ms / 36.4 ms | 817.3 ms / 44.0 ms |
| `fence.i` #1 | 513.2 ms / 43.4 ms | **50.4 ms / 6.2 ms** |
| `fence.i` #2 | 536.9 ms / 43.7 ms | **70.5 ms / 5.8 ms** |

Boot costs more, because it now emits two modules and two sets of tables. Each
`fence.i` costs a tenth of what it did. Coverage rises from **93.46 % to
97.37 %** as a side effect: the replacement walk re-finds the writable side at
every event rather than once.

**The whole-module retire's other end.** A module that goes stale stops being
entered and the run continues interpreted until the next event replaces it —
per module now, rather than per core. That is sound for the same reason the
retire exists: entering a module can only run *that* module's blocks, because
every edge out of one is an exit.

**The function table is not grown.** The obvious alternative — keep one module
and add the new code to it — would need the flat selector's table to reach a
function that did not exist when the module was compiled. It cannot: the table
is declared `maximum = Some(count)` so that an engine can fold its bounds
check, and JavaScriptCore's fused `table.grow(delta, ref)` accepts a new entry,
reports the funcref back from `table.get`, and then traps on `call_indirect`
against that same slot (P2, S4). A shared *imported* table would be a different
design, not a smaller change.

### The seeding scope (JD26, item 4)

`--jit-seeds`. Measured at the last translation event, emit-only:

| image | scope | blocks | instructions | module | installed coverage |
|---|---|---:|---:|---:|---:|
| `render-basic` | all-symbols | 201,237 | 705,529 | 64.4 MB | **93.46 %** |
| `render-basic` | entry-reachable | 47,322 | 54,295 | 6.6 MB | **18.75 %** |
| `render-rocaille` | all-symbols | 195,962 | 701,472 | 63.7 MB | **96.51 %** |
| `render-rocaille` | entry-reachable | 39,194 | 43,836 | 5.4 MB | **19.36 %** |

Coverage decides and it is not close, so **all-symbols** stays the default. At
the *boot* event entry-reachable finds **one block and two instructions** —
rule 1 of the discovery section, verbatim: `_start` runs into a CSR write the
translator refuses and the walk stops, and the trap vector is still zero. What
it finds at a `fence.i` is almost all published-code sweep: 47,322 blocks
holding 54,295 instructions is 1.15 instructions a block, which is what a walk
seeded on data looks like.

### `LP_EMU_JIT_PAD`

JD1 carried the spike's "dispatcher-scaling knob" onto M7's list, and P5 is
where it would have been used. It is **not here, and it is not coming**: no
commit on `main` ever carried it, and what it did — grow a `br_table` with
unreachable copies of a region's blocks — is worse than what replaced it.
`--jit-fn-blocks` varies the split over the **real** whole-image block set, so
every row of the sizing table above is real blocks running a real recording,
with identity checked in both engines. The spike's own caveat was that above
~10,000 blocks its padding harness emitted malformed bodies; a knob whose
large sizes are known-broken is not the instrument for a question about large
sizes. Nothing to bound and nothing to fix — it was replaced.

## The after-store exit, and what it was worth (M7b P2, DD17)

The mechanism is in "Polling after a store" above. This is what the phase
measured, and the number is smaller than the milestone plan projected — which
is the more useful half of the finding.

**The exit census**, `render-basic` t2, 16 blocks a function, node/V8,
`--jit --jit-report`, one invocation per stage:

| why | before | share | instr after | after | share | instr after |
|---|---:|---:|---:|---:|---:|---:|
| after-store | 4,245,005 | 35.8 % | 3,085,814 | **1** | 0.0 % | **0** |
| budget | 2,714,116 | 22.9 % | 7,688,630 | 2,834,596 | 36.7 % | 7,689,411 |
| indirect-miss | 3,990,254 | 33.7 % | 2,468,809 | 3,993,292 | 51.7 % | 2,468,874 |
| undecodable | 710,536 | 6.0 % | 827,370 | 710,536 | 9.2 % | 827,370 |
| edge out | 183,531 | 1.5 % | 185,117 | 183,783 | 2.4 % | 185,369 |
| **total** | **11,843,442** | | **14,255,740** | **7,722,208** | | **11,171,024** |

The class is **gone**, not reduced: one exit in a whole run, and it is a real
one — a poll that delivered an interrupt. 4,311,988 polling points ran inside
stays, exactly the MMIO store count, and 4,311,987 of them answered "carry on".
Mean stay 44.6 → 68.9 instructions; coverage 97.37 % → 97.94 %; cross-function
transfers 41,659,754 → 41,772,422 (+0.27 %), which is what a longer stay costs.
Budget exits rise 120,480, because a stay that no longer breaks at a store
reaches more block-budget checks.

**What it bought, and what it cost.** Best-of-5 interleaved, one invocation per
engine, load quoted:

| engine | size | before | after | Δ wall | translation Δ (emit + compile + instantiate) | steady-state Δ |
|---|---:|---:|---:|---:|---:|---:|
| node/V8 25.2.1 | 16 | 6.56 s / **0.839×** | 6.44 s / **0.854×** | **−120 ms** | +131 ms | **−251 ms** |
| bun 1.1.18 / JSC | 8 | 7.76 s / **0.709×** | 7.84 s / **0.702×** | **+80 ms** | +91 ms | **−11 ms** |

Two things that were not predicted:

1. **An exit costs about 61 ns in V8 and about 3 ns in JSC.** 251 ms over
   4,121,234 removed exits is 60.9 ns; 11 ms over the same is 2.6 ns. The
   milestone plan budgeted ~600 ms for this class, which is ~145 ns an exit —
   the entry path, the entry index, the module-side entry and a `run_blocks`
   iteration added up from P6c's *instrumented* profile. The instrumentation is
   the reason: `LP_EMU_JIT_ENTRY_TIME`'s own six `Instant::now()` calls cost
   **1,618 ns** of the 2,114 ns it reports per entry, and the unclocked figure
   it also prints is 495.9 ns before and 609.2 ns after — the whole entry,
   module work included, not the part an exit pays twice.

   **JSC's near-zero answer is the one that matters for the phone**, which is
   JSC-family. Whatever P6c's 140 ns module-side entry at 8 blocks a function
   is measuring, it is not a cost a removed exit gets back.

2. **Emitted bytes are a real price.** Carrying the polling point inline costs
   about 90 bytes a store, and the module goes 68,538,511 B → 76,166,092 B
   (+11.1 %) at 16 blocks and 71,865,070 B → 79,492,518 B (+10.6 %) at 8. That
   is +131 ms of translation in V8 and +91 ms in JSC — and in JSC it is the
   *whole* of the row. The first cut of the emission was worse still
   (76,759,632 B); folding the two exits into one and hoisting the `poll` call
   into a single cold arm recovered 593,540 B and made the common path one
   `or` and one never-taken branch.

The MMIO census barely moves, as it should: **14,663,530 → 14,740,602
operations (+0.53 %)**, SYSTIMER unchanged at 11,626,424 in both. The +77,072
is the denominator, not the numerator — coverage rose 0.57 points, so slightly
more of the run's peripheral traffic is issued by translated code. Stores are
4,272,890 → 4,311,988, and the poll count equals the store count exactly, which
is the contract: one polling point per store the bus served.

SYSTIMER stayed at 11,626,424 through this phase and was the whole of the next
one — see "The published-read path" above, which takes the census to
5,881,883.

## Two more structural facts a reader will otherwise rediscover
- **The hart's entry index has to be exact once the whole image is
  installed.** It was a 64 K-slot direct-mapped filter keyed by `pc >> 1`,
  first claim wins, sized in P4 against the ~37,600 block starts a render image
  executes; a collision cost an interpreted block. 155,608 pcs into 65,536
  slots is not a filter, and coverage measured **58.87 %** with every block
  installed — the hart kept leaving translated code at a pc that *was* a block
  start and could not get back in. `lp_riscv_emu::mach::translated::EntryIndex`
  is exact now, in the same two-level shape as the target table above.
- **Translated code's inline RAM stores never reach the bus, and the
  guest-published-code watcher lives on the bus.** While only two thousand
  blocks were installed this was invisible: the shader JIT's own writer ran
  interpreted, the bus saw its stores, and the publish was seeded. Install the
  whole image and the writer is translated, the bus sees nothing, and the walk
  stops finding 12 points of the run's instructions. The machine diffs
  executable writable memory at each `fence.i` instead — word-granular, because
  a coalesced run's base is wherever the diff started and on this chip that is
  usually the stack. Found coverage 97.29 % → **99.16 %**.
- **An invalidation retires the whole module, not the blocks whose bytes
  changed.** The entry index says where the hart may *enter*; a block's edges
  to other blocks are compiled into the module, so dropping one from the
  index does not stop a surviving block branching into its body. A changed
  byte therefore makes the module stale, and JD5's second event replaces it.
- **`wasmtime::Memory::new` silently ignores a host memory creator.** It builds
  its instance with `OnDemandInstanceAllocator::default()`, which has none, so
  the module runs against a fresh zeroed allocation and nothing errors — the
  guest's memory simply is not the bus's. A module that *defines* the memory
  goes through the configured allocator, so the host instantiates a one-memory
  shim and imports that.

## What this crate is not

Not an emulator, not a host, and not a policy. It owns no bus, no machine and no
cycle counter. It does not decide when to translate, what to translate, or where
the module runs — the machine crates do that, installing a translated core
through `lp-riscv-emu`'s seam.

It also never guesses. An encoding `decode` does not recognise ends the block
and goes back to the interpreter, so an unsupported extension, a block swept
onto data-in-text and code the guest has not published yet all degrade to
interpretation rather than to a wrong answer. That escape is the reason
byte-identity was reachable in a single spike, and it is deliberate rather than
incidental.

### Two decoders, and the test that makes that safe

`lp-riscv-emu` fuses decode into execution: you cannot ask it what an
instruction *is* without also telling it to do it. A translator must, so this
crate decodes for itself — and `tests/decoder_agreement.rs` is the price. It
holds both decoders to the same `(width, InstClass)` over every 16- and 32-bit
word of both pinned render images and over the whole compressed encoding space,
and it names every encoding the two deliberately differ on. A decode divergence
does not announce itself; it shows up as a cycle count a few parts per million
off, three images later.

The sweep half runs in every `cargo test`. The corpus half needs the pinned
render images and so is `#[ignore]`d — `just test-emu-jit` is what runs it, and
once running it never skips.

## Licence posture

Everything under `lp-emu/` is **MIT** (`../LICENSE-MIT`) while the rest of the
repository is AGPL-3.0-or-later, and `just lint-emu-fence` is what keeps the
boundary real. See `docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md`.

- The only default dependency outside the fence is **`wasm-encoder`**
  (Apache-2.0 WITH LLVM-exception) — a permissive byte emitter, not a compiler.
- **`wasmtime`** is optional, behind the `host-wasmtime` feature, never a
  default, and reached from `lp-emu-esp32c6` only through *its* optional `jit`
  feature. The product host is the browser's own engine.

  JD18 requires any wasmtime host to set guard-page traps
  (`signals_based_traps(true)`, 4 GiB `memory_reservation`, 2 GiB
  `memory_guard_size`, `memory_may_move(false)`); without them every *speed*
  number is ~4.8× wrong and nothing reports an error. **P3's host cannot use
  that configuration**, and the reason is structural: JD4 gives the arena to the
  bus as a `Vec`, the module must import that memory rather than a copy, and
  `MemoryCreator`'s contract requires unmapped space after a host memory because
  cranelift elides bounds checks on the strength of it. Configuring the guard
  and handing over a `Vec` would turn a translator bug into a silent write into
  the host heap. So this host asks for explicit bounds checks and pays the
  4.8×. **The browser has no such conflict** — there the arena is a `Vec` inside
  the emulator's own linear memory and the engine's guards are already there.
  A native host that wants the guard pages has to own the arena, which is P8's
  to weigh against caching compiled modules.
- No workspace-local AGPL edge is added. In particular the translator does
  **not** use `lp-riscv-inst`, even though the fence allowlist would permit it.
  The agreement test's oracle is a dev dependency on `lp-riscv-emu`, which is
  inside the fence.
