---
status: open
found: 2026-10-10      # e2e (an emulated boot of a diag image)
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

**Fix** — none yet in the emulator. The diag uses a compare-exchange loop
(`lr.w`/`sc.w`) instead. The emulator fix is four arms in `atomic.rs` beside
`amoand`, plus an instruction-conformance case each.
