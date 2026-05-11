# Scope Of Work

Make LPIR narrow-load alignment intentional and consistent across the backends
used by texture filetests.

This plan is an interstitial cleanup/fix after M3b exposed a suspicious
`lpvm-cranelift` `Load16U` workaround. The goal is to:

- Define LPIR `Load16U` / `Load16S` as requiring 2-byte alignment.
- Define LPIR `Load32` as requiring 4-byte alignment.
- Keep `Load8U` / `Load8S` byte-addressable.
- Prove the RV32 emulator traps misaligned `lh` / `lhu` / `lw` in strict mode.
- Remove the `lpvm-cranelift` `Load16U` decomposition and use Cranelift's
  ordinary `uload16 -> lhu` path.
- Preserve the M3b texture behavior: texture fixture addresses are already
  naturally aligned, so this should not change valid texture tests.

Out of scope:

- Adding a byte-aligned or unaligned LPIR load operation.
- Adding a WASM-style alignment hint to LPIR.
- Implementing full emulator ISA-profile gating for `rv32imac`.
- Fixing broader `lp-cranelift` target feature modeling beyond what is needed
  to prove `Load16U -> lhu`.

# Current State

`docs/reports/2026-04-27-rv32-load16-issue.md` captures the investigation.
The important findings are:

- RV32I includes ordinary `lh` / `lhu`; no Zcb support is needed for 16-bit
  scalar loads.
- Misaligned halfword/word loads are not portable on RISC-V. Generated device
  code should use natural alignment unless there is an explicit fallback.
- `lpvm-native` lowers `Load16U` directly to `lhu`, so it already assumes
  2-byte alignment.
- `lpvm-cranelift` currently composes `Load16U` from aligned 32-bit loads and
  bit extraction on RV32, which permits odd addresses and bypasses the actual
  Cranelift `uload16` lowering path.
- `lpvm-wasm` emits `i32.load16_u` with `align=1` (2-byte declared alignment),
  but WebAssembly still permits unaligned loads at runtime.
- `lpir::interp` byte-reads two bytes and therefore permits odd addresses.
- M3b texture fixtures allocate texture bytes at 4-byte alignment, all current
  texture formats have even bytes-per-pixel, and channel offsets are
  `channel_index * 2`. Texture `Load16U` addresses should be 2-byte aligned.

# Questions That Need To Be Answered

No open questions. The chosen direction is:

- LPIR narrow loads should require natural alignment for performance and device
  simplicity.
- WASM/interp may remain more permissive internally, but the LPIR contract and
  tests should make misaligned `Load16*` invalid for generated code.
- The Cranelift workaround should be removed unless a focused test proves the
  lower layer cannot emit or execute legal `lhu`.

# Notes

- Emulator ISA-profile gating is important but broader than this plan. Track it
  as follow-up so this cleanup stays small.
- If future language features require byte-addressable unaligned 16-bit reads,
  add a distinct LPIR operation or explicit option rather than weakening
  `Load16U`.
