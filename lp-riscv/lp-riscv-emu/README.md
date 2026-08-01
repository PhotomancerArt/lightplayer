# lp-riscv-emu

The RISC-V 32-bit **emulator** LightPlayer uses to run and debug generated
code on the host: instruction executors, the register files, the run loops,
`EmulatorError`, and the rv32 frame-pointer backtrace walk. The arch-neutral
machinery it builds on — memory model, `StepResult` / `TrapCode`, serial, time,
cycle accounting, the profiler — lives in [`lp-emu-core`](../../lp-emu/lp-emu-core);
decoding of the base ISA is delegated to [`lp-riscv-inst`](../lp-riscv-inst).

```
src/emu/
  emulator/      Riscv32Emulator: state, run loops, single-step, registers,
                 function-call ABI helpers, backtraces, debug dumps.
  executor/      one module per instruction group:
                 arithmetic · immediate · load_store · branch · jump ·
                 compressed · atomic · system · float
  fp_regs.rs     RV32F architectural state: f0-f31 and fcsr.
  error.rs       EmulatorError.
  logging.rs     LogLevel-gated instruction ring log (InstLog).
```

The crate is `#![no_std]` by default (`fw-emu` links it that way on bare-metal
RV32); the `std` feature adds host-only conveniences.

## Supported ISA

`RV32IMAC` plus the `Zicsr` CSR instructions, and — since milestone M9 of the
f32 roadmap — the **`F` standard extension (RV32F)**.

## RV32F floating point

### Provenance

Implemented from *The RISC-V Instruction Set Manual, Volume I: Unprivileged
Architecture*, version **20240411**, Chapter 21 (`"F" Standard Extension for
Single-Precision Floating-Point, Version 2.2`), cross-read with IEEE 754-2008
where the RISC-V text defers to it. Section numbers in the source are from that
release, and each citation also names the section *title* so it survives a
renumbering:

| Section | Title | What it pins here |
|---|---|---|
| §21.1 | F Register State | 32 registers, FLEN = 32, `f0` is ordinary |
| §21.2 | Floating-Point Control and Status Register | `fflags` / `frm` / `fcsr`, the rounding-mode encodings, accrued flags |
| §21.3 | NaN Generation and Propagation | the canonical NaN `0x7fc00000` |
| §21.4 | Subnormal Arithmetic | full IEEE subnormals, no flush-to-zero |
| §21.5 | Single-Precision Load and Store Instructions | `FLW` / `FSW` |
| §21.6 | Single-Precision Floating-Point Computational Instructions | `FADD`…`FSQRT`, sign injection, `FMIN`/`FMAX`, the fused multiply-add family |
| §21.7 | Single-Precision Floating-Point Conversion and Move Instructions | `FCVT.*`, `FMV.X.W`, `FMV.W.X` |
| §21.8 | Single-Precision Floating-Point Compare Instructions | `FEQ` quiet vs `FLT`/`FLE` signaling |
| §21.9 | Single-Precision Floating-Point Classify Instruction | the 10-bit `FCLASS.S` mask |
| §22.2 | NaN Boxing of Narrower Values (`D` chapter) | why there is **no** boxing at FLEN = 32 |

**No GPL implementation was read or transliterated.** QEMU, GDB, GCC and
glibc's soft-float are behavioral references at most; nothing here is derived
from their source. See `docs/adr/2026-07-29-license-provenance-discipline.md`.

`lp-riscv-inst` has no `F` support and does not gain any: `executor/float.rs`
decodes the instruction word itself.

### It is a soft-float implementation, deliberately

Every arithmetic result is computed with integer arithmetic on the exact
significand and rounded exactly once. Native host `f32` operations are not used,
for three individually-disqualifying reasons:

1. **Rounding modes.** RV32F has five (`RNE RTZ RDN RUP RMM`); Rust's `f32`
   operators only ever give the host's, which is `RNE`.
2. **Exception flags.** `NV DZ OF UF NX` must be reported exactly, and Rust
   exposes no access to the host FPU status word.
3. **NaN canonicalization.** §21.3 requires the canonical NaN out of every
   NaN-producing operation; hosts propagate operand payloads instead.

The consequence worth stating plainly: **the exception flags are exact, not
estimated.** They fall out of the same rounding step that produces the result,
so there is no approximation to disclose. It also keeps the crate `no_std`:
`f32::sqrt` and `f32::mul_add` live in `std`.

The arithmetic core is small — an unpack, an exact aligned add on `u128`
significands, and one `round_pack` that every operation funnels through — plus
a digit-by-digit integer square root for `FSQRT.S`.

### Semantics worth knowing before you change anything

- **FLEN is 32, so there is no NaN boxing.** Boxing (§22.2) applies only when
  FLEN > 32. `FpRegs::read_single` / `write_single` exist as the single hook
  where a future FLEN = 64 widening would add it.
