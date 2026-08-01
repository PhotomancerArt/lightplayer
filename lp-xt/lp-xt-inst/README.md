# lp-xt-inst

Xtensa (ESP32-S3, LX7) instruction model, encoder, decoder, and disassembler for
the subset lightplayer's backend emits and its fixtures exercise. Mirrors
`lp-riscv-inst`'s role: every other layer (emulator decode, emitter encode,
disassembly in traces and tests) sits on this crate.

`#![no_std]` + `alloc` so it can later run on-device; tested on the host.

## What it covers

- **Model** (`Inst`): RISC-core ALU (`add`/`sub`/`and`/`or`/`xor`/`addx*`/`subx*`/
  `src`/`neg`/`abs`/`min`/`max`/`minu`/`maxu`/mul32/div32/`mul16*`/cmov), shifts
  (`sll`/`sra`/`srl`/`slli`/`srli`/`srai`/`ssl`/`ssr`/`ssai`/`extui`/`sext`),
  `addi`/`addmi`/`movi`, loads/stores (`l8ui`/`l16ui`/`l16si`/`l32i`/`s8i`/`s16i`/
  `s32i`), `l32r`, all core branches (`beq`/`bne`/`blt`/`bge`/`bltu`/`bgeu`/`ball`/
  `bany`/`bnall`/`bnone`/`bbc`/`bbs`/`beqi`/`bnei`/`blti`/`bgei`/`bltui`/`bgeui`/
  `beqz`/`bnez`/`bltz`/`bgez`/`bbci`/`bbsi`), `j`/`jx`, `call0/4/8/12`/
  `callx0/4/8/12`/`ret`/`retw`, `entry`, `movsp`, `nsa`/`nsau`, barriers
  (`memw`/`extw`/`isync`/`rsync`/`esync`/`dsync`/`nop`/`ill`), and the narrow
  16-bit density forms (`add.n`/`addi.n`/`mov.n`/`movi.n`/`l32i.n`/`s32i.n`/
  `ret.n`/`retw.n`/`nop.n`/`ill.n`/`beqz.n`/`bnez.n`).
- **Floating point** (`FReg` = `f0..f15`, a separate type from `Reg` — the FR
  file is flat where the AR file is windowed, and mixing them up is the mistake
  worth making impossible): arithmetic (`add.s`/`sub.s`/`mul.s`/`madd.s`/
  `msub.s`/`maddn.s`/`divn.s`), unary and transfer (`mov.s`/`abs.s`/`neg.s`/
  `const.s`/`rfr`/`wfr`), the divide/sqrt helper family (`div0.s`/`recip0.s`/
  `sqrt0.s`/`rsqrt0.s`/`nexp01.s`/`mkdadj.s`/`addexp.s`/`addexpm.s`), compares
  (`oeq.s`/`olt.s`/`ole.s`/`ueq.s`/`ult.s`/`ule.s`/`un.s`), conditional moves
  (`moveqz.s`/`movnez.s`/`movltz.s`/`movgez.s`/`movf.s`/`movt.s`), conversions
  (`round.s`/`trunc.s`/`floor.s`/`ceil.s`/`utrunc.s`/`float.s`/`ufloat.s`), and
  load/store (`lsi`/`ssi`/`lsip`/`ssip`/`lsx`/`ssx`/`lsxp`/`ssxp`).
- **Boolean registers** (`BReg` = `b0..b15`): `bt`/`bf` and `movt`/`movf`. FP
  compares write here, not to an AR, so without them a compare result cannot be
  read back at all.
- **Special / user registers, narrowly**: `rsr`/`wsr`/`xsr` for `BR` and
  `CPENABLE`, `rur`/`wur` for `FCR` and `FSR`. Deliberately not a general SR
  model — see `src/sr.rs` for which four registers earn their place and why.
  The normative FP subset table, with what is and is not silicon-verified, is
  the module doc of `src/fp.rs`.
- **Variable-length decode**: `decode(&[u8]) -> (Inst, len)`. Length (2 or 3 bytes)
  comes from the density rule on the first byte *before* opcode recognition, so an
  unsupported opcode still reports the right length to advance by.
