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

## What it emits (M7 P06)

`translate` emits the guest's integer core natively — the ALU, the shifts
through `SAR`, the multiplies and divides, `l32r` and the plain loads and
stores behind the permission byte, every branch form, `j`/`jx`, the calls and
returns, `entry` and `retw`, `loop` and the loop-back — and **escapes** the
rest to the interpreter through `step_one`, one instruction at a time, with
the file marshalled around the call. The escape-everything build
(`Emit::NOTHING`) is kept as the comparison: the round-trip suites run every
program both ways against a real `XtHart` behind the hatch and assert the
two leave the same file, window, counters and memory at 1, 8 and 64 blocks a
function.

### The register model (XD8)

Sixteen `i32` locals hold the **current window** `a0..a15`; the physical
`AR[0..64]` file lives in the exchange area with `WindowBase`,
`WindowStart`, `SAR`, `LBEG`/`LEND`/`LCOUNT`, `PS.CALLINC` and the dirty
mask (`lp_xt_jit::LAYOUT`, `extra::*`). `a_i` is
`AR[(WindowBase * 4 + i) mod 64]`, so the prologue loads the sixteen from
wherever the base says (a fast path of sixteen constant-offset loads while
the window does not wrap past `AR[63]`, a masked path when it does), and a
rotate moves locals rather than renaming them.

