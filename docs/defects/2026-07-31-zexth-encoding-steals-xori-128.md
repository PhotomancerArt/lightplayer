---
status: fixed
found: 2026-07-31      # how: ci
fixed: this change
area: lp-riscv/lp-riscv-inst, lp-riscv/lp-riscv-emu
class: invented-encoding
related:
  - docs/defects/2026-07-31-elf-loader-riscv-reloc-numbering.md
---
# `zext.h` was given an encoding that is really `xori rd, rs, 128`

**Symptom** — On `claude/f32-m5-builtins` (PR #224), and only there,
`cargo test -p fw-tests --test scene_render_emu` rendered a black frame:

```
[shader-node] compilation failed: compile: internal: JIT link failed:
  internal: unresolved symbol `__lp_lpir_fdiv_recip_q32` for JIT relocation at offset 96
```

The symbol was present in the firmware ELF and `jit_builtin_code_ptr` returned
an address for it, yet `BuiltinTable` held 40 entries after 85 successful
inserts. Probing inside the guest: `BuiltinId::all()` iterated all 166 ids with
correct discriminants, but `name()` returned only **77 distinct strings** for
those 166 ids — `name()` on discriminant 0 came back
`"__lp_lpfn_saturate_q32"` instead of `"__lps_acos_f32"`.

**Root cause** — `lp-riscv-inst` encoded Zbb's `zext.h rd, rs1` as an OP-IMM
form: opcode `0x13`, funct3 `0x100`, funct12 `0x080`. That is not the
instruction's encoding. RV32 spells `zext.h` as `pack rd, rs1, x0`, an **OP**
(R-type) encoding — `zext.h` is the one member of the `clz` / `ctz` / `cpop` /
`sext.b` / `sext.h` family that does *not* live in OP-IMM, and the invented
encoding assumed the symmetry held.

Those 32 bits already mean something: `xori rd, rs1, 128`. The decoder and the
emulator's OP-IMM executor both special-cased funct12 `0x080` before reaching
XORI, so every `xori rd, rs, 128` in a guest image executed as a halfword
zero-extend instead.

LLVM emits exactly that instruction as the **index bias of a large jump
table**: once a `match` is big enough to become a `.rodata` lookup table whose
case values cluster away from zero, it re-bases the index with an `xori`.
`BuiltinId::name()` crossed that threshold when the enum grew 117 → 166 arms
for the native-f32 family, so `name()` began indexing its 256-entry
`(ptr, len)` table without the bias. Since `zext.h` is the identity on values
below 65536, the mis-execution was silent: no fault, no wrong-looking
register, just the wrong table slot. The 166 ids read table slots 0..165 —
38 valid, 90 unreachable-default filler, 38 valid — which is precisely the 77
distinct names observed, collapsing 85 `VecMap` inserts to 40 keys.

**Latent on main.** Nothing about the defect is specific to this branch; any
guest code containing `xori rd, rs, 128` was mis-executed, and had been since
the encoding was written. The f32 work only supplied the first hot-path
instance.

**Fix** — `encode::zexth` now emits the real OP encoding
(`0x33`, funct7 `0x04`, funct3 `0x4`, rs2 = `x0`); the decoder and the
emulator's R-type executor recognize it there, with `rs2 == x0` required. The
OP-IMM funct3 `0x4` arm in both the decoder and the executor is now
unconditionally XORI.

**Regression coverage** — `lp-riscv-inst/tests/instruction_tests.rs`:
`xori_128_is_xori_not_zexth` (decode), `xori_128_flips_bit_seven` (execute),
`zexth_uses_the_op_encoding`, `zexth_zero_extends_halfword`. The first two
fail on the old code. `scene_render_emu` is the end-to-end oracle.

**Lesson** — An instruction encoding invented by analogy is a claim about a
bit pattern nobody else agreed to, and the ISA's opcode space is dense enough
that such a claim almost always collides with a real instruction. The
collision here was with a *base-ISA* instruction, so the emulator was wrong
about RV32I itself while passing every test — because the stolen form
(`imm == 128`) is rare in hand-written code and only appears when a compiler
optimizes at a size threshold nothing in the suite crossed. Two rules follow.
First: encodings come from the spec, never from the shape of a sibling
instruction; if a family has an exception, that exception is exactly where the
bug will be. Second: a decoder that carves a special case out of a base-ISA
opcode's immediate space should be read as a bug report until proven
otherwise — the base ISA owns that space, and any extension claiming part of
it is either misencoded or genuinely ambiguous, and both are worth stopping
for.
