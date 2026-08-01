# M6 P6 silicon captures

The behavior contract these captures back is
`docs/adr/2026-07-31-xtensa-fp-behavior-contract.md`; the row-by-row triage
that produced them is `p6-campaign-results.md` in the M6 planning directory.

Verbatim serial transcripts (filtered to their `[FPCONF]` lines) from the
hardware conformance campaign, desk session **2026-07-31**:

| File | Contents |
|---|---|
| `tables.txt` | 60 run-length-encoded estimate-ROM sweeps: the full 2²³ significand space over 15 `(sign, exponent)` planes for each of `recip0.s` / `rsqrt0.s` / `sqrt0.s` / `div0.s` |
| `helpers.txt` | 5 328 divide-step helper probes (`nexp01.s`, `mksadj.s`, `mkdadj.s`, `addexp.s`, `addexpm.s`, `maddn.s`, `divn.s`, plus `madd.s` as the contrast row) and the sixteen `const.s` outputs |
| `families.txt` | All 5 630 conformance vectors of the six families — result bits **and** FSR |

## Board identity

- XIAO-class ESP32-S3 devkit, chip rev **v0.2**, MAC **d8:3b:da:47:29:70**
- 16 MB flash, always flashed `--flash-size 8mb`
- Port at capture time `/dev/cu.usbmodem1201` — ports renumber; identify by MAC
- Firmware commit `4e7a3da28728`, feature `test_xt_fp_conformance`
- Toolchain: espup `esp-14.2.0_20240906` (`xtensa-esp32s3-elf-gcc` 14.2.0)

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
