# `lp-xt-jit` — the Xtensa half of the emulator's wasm translator

MIT, as a unit with the rest of `lp-emu/` (see `lp-emu/LICENSE-MIT` and
`just lint-emu-fence`). `publish = false`.

`lp-emu-jit` is the **ABI**; this crate is Xtensa's decoded form and Xtensa's
body emitter. That split is M7 XD7, taken on its merits after the question was
re-asked: **there is no shared mini-IR.** wasm is the IR, and what the two
architectures share is the exchange protocol, the four imports (`mmio_load`,
`mmio_store`, `step_one`, `poll`), the module shape and its selector, the
permission and indirect-target tables, the budget rule and the record/replay
shapes. Each emits wasm directly from its own decoded form.

The reason is not tidiness. The RV32 emitter is a flat match over about ten
instruction shapes; an IR would add a lowering per architecture plus a backend,
and neither architecture's hard part — windowed registers here, the `fence.i`
contract there — is expressible in a register-machine IR without exactly the
special casing the IR was supposed to remove.

## The edge to `lp-xt-inst`, and the price not paid

`lp-xt-inst` is a plain dependency with **no licence edge and no fence-lint
allowlist entry**: it was flipped to MIT in #763, so `scripts/check-emu-fence.sh`
accepts it by licence like any other MIT workspace dependency.

That is why there is no second decoder here and no `tests/decoder_agreement.rs`.
`lp-emu-jit` carries its own RV32 decoder because `lp-riscv-inst` was AGPL when
M7 P3 was written, and it pays an agreement corpus against the real executors
for the privilege. This crate calls `lp_xt_inst::decode` and adds only the
classification.

## What it does today

Everything escapes. `translate` emits the prologue, the dispatcher loop, the
per-block budget compare and one `step_one` call per guest instruction — no
guest semantics at all. It is the RV32 side's `Emit::NOTHING` build, which that
side keeps as a test because it is the one translation that cannot be wrong
about an instruction.

It is **slower than the interpreter** and it is not a product path. What it
proves is the seam: that a stay entered here leaves the machine byte-for-byte
where the interpreter would have. `scripts/emu/v3-oracle.sh --flags-a --jit
--flags-b --interpreter` is that proof at machine scale; `tests/seam_escape_all.rs`
is it at instruction scale, against a scripted interpreter rather than a hart.

## The register model

The exchange area holds the **physical** `AR[0..64]` file and eight further
words — `WindowBase`, `WindowStart`, `SAR`, `LBEG`, `LEND`, `LCOUNT`,
`PS.CALLINC`, and a dirty mask (XD8's data half, `lp_xt_jit::LAYOUT`).

The physical file, not the window, because `a3` is not a register on this
machine: it is `AR[(WindowBase * 4 + 3) mod 64]`, and a translator that cached
"a3" across an `ENTRY` would be wrong in a way no RV32 shape warns about.

**This phase writes none of those words.** With every instruction escaping the
guest state never leaves the hart: the driver overrides `HostOps::step_one_wide`
and runs `XtHart::step_one` on the hart itself, so there is no register file to
marshal and no marshalling bug to have. The layout is reserved now because P06
— sixteen window locals, the three fixed rotate sequences, writeback on the
dirty mask — is what fills it in.

## What it does not do yet

| | phase |
|---|---|
| Emitted guest semantics, the window model, the FP arms | P06 |
| The real discovery sweep (symbol extents, `l32r` targets struck out as data, `LEND` back-edges, the code-write seeds) | P05 |
| Static branch-target resolution inside the module — today every escaped terminator leaves | P05/P06 |
| The publish-by-store translation event | P07 |
| The wasm build and the browser host | P08 |
| The ESP32-S3 driver twin | P09 |

`discover` is a stub on purpose: it takes a **supplied** list of block starts
and follows decoded widths. Nothing it produces can be wrong, only short — a
start it was not given is a pc the interpreter runs.

## Running it

```bash
just test-emu-xt-jit     # the crate's tests under wasmtime
just clippy-xt-jit       # the `jit` feature's lint seat
just lint-emu-fence      # the MIT fence
```
