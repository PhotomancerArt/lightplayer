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
| `translate` | the block set → one wasm function |
| `host` | what an emitted module is run against, and the exit protocol below |
| `host_wasmtime` | a native host, behind `host-wasmtime`, so identity can be proven on the desk |
| `replay` | the identity harness's record and compare shapes, including the per-entry memory-granule diff |

Splitting the module across functions is P5's, and the browser host is P6's.

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

and it returns **the guest pc to resume at**. Everything else goes through the
*exchange area*, a fixed 256-byte window of the imported memory whose offset is
folded into the module at emission time:

| offset | what |
|---|---|
| `+0`   | `regs[32]`, one `i32` each, `x0` first |
| `+128` | `mcycle` |
| `+136` | `minstret` |
| `+144` | flags — bit 0 "left after a store", bit 1 "the slice ended" |
| `+148` | the status the last `step_one` reported |

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

## Two structural facts a reader will otherwise rediscover

- **wasm caps one function body at 7,654,321 bytes.** This crate emits one
  function per block set, so a set has to fit. Measured on `render-basic`:
  17,104 blocks emit 8.06 MB and are refused, so the ceiling is ~16,000
  blocks — against the **156,053** a whole-image walk finds and the ~37,600 a
  render run executes. **Cranelift refuses far earlier**, somewhere under
  8,586 blocks, and takes 116 s to compile 4,272. Two-level dispatch (JD8) is
  P5's, and these are the numbers it is sized against.
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
