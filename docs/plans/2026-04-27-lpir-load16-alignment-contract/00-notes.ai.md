# Scope Of Work

Make LPIR narrow-load alignment semantics explicit and bring RV32 validation
back to the natural hardware path:

- Define `Load16U` / `Load16S` as requiring 2-byte alignment.
- Define `Load32` / `Store32` as requiring 4-byte alignment, and `Store16` as
  requiring 2-byte alignment.
- Keep `Load8*` / `Store8` byte-addressable.
- Prove the emulator traps misaligned strict-mode halfword/word accesses.
- Remove the `lpvm-cranelift` `Load16U` word-load decomposition once the
  ordinary `uload16 -> lhu` path is validated.
- Add focused backend/filetest guardrails for aligned texture channel reads.
- Track emulator ISA-profile gating as follow-up work, not part of this small
  interstitial plan.

# Current State

- `lpir::LpirOp::Load16U` and `Load16S` are documented only as 16-bit loads,
  without an alignment contract.
- `lpvm-native` lowers `Load16U` to RV32 `lhu` and `Load16S` to `lh`, which
  requires natural 2-byte alignment on strict RV32 execution.
- `lpvm-cranelift` currently decomposes `Load16U` on RV32 into aligned word
  loads plus bit extraction. This permits odd byte addresses and bypasses the
  ordinary Cranelift `uload16 -> lhu` path.
- `lpvm-wasm` lowers `Load16U` to `i32.load16_u` with `align=1` (2-byte
  declared alignment). WASM permits unaligned accesses, so it may pass cases
  that strict RV32 would reject.
- `lpir::interp` reads two bytes directly and therefore also permits odd
  addresses today.
- Texture fixtures allocate storage with 4-byte alignment. Supported texture
  formats have even bytes-per-pixel and channel offsets are `channel * 2`, so
  M3b `texelFetch` channel loads are naturally halfword-aligned.
- `lp-riscv-emu` strict mode rejects misaligned halfword/word memory accesses
  by default, but broader ISA-profile gating is not explicit.

# Decisions

## Natural Alignment Is The LPIR Contract

`Load16*` and `Store16` require 2-byte alignment; `Load32` and 32-bit stores
require 4-byte alignment. We are not adding a byte-aligned `Load16U` mode now.

## WASM Permissiveness Is Backend Behavior, Not LPIR Semantics

WASM may legally execute unaligned `i32.load16_u`, but LPIR users should not
depend on that behavior. Tests for shared LPIR behavior should assume the
natural-alignment contract.

## Remove The Cranelift Workaround

The RV32 Cranelift path should use ordinary `uload16 -> lhu` for `Load16U`.
If that fails, the failure should be fixed in `lp-cranelift` or the emulator
rather than hidden in `lpvm-cranelift` lowering.

## ISA Gating Is Follow-Up

Adding explicit emulator feature-profile gating for `rv32imac` is important,
but it is broader than this alignment cleanup. This plan should leave a clear
follow-up note instead of folding that larger change into the alignment fix.

# Questions That Need To Be Answered

No open questions. The intended path is intentionally narrow:

- Prefer the natural-alignment LPIR contract.
- Validate strict emulator alignment traps.
- Remove the `lpvm-cranelift` unaligned `Load16U` decomposition.
- Add enough tests/docs to prevent the mismatch from returning.
