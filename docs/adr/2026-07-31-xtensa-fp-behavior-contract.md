# The Xtensa FP behavior contract: silicon is the spec, and it has been read

- Status: **draft** — G2 pends; P7 finalizes with Yona's calls folded in
- Date: 2026-07-31
- Plan: `2026-07-30-1745-f32-native-math/m6-xtensa-fpu-emulator` (M6 P6)
- Deciders: Yona + M6 campaign

## 1. Context

The ESP32-S3's Xtensa LX7 FPU is the first f32-capable silicon this project
ships on, and `lp-xt-emu` is the oracle every downstream f32 milestone leans
on: M7 compiles float shaders against it, M8 triages filetest corpora with
it, M9 inherits its policy shape. It is not documented to RV32F's standard.
The 2011 ISA Reference Manual predates the estimate/helper instructions
entirely (its Table 4-46 does not list them), leaves rounding-mode
consultation unspecified, and makes claims (§4.3.11.4: implementations raise
no FSR flags) that this silicon demonstrably violates. QEMU, binutils and GCC
*source* are off limits by license rule; their *output* (objdump) is fact.

So silicon is the spec — and the M6 campaign read it: 5 630 committed-first
predictions diffed against a full device sweep, three ROMs extracted
exhaustively, 5 328 helper probes, and the toolchain's real divide and
square-root sequences run end-to-end on both sides. **The emulator now
matches the desk S3 on every corpus row — result bits and FSR — with zero
divergence**, and the captures replay boardlessly forever
(`lp-xt/lp-xt-emu/tests/fp_silicon_replay.rs`).

Board: XIAO-class ESP32-S3, chip rev v0.2, MAC `d8:3b:da:47:29:70`, 16 MB
flash (`--flash-size 8mb`), espup toolchain `esp-14.2.0_20240906`. Captures:
`lp-xt/lp-xt-emu/tests/fixtures/fp/captures/`.

## 2. The emitted subset

Normative (all present on silicon, M6 P1; all modeled and measured, P6):

- Arithmetic: `add.s sub.s mul.s madd.s msub.s`
- Moves and sign ops: `mov.s abs.s neg.s`, `moveqz.s movnez.s movltz.s
  movgez.s`, `movt.s movf.s`
- Compares (→ BR): `un.s oeq.s ueq.s olt.s ult.s ole.s ule.s`
- Conversions: `float.s ufloat.s trunc.s utrunc.s round.s floor.s ceil.s`
  (scale immediate 0..15)
- Transfers and memory: `rfr wfr lsi ssi lsiu ssiu lsx ssx lsxu ssxu`
- Control registers: `rur.fcr wur.fcr rur.fsr wur.fsr`, `rsr/wsr CPENABLE`
- Divide/sqrt building blocks: `div0.s recip0.s sqrt0.s rsqrt0.s nexp01.s
  mksadj.s mkdadj.s addexp.s addexpm.s maddn.s divn.s const.s`

Excluded deliberately: double precision (not on the chip), the DFP
accelerator forms, `lddec/ldinc` MAC16 forms (integer-side, unused by
shaders). `mksadj.s` joined the subset in P6 — it was wrongly recorded as an
unassigned slot, and the real sqrt sequence uses it
(`docs/defects/2026-07-31-mksadj-missing-from-fp-subset.md`).

## 3. Register model

- **FR `f0..f15` is flat.** No windowing, no rotation, no analogue of the AR
  file's free preservation across `call8`. An FR value that must survive a
  call is the caller's problem — and the esp toolchain agrees: at `-O3` it
  treats **every FR as call-clobbered** (P4 objdump probe; floats arrive in
  `a2..a7`, spill via `s32i.n`, reload via `lsi`). M7's frame layout must
  provide spill slots; nothing else will.
- **BR `b0..b15`** — FP compares write Boolean registers only; `movt.s` /
  `movf.s` / `bt` / `bf` consume them.
- **FCR/FSR** are user registers 232/233. Bit layouts per ISA RM Table
  4-47/4-48, transcribed in `lp-xt-emu/src/cpu.rs`. Reset values measured 0.
- **CPENABLE bit 0 gates the FPU**; un-armed access raises EXCCAUSE 32. On
  this boot chain CPENABLE arrives as `0xff` (everything enabled) with the
  write's provenance unpinned — M7 arms it defensively anyway (2
  instructions), because "armed under this boot chain" is not "armed by
  architecture".

## 4. Numeric behavior — measured, row by row