- **Canonical NaN, always.** `0x7fc00000` out of every NaN-producing operation.
  Payloads are never propagated. The exceptions are the pure bit movers —
  `FSGNJ.S`/`FSGNJN.S`/`FSGNJX.S`, `FMV.X.W`/`FMV.W.X`, `FLW`/`FSW` — which
  never interpret their operand, never canonicalize, and never raise a flag.
- **`FMIN.S`/`FMAX.S`** return the non-NaN operand when exactly one is a NaN,
  the canonical NaN when both are, and set `NV` for a signaling NaN operand
  *even when the result is a number*. For these instructions only, `-0.0` is
  less than `+0.0`.
- **`FEQ.S` is quiet; `FLT.S`/`FLE.S` signal.** Only a signaling NaN sets `NV`
  for `FEQ.S`; any NaN operand sets it for the other two.
- **Float → integer saturates, and the range check is on the *rounded*
  result.** NaN converts to the destination's **maximum** (`i32::MAX` /
  `u32::MAX`), not to `0` as a Rust `as` cast would. Out of range sets `NV`
  only — invalid suppresses inexact.
- **`FNMSUB.S` is `-(a*b) + c` and `FNMADD.S` is `-(a*b) - c`**, not
  `-(a*b - c)` / `-(a*b + c)`. The difference is observable in the sign of an
  exactly-zero result, and it is implemented as the spec words it.
- **Subnormals are not flushed.** §21.4 requires full IEEE subnormal
  arithmetic. `docs/design/float.md` §4 lists flush-to-zero as *target-defined*,
  but that latitude is about the **shader** tier on wasm/GPU/S3 — the same
  document records that "wasm and RV32F preserve denormals (their specs require
  it)". Do not add an FTZ mode here.
- **Underflow is detected after rounding.** `UF` is raised only when the
  result is tiny *after* rounding *and* inexact, so a value that rounds up to
  the smallest normal is inexact without being an underflow.

### CSRs

`fflags` (`0x001`), `frm` (`0x002`) and `fcsr` (`0x003`) are **real state**,
reachable through the ordinary `CSRRW`/`CSRRS`/`CSRRC` family and their
immediate forms. Every *other* CSR keeps this emulator's long-standing
behaviour — reads return 0, writes are discarded — because nothing here models
`mstatus`, the counters, or the machine-mode CSRs, and an honest no-op beats a
plausible fiction. That split lives in `executor/system.rs`.

Rounding-mode encodings `101` and `110` are reserved, and `rm = DYN` (`111`)
against an `frm` holding `101`, `110` or `111` is equally invalid; all four
raise `EmulatorError::InvalidInstruction`.

### Not implemented

- **`Zcf` (`C.FLW` / `C.FSW`).** Compressed float encodings are a separate
  extension. Nothing in this repo emits them — the shipping target is
  RV32IMAC, which has no `F` at all — and the F-bearing successor we expect
  (`RV32IMAFC`) would need `Zcf` added deliberately, with its own encodings and
  tests. A `C.FLW` word today falls through `compressed.rs` as an unknown
  compressed instruction, which is the right answer for a hart without `Zcf`.
- **`D` / `Q` / `Zfh`.** A non-`00` `fmt` field is an illegal instruction.
- **Trapping on FP exceptions.** The flags accrue in `fcsr`; nothing traps,
  which is what the F extension specifies.
- **A measured FP cycle cost.** FP instructions are classified into
  `lp_emu_core::InstClass`'s `Float*` buckets, but no `CycleModel` assigns them
  a measured cost — `Esp32C6` is a core with no FPU. Treat FP cycle counts as
  instruction counts.

### Tests

`executor/float.rs` and `fp_regs.rs` carry the conformance claim in their
`#[cfg(test)] mod tests`. Because this emulator is intended as the oracle for a
future RV32F code generator, the tests *are* the claim, and they assert on bit
patterns rather than on float equality. Notable strands:

- Every `+ - * /` result is diffed **bit for bit against the host FPU** over a
  cross product of ordinary, extreme, and subnormal values — IEEE 754 pins
  those four operations exactly, so the host is a valid oracle for `RNE`
  (NaN results excluded, since that is where RISC-V deliberately differs).
- `FSQRT.S` is checked against an independent Newton–Raphson root computed in
  `f64` with `core`-only arithmetic.
- All five rounding modes, including an exact-tie case, which is the only way
  to distinguish `RNE` from `RMM`.
- Every one of the ten `FCLASS.S` classes, both saturation ends of both
  integer conversions, the `FMIN`/`FMAX` NaN and signed-zero rules, `FEQ` vs
  `FLT`/`FLE` NaN behaviour, sticky flag accrual, and a fused multiply-add case
  where the fused and unfused results genuinely differ.