**The dirty mask.** A stay writes back only the four-register groups it
wrote: one local, OR'd with a per-block constant at each block's start
(over-approximating a group as dirty writes back a local that already equals
the file, never wrong), shifted at each rotate, read wherever the file has to
be complete — a rotate's *leaving* groups, an escape, an exit, a
cross-function edge. The invariant: **every register outside the window is
in `AR`; every one inside is in its local, and in `AR` unless its group is
dirty.** That is what keeps a caller's registers in `AR` whenever a `retw`
may reload them (they left the window at the `entry` that created the
callee's frame), and what lets the escape hatch hand the interpreter a
complete file. `tests/window_roundtrip.rs` is the invariant under test:
every callee writes its whole window before returning.

**Always current in the exchange area**, rather than only at an exit:
`WindowBase`/`WindowStart` (written at every rotate), the loop registers
(at every `loop` and every loop-back decrement), `PS.CALLINC` (at every
windowed call). Those are the words a polling point can observe from inside
a stay — interrupt entry saves `PS` whole; the handler's `save_context` reads
the loop registers — so the host's fused poll marshals them into the hart
before it runs `XtHart::poll_after_store`, and a store needs no flush.

### `entry`, `retw`, the calls

`entry as, imm` first tests the callee's frame with the **live** `CALLINC`
(`WindowCheck(00, CALLINC, 00)`) and refuses at its own pc if a
`WindowStart` bit is within reach; then rotates by `CALLINC` as one of four
fixed sequences behind a `br_table` — the leaving groups written back if
dirty, the staying locals slid down, the entering groups loaded from the
file, the mask slid with them — then writes the callee's `a_s = a_s - imm`
in the new window and sets its bit. `retw` refuses at its own pc unless
`n = a0[31:30]` is non-zero, the caller's frame `base - n` is resident and
no closer frame is (the underflow exception and the RM's illegal cases are
the interpreter's), then rotates back and looks `(a0 & 0x3FFF_FFFF) | (pc &
0xC000_0000)` up in the target table. `call4/8/12` and `callx4/8/12` write
`a[4n] = (n << 30) | ((pc + 3) & 0x3FFF_FFFF)` in the caller's numbering and
`PS.CALLINC` in the local and the exchange area; `call0`/`callx0` write
`a0`. `callx*` reads its target before it writes the link.

### The window precondition, hoisted per block (XD5)

Every block opens with the interpreter's own `overflow_in_reach` on the
block's maximum group (`entry` counted at its `as`; its `CALLINC` half is
the arm's), read as `((WindowStart << 16) | WindowStart) >> WindowBase`
masked to the reach. A bit within reach **refuses the block at its own pc
with nothing retired** (`why::WINDOW`); the interpreter runs it slot by slot,
raises the overflow at the exact instruction, runs the guest's own
`_WindowOverflow4/8/12` and re-enters translated code after `rfwo`. No
handler runs in wasm. A stay only starts under `PS.WOE && !PS.EXCM` (the
driver refuses otherwise), and nothing emitted changes either bit — the
instructions that do are escapes that leave, or undecodables that end the
block before them.

### Memory, and the classic's fourth permission byte

The RV32 fast path with one value added: `PERM_READ_WORD` marks an
executable, **word-only** page (SRAM0, DD37) — an aligned word load inline,
every other access the bus's. An executable page never takes an inline store
whatever the bus says about writability, because a guest store into
executable memory is the invalidation event (XD3) and the bus is what records
it (the classic's driver writes the table that way). Every access the fast
path declines goes through the import, where the bus serves it exactly as it
serves the interpreter; a refused one leaves at the instruction's pc with
nothing retired. There is no straddle exit.

### The loop-back

The hart tests `LCOUNT != 0 && seq == LEND` before every instruction on the
live registers, decrements, and makes the next pc the live `LBEG` — a taken
branch then overrides the pc but the decrement stays; an abort undoes it.
The walk marks the one instruction that can fire it (rule 6), and the arm
reproduces that order: the verdict before the instruction, the decrement
once it is certain to retire (before a store's poll, whose post-store pc is
the looped one; undone if the bus refused), then the live `LBEG` — the
static back-edge when it agrees with the walk, `why::LOOP_BACK_MISS` out
of the stay when it does not. A stay may not begin with `LCOUNT != 0` and a
live `LEND` the walk never marked (the driver refuses); a `loop` inside the
stay sets a `LEND` the walk did mark.

### The refusals — the design

Everything the emitter cannot make exact, by name (`translate::refusal_of`;
the driver's escape census counts by the same names):

| refusal | how |
|---|---|
| every FP instruction (`fp`, `fp load/store`) — DD113, 0.08 % of the render loop | escape |
| `movt`/`movf`, `bt`/`bf`, the Boolean logic and reductions | escape |
| `clamps` | escape |
| `movsp`, `rotw`, `rfe`/`rfde`/`rfwo`/`rfwu`, `rfi`, `rsil`, `waiti` | escape; the reload re-reads the whole window |
| `entry` with `s > 3` | escape (the illegal-instruction trap is the interpreter's) |
| a `loop` whose own successor is another loop's `LEND` | escape |
| a zero divisor | escape at the instruction (the trap is the interpreter's) |
| the block's window precondition, `entry`'s frame check, `retw`'s residency check | exit at the pc, nothing retired (`why::WINDOW`) |
| a loop-back whose live `LBEG` is not the walk's | exit at the live `LBEG` (`why::LOOP_BACK_MISS`) |
| an access the fast path declines that the bus refuses | exit at the pc (`why::LOAD_REFUSED` / `STORE_REFUSED`) |
| a store watchpoint hit | exit at the pc (`why::STORE_PERM`) |
| `rsr`/`wsr`/`xsr`/`rur`/`wur`, `isync`, `break`, `syscall`, `ill`, the atomics, `l32e`/`s32e`, the TLB ops, `rer`/`wer`, MAC16 | undecodable for the translator: the block ends before them (P04) |

At entry the driver refuses `PS.WOE` clear or `PS.EXCM` set, an `IBREAK`
armed, and `LCOUNT != 0` with an unmarked `LEND`, beside the C6's rules.

### The escape

Flush the dirty groups and `SAR` and the counters, `step_one`, then reload
**everything** — the window words, the loop registers, `CALLINC`, all
sixteen locals — because the instruction may have rotated the window or
written any register. An escaped Load-class instruction sets the pending
flag so the next store or System-class instruction polls; a poll that finds
nothing is not observable.

## The numbers (P06)

Every number is the classic at `t1`, both cores at `--core-quantum 256`,
under wasmtime on this desk; `--jit` against `--interpreter` on the same
binary (`scripts/emu/v3-oracle.sh`), the previous head's binary against this
one both `--interpreter`. **Every column `same`** on every cell:

| image | window | jit vs interp | main vs branch |
|---|---|---|---|
| `boot-idle` | 100 ms | same ×5 | same ×5 |
| `boot-idle` | 20 ms (trace, 427,900 lines) | same ×5 | — |
| `shader-compile-stress` | 2 s (to the sentinel) | same ×5 | same ×5 |
| `render-loop` | 20 ms (trace, 601,344 lines) | same ×5 | — |
| `render-loop` | 2200 ms (256 frames) | same ×5 | same ×5 |

The 20 ms cells refuse the core under `--trace` (P04's rule) and are two
interpreted legs. The three-engine differential: the crate's 266 round-trip
cases and a 200-entry `render-loop` recording (`LP_EMU_XT_JIT_RECORD`, taken
300 M cycles in) match the wasmtime run in node v25.2.1 / V8 and bun 1.1.18
/ JavaScriptCore, every field.

### `render-loop` 2200 ms, core 0 — what the module did

| | |
|---|---|
| retired by the core | 434,061,843 |
| **retired natively** | **297,063,861 (68.44 %)** — 99.85 % of what entered translated code |
| inside the walk's blocks (P05) | 96.08 % |
| entries | 5,810,220 — 51.2 instructions per entry |
| escaped to the interpreter from inside a stay | 452,813 (0.10 % of the core): `rsil` 212,212, FP load/store 130,297, FP 64,796, `bt`/`bf` 23,347, `rotw` 19,801, `rfi` 2,103, `waiti` 257 |
| entry refusals | `PS.EXCM`/`!WOE` 83,353; none for pending, watch, impure, timer, stale, IBREAK, loop |
| exits by reason | budget 2,757,214 · indirect-miss 1,702,522 · undecodable 697,734 · **window 361,541** · edge-out 287,341 · escape-target 2,103 · after-store 1,508 · slice-ended 257 |
| no-progress exits | 961,995 (a refused block's pc handed to the interpreter for one instruction) |
| invalidations | 11,739 answered by re-reading the bytes, 0 changed |

The interpreted remainder is now two things: the 3.92 % outside the walk
(P05's page table: the window handlers, `jx` targets, `xthal_get_ccount`),
and the 27.6 % *inside* the walk that the stays hand back — the budget at the
slice edge, the indirect misses (a `retw`/`jx`/`callx*` target the table does
not hold), the undecodables the walk ends blocks before, and the window
refusals. Each of those is a name on the exits row, not a mystery.

### The window (XD8, the numbers XD5 asked for)

| | |
|---|---|
| `WINDOW` refusals | **361,541** — 235,228 at a block's precondition, 126,313 at a `retw`, 0 at an `entry`'s own check — against the interpreter's 458,373 exceptions (229,319 overflow, 229,054 underflow); the rest fire inside interpreted stretches |
| `entry` rotates emitted natively | **2,665,077 of 5,138,231** (51.9 %) |
| `retw` rotates emitted natively | **3,816,394 of 5,137,969** (74.3 %) |
| writeback per exit, by groups written | 0: 1,383,884 · 1: 2,333,268 · 2: 1,436,217 · 3: 348,654 · 4: 308,197 |
| **mean words written back per exit** | **5.15 of 64** (8.0 % of a full writeback; 32 % of the window) |

The dirty mask is the design's justification, measured: a full 64-word
writeback per exit would write 12× what the mask writes. The `entry`
one-instruction block (1.18 % of retires, P05) is still its own block: with
the rotate at ~90 wasm instructions in the `CALLINC = 2` case, folding it
into its successor saves one dispatch per call and is P08's to price with a
real engine.

### Size and boot cost (a product number, JD20)

| | `render-loop` | `shader-compile-stress` |
|---|---|---|
| blocks · instructions | 140,424 · 599,278 | 63,342 · 256,787 |
| emitted natively · escaped (static) | 590,572 · 9,036 (**1.51 %**) | 250,909 · 6,164 (2.40 %) |
| static escapes by name | FP 5,297 · FP ld/st 2,548 · `bt`/`bf` 721 · `rsil` 88 · `rotw` 19 · `rf*` 17 · `rfi` 11 · `waiti` 4 · `movsp` 1 | FP 3,902 · FP ld/st 1,467 · `bt`/`bf` 401 · `rsil` 58 · … |
| module | **89,970,707 B — 150.0 B per instruction** | 39,728,527 B — 154.5 B |
| largest body (64 blocks a function, 2,195 functions) | 398,288 B | 351,825 B |
| discover · emit · **compile** (cranelift, per core) | 130 ms · 730 ms · **149.0 s** | 56 ms · 304 ms · 62.4 s |

Against P05's escape-everything module (61 MB, 59.6 s): the real bodies are
1.5× the bytes and 2.5× the compile. The native compile is not the product
path (JD24); the browser engines are P08's, and `fn_blocks` is untouched.

`boot-idle` is the one caveat: its module went **stale** after 1,487,935
native instructions — one block's bytes changed under it (the 1-of-2
invalidations that found them changed) — and the rest of that image ran
interpreted, exactly as P05's rule says. Incremental retranslation is P07's.

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
   body that runs once and falls through, silently. P06's emitter turns
   `lbeg` into the loop-back arm.
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
| FP arms — DD113 says no (0.08 % of the render loop) | — |
| The publish-by-store translation event that installs the third path's walk | P07 |
| The wasm build and the browser host | P08 |
| The ESP32-S3 driver twin (`exec_of` = the alias offset) | P09 |

## Running it

```bash
just test-emu-xt-jit           # the crate's tests under wasmtime
just test-emu-xt-jit-engines   # the round-trip cases replayed in V8 and JavaScriptCore
just test-emu-xt-jit-identity  # both of the above
just clippy-xt-jit             # the `jit` feature's lint seat
just lint-emu-fence            # the MIT fence
```

On the classic, `LP_EMU_XT_JIT_RECORD=<dir>` (with `_AFTER=<cycles>`,
`_ENTRIES=<n>`) records core 0's entries as one engine case, and `node
scripts/emu/jit-engine-check.mjs <dir>` / `bun …` replay it;
`LP_EMU_XT_JIT_ESCAPE_ALL=1` builds the escape-everything module instead.
