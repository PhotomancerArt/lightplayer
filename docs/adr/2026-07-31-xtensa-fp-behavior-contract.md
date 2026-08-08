# The Xtensa FP behavior contract: silicon is the spec, and it has been read

- Status: **Accepted** — G2 passed 2026-08-01
- Date: 2026-07-31 (campaign); finalized 2026-08-01
- Plan: `2026-07-30-1745-f32-native-math/m6-xtensa-fpu-emulator` (M6 P6/P7)
- Deciders: Yona + M6 campaign
- Supersedes: none
- Superseded by: none
- Relates: `2026-07-28-xtensa-abi-contract` (experiment repo),
  `2026-07-28-emu-core-crate-family`, `2026-07-30-integer-division-never-traps`
  (*"It will be asked again — when F32 mode lands"*), `docs/design/float.md`
  §4 (Target-defined), and this planning directory

**G2 — Yona's hardware-conformance walk — passed 2026-08-01**, accepting all
four leans the campaign proposed: the emulator is trusted as the f32 oracle
for everything downstream (M7/M8/M9), the bounded `divn.s` off-envelope gap is
acceptable for M7 entry, denormals are recorded target-defined as measured
full-IEEE (§4, no product decision routed to M8), and non-default rounding
modes stay refused outside `add.s`/`sub.s`/`mul.s`. Full question-by-question
record: `m6-xtensa-fpu-emulator.md` §"Review gate: G2 — PASSED 2026-08-01" and
`p6-campaign-results.md` in the planning directory.

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

**Two corrections from M7's early implementation (P1–P4, PR #241), recorded
here so this contract stays the thing downstream cites instead of the plan
that predated the code:**

- **`f15` is reserved as the emitter's float scratch register — 15 of 16 FRs
  are allocatable, not 16.** The M7 plan's D8 assumed all 16 were free because
  spill/reload needs no third register — true, but it did not account for a
  **spilled def**: the register allocator can send an instruction's
  *destination* to `Alloc::Stack` when a later eviction in the backward walk
  freed its home, and an FP instruction still has to write its result
  somewhere before the store. `f15` is that somewhere. This is an M7 emitter
  decision, not a silicon fact — recorded here because it is the kind of
  thing the next reader of "the FR file is flat" needs to know before sizing
  a pool.
- **`!=` compiles to `oeq.s` + `movf`, not `ueq.s` + `movf`.** The M7 plan's
  compare table had it backwards: `ueq.s` consumed with `movf` computes
  `!ueq.s` = "ordered and unequal", which is **false when either operand is
  NaN** — but `docs/design/float.md` §3 makes `!=` on NaN a *Guaranteed*
  `true` (IEEE: any comparison with NaN except `!=` is false). `oeq.s` +
  `movf` computes `!oeq.s` = "unordered or unequal", which is correct on NaN.
  `lpvm-native`'s `fcmp_is_correct_when_an_operand_is_nan` emulator test is
  what caught it — the IEEE compare semantics were never in question (§4 and
  `float.md` §3 already state them correctly), only which *instruction pair*
  the emitter's compare table mapped `!=` onto. A plan-level table is not
  silicon behavior and can be wrong even when the ADR it is implementing is
  right.

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

- ~~**LX6 (classic ESP32) FPU** — untouched (plan Q5, future work).~~
  **Amended 2026-08-06: measured, and it agrees. This contract now covers both
  Xtensa FPUs in this project.** The same rig (lifted to `lp-xt-fp-harness`) ran
  the same corpus on a classic ESP32 rev v3.1 (MAC `30:76:f5:ec:f6:34`):
  **5 630 / 5 630 AGREE, 0 DIVERGE** against the predictions committed and
  fitted for the S3, on the same corpus fingerprint `0xa0a36dc3`.

  The stronger half is the estimate ROMs. `tables-esp32v3.txt` is
  **byte-identical** to the S3's `tables.txt` across all 1 570 sweep rows
  (369 865 bytes each) — the full 2²³ significand space over 15
  `(sign, exponent)` planes for each of `recip0.s` / `rsqrt0.s` / `sqrt0.s` /
  `div0.s`. Those tables are implementation-defined, so §5's characterization
  had no host oracle and could not have one; with a second part it does, and the
  answer is that both parts carry the same lookup silicon.

  So everything in this document — including the fitted `divn.s` model whose
  limits the entry below quantifies — applies to the LX6 without a per-chip arm.
  Captures and full provenance:
  `lp-xt/lp-xt-emu/tests/fixtures/fp/captures/README.md`.

  ⚠️ **This is a numeric-behavior result, not a performance one.** An f32 shader
  on the classic renders ~17 % *slower* than the same shader in Q32 (20 fps vs
  24 fps at 1500 LEDs) — see `docs/design/float.md`. Agreement to the bit says
  nothing about time, and §10 still excludes cycle behavior repo-wide.
