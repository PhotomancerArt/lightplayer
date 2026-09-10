# lp-xt-inst

Xtensa (LX6 and LX7 — ESP32 and ESP32-S3) instruction model, encoder, decoder,
and disassembler. Mirrors `lp-riscv-inst`'s role: every other layer (emulator
decode, emitter encode, disassembly in traces and tests) sits on this crate.

`#![no_std]` + `alloc` so it can later run on-device; tested on the host.

## Coverage

As of M1 P1 this is **not a compiler subset**. It decodes, encodes and
disassembles the ISA the shipped firmware, the classic mask ROM and the ESP-IDF
second-stage bootloader actually contain — including the privileged and
machine-mode families a bare-metal hart has to run — and it holds **no
behaviour** for any of it: no execution, no register state, no semantics. What
a `wsr.ps` *does* belongs to the machine-mode hart, not here.

Measured with this crate's own `objdiff` rig against
`xtensa-esp32-elf-objdump`, over **every executable section** of each artefact:

| artefact | instructions | matched | coverage | mismatched | unsupported |
|---|---:|---:|---:|---:|---:|
| shipped `fw-esp32v3` image (LX6) | 721,066 | 712,818 | **98.86 %** | 1 | 8,247 |
| classic mask ROM `esp32_rev300_rom.elf` | 144,594 | 144,539 | **99.96 %** | 0 | 55 |
| IDF 2nd-stage bootloader (segments 1 + 3) | 7,041 | 6,638 | **94.28 %** | 0 | 403 |

M0's inventory measured the same three artefacts before this widening at
98.21 %, 95.69 % and 93.58 % (`docs/reports/2026-09-10-xtensa-firmware-isa-inventory.md`).

### What the residue is, by name — and why a percentage is not the deliverable

**Almost all of it is objdump mis-disassembling literal pools.** Xtensa literal
pools sit inside `.text` interleaved with code and carry no marker; objdump
walks them as instructions and prints plausible mnemonics for constants. Two
LX6-objdump behaviours make this loud:

- **`lsi` (7,360 sites in the image, 38 in the ROM, 314 in the bootloader) is
  an objdump artefact, not a missing instruction.** This crate decodes `lsi`
  and has since M6. `xtensa-esp32-elf-objdump` carries a **loose** `lsi` table
  entry that matches essentially any word whose `(op0, op1, op2, r)` fall in an
  unassigned slot: `0x400040`, `0x401040`, `0x402040`, `0x403040`, `0x404040`,
  `0x405040`, `0x409040` … all disassemble as the *same* `lsi f4, a0, 0x100`,
  and so does `0x404044` (`op0 = 4`). The assembler encodes that `lsi` as
  `0x400043` and nothing else; `xtensa-esp32s3-elf-objdump` (LX7) calls
  `0x404040` undecodable. Treating those words as `lsi` would mean
  transliterating a binutils bug into the crate.
- **The `f64*` group** (`f64cmph` 566, `f64addc` 91, `f64iter` 72, `f64rnd` 47,
  `f64sexp` 38, `f64norm` 21, `f64cmpl` 6, `f64subc` 6, `f64sig` 2, `wf64r` 1
  in the image; similar shares elsewhere) is the ESP32 LX6 double-precision
  **acceleration** option. It occupies `op0 = 0`, `op1 = 0xE`/`0xF` — 1/256 of
  the 24-bit space — and a D-bus address literal such as `0x3FXXXXX0` lands in
  exactly that slot, which is why the count tracks the literal-pool volume.
  807 of the 836 `f64*` sites in the shipped image sit within three lines of a
  `.byte` directive or of one of the bogus `lsi`s above, the image contains no
  soft-double helper symbol (`__adddf3` and friends are absent), and the LX7
  assembler does not know the mnemonics at all. **Not decoded**; the `F64R_LO`
  / `F64R_HI` / `F64S` *user registers* are.

The rest of the residue, by name:

- **`ret` (7), `ret.n` (15), `retw.n` (1)** — objdump renders any `s` field as
  `ret.n`; the assembler emits only `s = 0` (`0xF00D`). This crate follows the
  assembler, so a word with `s ≠ 0` is not a `ret.n`.
- **`all4` (3), `all8` (1), `any4` (1)** — the boolean reductions read an
  *aligned* group base. `all4 b0, b1` and `all8 b0, b4` are assembler errors;
  objdump silently aligns the base down and prints a group anyway.
- **`s32nb` (8), `sddr32.p` (1), `simcall` (2, ROM), `wrmsk_expstate`,
  `read_impwire`, `excw`, `rfdo`** — genuinely outside this crate's set. None
  is on a boot, idle or walk path.

The **one** remaining `MISMATCHED` site in the shipped image (`0x40197020`,
word `0x1AD544`) is the loose-`lsi` entry again, disagreeing with this crate's
`mula.dd.lh.lddec`. The assembler round-trips that MAC16 form exactly
(`mula.da.ll.lddec m1, a9, m1, a9` ⇄ `0x585994`, both toolchains), so the
disagreement is objdump's.