Every row below is MEASURED on the desk S3; the proving family is named.
The policy lives as citations in `lp-xt-emu/src/fp_policy.rs`.

| Corner | Behavior | Proof |
|---|---|---|
| Default rounding | RNE; FCR resets to 0 | P1 + every family |
| **FCR.RM honored?** | **Yes.** All four modes work; the three directed modes match IEEE-754 directed rounding bit-for-bit, including subnormal and overflow endpoints (largest-finite under truncating modes) | F1: 556/648 operand groups mode-dependent; 1944/1944 directed rows exact |
| **Denormals** | **Full IEEE, no flushing** — subnormal operands compute, subnormal results emerge intact, in and out | F3: 350/350 IEEE; all 80 flush-distinguishing rows |
| NaN propagation | Last NaN operand in `(fs, ft)` order wins, **quiet bit forced, payload and sign preserved**; for `madd.s`/`msub.s` the accumulator `fr` outranks both | F2 (270 rows); acc priority pinned by the qNaN/qNaN divide sequence |
| Generated NaN | `0x7FC00000`, FSR INVALID | F2 + F4 (18 rows) |
| Signed zero | IEEE throughout (`+0 == -0`, sign-correct products, `neg.s`/`abs.s` are pure sign-bit ops) | F4: 206/206 |
| `madd.s`/`msub.s` | **Fused** — one rounding (RM p. 406, confirmed); `0×∞` product raises INVALID even when a quiet NaN accumulator propagates | F2 + probe grids |
| Conversion boundaries | Signed ops saturate (`0x7fffffff`/`0x80000000`), NaN → `0x7fffffff`; `utrunc.s` NaN/overflow → `0xffffffff` | F6 |
| **`utrunc.s` negatives** | **RM FALSIFIED**: in-range negatives truncate and **wrap like the signed conversion** (`-1.5 → 0xffffffff`, INVALID); the RM's `0x80000000` sentinel holds only below `i32::MIN`; a negative truncating to 0 is merely INEXACT | F6: the campaign's 16 DIVERGE rows |
| `round.s` ties | **To even** (`0.5→0`, `1.5→2`, `2.5→2`) | F6 |
| Scale immediate | Fractional-bits reading both directions (RM pp. 346/548, confirmed) | F6 scale sweep |
| `const.s` | `[0.0, 1.0, 2.0, 0.5]` selected by `imm & 3` | helpers capture |
| **FSR flags** | **RM §4.3.11.4 FALSIFIED — the flags work**: INEXACT on any rounded result; UNDERFLOW on tiny-and-inexact (after rounding); OVERFLOW with INEXACT; INVALID on sNaN operands, NaN generation, invalid conversions, and (IEEE signaling predicates) `olt.s`/`ole.s` with *any* NaN; DIV_BY_ZERO for finite/0 (raised by `mkdadj.s`, not by the estimates; ∞/0 raises nothing, per IEEE). Sticky, cleared only by `wur.fsr` | the FSR column of all 5 630 rows + 5 328 probes |
| sNaN | No traps ever; the INVALID *flag* is raised (quiet-bit-clear patterns are otherwise ordinary NaNs) | F2 |

## 5. Divide and square root

No divide or sqrt instruction exists. The normative sequences are the
toolchain's own — `__divsf3` (libgcc) and `__ieee754_sqrtf` (libm),
transcribed instruction-for-instruction from objdump and committed twice: as
`global_asm!` kernels in the device harness and as `Inst` vectors in
`tests/fp_conformance.rs`. These are what M7 emits.

The building blocks are implementation-defined and now measured
(`lp-xt-emu/src/fp_rom.rs`):

- **Three ROMs, not four tables**: `recip0.s`/`div0.s` share a 128-entry ROM
  (top 7 significand bits; 7 result bits at `frac[22:16]`);
  `rsqrt0.s`/`sqrt0.s` share an odd/even pair of 64-entry ROMs selected by
  biased-exponent parity. Extracted exhaustively — 60 RLE sweeps over 15
  `(sign, exponent)` planes each, and the model reproduces **every one of
  the ~503M measured points** (verified; replayed at run boundaries in CI).
- Denormal inputs are normalized first; exponent arithmetic continues below
  biased zero; results denormalize by truncating shift and saturate to ∞.
- `nexp01.s` normalizes into ±[1,4) by exponent parity and negates;
  `mksadj.s`/`mkdadj.s` encode exponent adjustments **split into two
  mod-256 byte fields** (`8·(A mod 32)` for `addexp.s`, `8·(A div 32) + 127`
  for `addexpm.s`, `frac[15:14] = 0b11`) plus a result-class channel
  (`m = 223`, codes ±0/±∞/NaN);
