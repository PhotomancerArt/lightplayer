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
| `blocks` | the guest blocks a translation is *of*, and the deliberately simple walk P3 uses to find some |
| `translate` | the block set → one wasm function |
| `host` | what an emitted module is run against, and the exit protocol below |
| `host_wasmtime` | a native host, behind `host-wasmtime`, so identity can be proven on the desk |
| `replay` | the identity harness's record and compare shapes, including the per-entry memory-granule diff |

Whole-image discovery is P4's, splitting the module across functions is P5's,
and the browser host is P6's.

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

- **wasm caps one function body at 7,654,321 bytes.** P3 emits one function per
  block set, so a set has to fit; at ~400 B per translated instruction that is
  around ten thousand blocks, against the ~37,600 a render image executes.
  Two-level dispatch (JD8) is P5's.
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