> ⚠️ **Do not measure this with a per-section lifted ELF.** Lifting a section
> into a one-section ELF (as M0's `census.py` does) drops the Xtensa
> configuration the original ELF carries, and objdump then falls back to a
> config-less table where the loose `lsi` beats the real MAC16 entries. The
> classic ROM scores 99.96 % with zero mismatches read from the original ELF
> and 96.52 % with sixty mismatches read section-by-section — same bytes, same
> objdump binary, same decoder.

## What it covers

- **Model** (`Inst`): RISC-core ALU (`add`/`sub`/`and`/`or`/`xor`/`addx*`/`subx*`/
  `src`/`neg`/`abs`/`min`/`max`/`minu`/`maxu`/mul32/div32/`mul16*`/cmov), shifts
  (`sll`/`sra`/`srl`/`slli`/`srli`/`srai`/`ssl`/`ssr`/`ssai`/`extui`/`sext`/
  `clamps`), `addi`/`addmi`/`movi`, loads/stores (`l8ui`/`l16ui`/`l16si`/`l32i`/
  `s8i`/`s16i`/`s32i`), `l32r`, all core branches (`beq`/`bne`/`blt`/`bge`/`bltu`/
  `bgeu`/`ball`/`bany`/`bnall`/`bnone`/`bbc`/`bbs`/`beqi`/`bnei`/`blti`/`bgei`/
  `bltui`/`bgeui`/`beqz`/`bnez`/`bltz`/`bgez`/`bbci`/`bbsi`), `j`/`jx`,
  `call0/4/8/12`/`callx0/4/8/12`/`ret`/`retw`, `entry`, `movsp`, `nsa`/`nsau`,
  barriers (`memw`/`extw`/`isync`/`rsync`/`esync`/`dsync`/`nop`/`ill`), and the
  narrow 16-bit density forms (`add.n`/`addi.n`/`mov.n`/`movi.n`/`l32i.n`/
  `s32i.n`/`ret.n`/`retw.n`/`nop.n`/`ill.n`/`beqz.n`/`bnez.n`/`break.n`).
- **Synchronising memory access**: `l32ai`, `s32ri`, `s32c1i` (with `SCOMPARE1`
  and `ATOMCTL`), kept in their own operand enum because their semantics differ
  from `l32i`/`s32i` even though their shape does not.
- **Zero-overhead loops**: `loop`, `loopnez`, `loopgtz`, with `LBEG`/`LEND`/
  `LCOUNT`. `disasm::loop_end(pc, imm8)` is the single place the `LEND` address
  is computed (`pc + 4 + imm8`, imm8 **unsigned**) — a decoder that drops the
  loop back-edge produces a wrong answer with no fault, so nothing recomputes
  it locally.
- **Privileged control flow and the register window**: `rsil`, `waiti`, `rfe`,
  `rfi`, `rfde`, `rfwo`, `rfwu`, `rotw`, `l32e`, `s32e`, `break`, `syscall`.
- **The full LX6/LX7 SR/UR space**: `rsr`/`wsr`/`xsr` over 67 special registers
  and `rur`/`wur` over 7 user registers, with the access asymmetries modelled
  rather than smoothed over (SR 226 reads `INTERRUPT` and writes `INTSET`,
  SR 227 `INTCLEAR` and SR 89 `MMID` are write-only, SR 235 `PRID` is
  read-only). `from_num` is deliberately **partial** — see `src/sr.rs`.
- **The boolean register file** (`BReg` = `b0..b15`): `bt`/`bf`, `movt`/`movf`,
  the logic ops `andb`/`andbc`/`orb`/`orbc`/`xorb`, and the aligned-group
  reductions `all4`/`any4`/`all8`/`any8`.
- **MAC16** (`MReg` = `m0..m3`): `umul`/`mul`/`mula`/`muls` × `aa`/`ad`/`da`/`dd`
  × the four half selectors, `mula.*.ldinc`/`.lddec`, and `ldinc`/`lddec`.
- **Region protection**, decode and disassembly only: `ritlb0/1`, `rdtlb0/1`,
  `pitlb`, `pdtlb`, `witlb`, `wdtlb`, `iitlb`, `idtlb`, plus `rer`/`wer`.
- **Floating point** (`FReg` = `f0..f15`, a separate type from `Reg` — the FR
  file is flat where the AR file is windowed, and mixing them up is the mistake
  worth making impossible): arithmetic (`add.s`/`sub.s`/`mul.s`/`madd.s`/
  `msub.s`/`maddn.s`/`divn.s`), unary and transfer (`mov.s`/`abs.s`/`neg.s`/
  `const.s`/`rfr`/`wfr`), the divide/sqrt helper family (`div0.s`/`recip0.s`/
  `sqrt0.s`/`rsqrt0.s`/`nexp01.s`/`mksadj.s`/`mkdadj.s`/`addexp.s`/
  `addexpm.s`), compares (`oeq.s`/`olt.s`/`ole.s`/`ueq.s`/`ult.s`/`ule.s`/
  `un.s`), conditional moves (`moveqz.s`/`movnez.s`/`movltz.s`/`movgez.s`/
  `movf.s`/`movt.s`), conversions (`round.s`/`trunc.s`/`floor.s`/`ceil.s`/
  `utrunc.s`/`float.s`/`ufloat.s`), and load/store (`lsi`/`ssi`/`lsip`/`ssip`/
  `lsx`/`ssx`/`lsxp`/`ssxp`). The normative FP subset table, with what is and is
  not silicon-verified, is the module doc of `src/fp.rs`.
- **Variable-length decode**: `decode(&[u8]) -> (Inst, len)`. Length (2 or 3 bytes)
  comes from the density rule on the first byte *before* opcode recognition, so an
  unsupported opcode still reports the right length to advance by.
- **Encode**: `encode(&Inst) -> Vec<u8>` (little-endian). Exact inverse of
  `decode` — round-trip property-tested across the whole set.
- **Disassemble**: `format_instruction(&[u8], pc) -> String`, objdump-style, with
  `l32r`/branch/call/loop targets resolved to absolute addresses.

Out of scope, reported as `DecodeError::Unsupported` and never silently
skipped: the ESP32-S3 `ee.*` DSP extension, the LX6 `f64*` double-precision
acceleration group, `s32nb`, `sddr32.p`, `simcall`, `read_impwire`,
`wrmsk_expstate`, `excw`, `rfdo`, and double precision proper (not on either
chip).

### Reserved fields are required to be zero

Where the assembler always emits a field as zero, this crate requires it. A
word with a reserved field set is **not** that instruction and decodes as
`Unsupported`, because an instruction that decodes into something plausible but
wrong is the failure mode this crate exists to prevent. M0's inventory found
one such bug and M1 P1 fixed it: `ssai`'s `t{3-1}` is reserved, and accepting
any `t` mis-decoded the literal-pool word `0x404040` as `ssai 0` at six sites
in the shipped image and one in the classic ROM.

## Testing

```bash
cargo test -p lp-xt-inst
# Differential disassembler conformance rig over any Xtensa ELF:
lp-xt/fixtures/build.sh && lp-xt/fixtures/fp/build.sh
cargo run -p lp-xt-inst --features objdiff --bin objdiff -- \
    lp-xt/fixtures/fp/obj/fp_subset.elf
```

The `objdiff` rig disassembles **every `SHF_EXECINSTR` section** of an ELF with
this crate and diffs it against `xtensa-esp*-elf-objdump -d`. Every instruction
is either matched (mnemonic + operand values, resolving hex/decimal/target
formatting) or placed on a printed, counted UNSUPPORTED allowlist. Data
directives are counted apart and must stay that way — reclassifying them would
"improve" the number by deleting the evidence. The objdump binary defaults to
the espup toolchain path and can be overridden with `$XT_OBJDUMP` or a second
CLI argument; use the **LX6** binary (`xtensa-esp32-elf-objdump`) on an LX6
artefact.

Golden vectors GV1–GV3b (from the spike, `FINDINGS.md`) are decode/encode unit
tests in `tests/golden_vectors.rs`; the FP/Boolean/SR goldens are in
`tests/fp_golden_vectors.rs`; the machine-mode families M1 P1 added are in
`tests/machine_golden_vectors.rs`, all derived by the procedure written down in
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

Special/user-register numbers and field layouts, and every family M1 P1 added,
are **assembler-derived**: assembled from one-instruction `.S` files with both
`xtensa-esp32-elf-as` (LX6) and `xtensa-esp32s3-elf-as` (LX7) and read back with
the matching `-objdump -d`. Repo rule, and it holds here: *get encodings from an
assembler, never by analogy.*

PC-relative target formulas (`l32r`, branch, `call`, `loop`) and the density
instruction-length rule are facts from the Xtensa ISA Reference Manual,
cross-checked against `xtensa-esp32s3-elf-objdump`.

No GPL source was copied, transliterated, or line-by-line adapted. binutils
(`xtensa-modules.c`) and QEMU were **not** used — they are behavioral references
only per `docs/adr/2026-07-29-license-provenance-discipline.md`. Per-file
provenance headers appear at the top of `src/lib.rs`, `src/decode.rs`,
`src/encode.rs` and `src/sr.rs`.

### LX6 vs LX7

Every family in this crate was assembled with **both** toolchains and the bytes
compared. They agree everywhere except:

- the user registers `EXPSTATE` (230), `F64R_LO` (234), `F64R_HI` (235) and
  `F64S` (236) — LX6 accepts them, LX7 rejects the mnemonics outright;
- the `f64*` instruction group — LX6 only, and not decoded here.

`MEMCTL` (SR 97) is **not** a divergence: both assemblers accept it, contrary to
the M1 planning note's expectation.
