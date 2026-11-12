---
status: fixed
found: 2026-10-10      # e2e (an emulated boot of a diag image)
fixed: PR claude/auto-2026-10-06-emu-rv32a-amo-min-max (2026-10-20)
area: lp-emu/lp-riscv-emu executor/atomic.rs (the C6's rv32 A extension)
class: emulator-fidelity
related:
  - lp2025/2026-10-08-2339-tree-store-firmware-and-emulator
---
# The emulated C6 does not decode `amomin`/`amomax` (`.w`, signed or unsigned)

**Symptom** — an `fs-tree,heap_peak_diag` image panicked 19 ms into its
emulated boot: `Exception 'Illegal instruction' mepc=0x420c01a0,
mtval=0xe0a5a02f`. `0xe0a5a02f` is `amomaxu.w` (funct5 `11100`), from an
`AtomicUsize::fetch_max` in the diag's allocation hook. Under
`reboot_on_reset` the board reset-looped and a walk built on it waited out
its whole budget with no line.

**Root cause** — `lp-riscv-emu`'s A extension decodes `lr.w`, `sc.w`,
`amoswap`, `amoadd`, `amoxor`, `amoand` and `amoor`; funct5 `0x10`/`0x14`/
`0x18`/`0x1c` (`amomin`, `amomax`, `amominu`, `amomaxu`) fall to
`InvalidInstruction`. The C6 is RV32IMAC: silicon executes all eleven. No
shipped image emits them today (or every emulated boot would trap), so the
gap hid until a diag used `fetch_max`/`fetch_min` on an atomic.

**Fix** — `atomic.rs` decodes funct5 `0x10` `amomin.w`, `0x14` `amomax.w`
(signed), `0x18` `amominu.w` and `0x1c` `amomaxu.w` (unsigned) through one
helper beside the other AMOs: read the word, write back the min or max of it
and `rs2`, give `rd` the old word (nothing written to `x0`). Written from the
RISC-V Unprivileged ISA's "A" chapter, not from any other emulator. aq/rl are
ignored and an LR/SC reservation is left alone, as for the existing AMOs.
Tests beside the others: one per op over `i32::MIN`/`MAX`, `-1` (`u32::MAX`),
`0` and `1`, plus `rd = x0` and aq/rl. `lp-riscv-inst` lacked the four too
(its decoder, disassembler and encoder placeholder), so a disassembly of a
real image would have shown "Unknown atomic instruction"; it names them now.
The diag's compare-exchange loop can go back to `fetch_max`; that is a
firmware-linked change and was left for its own PR.
