---
status: fixed
found: 2026-09-09      # how: reading, while writing the lp-emu-jit decoder-agreement test
fixed: this change
area: lp-emu/lp-riscv-emu
class: invented-encoding
related:
  - docs/defects/2026-07-31-zexth-encoding-steals-xori-128.md
  - docs/reports/2026-04-27-rv32-load16-issue.md
---
# Most of the OP-IMM bit-manipulation dispatch was dead, and one arm computed the opposite instruction

**Symptom** — None. Nothing this repository builds emits a B-extension
instruction: every target is `riscv32imac`, the ESP32-C6 traps all of it, and
no backend calls `lp_riscv_inst::encode::{rori, rev8, brev8, bexti, bclri,
bseti}`. The defect was found by reading, while `lp-emu-jit`'s
`decoder_agreement` test (M7 P1, PR #651) was being written to hold two
decoders to the same `(width, InstClass)`.

**Root cause** — `decode_execute_itype`'s `funct3 == 5` arm tested funct7 bit 5
first:

```rust
if funct7_bit5 {
    execute_srai::<M>(..)
} else if funct6 == 0x18 { execute_rori::<M>(..) }
  else if funct6 == 0x09 { execute_bexti::<M>(..) }
  else { match funct12 { 0x6b8 => execute_rev8, 0x287 => execute_orcb, 0x687 => execute_brev8, _ => execute_srli } }
```

`rori` (funct7 `0b0110000`), `rev8` (`0b0110100`), `brev8` (`0b0110100`) and
`bexti` (`0b0100100`) all have bit 5 set, so `srai` claimed every one of them
and the four arms below could not run. `orc.b` (`0b0010100`) is the single Zb*
encoding in this funct3 with the bit clear, and the only one that ever reached
its own arm.

Checking the block against an assembler — `rustc --target
riscv32imac-unknown-none-elf` over a `global_asm!` opening with
`.option arch, +zba,+zbb,+zbs,+zbkb`, disassembled with `llvm-objdump` — turned
up three more problems that dispatch order alone does not explain:

| written as | funct6 in the code | funct6 per the assembler | what it did |
| --- | --- | --- | --- |
| `bexti` | `0x09` | `0x12` | dead (funct7 bit 5), and the constant is wrong anyway |
| `rev8` | funct12 `0x6b8` | `0x698` on RV32 | dead; `0x6b8` is the **RV64** encoding |
| `bclri` | `0x09` | `0x12` | dead |
| `bseti` | `0x12` | `0x0a` | **`0x12` is `bclri`'s funct6**, so a `bclri` word ran `execute_bseti` |
| `bseti` (real encoding) | — | `0x0a` | no arm; faulted |
| `slli.uw` | `0x02` | RV64-only Zba | executed; on RV32 that funct7 is a reserved `slli` |

The `bclri` row is the only one that was not merely dead: `bclri a0, a1, 3`
executed as `bseti` and **set** the bit its mnemonic clears. Live on `main`,
harmless only because nothing emits `bclri`.

The comments record the slip: `// BCLRI: funct6=0b010010 (0x09)` and
`// BEXTI: funct6=0b010010 (0x09)` both carry the correct bit string beside the
wrong hex — `0b010010` is `0x12`. The same pair appears verbatim in
`lp-riscv/lp-riscv-inst/src/decode.rs`, which is disassembly and tooling only,
not an execution path; `encode::rev8` there likewise emits the RV64 word. Not
fixed here — see "Still open".

**Fix** — Delete rather than wire up. The rule applied: *an arm stays only if
it is reachable and its encoding and result match the assembler; nothing gains
reachability.* Removed `execute_rori`, `execute_rev8`, `execute_brev8`,
`execute_bexti`, `execute_bclri`, `execute_bseti` and `execute_slliuw` with
their dispatch. `binvi`, `orc.b`, `clz`, `ctz`, `cpop`, `sext.b` and `sext.h`
are reachable and agree with the assembler, and are untouched.

Making them reachable was the other option and was rejected: it would mean
correcting four encodings and turning on seven implementations that have never
executed once, so the emulator would accept more of an extension the C6 traps —
the direction `docs/reports/2026-04-27-rv32-load16-issue.md` warns about, and
away from `lp-emu-jit`, which refuses all Zb* on purpose.

**Behaviour change** — `bclri`, `bseti` and `slli.uw` words now fault instead of
executing. `bseti` already faulted. `bclri` previously returned the wrong
answer, and `slli.uw` does not exist on RV32; a fault is what the C6 does with
all three. Everything else is byte-identical: the four dead arms' encodings
executed as shifts before this change and execute as the same shifts after it.
`scripts/emu/oracle-sweep.sh` confirms the transcripts are unchanged.

**Regression coverage** —
`emu::executor::immediate::tests::zb_immediates_are_shifts_or_faults` pins the
whole OP-IMM Zb* surface using assembler-derived words: what each survivor
computes, that the four dead encodings are arithmetic shifts and *not* the
instruction they are written as, and that `bclri`/`bseti`/`slli.uw` fault. It
fails on the old code at the `bclri` assertion, with `Ok(0x12345678)` — the
`bseti` answer — where `Err` is expected.

**Still open** — Whether an RV32IMAC emulator should execute any of the
survivors, rather than trapping them as the chip does, is the target-profile
question from `docs/reports/2026-04-27-rv32-load16-issue.md`. This change does
not answer it. `lp-riscv-inst`'s decoder and `encode::rev8` carry the same
wrong constants and are also unfixed.

**Lesson** — The `zext.h` defect's rule was "encodings come from the spec, never
from the shape of a sibling". This block shows the second half of it: a
dispatch chain that tests a *summary* of a field before testing the field can
make correct encodings unreachable, and unreachable code cannot be wrong in any
way a test will notice. Both halves failed silently here for the same reason —
no guest emits these instructions, so nothing ever disagreed. When a decoder
arm has never run, the question is not whether to fix it but whether it should
exist.