- **Cycle/timing behavior** — out of scope repo-wide.
- `divn.s` off the sequence envelope — **measured 2026-08-01, and it is worse
  than the round-1 number suggested.** Round 2 (`helpers::probe2`, 7 073
  probes) ran on the same board: the fit reproduces **4 985 / 6 897**
  off-envelope rows (72.3%), against 90.3% on the grid it was fitted to, with
  the class region at `A − 384 ∈ 0..5` weakest at 41.7%. Still not a
  correctness problem — these are operand shapes no divide or sqrt sequence
  produces, and the model stays exact on all 272 end-to-end sequence rows and
  every family vector — but the limit is now quantified rather than assumed.
  **Anything that emits `divn.s` sequences directly (inline divide/sqrt) needs
  a re-fit first.** Counts pinned in `tests/fp_silicon_replay.rs`.
- Non-default FCR.RM for ops other than `add.s`/`sub.s`/`mul.s` (refused).
- `maddn.s`/`divn.s` under non-default FCR.RM (refused; probes ran at RNE).
- FSR of the estimates on zero inputs in isolation (P1's `0x400` is
  attributable to its `mkdadj` probe; the family data pins `mkdadj` as the
  Z-flag source and the emulator models the estimates flag-free).
- ~~NaN payload priority when the *accumulator* carries a distinctive
  payload.~~ **Closed 2026-08-01.** The round-2 `MaddNan`/`MaddnNan`/`DivnNan`
  grids staged four distinct payloads in every operand position: **176/176
  rows exact**, results and flags. Acc-first was previously *inferred* from
  sequence behaviour; it is now measured, and the model was already right.
  The row in the behaviour table above therefore stands on measurement rather
  than inference.
- `CPENABLE`'s boot-time arming provenance (ROM vs 2nd-stage bootloader).

None of these blocked M7, which shipped and ran on silicon (27/27). The
sequence envelope and the measured-surface rows above are what the emitter
actually produces.

**`probe2` has now run** (2026-08-01, `tests/fixtures/fp/captures/helpers2.txt`,
fingerprint `0x67c29b75`). It closed the accumulator-NaN item outright and
converted the `divn.s` item from an estimate into a pinned number — the two
outcomes are recorded separately above because they point in opposite
directions, and summarising them as "probe2 done" would lose the half that
constrains future work.

## 11. Alternatives considered

- **Write the emulator from the ISA Reference Manual alone, skip silicon.**
  Rejected — the manual predates the estimate/helper instructions entirely
  (its Table 4-46 does not list `recip0.s`/`rsqrt0.s`/`sqrt0.s`/`div0.s`/the
  divide-step helpers), so their lookup ROMs are not derivable from any
  document. A manual-only emulator would have to guess the one thing that
  most needed to be exact.
- **A full IEEE-754 soft-float core instead of native `f32` plus a policy
  layer.** Deferred, not rejected — native `f32` already agrees with the FPU
  bit-for-bit on the ordinary input space (§4), so a soft-float core would
  duplicate correct behavior to fix corners a policy layer already isolates.
  M9's RV32F is the second consumer that would justify building one; one
  consumer does not.
- **Sample the estimate ROMs rather than extract them exhaustively.**
  Rejected — a sampled table is *close*, and close is precisely the failure
  mode D5 exists to prevent: `div0.s`/`recip0.s`/`sqrt0.s`/`rsqrt0.s` values
  compiled into a shader would be silently wrong on whichever input the
  sample missed, with no way to tell which without re-sampling. Exhaustive
  extraction (60 RLE sweeps, ~503M points) costs one desk session and is
  exact by construction; sampling costs the same session and stays a guess.
- **Carry the campaign in the experiment repo, alongside the P1 capability
  probe.** Rejected on the runner protocol's own numbers: a `Response` is
  exactly one `u32` and every faulting payload costs a full board reboot
  (`PROTO_VERSION = 2`). Tens of thousands of (input → output, FSR) pairs
  plus a 2²³-wide table sweep do not fit that channel without a protocol
  version bump and firmware changes in two crates — lp2025's
  `fw-esp32s3`/`espflash flash --monitor` rig already existed and returns
  bulk results over the serial monitor for free (D1).
