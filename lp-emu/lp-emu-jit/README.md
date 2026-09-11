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

Three imports, all from the module named `emu`, plus that module's `memory`:

| import | shape | says |
|---|---|---|
| `mmio_load` | `(pc, cycle, address, kind) -> (status << 32) \| value` | 0 ok · 1 the access did not happen, leave at this pc · 3 ok, and a yield is now pending |
| `mmio_store` | `(pc, cycle, address, kind, value) -> status` | 0 ok · 1 did not happen · 2 happened, and the hart must observe a side-band now |
| `step_one` | `(pc) -> pc` | the escape hatch: run exactly one guest instruction the way the interpreter would |

**`cx.now` discipline (JD17).** The counters live in wasm locals and are handed
back at every point the bus can observe them: as arguments to every MMIO import,
in the exchange area before every `step_one`, and in the exchange area at every
exit. Charging a block's cycles at entry is *not* exact — P1b measured
peripheral models reading `cx.now` mid-block.

**Leaving after a store.** An MMIO store always leaves, because the bus always
raises its side-band on one and the hart's polling point (c) does not move
because a block was translated. An MMIO *load* can leave a yield behind too, and
the interpreter does not look after a load — so neither does translated code,
but it remembers, and the next store leaves even if it is an inline RAM store
the bus never sees.

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

**No sub-dispatcher may exceed 80 % of the limit** (`dispatch::BODY_BUDGET`,
6,123,456 B). It is a test, not a habit: `no_sub_dispatcher_exceeds_the_body_budget`
emits 40,000 blocks in one function, asserts it is over, and asserts every
split of it is under. The host refuses a module over the budget and halves
`--jit-fn-blocks`, because a block set is not the block set a size was chosen
on — `--jit-escape-all` emits several times the bytes per block.

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
