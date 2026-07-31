---
status: open
found: 2026-07-31      # how: live-debugging
area: lp-riscv/lp-riscv-elf
class: invented-encoding
related:
  - docs/defects/2026-07-31-elf-loader-drops-relocation-addends.md
  - docs/defects/2026-07-31-zexth-encoding-steals-xori-128.md
---
# The RV32 ELF loader's relocation type numbers are off by one slot

**Symptom** — Not yet reached by a failing build. Surfaced while writing a
hand-built relocation fixture for `lp-riscv-elf`: emitting the psABI's
`R_RISCV_PCREL_HI20` produces

```
Unsupported relocation type 23 at offset 0x4 in section '.text'.
Supported types: R_RISCV_CALL_PLT (17), R_RISCV_PCREL_HI20 (20),
R_RISCV_PCREL_LO12_I (21/24), R_RISCV_32 (1), R_RISCV_GOT_HI20 (19)
```

**Root cause (as understood)** — The loader's names and numbers do not line up
with the RISC-V ELF psABI:

| loader calls it | number | psABI says |
|---|---|---|
| `R_RISCV_CALL_PLT` | 17 | `R_RISCV_JAL` |
| `R_RISCV_GOT_HI20` | 19 | `R_RISCV_CALL_PLT` |
| `R_RISCV_PCREL_HI20` | 20 | `R_RISCV_GOT_HI20` |
| `R_RISCV_PCREL_LO12_I` | 21 | `R_RISCV_TLS_GOT_HI20` |
| `R_RISCV_PCREL_LO12_I` | 24 | `R_RISCV_PCREL_LO12_I` ✓ |
| — (rejected) | 23 | `R_RISCV_PCREL_HI20` |

The mislabels are currently harmless by luck: `handle_got_hi20` with no GOT
entry patches an `auipc`+`jalr` pair, which is what a real `R_RISCV_CALL_PLT`
(19) needs, and `handle_pcrel_hi20` with no GOT entry does direct PC-relative
addressing, which is what a statically linked `R_RISCV_GOT_HI20` (20) needs.
So each handler happens to do the right thing for the relocation it actually
receives — under a different name.

What is *not* covered is the psABI's real `R_RISCV_PCREL_HI20` (23), which is
a hard error. It has never appeared because both the cranelift shader objects
and `object_file/tests.rs`'s fixture are built PIC, and PIC lowers global
references through `GOT_HI20` rather than `PCREL_HI20`. Any object compiled
`-C relocation-model=static` would fail to load.

**Fix** — not applied. The minimal safe step is additive: route 23 to
`handle_pcrel_hi20` as well, leaving the existing numbers alone. Renaming the
constants to match the psABI is the real repair but touches the handlers'
meaning, and should be done with a relocation corpus that can falsify it.

**Regression coverage** — none yet. The new
`pcrel_pair_folds_the_addend_into_both_halves` fixture in
`lp-riscv-elf/src/elf_loader/relocations/tests.rs` carries a
`LOADER_PCREL_HI20 = 20` constant and a comment pointing here, so the
mismatch is at least named at the one place that would otherwise quietly
teach the wrong numbering.

**Lesson** — Same shape as the `zext.h` entry filed the same day: a
number that means one thing to the toolchain was given a different name here,
and the code stayed correct only because the two meanings happened to want the
same handler. That is a coincidence, not a design, and it will stop holding
the first time a non-PIC object arrives.
