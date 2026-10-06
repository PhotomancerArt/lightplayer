---
status: fixed
found: 2026-10-05      # how: build (the split tool's own verifier)
fixed: this change
area: tools/lp-fw-split section_graph
class: assumed-context
related:
  - docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md
  - lp2025/2026-10-04-0757-ota-update-protocol (Part B, P03)
---
# The split tool missed a core reference written as `sym - 12`, and placed a table the core reads in the engine region

**Symptom** — the first split build with the over-the-air update session in
the core failed its own verifier:

```
== verify: core nodes in engine region: 1; engine nodes left in core region: 26 (4493 B)
lp-fw-split: pass 2 placed core code in the engine region: .Lanon.d6e3833e5b667f7308b8d048ee671fa8.31
```

`.Lanon…31` is the 12-byte string `"lpc_shared::"`, which nothing in the
core uses.

**Root cause** — the section graph turns every relocation into an edge to
the input section holding `S + A`. `board_facts` (core) looks a chip word up
in `lpc_update`'s code table, and codegen addresses that 12-byte table as
`.Lanon…62 - 0xc` and indexes it from 12 up
(`R_RISCV_HI20/LO12_I .Lanon.7b46…62-0xc`). `S + A` then lands in whichever
input section the link placed before the table: a core section in pass 1, so
the table itself was never reached from a core root and pass 1 put it in the
engine; `.Lanon…31` in pass 2, which the verifier then flagged. The verifier
caught the wrong node, but the real miss was worse: **the table the core
reads was in the engine region**, which a core-only board never maps.

**Fix** — `read_relocations` also gives a relocation's symbol's own address
as a target whenever its addend is non-zero (a section symbol excepted: its
value is its section's start). An extra edge only ever moves a node into the
core, so the split stays conservative.

**Regression coverage** — `section_graph::tests::a_negative_addend_reaches_the_symbols_own_section_as_read_gives_it`;
the verifier itself runs on every split build (CI's size check).

**Lesson** — `S + A` is the address a relocation *computes*, not the section
it *means*: an addend can point outside its symbol on purpose. Any tool that
attributes relocations to sections by address must also follow the symbol.
