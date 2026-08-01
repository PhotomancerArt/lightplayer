# M6 P6 silicon captures

The behavior contract these captures back is
`docs/adr/2026-07-31-xtensa-fp-behavior-contract.md`; the row-by-row triage
that produced them is `p6-campaign-results.md` in the M6 planning directory.

Verbatim serial transcripts (filtered to their `[FPCONF]` lines) from the
hardware conformance campaign — desk sessions **2026-07-31** (first three) and
**2026-08-01** (`helpers2.txt`), same board both times:

| File | Contents |
|---|---|
| `tables.txt` | 60 run-length-encoded estimate-ROM sweeps: the full 2²³ significand space over 15 `(sign, exponent)` planes for each of `recip0.s` / `rsqrt0.s` / `sqrt0.s` / `div0.s` |
| `helpers.txt` | 5 328 divide-step helper probes (`nexp01.s`, `mksadj.s`, `mkdadj.s`, `addexp.s`, `addexpm.s`, `maddn.s`, `divn.s`, plus `madd.s` as the contrast row) and the sixteen `const.s` outputs |
| `families.txt` | All 5 630 conformance vectors of the six families — result bits **and** FSR |
| `helpers2.txt` | 7 073 **second-round** probes (desk session **2026-08-01**): the `divn.s` exponent-reassembly map, the fine sweep through the class region, sign combinations, a non-zero `s`, and mixed-sign distinct-payload NaN grids for `madd.s`/`maddn.s`/`divn.s` |

## What the second round settled

`helpers2.txt` was designed from the first round's fit and captured at the
next desk window. It answered its two questions in opposite directions, which
is why both are stated here rather than summarised as "queued work done":

- **Accumulator NaN priority: closed.** 176/176 rows exact, results and flags,
  with four distinct payloads in every operand position. The first round staged
  only the canonical NaN, so priority had been *inferred* from sequence
  behaviour; it is now measured, and the model was already right.
- **`divn.s` off-envelope: quantified, not closed.** 4 985 / 6 897 (72.3%),
  against 90.3% on the grid the model was fitted to. The class region at
  `A − 384 ∈ 0..5` is weakest at 41.7%. The model remains exact everywhere
  emitted code can reach — all 272 end-to-end sequence rows, every family
  vector — so this is a bounded limit rather than a defect. It does mean that
  inlining divide/sqrt on `divn.s` sequences would need a **re-fit first**.

Counts are pinned as equalities in `tests/fp_silicon_replay.rs`. An
"improvement" without a re-fit is as suspicious as a regression.

## Board identity

- XIAO-class ESP32-S3 devkit, chip rev **v0.2**, MAC **d8:3b:da:47:29:70**
- 16 MB flash, always flashed `--flash-size 8mb`
- Port at capture time `/dev/cu.usbmodem1201` — ports renumber; identify by MAC
- Firmware commit `4e7a3da28728` (first round) / `d01cdb4c8f81` (`helpers2.txt`),
  feature `test_xt_fp_conformance`
- Toolchain: espup `esp-14.2.0_20240906` (`xtensa-esp32s3-elf-gcc` 14.2.0)

> The `helpers2.txt` header reads `cpenable before=0x000000ff after=0x00000001`.
> That is the conformance rig deliberately setting a known value, and it is
> also the concrete reason `fw-esp32s3`'s app-path arming
> (`board::esp32s3::fpu::arm`) is a read-modify-write: a blind store of `1`
> **does** disable coprocessors 1–7, as this line shows it doing.

## Rules

- **Never edit a capture.** These are measurements. A replay mismatch
  (`tests/fp_silicon_replay.rs`) is an emulator regression, never a reason to
  touch the transcript.
- **The promotion direction was one-way and is over.** The emulator's
  predictions were committed *before* these captures existed and diffed
  against them; only after triage did measured values flow into the model.
  That order is what made the comparison meaningful. Do not generalize it:
  refreshing any golden from device output downstream inverts a test into a
  tautology that passes forever (see `lpvm-native/src/xt_corpus.rs`).
- A regenerated corpus (`UPDATE_FP_GOLDENS=1`) regenerates **predictions from
  the emulator** — never from these files.
