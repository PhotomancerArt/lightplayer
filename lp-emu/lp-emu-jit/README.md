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
| `replay` | the identity harness's record and compare shapes, including the per-entry memory-granule diff |

Translation, wasm emission, discovery and the hosts arrive in later phases.
`wasm-encoder` is declared as a dependency already because the crate's licence
posture is a decision, not an implementation detail, and a dependency that shows
up with the code that uses it is a dependency nobody reviewed.

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
- **`wasmtime`** is optional, behind the `host-wasmtime` feature, and is never a
  default and never enabled by `lp-emu-esp32c6`. The product host is the
  browser's own engine. Any wasmtime host built on that feature must set
  guard-page traps (`signals_based_traps(true)`, 4 GiB `memory_reservation`,
  2 GiB `memory_guard_size`, `memory_may_move(false)`); without them every
  number is ~4.8× wrong and nothing reports an error.
- No workspace-local AGPL edge is added. In particular the translator does
  **not** use `lp-riscv-inst`, even though the fence allowlist would permit it.
  The agreement test's oracle is a dev dependency on `lp-riscv-emu`, which is
  inside the fence.
