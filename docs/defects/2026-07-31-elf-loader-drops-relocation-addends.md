---
status: fixed
found: 2026-07-31      # how: live-debugging
fixed: this change
area: lp-riscv/lp-riscv-elf
class: partial-knowledge-loss
related:
  - docs/defects/2026-07-31-zexth-encoding-steals-xori-128.md
  - docs/defects/2026-07-31-elf-loader-riscv-reloc-numbering.md
---
# The RV32 ELF loader applied `S` where the psABI says `S + A`

**Symptom** — None observed in production; found while auditing the loader as
a suspect for the `scene_render_emu` failure that turned out to be a decoder
bug (see the related entry). The loader is exercised today only by objects
whose relocations happen to carry a zero addend, so nothing had failed yet.

**Root cause** — Three places disagreed about whether a relocation's addend is
part of its value.

`R_RISCV_32` is `S + A`. Phase 2 wrote the bare symbol address. The addend is
where a symbol-plus-offset reference keeps its offset, so a `.rodata` table of
pointers into one merged constant — the shape rustc emits for a large `match`
returning `&'static str`, one relocation per element against a single string
blob — would have loaded with every element pointing at the blob's base.

`R_RISCV_PCREL_LO12_I` recomputed its target from the paired HI20
relocation's *symbol* only, while `handle_pcrel_hi20` had already folded the
addend into the high half. The two halves of one `auipc`/`lw` pair therefore
described addresses `A` bytes apart.

Separately, `identify_got_entries` decided what was a GOT slot by asking how
the *symbol* was spelled: any `R_RISCV_32` against a `__lp_`- or
`_ZN`-prefixed name became a GOT entry. That is a guess about a relocation's
meaning made from its name, and it is wrong for the most ordinary use of
`R_RISCV_32` there is — a pointer table. Worse, the tracker is keyed by symbol
name, so a table of N relocations against one symbol overwrote itself down to
one surviving slot address, which subsequent `GOT_HI20` / `PCREL_HI20`
relocations to that symbol would then have been routed through.

**Fix** — `R_RISCV_32` writes `S + A`; `PCREL_LO12_I` folds the HI20
relocation's addend into the target it recomputes; `identify_got_entries`
classifies by the relocation's *section* (`.got`, `.got.plt`, and their
subsections) instead of its symbol's prefix. A dead `handle_abs32` carrying
the same missing addend was deleted rather than fixed in parallel.

**Regression coverage** —
`lp-riscv-elf/src/elf_loader/relocations/tests.rs`, on hand-built
`object::write` fixtures rather than compiled Rust, so the shapes are pinned
by the psABI and not by a particular rustc's codegen:
`abs32_table_entries_keep_their_addends`,
`rodata_pointer_table_is_not_a_got`,
`pcrel_pair_folds_the_addend_into_both_halves`.

**Lesson** — Every one of these three is the same move: deciding what a
relocation *means* from something other than what it *is*. Two dropped a field
of the relocation record because the objects in front of them always left it
zero; the third read a symbol's name as a type tag. A loader is a
specification implementation, and the spec's formula is short enough to write
in the code next to the arithmetic — when the formula is `S + A` and the code
computes `S`, the missing term is not a simplification, it is a bet that no
input will ever use it.
