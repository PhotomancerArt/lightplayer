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

## Discovery: the nine rules, and why each one is there (M7 P05, XD9)

The RV32 walk's five rules (`lp-emu-jit/README.md` §Discovery) transfer as
*shape* and not as rules. Every rule below is numbered the same way in
`src/discover.rs` and in `tests/{discover,literal_pools,loop_end,guest_written}.rs`.

1. **Symbols are the seeds, biggest first, one at a time.** The ELF entry,
   the fifteen vector arms at `VECBASE`, every function symbol in an
   executable region of the app and then of the mask ROM (the classic's
   `lp-emu/esp/roms/esp32_rev300_rom.elf` has 2,409 sized text symbols), each
   explored to exhaustion before the next is added. With symbol seeds alone
   the walk *entered* 0.84 % of the render loop's retired instructions (P04);
   the seeds are not what moves that number, the edges and the third path are.
2. **Widths come from the decoder — 2 or 3 — never from a stride.** All four
   `pc mod 4` residues are live in equal measure (study §2.3), so a fixed scan
   is meaningless here rather than merely lossy.
3. **A block never crosses its symbol's extent.** `[st_value, st_value +
   st_size)` is the primary bound — and it is available: **every** text
   symbol in both ELFs carries an `st_size` (app 4,768 / 0 zero-sized; ROM
   2,409 / 0). A fall-through past the extent is a `Fall` into the next symbol
   when one starts exactly there and `Undecodable` otherwise; a 3-byte
   instruction straddling the extent is not the block's. Where no symbol
   covers a start — the guest-written region — the executable span bounds it.
4. **Every word an `l32r` names is data and is never a block start.** `l32r`
   is 10.5 % of the image and its pools sit inside `.text`, immediately before
   the functions that read them. The targets are collected as the sweep
   decodes; a start landing on one is dropped and counted
   (`literal_starts_dropped`), a block walking onto one ends before it
   (`data_ends`). `.literal` sections would be honoured too — **but the link
   keeps none** (verified on the P1b images: the app has `.rwtext`, `.text`
   and `.vectors`; the ROM's fifteen executable sections are all
   `*.text`-shaped), so the `l32r` set is the whole pool rule.
5. **Edges.** `j` names its target; a conditional branch its target and its
   fall-through; `call0/4/8/12` and `callx0/4/8/12` their **return address**
   (the largest single rule on RV32, and the same here — without it the rest
   of every calling function is invisible); `jx` and `callx*` name no target;
   `entry`, `rotw`, `movsp`, `rsil` and `waiti` end a block and name the next
   instruction. The formulas live once, in `decode::edges`, and
   `tests/discover.rs` checks each against `lp_xt_inst::disasm`.
6. **A `loop` names `LEND` as a start and marks the instruction ending exactly
   at `LEND` a terminator with a static back-edge to `LBEG`**
   (`Decoded::lbeg`) — two blocks: the head ends at the `loop`, the body ends
   at `LEND`. No decoder sees this edge; a walk that misses it translates a
   body that runs once and falls through, silently. This phase's emitter only
   sees `control` and exits, so identity is unchanged; P06 turns `lbeg` into a
   counter compare.
7. **A refused instruction ends the block before it and the walk steps over
   it by the decoder's own width.** `Decode::Undecodable { width }` is the
   brief's `refused_width`: a `wsr`, an `isync` or a `break` in the middle of
   a function does not hide the rest of it, and the step is exact because the
   decoder decoded the instruction. There is **no bounded skip** over bytes
   the decoder does not decode at all (`Decode::Refused`): the length of an
   unknown Xtensa encoding is not knowable from its first byte (`op0 = 14/15`
   are reserved formats), so those end the walk and the next seed carries on
   (JD7: never guess a width).
8. **The third path — the guest's own code.** The shader the firmware JITs
   into SRAM0 is 11.86 % of the render loop, has no symbol and no extent.
   Its seeds are every **word-aligned** address in the spans the guest stored
   into executable memory that decodes (`discover::word_seeds`; the region is
   written by aligned word stores only, `SRAM0_WORD_ONLY`, so word granularity
   is exact), walked with `discover_from` and the installed module's starts
   as the stop set, bounded by the span itself. `exec_of` maps a write address
   to the execute address — identity on the classic, the alias offset on the
   S3 (P09). The classic machine drains the bus's code-write ring at every
   slice boundary while a core or the census is on (`Machine::note_code_writes`),
   because the ring holds eight spans and the render loop stores into IRAM
   data hundreds of times a second. P07 wires the event that installs the
   result; this phase supplies and measures the walk.
9. **Nothing here can be wrong, only short (JD7).** Every way the walk can
   fail ends a block and hands the pc to the interpreter, and every one has a
   counter in `DiscoverStats` so a coverage shortfall names its cause:
   `undecodable`, `refused`, `extent_ends`, `data_ends`, `literals`,
   `literal_starts_dropped`, `loop_ends`, `empty_starts`, `capped`,
   `truncated`.

`discover::build` is the door P04 left: one block per **supplied** start,
following widths, extents and literals but no edges. The seam tests and
`--jit-seeds` (now an override of the seeds, not a requirement) use it.

### The coverage census

`LP_EMU_XT_BLOCKPROF=<path>` turns on a per-pc retirement census on both of
the classic's harts (`XtHart::set_pc_census`, `lp_xt_emu::mach::PcCensus` —
off by default, one `Option` test per retire). At the end of the run the
machine walks the arena as it stands — once from the symbols, once from the
accumulated code-write spans with the first walk's starts as the stop set —
and scores the union against every retire: the share inside the walk's
blocks, the interpreted remainder by 64 KiB page with each page's hottest
interpreted pc symbolised, the `entry` one-instruction-block cost, and the FP
share (G-M7D-XT Q8's input). The same number comes out under `--interpreter`
and under `--jit`, because the census counts retires and this phase's stays
retire nothing natively.

### The indirect-target tables

Placed and written whole at install (`lp_emu_jit::dispatch::write_target_tables`
at `SLOT_SHIFT = 0`): a 1 MiB page map plus a 64 KiB slot array per covered
16 KiB page — twice RV32's per page. The boot line prints the bytes. Nothing
this phase emits reads them; `Layout::indirect` is `Some` so that P06's first
`jx` resolution finds a finished table.

## What it does not do yet

| | phase |
|---|---|
| Emitted guest semantics, the window model, the FP arms, the `lbeg` counter compare | P06 |
| Static branch-target resolution inside the module — today every escaped terminator leaves; `decode::edges` is the one place the targets come from | P06 |
| The publish-by-store translation event that installs the third path's walk | P07 |
| The wasm build and the browser host | P08 |
| The ESP32-S3 driver twin (`exec_of` = the alias offset) | P09 |

## Running it

```bash
just test-emu-xt-jit     # the crate's tests under wasmtime
just clippy-xt-jit       # the `jit` feature's lint seat
just lint-emu-fence      # the MIT fence
```