- **Encode**: `encode(&Inst) -> Vec<u8>` (little-endian). Exact inverse of
  `decode` — round-trip property-tested across the whole set.
- **Disassemble**: `format_instruction(&[u8], pc) -> String`, objdump-style, with
  `l32r`/branch/call targets resolved to absolute addresses.

Out of scope (reported as `DecodeError::Unsupported`, never silently skipped):
ESP32-S3 DSP (`ee.*`), every special register outside the four named above
(`rsil`/TLB/`PS`/`SAR`/…), atomics (`s32c1i`/`s32ri`/`l32ai`), the boolean
*logic* ops (`xorb`/`andb`/`orb`/`all4`/`any8`/…), windowed spill
(`l32e`/`s32e`), loop instructions, and double precision (not on this chip).

## Testing

```bash
cargo test -p lp-xt-inst
# Differential disassembler conformance rig over any Xtensa ELF:
lp-xt/fixtures/build.sh && lp-xt/fixtures/fp/build.sh
cargo run -p lp-xt-inst --features objdiff --bin objdiff -- \
    lp-xt/fixtures/fp/obj/fp_subset.elf
```

The `objdiff` rig disassembles the entire `.text` of an ELF with this crate and
diffs it against `xtensa-esp32s3-elf-objdump -d`. Every instruction is either
matched (mnemonic + operand values, resolving hex/decimal/target formatting) or
placed on a printed, counted UNSUPPORTED allowlist. The objdump binary defaults
to the espup toolchain path and can be overridden with `$XT_OBJDUMP` or a second
CLI argument.

Scores, all with **zero mismatches**:

| ELF | matched | unsupported |
|---|---|---|
| `fixtures/fp/obj/fp_subset.elf` — the whole FP/Boolean/SR subset | 134 | 0 |
| the 14 `fixtures/elf/*.elf` Rust corpus binaries, summed | 26,063 | 346 |
| the experiment repo's spike ELF (2026-esp32s3-experiment) | 10,969 | 329 → see note |

The corpus figure is the one to trend: adding FP/Boolean/SR decode moved it from
**26,010 matched / 399 unsupported** to **26,063 / 346**, measured on the same
fourteen ELFs. Those 53 instructions are not real FP code — the fixtures are
integer-only by rule — they are literal-pool words that objdump disassembles as
garbage `ule.s` / `moveqz.s` / `lsx`, which this crate now agrees with byte for
byte instead of refusing. The spike figure predates the FP work and will move the
same way when someone re-runs it.

Golden vectors GV1–GV3b (from the spike, `FINDINGS.md`) are decode/encode unit
tests in `tests/golden_vectors.rs`; the FP/Boolean/SR goldens are in
`tests/fp_golden_vectors.rs`, derived by the procedure written down in
`lp-xt/fixtures/fp/README.md`.

## Provenance

Instruction **encoding data** — bit layouts, opcode field values, and operand
ranges — is derived from the Apache-2.0-with-LLVM-exception TableGen sources of
`espressif/llvm-project`:

- `llvm/lib/Target/Xtensa/XtensaInstrFormats.td`
- `llvm/lib/Target/Xtensa/XtensaInstrInfo.td`
- `llvm/lib/Target/Xtensa/XtensaOperands.td`
- commit `f6ee8246025cea8986ce90f5fe3660efcd66cb5f`

License text: `licenses/LLVM-Apache-2.0-with-LLVM-exception.txt`.

PC-relative target formulas (`l32r`, branch, `call`) and the density
instruction-length rule are facts from the Xtensa ISA Reference Manual,
cross-checked against `xtensa-esp32s3-elf-objdump`.

No GPL source was copied, transliterated, or line-by-line adapted. binutils
(`xtensa-modules.c`) and QEMU were **not** used — they are behavioral references
only per `docs/adr/2026-07-29-license-provenance-discipline.md`. Per-file
provenance headers appear at the top of `src/lib.rs`, `src/decode.rs`, and
`src/encode.rs`.
