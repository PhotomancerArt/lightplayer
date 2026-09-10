---
status: fixed
found: 2026-09-09      # how: report
fixed: this change
area: lp-riscv/lp-riscv-inst (decode/encode/inst)
class: invented-encoding
related:
  - docs/defects/2026-07-31-zexth-encoding-steals-xori-128.md
  - docs/defects/2026-09-09-op-imm-bitmanip-dispatch-dead-and-mislabelled.md
---
# `bclri` disassembled as `bseti`, and three more Zb* words had no name at all

**Symptom** — `lp-riscv-inst` is the disassembler behind `lp-cli`,
`lpa-client` and the filetests. Fed the eight Zb* immediate words the
assembler actually emits for `riscv32imac-unknown-none-elf`, it produced:

```
0x48359513 bclri a0,a1,3  -> bseti a0, a1, 3   re-encode 0x28359513  <<< different instruction
0x6985d513 rev8  a0,a1    -> ERR Unknown I-type: funct3=0x5, funct7=0x34, funct6=0x1a, funct12=0x698
0x4835d513 bexti a0,a1,3  -> ERR Unknown I-type: funct3=0x5, funct7=0x24, funct6=0x12, funct12=0x483
0x28359513 bseti a0,a1,3  -> ERR Unknown I-type: funct3=0x1, funct6=0xa,  funct12=0x283
```

`rori`, `brev8`, `orc.b` and `binvi` were correct. Three of the four
failures are loud. The `bclri` one is not: it names a real instruction,
so a disassembly reads plausibly, and a disassemble/re-assemble round
trip silently rewrites a bit-clear into a bit-set at a *different*
encoding.

**Root cause** — three separate ways of not asking the assembler.

*Hex mis-transcribed from its own bit string.* The decoder's arms carried
the right funct6 in binary and the wrong one in hex, in the same comment:
`0x09 => Bclri` under `// BCLRI: funct6=0b010010 (0x09)`. `0b010010` is
`0x12`, not `0x09` — the bit string was read correctly and converted
wrongly, twice (`bclri`, `bexti`). `0x09` is nothing, so those two arms
were unreachable. The freed hex `0x12` was then handed to `bseti`, whose
real funct6 is `0x0a` — which is how a `bclri` word came out named
`bseti` while `bseti` itself stopped decoding.

*A sibling ISA's encoding used as this one's.* `rev8` is funct12 `0x698`
on RV32 and `0x6b8` on RV64, where the wider shift amount pushes the
field. Both `encode::rev8` and the decoder used `0x6b8`. They agreed with
each other, so the crate round-tripped its own output perfectly — on a
word that `llvm-objdump` renders as `.word` for `riscv32` with every Zb
extension enabled. The same held for Zba's `slli.uw`, which is RV64-only
outright: the assembler answers `instruction requires the following:
RV64I Base Instruction Set`, and the `Inst::SlliUw` doc comment's claim
that RV32 makes it "just a shift" was an invention.

*RV64's field layout applied to RV32 operands.* These forms were matched
and emitted against a 6-bit funct6 with a 6-bit shift amount. That is the
RV64 generalization; RV32 has a 5-bit shift amount and bit 25 belongs to
funct7. So `bclri` with shamt 35 encoded as funct7 `0x25` — a reserved
word — rather than being refused or masked.

**Not an execution defect.** `decode_instruction` is re-exported from
`lp-riscv-emu`'s `lib.rs` but is not on any execution path; the emulator
decodes in `emu/decoder.rs`. Nothing mis-executed here. What was wrong is
what a human reads.

**Fix** — the RV32 immediate forms now match and emit on **funct7**, the
field RV32 actually has, with the shift amount masked to 5 bits:
`bclri`/`bexti` `0x24`, `bseti` `0x14`, `binvi` `0x34`, `rori` `0x30`.
Those are the same values the register forms (`bclr`, `bset`, `binv`,
`bext`) were already using correctly one screen above, so the two halves
of each instruction now read alike. `rev8` encodes and decodes funct12
`0x698`. The `Inst::SlliUw` variant, its encoder and its decode arm are
deleted rather than corrected — there is no RV32 encoding to correct them
to. Both RV64 spellings get an explicit `Err` naming themselves, matching
how this decoder already refuses `ld`. `Inst::Orcb` printed `orcb`, which
no assembler accepts; it prints `orc.b`.

**Regression coverage** — `lp-riscv-inst/tests/instruction_tests.rs`:
`zb_immediate_forms_match_the_assembler` (all eight, plus the funct12
neighbours and the base-ISA shifts they sit beside — decode, re-encode
and printed mnemonic), `bclri_disassembles_as_bclri_not_bseti`,
`rev8_uses_the_rv32_funct12`,
`rv64_only_bitmanip_words_do_not_decode_on_rv32`,
`zb_shift_immediates_stay_within_five_bits`. All five fail on the old
code. Every word in them was assembled by `rustc --target
riscv32imac-unknown-none-elf` from a `global_asm!` block opening
`.option arch, +zba, +zbb, +zbs, +zbkb` and read back with
`llvm-objdump`; the three negative words were checked the same way, as
`.4byte` directives that `llvm-objdump` declines to name.

**Lesson** — The zext.h entry's rule was "encodings come from the spec,
never from the shape of a sibling instruction." This is its quieter half:
an encoding can be *sourced* correctly and still arrive wrong, because
the transcription is a second place to fail and nothing checks it. Two
guards follow from what actually happened here. First, a constant and the
comment explaining it are two claims about one fact, and a comment
carrying a bit string beside a hex value is a free consistency check that
no one ran — where the two disagree, the *code* is the one that was
typed last and the one to distrust. Second, a round-trip test through a
crate's own encoder and decoder proves only that they were written by the
same person on the same day; `rev8` round-tripped perfectly for as long
as it existed, on a word no RV32 assembler will ever produce. For any
external format, at least one end of every test has to be an artifact of
the real toolchain.
