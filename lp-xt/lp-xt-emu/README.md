# lp-xt-emu

A pure-Rust Xtensa (ESP32-S3 / LX7 and classic ESP32 / LX6) instruction-set
**emulator core** with the windowed-register machinery, mirroring
`lp-riscv-emu`'s architecture. Built and hardware-verified in the
[2026-esp32s3-experiment](https://github.com/PhotomancerArt/2026-esp32s3-experiment)
repo, landed here per its `BACKPORT.md`.

Scope is *core*: executors + memory + the window machinery, plus the
`lp-emu-core` consumer surface (`LogLevel`-gated instruction ring log,
`CycleModel`/`InstClass` instruction+cycle counters, `dump_state` /
`format_debug_info`). FPU, peripherals, a *measured* Xtensa cycle model
(`CycleModel::InstructionCount` is the default), and full `InstLog` parity
remain out of scope.

## Architecture

```
src/
  cpu.rs         CPU state: PC, 64 physical ARs, WindowBase, WindowStart, SAR,
                 PS.CALLINC, and the live call-stack shadow.
  memory.rs      Vec-backed regions + per-region D-bus/I-bus AliasRule.
  board.rs       BoardProfile: per-board memory maps (esp32s3 / esp32).
  trace.rs       `trait Tracer` (no-op default) + a basic text tracer.
  error.rs       Trap { Exception | Timeout }, mirroring CrashReport.
  emu.rs         Emulator: fetch/decode/execute loop + the windowed-ABI run API.
  executor/      one module per instruction group (the lp-riscv-emu split):
                 arith · imm · load_store · branch · jump · call · window · misc
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

## Run API

`Emulator::run(code, entry_offset, arg)` loads `code` into SRAM1 and invokes it
exactly as the device runner does — a synthesized windowed `CALL8`, `arg` staged
in `a10` and arriving in the callee's `a2` after its `ENTRY` — returning
`RunOutcome::Ok(result)` or `RunOutcome::Trap`. `run_traced` additionally emits
`TraceEvent`s (per-instruction, register/memory writes naming the physical AR,
and window rotate/spill/reload events).

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
