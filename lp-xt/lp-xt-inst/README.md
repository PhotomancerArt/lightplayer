# lp-xt-inst

Xtensa (ESP32-S3, LX7) instruction model, encoder, decoder, and disassembler for
the integer subset lightplayer's backend emits and its fixtures exercise. Mirrors
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
- **Variable-length decode**: `decode(&[u8]) -> (Inst, len)`. Length (2 or 3 bytes)
  comes from the density rule on the first byte *before* opcode recognition, so an
  unsupported opcode still reports the right length to advance by.
- **Encode**: `encode(&Inst) -> Vec<u8>` (little-endian). Exact inverse of
  `decode` — round-trip property-tested across the whole set.
- **Disassemble**: `format_instruction(&[u8], pc) -> String`, objdump-style, with
  `l32r`/branch/call targets resolved to absolute addresses.

Out of scope (reported as `DecodeError::Unsupported`, never silently skipped):
FPU (`*.s`), ESP32-S3 DSP (`ee.*`), system/privileged (`rsr`/`wsr`/`rsil`/TLB),
atomics (`s32c1i`/`s32ri`/`l32ai`), boolean (`xorb`/`andb`), windowed spill
(`l32e`/`s32e`), and loop instructions.

## Testing

```bash
cargo test -p lp-xt-inst
# Differential disassembler conformance rig over any Xtensa ELF, e.g. the
# experiment repo's spike ELF (2026-esp32s3-experiment):
cargo run -p lp-xt-inst --features objdiff --bin objdiff -- \
    <path-to>/xtensa-esp32s3-none-elf/release/spike-esp32s3
```

The `objdiff` rig disassembles the entire `.text` of an ELF with this crate and
diffs it against `xtensa-esp32s3-elf-objdump -d`. Every instruction is either
matched (mnemonic + operand values, resolving hex/decimal/target formatting) or
placed on a printed, counted UNSUPPORTED allowlist. Over the spike ELF it reports
**10969 / 10969 supported instructions matched, 0 mismatches** (329 unsupported,
all genuinely out-of-scope). The objdump binary defaults to the espup toolchain
path and can be overridden with `$XT_OBJDUMP` or a second CLI argument.

Golden vectors GV1–GV3b (from the spike, `FINDINGS.md`) are decode/encode unit
tests in `tests/golden_vectors.rs`.

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