- `maddn.s` is bit-identical to `madd.s` at RNE on all 1 536 probe points —
  but never sets a flag;
- `divn.s` reassembles the split encoding (exponent excess sign-extended to
  8 bits, decomposed `8k + p`, result scale `A = 32k_r + (k_t mod 32)`,
  class window at `A − 384`), computes the fused sum with the exact residual
  as sticky, and rounds once — which is what makes the sequences correctly
  rounded. Honest caveat: its model is exact on the sequence envelope (all
  272 end-to-end rows, and 1 387/1 536 probe rows, count pinned); the
  off-envelope remainder is characterized in the campaign record and a
  second probe round is queued.

The F5 family proves the sequences end-to-end: 1 296/1 296 rows agree,
including 0/0, ±x/0, ∞/∞, NaN operands, denormal quotients, and
`sqrt(-0) = -0`.

## 6. Non-default rounding modes

Measurement decided D6: FCR.RM is real, so the emulator **implements** the
directed modes for the measured surface — `add.s`, `sub.s`, `mul.s` — and
**refuses loudly** for every other operation (madd, conversions, estimates,
sequences), where no measurement exists. Shader code never leaves RNE
(`float.md` §2), so the refusal is unreachable in production; it exists so an
unmeasured corner stays a panic instead of becoming a plausible default.

## 7. The toolchain FP ABI

`xtensa-esp32s3-elf-gcc` 14.2.0 at `-O3` (P4 static probe, objdump):

- **No FR is callee-saved.** Float arguments arrive in AR `a2..a7`; `wfr` at
  point of use; values live across `call8` spill through ARs/stack.
- The toolchain contracts `a*b + c` to `madd.s` at `-O3` — the fused
  semantics of §4 are what compiled C shaders already exhibit.

Consequence for M5/M7: calls into `-O3` builtins clobber every FR; the JIT
frame owns all spills.

## 8. Single source of truth

- `tests/fixtures/fp/*.txt` — 5 630 predictions (result + FSR), regenerable
  only from the emulator (`UPDATE_FP_GOLDENS=1`), never from a device.
- `tests/fixtures/fp/captures/` — the verbatim silicon transcripts.
- `tests/fp_conformance.rs` — predictions match the emulator, zero UNKNOWN.
- `tests/fp_silicon_replay.rs` — emulator matches the captures: ROM sweeps,
  helper probes, and the full family diff (5 630/5 630), **no board**.
- Re-run on hardware: `just fwtest-xt-fp-esp32s3 <port> [family|tables|helpers]`.

"The emulator is trusted" means: any change that moves it off silicon
behavior fails `fp_silicon_replay` immediately.

## 9. Consequences

- **M7**: frame layout owns FR spills (§7); arm CPENABLE defensively; emit
  the §5 sequences verbatim; never touch FCR. Division/sqrt results will be
  bit-exact against the emulator by construction.
- **M8**: corpus triage can treat `xtn.f32` vs `interp.f32` denormal
  differences as **nonexistent** — this silicon does not flush (Q2 answered
  in the best direction), removing an entire divergence class.
- **M9**: RV32F is spec-defined; no campaign needed. The policy-layer shape
  (measured-or-refuse) lifts to `lp-emu-core` if wanted.

## 10. Remains unverified

Stated, not implied away:

- **LX6 (classic ESP32) FPU** — untouched (plan Q5, future work).
- **Cycle/timing behavior** — out of scope repo-wide.
- `divn.s` off the sequence envelope — 149/1 536 round-1 probe rows
  (operand shapes no sequence produces); round-2 grids
  (`helpers::probe2`, 7 073 probes) are built, fingerprint-pinned, and
  queued behind a board replug.
- Non-default FCR.RM for ops other than `add.s`/`sub.s`/`mul.s` (refused).
- `maddn.s`/`divn.s` under non-default FCR.RM (refused; probes ran at RNE).
- FSR of the estimates on zero inputs in isolation (P1's `0x400` is
  attributable to its `mkdadj` probe; the family data pins `mkdadj` as the
  Z-flag source and the emulator models the estimates flag-free).
- NaN payload priority when the *accumulator* carries a distinctive payload
  (probe grids staged only canonical NaNs there; the chosen acc-first order
  is pinned by sequence behavior, and the round-2 `MaddNan` grids will close
  it).
- `CPENABLE`'s boot-time arming provenance (ROM vs 2nd-stage bootloader).
