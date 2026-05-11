# Summary

## What was built

- **LPIR contract:** Scalar `Load16U`/`Load16S`/`Load32` (and matching stores where applicable) document natural alignment (`base + offset`): 2 bytes for halfword ops, 4 bytes for word ops; `Load8*` remain byte-aligned.
- **Emulator:** Misaligned `lh`/`lhu`/`lw` traps in strict mode are covered so LPVM regressions surface early.
- **Cranelift RV32 lowering:** Halfword loads lower to CLIF `load i16` plus `uextend`/`sextend` (matching halfword semantics; raw `uload16` fails the bundled riscv32 validator). The prior 32-bit word-load decomposition workaround is gone.
- **Backend/`lps-filetests`:** Contract-focused coverage (textures and related) exercised on backends used in CI.

## Decisions for future reference

#### Natural Alignment For LPIR Loads

- **Decision:** `Load16*` requires 2-byte alignment and `Load32` requires 4-byte alignment.
- **Why:** RV32 device code can use direct `lh` / `lhu` / `lw` without expensive unaligned sequences.
- **Rejected alternatives:** Byte-addressable `Load16*` by default (costly on RV32); WASM-style alignment hints (not needed yet).
- **Revisit when:** A real shader feature needs unaligned 16-bit reads.

#### ISA Gating Deferred

- **Decision:** Track emulator `rv32imac` ISA-profile gating separately.
- **Why:** Important for false-success prevention, but broader than the Load16 alignment contract.
- **Rejected alternatives:** Fold ISA gating into this cleanup (would expand scope).
