# lp-xt-emu

A pure-Rust Xtensa (ESP32-S3 / LX7 and classic ESP32 / LX6) instruction-set
**emulator core** with the windowed-register machinery, mirroring
`lp-riscv-emu`'s architecture. Built and hardware-verified in the
[2026-esp32s3-experiment](https://github.com/PhotomancerArt/2026-esp32s3-experiment)
repo, landed here per its `BACKPORT.md`.

Scope is *core*: executors + memory + the window machinery, plus the
`lp-emu-core` consumer surface (`LogLevel`-gated instruction ring log,
`CycleModel`/`InstClass` instruction+cycle counters, `dump_state` /
`format_debug_info`). Peripherals, a *measured* Xtensa cycle model
(`CycleModel::InstructionCount` is the default), and full `InstLog` parity
remain out of scope.

The **FPU is in scope as of M6** and partially built — see
[Floating point](#floating-point). **None of its numeric behavior is proven
against silicon until the M6 P6 hardware campaign runs.** Do not trust an FP
result out of this emulator before then.

## Architecture

```
src/
  cpu.rs         CPU state: PC, 64 physical ARs, WindowBase, WindowStart, SAR,
                 PS.CALLINC, the live call-stack shadow, and the FP coprocessor
                 state (flat FR file, BR file, FCR/FSR, CPENABLE).
  memory.rs      Vec-backed regions + per-region D-bus/I-bus AliasRule.
  board.rs       BoardProfile: per-board memory maps (esp32s3 / esp32).
  trace.rs       `trait Tracer` (no-op default) + a basic text tracer.
  error.rs       Trap { Exception | Timeout }, mirroring CrashReport.
  emu.rs         Emulator: fetch/decode/execute loop + the windowed-ABI run API.
  executor/      one module per instruction group (the lp-riscv-emu split):
                 arith · imm · load_store · branch · jump · call · window · misc
                 float       FP/Boolean/SR data movement + the CPENABLE gate
                 float_math  everything that computes a float value (M6 P3)
  fp_policy.rs   every behavior IEEE-754 does not fix, measured or Unknown.
  fp_capture.rs  parse + diff a device conformance capture (M6 P5).
```

Decoding is delegated to [`lp-xt-inst`](../lp-xt-inst); this crate never
re-implements it. Instruction semantics come from the Xtensa ISA Reference
Manual and are validated by diffing against real hardware (see below).

### The windowed register view

The LX7 has a 64-entry *physical* address-register file. Software sees a
rotating 16-register window; `a{i} == AR[(WindowBase*4 + i) mod 64]`.
`WindowStart` bit `k` marks a live call frame based at `WindowBase == k` whose
registers are currently *resident* (not spilled).

- **CALL8/CALLX8** (and call4/12) do **not** rotate. They stage the return
  address — with the call-increment in the top two bits — into the caller's
  `a[4*inc]`, and record `PS.CALLINC`.
- **ENTRY** rotates `WindowBase` forward by `PS.CALLINC`, allocates the stack
  frame, and sets the new `WindowStart` bit. The caller's `a10..` become the
  callee's `a2..` — this is how `f(arg)` receives `arg`.
- **RETW** rotates back by the increment recorded in `a0`'s top two bits and
  unmangles the return PC (`(PC & 0xC000_0000) | (a0 & 0x3FFF_FFFF)`).

### Window overflow / underflow — modeled directly

When the register ring wraps so a new frame's registers would overwrite a still
live ancestor, the ancestor is **spilled** to its ABI stack save area, and
**reloaded** on the return path — the effect of the `_WindowOverflow` /
`_WindowUnderflow` handlers, implemented directly rather than by emulating the
handler vectors. See the experiment repo's ADR
[`2026-07-28-emu-window-overflow-direct.md`](https://github.com/PhotomancerArt/2026-esp32s3-experiment/blob/main/docs/adr/2026-07-28-emu-window-overflow-direct.md).

The frame chain is tracked as an explicit call-stack shadow (not a per-base
table): `WindowBase` is reused as the ring wraps, so it is *not* a stable frame
identity. A frame's base save area (`a0..a3`) is located from its **callee's**
stack pointer at `[callee_sp-16, callee_sp)`, exactly as the hardware handler
chain recovers it by walking the resident window — so a spill and its later
reload always address the same bytes. Extra register groups (`a4..`, `a8..`) for
call8/call12 frames are placed just below; their exact byte placement is not
observable for bare payloads (which never read another frame's save area), the
deliberate "model the effect, not the handler vectors" boundary.

### Board profiles — the memory map is a parameter

Instruction semantics are board-independent (FINDINGS: LX6 vs LX7 divergence is
entirely in the memory system, not the core). What differs per board is where
code and stack live and how the D-bus (data) view of code memory maps to the
I-bus (executable) view. `board.rs` captures that as a `BoardProfile`:

- `Emulator::new()` — the **ESP32-S3** profile, unchanged default: code
  `0x3FC8_8000+128K` and stack `0x3FCC_0000+128K`, both in the SRAM1
  dual-mapped window `0x3FC8_8000..0x3FCF_0000` with the executable alias a
  constant offset `+0x6F_0000` (FINDINGS E2).
- `Emulator::with_profile(BoardProfile::esp32())` — the **classic ESP32**
  profile, from the C1–C5 hardware ladder (FINDINGS classic section):
  - SRAM1's dual mapping (D-bus `0x3FFE_0000..0x4000_0000` ↔ I-bus
    `0x400A_0000..0x400C_0000`) is **word-mirrored**:
    `iram = 0x400B_FFFC − (dram − 0x3FFE_0000)` at word granularity, bytes
    within each word verbatim (C2b, 5 sentinels; the linear hypothesis matched
    none). So the alias is an `AliasRule` — `Offset`, `Identity`, or
    `WordMirrored` — not a constant.
  - Code: 92 KiB at D-bus `0x3FFE_8000` inside the measured-free span
    (dram2_seg `0x3FFE_7E30..0x3FFF_FF80`, ~96 KB usable). SRAM1 is the region
    a runner would use; the alternatives are SRAM0 (~125 KB, identity-mapped,
    **word-only writes**) and RTC-fast (8 KB, `+0xC4_0000`).
  - Stack: 64 KiB at `0x3FFC_0000` in SRAM2 (dram_seg, plain data RAM — C5
    measured 98 304 B heap free, so 64 KiB fits with headroom). SRAM2 has no
    I-bus view, so fetching there faults (EXCCAUSE=2) exactly as the classic
    heap does on hardware (C2g) — S3's "the heap is executable" does not carry.

`Emulator::run` loads the blob at `profile.code_ibus_base()` so byte *i* of the
code lands at I-bus address `base + i`; under the classic mirror the backing
D-bus image walks downward word by word, exactly as the device writer lays it
out. Not modeled: classic's *word-only* data access to I-bus addresses (byte
stores to SRAM0/I-bus fault EXCCAUSE=3 on hardware) — the emulator is more
permissive there; the device-side writer honors the constraint.

### D-bus / I-bus dual mapping

The runner firmware writes payloads via the D-bus view and *executes them at
the I-bus view*, so self-addressing code (`l32r` literals, `call8` targets)
only behaves identically if the emulator models the same alias. `memory.rs`
backs each dual mapping with one store reachable at both address ranges (the
region carries its `AliasRule`); fetch is permitted only at the executable
(I-bus) view, so jumping to a D-bus address faults exactly as hardware does
(FINDINGS E2D, and classic C2g).

### The host-shared window — data the host and guest both own

`Memory::add_shared(dbus_start, Arc<Mutex<Vec<u8>>>)` maps a **host-owned**
buffer into the guest's data space, so guest loads and stores there read and
write the host's bytes with no copying. This is how a host emulation engine
(`lpvm-native`'s `rt_emu`) hands compiled shader code its vmctx — uniforms,
globals, snapshot region, texture buffers — and reads results back out.

The window is **data only**: it carries no `AliasRule`, so an instruction fetch
from it faults (`EXC_INSTR_FETCH_ERROR`) exactly as jumping into a data region
does. `SHARED_DBUS_BASE` (`0x3F40_0000`) is the base both board profiles leave
free; `add_shared` **asserts** the range does not overlap any installed region,
in either its D-bus range or its I-bus image, so the choice cannot rot silently
as profiles change.

This address is **host-emulator fiction** — no ESP32 has a host-shared region,
and the on-device JIT never needs one. See
`docs/adr/2026-07-30-xtensa-host-shared-memory.md`, which also records why it is
deliberately *not* the rv32 engine's `0x4000_0000` (that address is
`SENTINEL_PC`).

## Floating point

Modeled since M6, in three pieces:

- **State** (`cpu.rs`): the FR file `f0..f15` as **raw bits**, the BR file
  `b0..b15`, `FCR`/`FSR`, and `CPENABLE`. The FR file is **flat** — it takes no
  part in the `WindowBase` rotation, so the AR file's free preservation across a
  windowed call has *no FR analogue*. A callee that wants an FR value to survive
  `call8`/`entry` spills it itself. This is the asymmetry M7's frame layout has
  to answer for.
- **Data movement** (`executor/float.rs`): `rfr`/`wfr`, FP load/store including
  the base-updating `p` forms, `mov.s`, `BR`/`CPENABLE`/`FCR`/`FSR` access, and
  the Boolean branches and moves. Every coprocessor-0 instruction is gated on
  `CPENABLE` bit 0 and raises **EXCCAUSE 32** when it is clear — `Cpu::new()`
  leaves it clear on purpose, so firmware that forgets to arm the coprocessor
  faults on the host rather than on a board.
- **Numerics** (`executor/float_math.rs`) behind the policy layer in
  `fp_policy.rs`.

### The policy layer, and what `UNKNOWN` means

Rust's `f32` is IEEE-754 binary32 under round-to-nearest-even, which is what the
FPU does for nearly the whole input space — and for normal, finite, non-zero
operands with a normal finite result, `add.s`/`sub.s`/`mul.s` here are
bit-exact against host `f32` by construction (asserted over a 20 000-case
randomized sweep). But Rust cannot express *which* NaN propagates, whether
denormals flush, or a rounding mode.

So each behavior IEEE does not fix is a named field on `FpPolicy`, and each is
either **measured with a citation** or `Unknown`. **Reading an unresolved field
panics**, naming the field and the vector family that closes it. That is
deliberate: a plausible default is indistinguishable from knowledge once it is
in the code, and an emulator that is 99% right and silently confident about the
rest is the exact failure M6 exists to prevent.

Seven of the seventeen fields are resolved, and the citation says how:
**`fsr_sticky`** from the 2026-07-31 desk session, and six — `madd_fused`,
`conversion_scale`, `float_to_int_out_of_range`, `float_to_int_nan`,
`utrunc_negative`, `snan_compare_signals` — from the Xtensa ISA Reference
Manual, each citing the instruction page that states it. The distinction
matters: a manual reading is still falsifiable by the P6 campaign, a silicon
measurement *is* the campaign. The other ten are the row list of the M6
FP-contract ADR's §4. A non-default `FCR.RM` is likewise **refused**, not
ignored (D6) — its encoding is architectural (`cpu::FCR_RM_*`, ISA RM Table
4-47), but whether this silicon honors it is still F1's measurement.

The `FSR` flag *layout* is architectural too (`cpu::FSR_*`, Table 4-48), and it
explains the P1 measurement: the `0x400` read back after that sweep is
`FSR_DIV_BY_ZERO`, and the sweep ran `div0.s` on a staged zero. What stays open
is which operation raises which flag — where the manual is actually *falsified*,
since §4.3.11.4 says current implementations raise none and this one did.

`recip0.s`/`rsqrt0.s`/`sqrt0.s`/`div0.s` return implementation-defined lookup
ROMs; they sit behind an empty table that P6 extracts exhaustively, so they
become exact by construction. There is deliberately no polynomial placeholder.

### `tests/fp_conformance.rs` — the replay, with no board

```bash
cargo test -p lp-xt-emu --test fp_conformance -- --nocapture
```

Runs every vector of [`lp-xt-fp-vectors`](../lp-xt-fp-vectors)' six families
through the emulator and compares to the predictions committed under
`tests/fixtures/fp/`. It needs no feature flag and no hardware: `lp-xt-emu` is in
`default-members`, so plain `cargo test` (and therefore `just test-rust-core`,
`just test`, and CI's Validate job) runs it. That is deliberate — a corpus wired
behind a stale `--test` allowlist has twice in this repo "reported success by
executing nothing".

**An `UNKNOWN:<field>` row is not a failure.** It is a question addressed to
silicon, naming the policy field that closes it, and the set is *derived* — the
harness catches the policy panic and reads the field name out of it, so it
cannot drift from what the executors actually need. Today: **3886 of 5630 rows
UNKNOWN (69.0%)**, and the test asserts the count is not zero, because zero
before the campaign would mean the policy layer had quietly acquired defaults.
Each corpus file's header breaks its own count down by the field that closes it,
so P6 can triage one field at a time rather than face a single number.

To regenerate after a generator or executor change:

```bash
UPDATE_FP_GOLDENS=1 cargo test -p lp-xt-emu --test fp_conformance
```

**Never** regenerate a row from device output. That inverts the test into a
tautology that passes forever, and it is already the repo's stated rule
(`lpvm-native/src/xt_corpus.rs`).

### `src/fp_capture.rs` — the campaign's diff tool

```bash
just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX signed_zero 50   # capture
just fp-diff target/fp-capture/fpconf-YYYYmmdd-HHMMSS.txt       # classify
```

Parses a capture from `fw-esp32s3`'s `test_xt_fp_conformance` harness and
classifies every row **AGREE** / **DIVERGE** / **RESOLVED** / **SKIPPED**.
Sans-IO, like the rest of `lp-xt/*`: it takes `&str` and returns values; the
file reading lives in `tests/fp_capture.rs`, which also shares this module's
corpus parser with `fp_conformance.rs` so a file one accepts and the other
chokes on cannot exist.

Two conditions **abort** rather than colour a row, and both are asserted against
deliberately damaged fixtures rather than tried once:

- a **fingerprint mismatch**, because the device regenerates its own inputs and
  a disagreement means the two sides ran different vectors — every comparison
  after it would compare unrelated things, and 5 630 divergences would look like
  a discovery;
- a **missing or short-counted sentinel**, because a serial capture that stops
  early is otherwise indistinguishable from one that finished.

A `DIVERGE` row does *not* fail the command. It is the campaign's product, to be
triaged into an emulator bug, a harness bug, or documented silicon behavior —
failing here would push the next person toward editing a golden to get green.

## Run API

`Emulator::run(code, entry_offset, arg)` loads `code` into SRAM1 and invokes it
exactly as the device runner does — a synthesized windowed `CALL8`, `arg` staged
in `a10` and arriving in the callee's `a2` after its `ENTRY` — returning
`RunOutcome::Ok(result)` or `RunOutcome::Trap`. `run_traced` additionally emits
`TraceEvent`s (per-instruction, register/memory writes naming the physical AR,
and window rotate/spill/reload events). `run_with_args` takes up to
`OUT_ARG_REG_COUNT` (6) register arguments; `run_loaded` runs an image a loader
already placed in memory.

`run_loaded_with_args(entry, args, tracer, handler)` is the **host-call** entry
point — what an emulation engine uses to invoke a compiled shader function in an
already-loaded image. Over the others it adds:

- **stack arguments**: `args[6..]` go to the caller's outgoing argument area at
  `[caller SP + 4*k]`, matching `isa/xt`'s `classify_params`
  (`ArgLoc::Stack { offset }`) and what its emitter stores;
- **outgoing-area headroom**: the caller SP is lowered by that area's
  (16-aligned) size first, since `initial_sp` sits only 16 bytes below the top
  of the stack region;
- **two-word results**: `CallOutcome::Ok { lo, hi }` carries the caller-view
  `a10`/`a11` pair, which a two-scalar return uses in full.

An sret return needs nothing extra: the buffer pointer is simply the first
argument (callee `a2`), and the buffer normally lives in the shared window.

`self.step_budget` bounds a run. A host engine that arms in-guest fuel should
raise it well above the fuel tank so fuel traps fire first and the budget stays
a backstop for fuel-off compiles; `DEFAULT_STEP_BUDGET` is sized for the fixture
corpus, not for real shaders.

## Validation

`tests/conformance.rs` runs every corpus case on the emulator under **every**
`BoardProfile` against its known answer:

```bash
cargo test -p lp-xt-emu
```

The same corpus was N-run against attached ESP32-S3 **and** classic ESP32
silicon in the experiment repo
([2026-esp32s3-experiment](https://github.com/PhotomancerArt/2026-esp32s3-experiment),
via `xt-runner` + `xt-testkit`) with **zero divergences**; the hardware oracle
stays there as the candidate tethered-CI rig.

Corpus: golden vectors GV1–GV3b plus a generated set — arithmetic, load/store
round-trips, branches both directions, a backward-branch loop, `call8` and
`call12` self-recursion past depth 16/60/100 (the key window-overflow/underflow
stress), a `callx8`-to-builtin case, and illegal-instruction / hang faults. The
recursion blobs use PC-relative `call8`/`call12` so they are position-independent
and dual-runnable; the `callx8` + `l32r` golden vectors self-address via absolute
literals and so are emulator-only known-answer.

All payload bytes are objdump-derived from toolchain-assembled sources, never
hand-recalled (repo lesson: 2/3 hand-recalls are wrong).

`tests/host_call.rs` covers the host-shared window and the host-call entry point:
the host↔guest byte round-trip, the fetch fault, the overlap assertion, both
board profiles, an **eight-argument** call (distinct powers of two summed, so any
misplaced argument changes the answer — a wrong stack base is otherwise a silent
wrong-value bug), a two-word return, and an sret buffer in the shared window.
Its payloads are assembled with `lp_xt_inst::encode` — the objdiff-verified
encoder — rather than transcribed, because these programs need specific register
and stack-offset shapes that no golden vector has.

## Provenance

**This is original code.** Instruction semantics and the windowed-register model
are implemented from the **Xtensa ISA Reference Manual** and validated
behaviorally against real ESP32-S3 hardware (the `xt-runner` oracle). Encoding
data is consumed via `lp-xt-inst` (whose provenance derives from the Apache-2.0
LLVM Xtensa tables). Both board memory maps are **hardware-measured, never
recalled**: the S3 map from the original spike (experiment FINDINGS E1–E5,
`fw/spike-esp32s3`), the classic map from the C1–C5 ladder run 2026-07-28 on
rev v3.0 silicon (experiment FINDINGS classic section, `fw/spike-esp32`) —
including the word-mirrored SRAM1 rule, established with 5 sentinel probes.

**No GPL source was used.** QEMU (`espressif/qemu`) and binutils/GDB — including
their windowed-register handling and the `_WindowOverflow`/`_WindowUnderflow`
handlers — are behavioral references only: observed to understand semantics,
never copied or transliterated. See the repo license ADR
(`docs/adr/2026-07-29-license-provenance-discipline.md`) and `AGENTS.md`.
